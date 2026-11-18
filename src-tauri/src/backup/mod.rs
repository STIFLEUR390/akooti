//! Backup drivers and the snapshot runner.
//!
//! A snapshot is produced by the core (`.akooti`, already encrypted) and handed
//! to one provider. Providers only move bytes: they never see a key. Retention
//! deletes the oldest snapshots beyond `retention_count`.

pub mod scheduler;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use akooti_core::{
    BackupConfig, BackupRun, BackupTarget, NewBackupRun, SNAPSHOT_SUFFIX, expired_snapshots,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use tauri::AppHandle;

use crate::error::CommandError;
use crate::sync::time::now_seconds;
use crate::vault_runtime::VaultRuntime;

const NETWORK_TIMEOUT: Duration = Duration::from_secs(30);

/// Event emitted after a scheduled or manual run.
pub const BACKUP_EVENT: &str = "backup:status";
/// Guards against two runs touching the same vault.
pub static BACKUP_RUNNING: AtomicBool = AtomicBool::new(false);

/// Provider failure, already phrased for the run history.
#[derive(Debug, Clone)]
pub struct StoreError(pub String);

impl StoreError {
    fn new(detail: impl Into<String>) -> Self {
        Self(detail.into())
    }
}

/// Minimal object store used by the backup runner.
trait Store {
    fn put(&self, bytes: &[u8], name: &str) -> Result<(), StoreError>;
    fn list(&self) -> Result<Vec<String>, StoreError>;
    fn delete(&self, name: &str) -> Result<(), StoreError>;
    fn probe(&self) -> Result<String, StoreError>;
}

fn store_for(config: &BackupConfig) -> Box<dyn Store> {
    match config {
        BackupConfig::Local { path } => Box::new(LocalStore {
            dir: PathBuf::from(path),
        }),
        BackupConfig::S3 {
            endpoint,
            region,
            bucket,
            access_key_id,
            secret_access_key,
            prefix,
        } => Box::new(S3Store {
            endpoint: endpoint.clone(),
            region: region.clone(),
            bucket: bucket.clone(),
            access_key_id: access_key_id.clone(),
            secret_access_key: secret_access_key.clone(),
            prefix: prefix.clone().unwrap_or_default(),
        }),
        BackupConfig::GDrive {
            client_id,
            client_secret,
            refresh_token,
            folder_id,
        } => Box::new(DriveStore {
            client_id: client_id.clone(),
            client_secret: client_secret.clone(),
            refresh_token: refresh_token.clone(),
            folder_id: folder_id.clone().unwrap_or_else(|| "root".to_owned()),
        }),
    }
}

/// Runs one target: snapshot, upload, retention, then records the outcome.
pub fn run_target(runtime: &VaultRuntime, target_id: &str) -> Result<BackupRun, CommandError> {
    let _guard = BackpressureGuard::acquire()?;
    let now = now_seconds();
    let (target, config) = runtime.with_vault(|vault| {
        Ok((
            vault.backup_target(target_id)?,
            vault.backup_config(target_id)?,
        ))
    })?;
    // Milliseconds keep two runs in the same second from overwriting each other;
    // the numeric prefix stays sortable for retention.
    let name = format!(
        "{}{}{SNAPSHOT_SUFFIX}",
        akooti_core::SNAPSHOT_PREFIX,
        crate::sync::time::now_millis()
    );
    let temp = std::env::temp_dir().join(&name);

    let outcome = (|| -> Result<(u64, String), StoreError> {
        runtime
            .with_vault(|vault| vault.write_snapshot(&temp, now))
            .map_err(|_| StoreError::new("instantané illisible"))?;
        let bytes = std::fs::read(&temp).map_err(|_| StoreError::new("instantané illisible"))?;
        let size = u64::try_from(bytes.len()).unwrap_or(0);
        let store = store_for(&config);
        store.put(&bytes, &name)?;
        let kept = store.list().unwrap_or_default();
        for expired in expired_snapshots(&kept, target.retention_count) {
            let _ = store.delete(&expired);
        }
        Ok((size, format!("{} · {name}", target.summary)))
    })();
    let _ = std::fs::remove_file(&temp);

    let run = match outcome {
        Ok((size, detail)) => NewBackupRun {
            status: "success".to_owned(),
            detail: Some(detail),
            size_bytes: Some(i64::try_from(size).unwrap_or(i64::MAX)),
            snapshot_name: Some(name),
        },
        Err(error) => NewBackupRun {
            status: "error".to_owned(),
            detail: Some(error.0),
            size_bytes: None,
            snapshot_name: Some(name),
        },
    };
    let recorded =
        runtime.with_vault_mut(|vault| vault.record_backup_run(target_id, run.clone(), now))?;
    if run.status == "error" {
        return Err(CommandError::new(
            "backup",
            run.detail
                .clone()
                .unwrap_or_else(|| "Sauvegarde échouée.".to_owned()),
        ));
    }
    Ok(recorded)
}

/// Tests a configuration without storing it.
pub fn test_config(config: &BackupConfig) -> Result<String, CommandError> {
    store_for(config)
        .probe()
        .map_err(|error| CommandError::new("backup", error.0))
}

/// Spawns the hourly scheduler loop.
pub fn spawn_scheduler(runtime: std::sync::Arc<VaultRuntime>, app: AppHandle) {
    scheduler::spawn(runtime, app);
}

/// Whether a run is already in flight.
#[must_use]
pub fn is_running() -> bool {
    BACKUP_RUNNING.load(Ordering::SeqCst)
}

/// Targets due right now, used by the scheduler.
pub(crate) fn due(runtime: &VaultRuntime, now: i64) -> Vec<BackupTarget> {
    runtime
        .with_vault(|vault| vault.due_backup_targets(now))
        .unwrap_or_default()
}

struct BackpressureGuard;

impl BackpressureGuard {
    fn acquire() -> Result<Self, CommandError> {
        if BACKUP_RUNNING.swap(true, Ordering::SeqCst) {
            return Err(CommandError::new(
                "backup_busy",
                "Une sauvegarde est déjà en cours.",
            ));
        }
        Ok(Self)
    }
}

impl Drop for BackpressureGuard {
    fn drop(&mut self) {
        BACKUP_RUNNING.store(false, Ordering::SeqCst);
    }
}

struct LocalStore {
    dir: PathBuf,
}

impl Store for LocalStore {
    fn put(&self, bytes: &[u8], name: &str) -> Result<(), StoreError> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|_| StoreError::new("dossier de sauvegarde inaccessible"))?;
        write_atomic(&self.dir.join(name), bytes)
            .map_err(|_| StoreError::new("écriture du snapshot impossible"))
    }

    fn list(&self) -> Result<Vec<String>, StoreError> {
        let entries = std::fs::read_dir(&self.dir)
            .map_err(|_| StoreError::new("dossier de sauvegarde inaccessible"))?;
        let mut names = Vec::new();
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                names.push(name.to_owned());
            }
        }
        Ok(names)
    }

    fn delete(&self, name: &str) -> Result<(), StoreError> {
        std::fs::remove_file(self.dir.join(name))
            .map_err(|_| StoreError::new("suppression du snapshot impossible"))
    }

    fn probe(&self) -> Result<String, StoreError> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|_| StoreError::new("dossier de sauvegarde inaccessible"))?;
        let probe = self.dir.join(".akooti-write-test");
        write_atomic(&probe, b"akooti")
            .map_err(|_| StoreError::new("écriture impossible dans ce dossier"))?;
        let _ = std::fs::remove_file(&probe);
        Ok(format!("Dossier accessible : {}", self.dir.display()))
    }
}

