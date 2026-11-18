//! Encrypted backup targets and their run history.
//!
//! Provider credentials (S3 secret, Google refresh token) are secrets: they are
//! encrypted with the master key under an AAD bound to the target id, and never
//! leave the core except when the shell actually needs to upload. Nothing here
//! is synced or exported (rule AI-5).

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto::{EncryptedValue, KEY_LEN, backup_config_aad, decrypt_bytes, encrypt_bytes};
use crate::error::VaultError;

/// File suffix of a snapshot written by a backup target.
pub const SNAPSHOT_SUFFIX: &str = ".akooti";
/// Prefix every snapshot name starts with, so retention can recognise its own.
pub const SNAPSHOT_PREFIX: &str = "akooti-";

/// Where snapshots are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupProvider {
    /// A folder on this machine or a mounted NAS.
    Local,
    /// Any S3-compatible bucket (AWS, R2, MinIO, Wasabi).
    S3,
    /// The user's own Google Drive.
    GDrive,
}

impl BackupProvider {
    /// Wire name stored in `provider`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::S3 => "s3",
            Self::GDrive => "gdrive",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "local" => Some(Self::Local),
            "s3" => Some(Self::S3),
            "gdrive" => Some(Self::GDrive),
            _ => None,
        }
    }
}

/// Provider configuration, stored encrypted.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum BackupConfig {
    /// Local folder or mounted NAS path.
    Local {
        /// Absolute directory receiving the snapshots.
        path: String,
    },
    /// S3-compatible bucket, addressed with SigV4.
    S3 {
        /// Base endpoint, e.g. `https://s3.eu-west-3.amazonaws.com`.
        endpoint: String,
        /// Signing region, e.g. `eu-west-3`.
        region: String,
        /// Bucket name.
        bucket: String,
        /// Access key id.
        access_key_id: String,
        /// Secret access key, never returned to the UI.
        secret_access_key: String,
        /// Optional key prefix inside the bucket.
        prefix: Option<String>,
    },
    /// Google Drive, with a refresh token obtained once by the user.
    GDrive {
        /// OAuth2 client id of the user's Google project.
        client_id: String,
        /// OAuth2 client secret.
        client_secret: String,
        /// Long-lived refresh token.
        refresh_token: String,
        /// Optional destination folder id.
        folder_id: Option<String>,
    },
}

impl BackupConfig {
    /// Provider this configuration belongs to.
    #[must_use]
    pub const fn provider(&self) -> BackupProvider {
        match self {
            Self::Local { .. } => BackupProvider::Local,
            Self::S3 { .. } => BackupProvider::S3,
            Self::GDrive { .. } => BackupProvider::GDrive,
        }
    }

    /// Non-secret summary shown in the UI.
    #[must_use]
    pub fn summary(&self) -> String {
        match self {
            Self::Local { path } => path.clone(),
            Self::S3 {
                endpoint, bucket, ..
            } => format!("{endpoint} · {bucket}"),
            Self::GDrive { folder_id, .. } => folder_id.as_ref().map_or_else(
                || "Google Drive".to_owned(),
                |id| format!("Google Drive · {id}"),
            ),
        }
    }

    /// Whether the configuration carries every field its provider needs.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        match self {
            Self::Local { path } => !path.trim().is_empty(),
            Self::S3 {
                endpoint,
                region,
                bucket,
                access_key_id,
                secret_access_key,
                ..
            } => [endpoint, region, bucket, access_key_id, secret_access_key]
                .iter()
                .all(|value| !value.trim().is_empty()),
            Self::GDrive {
                client_id,
                client_secret,
                refresh_token,
                ..
            } => [client_id, client_secret, refresh_token]
                .iter()
                .all(|value| !value.trim().is_empty()),
        }
    }
}

/// One configured target, without its credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupTarget {
    /// Target identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Provider kind.
    pub provider: BackupProvider,
    /// Non-secret summary of the configuration.
    pub summary: String,
    /// Minutes between two runs.
    pub interval_minutes: u32,
    /// Snapshots kept per target.
    pub retention_count: u32,
    /// Whether the scheduler considers it.
    pub enabled: bool,
    /// Last run time, Unix seconds.
    pub last_run_at: Option<i64>,
    /// Last run status (`success`, `error`, ...).
    pub last_status: Option<String>,
}

/// Target creation or update payload.
#[derive(Debug, Clone)]
pub struct BackupTargetDraft {
    /// Display name.
    pub name: String,
    /// Provider configuration, encrypted on write.
    pub config: BackupConfig,
    /// Minutes between two runs, clamped by [`MIN_INTERVAL_MINUTES`].
    pub interval_minutes: u32,
    /// Snapshots kept.
    pub retention_count: u32,
    /// Whether the scheduler considers it.
    pub enabled: bool,
}

/// Fastest interval the scheduler accepts: hourly backups, as documented.
pub const MIN_INTERVAL_MINUTES: u32 = 60;
/// Lowest retention accepted, so a target never deletes its only backup.
pub const MIN_RETENTION: u32 = 1;