struct S3Store {
    endpoint: String,
    region: String,
    bucket: String,
    access_key_id: String,
    secret_access_key: String,
    prefix: String,
}

impl S3Store {
    fn key(&self, name: &str) -> String {
        if self.prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{}/{}", self.prefix.trim_matches('/'), name)
        }
    }

    fn send(
        &self,
        method: &str,
        key: &str,
        query: &[(&str, &str)],
        body: &[u8],
    ) -> Result<(u16, String), StoreError> {
        let canonical_query = canonical_query(query);
        let url = if canonical_query.is_empty() {
            format!(
                "{}/{}/{}",
                self.endpoint.trim_end_matches('/'),
                self.bucket,
                encode_path(key)
            )
        } else {
            format!(
                "{}/{}/{}?{}",
                self.endpoint.trim_end_matches('/'),
                self.bucket,
                encode_path(key),
                canonical_query
            )
        };
        let host = url
            .split("://")
            .nth(1)
            .and_then(|rest| rest.split('/').next())
            .unwrap_or_default()
            .to_owned();
        let now = now_seconds();
        let (date, stamp) = amz_timestamps(now);
        let payload_hash = hex::encode(Sha256::digest(body));
        let signed_headers = "host;x-amz-content-sha256;x-amz-date";
        let canonical_headers =
            format!("host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{stamp}\n");
        let canonical_uri = format!("/{}/{}", self.bucket, encode_path(key));
        let canonical_request = format!(
            "{method}\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
        );
        let scope = format!("{date}/{}/s3/aws4_request", self.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{stamp}\n{scope}\n{}",
            hex::encode(Sha256::digest(canonical_request.as_bytes()))
        );
        let signing_key = signing_key(&self.secret_access_key, &date, &self.region);
        let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
            self.access_key_id
        );

        let response = match method {
            "PUT" => signed(
                ureq::put(&url).header("Content-Type", "application/octet-stream"),
                &payload_hash,
                &stamp,
                &authorization,
            )
            .send(body),
            "GET" => signed(ureq::get(&url), &payload_hash, &stamp, &authorization).call(),
            "DELETE" => signed(ureq::delete(&url), &payload_hash, &stamp, &authorization).call(),
            _ => return Err(StoreError::new("méthode S3 inconnue")),
        };
        match response {
            Ok(mut response) => {
                let status = response.status().as_u16();
                let text = response.body_mut().read_to_string().unwrap_or_default();
                Ok((status, text))
            }
            Err(ureq::Error::StatusCode(code)) => {
                Err(StoreError::new(format!("S3 a refusé la requête ({code})")))
            }
            Err(_) => Err(StoreError::new("S3 injoignable")),
        }
    }

    fn list_keys(&self) -> Result<Vec<String>, StoreError> {
        let mut keys = Vec::new();
        let (status, body) = self.send(
            "GET",
            "",
            &[("list-type", "2"), ("prefix", &self.key(""))],
            &[],
        )?;
        if status != 200 {
            return Err(StoreError::new(format!("S3 a refusé la liste ({status})")));
        }
        for piece in body.split("<Key>").skip(1) {
            if let Some(key) = piece.split("</Key>").next() {
                let name = key.rsplit('/').next().unwrap_or(key);
                keys.push(name.to_owned());
            }
        }
        Ok(keys)
    }
}

impl Store for S3Store {
    fn put(&self, bytes: &[u8], name: &str) -> Result<(), StoreError> {
        let (status, _) = self.send("PUT", &self.key(name), &[], bytes)?;
        if (200..300).contains(&status) {
            Ok(())
        } else {
            Err(StoreError::new(format!("S3 a refusé l'envoi ({status})")))
        }
    }

    fn list(&self) -> Result<Vec<String>, StoreError> {
        self.list_keys()
    }

    fn delete(&self, name: &str) -> Result<(), StoreError> {
        let (status, _) = self.send("DELETE", &self.key(name), &[], &[])?;
        if (200..300).contains(&status) {
            Ok(())
        } else {
            Err(StoreError::new(format!(
                "S3 a refusé la suppression ({status})"
            )))
        }
    }

    fn probe(&self) -> Result<String, StoreError> {
        self.list_keys()?;
        Ok(format!("Bucket accessible : {}", self.bucket))
    }
}

struct DriveStore {
    client_id: String,
    client_secret: String,
    refresh_token: String,
    folder_id: String,
}

impl DriveStore {
    fn access_token(&self) -> Result<String, StoreError> {
        let url = "https://oauth2.googleapis.com/token";
        let body = format!(
            "client_id={}&client_secret={}&refresh_token={}&grant_type=refresh_token",
            encode_query(&self.client_id),
            encode_query(&self.client_secret),
            encode_query(&self.refresh_token),
        );
        let mut response = ureq::post(url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .config()
            .timeout_global(Some(NETWORK_TIMEOUT))
            .build()
            .send(body)
            .map_err(|_| StoreError::new("Google a refusé le jeton de rafraîchissement"))?;
        let text = response.body_mut().read_to_string().unwrap_or_default();
        let value: serde_json::Value =
            serde_json::from_str(&text).map_err(|_| StoreError::new("réponse Google illisible"))?;
        value
            .get("access_token")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| StoreError::new("Google n'a pas renvoyé de jeton d'accès"))
    }