/// One recorded backup attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupRun {
    /// Run identifier.
    pub id: String,
    /// Target it ran for.
    pub target_id: String,
    /// Start time, Unix seconds.
    pub started_at: i64,
    /// End time, Unix seconds.
    pub finished_at: Option<i64>,
    /// `running`, `success` or `error`.
    pub status: String,
    /// Short non-secret diagnostic.
    pub detail: Option<String>,
    /// Snapshot size in bytes.
    pub size_bytes: Option<i64>,
    /// Snapshot file name, as sent to the provider.
    pub snapshot_name: Option<String>,
}

/// Outcome recorded for one run.
#[derive(Debug, Clone)]
pub struct NewBackupRun {
    /// `success` or `error`.
    pub status: String,
    /// Short non-secret diagnostic.
    pub detail: Option<String>,
    /// Snapshot size in bytes.
    pub size_bytes: Option<i64>,
    /// Snapshot file name.
    pub snapshot_name: Option<String>,
}

pub(crate) fn list_targets(
    connection: &Connection,
    master_key: &[u8; KEY_LEN],
) -> Result<Vec<BackupTarget>, VaultError> {
    let rows = {
        let mut statement = connection.prepare(
            "SELECT id, name, provider, interval_minutes, retention_count, enabled,
                    last_run_at, last_status
             FROM backup_targets ORDER BY created_at, id",
        )?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, bool>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };

    let mut targets = Vec::with_capacity(rows.len());
    for row in rows {
        let (id, name, provider, interval, retention, enabled, last_run_at, last_status) = row;
        let provider = BackupProvider::parse(&provider).ok_or(VaultError::CorruptVault)?;
        let summary = read_config(connection, master_key, &id)?.summary();
        targets.push(BackupTarget {
            id,
            name,
            provider,
            summary,
            interval_minutes: u32::try_from(interval.max(0)).unwrap_or(MIN_INTERVAL_MINUTES),
            retention_count: u32::try_from(retention.max(0)).unwrap_or(MIN_RETENTION),
            enabled,
            last_run_at,
            last_status,
        });
    }
    Ok(targets)
}

pub(crate) fn create(
    connection: &Connection,
    master_key: &[u8; KEY_LEN],
    draft: &BackupTargetDraft,
    now: i64,
) -> Result<BackupTarget, VaultError> {
    let id = Uuid::new_v4().to_string();
    let encrypted = encrypt_config(master_key, &id, &draft.config)?;
    connection.execute(
        "INSERT INTO backup_targets (
           id, name, provider, encrypted_config, config_nonce, interval_minutes,
           retention_count, enabled, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            draft.name,
            draft.config.provider().as_str(),
            encrypted.ciphertext,
            encrypted.nonce,
            i64::from(draft.interval_minutes.max(MIN_INTERVAL_MINUTES)),
            i64::from(draft.retention_count.max(MIN_RETENTION)),
            draft.enabled,
            now,
        ],
    )?;
    get(connection, master_key, &id)
}

pub(crate) fn update(
    connection: &Connection,
    master_key: &[u8; KEY_LEN],
    id: &str,
    draft: &BackupTargetDraft,
) -> Result<BackupTarget, VaultError> {
    let encrypted = encrypt_config(master_key, id, &draft.config)?;
    let changed = connection.execute(
        "UPDATE backup_targets SET name = ?2, provider = ?3, encrypted_config = ?4,
                config_nonce = ?5, interval_minutes = ?6, retention_count = ?7, enabled = ?8
         WHERE id = ?1",
        params![
            id,
            draft.name,
            draft.config.provider().as_str(),
            encrypted.ciphertext,
            encrypted.nonce,
            i64::from(draft.interval_minutes.max(MIN_INTERVAL_MINUTES)),
            i64::from(draft.retention_count.max(MIN_RETENTION)),
            draft.enabled,
        ],
    )?;
    if changed == 0 {
        return Err(VaultError::NotFound);
    }
    get(connection, master_key, id)
}

pub(crate) fn delete(connection: &Connection, id: &str) -> Result<(), VaultError> {
    let changed = connection.execute("DELETE FROM backup_targets WHERE id = ?1", [id])?;
    if changed == 0 {
        return Err(VaultError::NotFound);
    }
    Ok(())
}

pub(crate) fn set_enabled(
    connection: &Connection,
    master_key: &[u8; KEY_LEN],
    id: &str,
    enabled: bool,
) -> Result<BackupTarget, VaultError> {
    let changed = connection.execute(
        "UPDATE backup_targets SET enabled = ?2 WHERE id = ?1",
        params![id, enabled],
    )?;
    if changed == 0 {
        return Err(VaultError::NotFound);
    }
    get(connection, master_key, id)
}

pub(crate) fn get(
    connection: &Connection,
    master_key: &[u8; KEY_LEN],
    id: &str,
) -> Result<BackupTarget, VaultError> {
    list_targets(connection, master_key)?
        .into_iter()
        .find(|target| target.id == id)
        .ok_or(VaultError::NotFound)
}