    fn files(&self) -> Result<Vec<(String, String)>, StoreError> {
        let token = self.access_token()?;
        let query = format!(
            "'{}' in parents and trashed = false and name contains '{}'",
            self.folder_id,
            akooti_core::SNAPSHOT_PREFIX.trim_end_matches('-')
        );
        let url = format!(
            "https://www.googleapis.com/drive/v3/files?q={}&fields=files(id,name)&pageSize=200",
            encode_query(&query)
        );
        let mut response = ureq::get(&url)
            .header("Authorization", &format!("Bearer {token}"))
            .config()
            .timeout_global(Some(NETWORK_TIMEOUT))
            .build()
            .call()
            .map_err(|_| StoreError::new("Google Drive injoignable"))?;
        let text = response.body_mut().read_to_string().unwrap_or_default();
        let value: serde_json::Value =
            serde_json::from_str(&text).map_err(|_| StoreError::new("réponse Drive illisible"))?;
        let mut files = Vec::new();
        if let Some(entries) = value.get("files").and_then(serde_json::Value::as_array) {
            for entry in entries {
                let id = entry.get("id").and_then(serde_json::Value::as_str);
                let name = entry.get("name").and_then(serde_json::Value::as_str);
                if let (Some(id), Some(name)) = (id, name) {
                    files.push((id.to_owned(), name.to_owned()));
                }
            }
        }
        Ok(files)
    }
}

impl Store for DriveStore {
    fn put(&self, bytes: &[u8], name: &str) -> Result<(), StoreError> {
        let token = self.access_token()?;
        let boundary = "akooti-boundary-7d3f";
        let metadata = json!({"name": name, "parents": [self.folder_id]});
        let mut body = Vec::with_capacity(bytes.len() + 512);
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(metadata.to_string().as_bytes());
        body.extend_from_slice(
            format!("\r\n--{boundary}\r\nContent-Type: application/octet-stream\r\n\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let url = "https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart&fields=id";
        let response = ureq::post(url)
            .header("Authorization", &format!("Bearer {token}"))
            .header(
                "Content-Type",
                &format!("multipart/related; boundary={boundary}"),
            )
            .config()
            .timeout_global(Some(NETWORK_TIMEOUT))
            .build()
            .send(&body);
        match response {
            Ok(_) => Ok(()),
            Err(ureq::Error::StatusCode(code)) => Err(StoreError::new(format!(
                "Google Drive a refusé l'envoi ({code})"
            ))),
            Err(_) => Err(StoreError::new("Google Drive injoignable")),
        }
    }

    fn list(&self) -> Result<Vec<String>, StoreError> {
        Ok(self.files()?.into_iter().map(|(_, name)| name).collect())
    }

    fn delete(&self, name: &str) -> Result<(), StoreError> {
        let token = self.access_token()?;
        let Some((id, _)) = self
            .files()?
            .into_iter()
            .find(|(_, existing)| existing == name)
        else {
            return Ok(());
        };
        let url = format!("https://www.googleapis.com/drive/v3/files/{id}");
        ureq::delete(&url)
            .header("Authorization", &format!("Bearer {token}"))
            .config()
            .timeout_global(Some(NETWORK_TIMEOUT))
            .build()
            .call()
            .map_err(|_| StoreError::new("Google Drive a refusé la suppression"))?;
        Ok(())
    }

    fn probe(&self) -> Result<String, StoreError> {
        self.files()?;
        Ok("Google Drive accessible".to_owned())
    }
}

fn signed<S>(
    builder: ureq::RequestBuilder<S>,
    payload_hash: &str,
    stamp: &str,
    authorization: &str,
) -> ureq::RequestBuilder<S> {
    builder
        .header("x-amz-content-sha256", payload_hash)
        .header("x-amz-date", stamp)
        .header("Authorization", authorization)
        .config()
        .timeout_global(Some(NETWORK_TIMEOUT))
        .build()
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temp = path.with_extension("tmp-write");
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path)
}

fn encode_query(value: &str) -> String {
    crate::sync::time::encode_query_value(value)
}

fn encode_path(key: &str) -> String {
    key.split('/')
        .map(crate::sync::time::encode_query_value)
        .collect::<Vec<_>>()
        .join("/")
}

fn canonical_query(query: &[(&str, &str)]) -> String {
    let mut pairs: Vec<(String, String)> = query
        .iter()
        .map(|(key, value)| (encode_query(key), encode_query(value)))
        .collect();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn amz_timestamps(seconds: i64) -> (String, String) {
    let compact = crate::sync::time::to_iso8601(seconds).replace(['-', ':'], "");
    let date = compact.split('T').next().unwrap_or(&compact).to_owned();
    (date, compact)
}

fn signing_key(secret: &str, date: &str, region: &str) -> Vec<u8> {
    let date_key = hmac_sha256(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    let region_key = hmac_sha256(&date_key, region.as_bytes());
    let service_key = hmac_sha256(&region_key, b"s3");
    hmac_sha256(&service_key, b"aws4_request")
}

#[allow(clippy::expect_used, reason = "HMAC accepts keys of any length")]
fn hmac_sha256(key: &[u8], value: &[u8]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(value);
    mac.finalize().into_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::{amz_timestamps, canonical_query, encode_path, expired_snapshots, signing_key};

    #[test]
    fn amz_timestamps_follow_the_aws_format() {
        let (date, stamp) = amz_timestamps(1_800_000_000);
        assert_eq!(date, "20270115");
        assert_eq!(stamp, "20270115T080000Z");
    }

    #[test]
    fn canonical_query_is_sorted_and_encoded() {
        assert_eq!(
            canonical_query(&[("prefix", "akooti/x"), ("list-type", "2")]),
            "list-type=2&prefix=akooti%2Fx"
        );
    }

    #[test]
    fn keys_keep_their_segments() {
        assert_eq!(
            encode_path("backups/akooti-1.akooti"),
            "backups/akooti-1.akooti"
        );
    }

    #[test]
    fn signing_key_is_stable() {
        let first = signing_key("secret", "20270115", "eu-west-3");
        let second = signing_key("secret", "20270115", "eu-west-3");
        assert_eq!(first, second);
        assert_ne!(first, signing_key("secret", "20270116", "eu-west-3"));
        assert_eq!(first.len(), 32);
    }

    #[test]
    fn retention_is_shared_with_the_core() {
        let names = vec![
            "akooti-3.akooti".to_owned(),
            "akooti-1.akooti".to_owned(),
            "akooti-2.akooti".to_owned(),
        ];
        assert_eq!(expired_snapshots(&names, 2), vec!["akooti-1.akooti"]);
    }
}