pub(crate) fn read_config(
    connection: &Connection,
    master_key: &[u8; KEY_LEN],
    id: &str,
) -> Result<BackupConfig, VaultError> {
    let row: Option<(Vec<u8>, Vec<u8>)> = connection
        .query_row(
            "SELECT encrypted_config, config_nonce FROM backup_targets WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (ciphertext, nonce) = row.ok_or(VaultError::NotFound)?;
    let decrypted = decrypt_bytes(
        master_key,
        &EncryptedValue {
            ciphertext,
            nonce: nonce.try_into().map_err(|_| VaultError::CorruptVault)?,
        },
        &backup_config_aad(id),
    )?;
    serde_json::from_slice(decrypted.as_slice()).map_err(|_| VaultError::CorruptVault)
}

pub(crate) fn record_run(
    connection: &Connection,
    target_id: &str,
    run: &NewBackupRun,
    now: i64,
) -> Result<BackupRun, VaultError> {
    let exists: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM backup_targets WHERE id = ?1",
            [target_id],
            |row| row.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return Err(VaultError::NotFound);
    }
    let id = Uuid::new_v4().to_string();
    connection.execute(
        "INSERT INTO backup_runs (
           id, target_id, started_at, finished_at, status, detail, size_bytes, snapshot_name
         ) VALUES (?1, ?2, ?3, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            target_id,
            now,
            run.status,
            run.detail,
            run.size_bytes,
            run.snapshot_name,
        ],
    )?;
    connection.execute(
        "UPDATE backup_targets SET last_run_at = ?2, last_status = ?3 WHERE id = ?1",
        params![target_id, now, run.status],
    )?;
    Ok(BackupRun {
        id,
        target_id: target_id.to_owned(),
        started_at: now,
        finished_at: Some(now),
        status: run.status.clone(),
        detail: run.detail.clone(),
        size_bytes: run.size_bytes,
        snapshot_name: run.snapshot_name.clone(),
    })
}

pub(crate) fn list_runs(
    connection: &Connection,
    target_id: Option<&str>,
    limit: usize,
) -> Result<Vec<BackupRun>, VaultError> {
    let limit = i64::try_from(limit.clamp(1, 500)).unwrap_or(50);
    let mut statement = connection.prepare(
        "SELECT id, target_id, started_at, finished_at, status, detail, size_bytes, snapshot_name
         FROM backup_runs
         WHERE (?1 IS NULL OR target_id = ?1)
         ORDER BY started_at DESC, rowid DESC LIMIT ?2",
    )?;
    let rows = statement.query_map(params![target_id, limit], |row| {
        Ok(BackupRun {
            id: row.get(0)?,
            target_id: row.get(1)?,
            started_at: row.get(2)?,
            finished_at: row.get(3)?,
            status: row.get(4)?,
            detail: row.get(5)?,
            size_bytes: row.get(6)?,
            snapshot_name: row.get(7)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Targets whose next run is due.
pub(crate) fn due_targets(
    connection: &Connection,
    master_key: &[u8; KEY_LEN],
    now: i64,
) -> Result<Vec<BackupTarget>, VaultError> {
    Ok(list_targets(connection, master_key)?
        .into_iter()
        .filter(|target| {
            target.enabled
                && target.last_run_at.is_none_or(|last| {
                    last + i64::from(target.interval_minutes.max(MIN_INTERVAL_MINUTES)) * 60 <= now
                })
        })
        .collect())
}

/// Snapshot names to delete so at most `retention` remain, newest kept.
///
/// Names are expected to carry the epoch they were written at
/// (`akooti-<epoch>.akooti`), which makes the order unambiguous without
/// trusting a remote timestamp.
#[must_use]
pub fn expired_snapshots(names: &[String], retention: u32) -> Vec<String> {
    let epoch = |name: &str| {
        name.trim_start_matches(SNAPSHOT_PREFIX)
            .trim_end_matches(SNAPSHOT_SUFFIX)
            .parse::<i64>()
            .unwrap_or(0)
    };
    let mut snapshots: Vec<&String> = names
        .iter()
        .filter(|name| name.starts_with(SNAPSHOT_PREFIX) && name.ends_with(SNAPSHOT_SUFFIX))
        .collect();
    snapshots.sort_by_key(|name| std::cmp::Reverse(epoch(name)));
    snapshots
        .into_iter()
        .skip(usize::try_from(retention.max(MIN_RETENTION)).unwrap_or(usize::MAX))
        .cloned()
        .collect()
}

fn encrypt_config(
    master_key: &[u8; KEY_LEN],
    target_id: &str,
    config: &BackupConfig,
) -> Result<EncryptedValue, VaultError> {
    if !config.is_complete() {
        return Err(VaultError::InvalidInput(
            "the backup target configuration is incomplete",
        ));
    }
    let serialized =
        Zeroizing::new(serde_json::to_vec(config).map_err(|_| VaultError::CorruptVault)?);
    encrypt_bytes(master_key, &serialized, &backup_config_aad(target_id))
}
