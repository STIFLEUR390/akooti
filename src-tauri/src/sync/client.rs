//! Supabase REST transport for the encrypted sync records.
//!
//! The client knows nothing about vault contents: it moves opaque records and
//! the public envelope. Every payload is already encrypted by `akooti-core`.

use std::time::Duration;

use akooti_core::{SyncEnvelope, SyncRecord, SyncRecordType, SyncSession};
use serde_json::{Value, json};

use super::time::{encode_query_value, from_iso8601, to_iso8601};

const PAGE_SIZE: usize = 500;
const TIMEOUT_SECONDS: u64 = 20;

/// Stable sync error, mapped to a command code without leaking a payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncError {
    /// Stable machine-readable code.
    pub code: &'static str,
    /// Short, non-secret description.
    pub detail: String,
}

impl SyncError {
    fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    /// The network did not answer.
    pub fn network() -> Self {
        Self::new("network", "Le serveur Supabase est injoignable.")
    }

    /// Credentials or tokens were rejected.
    pub fn unauthorized() -> Self {
        Self::new(
            "unauthorized",
            "Supabase a refusé les identifiants ou le jeton.",
        )
    }

    /// The server answered with an unexpected status or body.
    pub fn server() -> Self {
        Self::new("sync_server", "Supabase a renvoyé une réponse inattendue.")
    }

    /// The project is reachable but the Akooti tables are missing.
    pub fn schema_missing() -> Self {
        Self::new(
            "sync_schema",
            "Le schéma Akooti est absent de ce projet : appliquez supabase/setup.sql.",
        )
    }
}

/// A signed-in session and the Supabase user it belongs to.
#[derive(Debug, Clone)]
pub struct SignIn {
    /// Tokens to store encrypted.
    pub session: SyncSession,
    /// Supabase user identifier.
    pub user_id: String,
}

/// One delta page.
#[derive(Debug, Clone, Default)]
pub struct PullBatch {
    /// Records received.
    pub records: Vec<SyncRecord>,
    /// Highest `server_updated_at` of the page, kept as the next cursor.
    pub cursor: Option<String>,
}

/// Minimal Supabase REST client (PostgREST + GoTrue).
#[derive(Debug, Clone)]
pub struct SupabaseClient {
    base_url: String,
    anon_key: String,
    access_token: String,
}

impl SupabaseClient {
    /// Builds a client from a project URL and an authenticated session.
    #[must_use]
    pub fn new(base_url: &str, anon_key: &str, access_token: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            anon_key: anon_key.to_owned(),
            access_token: access_token.to_owned(),
        }
    }

    /// Exchanges an e-mail/password pair for a session (GoTrue password grant).
    pub fn sign_in(
        base_url: &str,
        anon_key: &str,
        email: &str,
        password: &str,
    ) -> Result<SignIn, SyncError> {
        let url = format!(
            "{}/auth/v1/token?grant_type=password",
            base_url.trim_end_matches('/')
        );
        let body = json!({"email": email, "password": password});
        let response = send_with_body(
            ureq::post(&url)
                .header("apikey", anon_key)
                .header("Content-Type", "application/json"),
            serde_json::to_vec(&body).map_err(|_| SyncError::server())?,
        )?;
        session_from_response(&response)
    }

    /// Renews an expired access token.
    pub fn refresh(
        base_url: &str,
        anon_key: &str,
        refresh_token: &str,
    ) -> Result<SyncSession, SyncError> {
        let url = format!(
            "{}/auth/v1/token?grant_type=refresh_token",
            base_url.trim_end_matches('/')
        );
        let body = json!({"refresh_token": refresh_token});
        let response = send_with_body(
            ureq::post(&url)
                .header("apikey", anon_key)
                .header("Content-Type", "application/json"),
            serde_json::to_vec(&body).map_err(|_| SyncError::server())?,
        )?;
        Ok(session_from_response(&response)?.session)
    }

    /// Pushes the public envelope (idempotent upsert).
    pub fn upsert_envelope(&self, user_id: &str, envelope: &SyncEnvelope) -> Result<(), SyncError> {
        let mut body = envelope.to_json();
        body["user_id"] = Value::String(user_id.to_owned());
        self.post(
            "/rest/v1/akooti_vault_envelopes?on_conflict=user_id",
            &serde_json::to_vec(&[body]).map_err(|_| SyncError::server())?,
        )?;
        Ok(())
    }

    /// Upserts every record, one idempotent request per batch.
    pub fn push(&self, user_id: &str, records: &[SyncRecord]) -> Result<usize, SyncError> {
        if records.is_empty() {
            return Ok(0);
        }
        let rows = records
            .iter()
            .map(|record| {
                json!({
                    "id": record.id,
                    "user_id": user_id,
                    "record_type": record.record_type.as_str(),
                    "ciphertext": record.ciphertext,
                    "nonce": record.nonce,
                    "client_updated_at": to_iso8601(record.client_updated_at),
                    "is_deleted": record.is_deleted,
                })
            })
            .collect::<Vec<_>>();
        for chunk in rows.chunks(100) {
            let body = serde_json::to_vec(chunk).map_err(|_| SyncError::server())?;
            self.post("/rest/v1/akooti_sync_records?on_conflict=user_id,id", &body)?;
        }
        Ok(records.len())
    }

    /// Reads one delta page after `cursor`.
    pub fn pull(&self, cursor: Option<&str>) -> Result<PullBatch, SyncError> {
        let mut path = format!(
            "/rest/v1/akooti_sync_records?select=id,record_type,ciphertext,nonce,\
             client_updated_at,server_updated_at,is_deleted\
             &order=server_updated_at.asc&limit={PAGE_SIZE}"
        );
        if let Some(cursor) = cursor {
            path.push_str("&server_updated_at=gt.");
            path.push_str(&encode_query_value(cursor));
        }
        let body = self.get(&path)?;
        parse_page(&body)
    }

    fn get(&self, path: &str) -> Result<String, SyncError> {
        let url = format!("{}{}", self.base_url, path);
        send_without_body(
            ureq::get(&url)
                .header("apikey", &self.anon_key)
                .header("Authorization", &format!("Bearer {}", self.access_token)),
        )
    }

    fn post(&self, path: &str, body: &[u8]) -> Result<String, SyncError> {
        let url = format!("{}{}", self.base_url, path);
        send_with_body(
            ureq::post(&url)
                .header("apikey", &self.anon_key)
                .header("Authorization", &format!("Bearer {}", self.access_token))
                .header("Content-Type", "application/json")
                .header("Prefer", "resolution=merge-duplicates,return=minimal"),
            body.to_vec(),
        )
    }
}

fn send_without_body(
    request: ureq::RequestBuilder<ureq::typestate::WithoutBody>,
) -> Result<String, SyncError> {
    let mut response = request
        .config()
        .timeout_global(Some(Duration::from_secs(TIMEOUT_SECONDS)))
        .build()
        .call()
        .map_err(map_error)?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|_| SyncError::server())
}

fn send_with_body(
    request: ureq::RequestBuilder<ureq::typestate::WithBody>,
    body: Vec<u8>,
) -> Result<String, SyncError> {
    let mut response = request
        .config()
        .timeout_global(Some(Duration::from_secs(TIMEOUT_SECONDS)))
        .build()
        .send(body)
        .map_err(map_error)?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|_| SyncError::server())
}

fn map_error(error: ureq::Error) -> SyncError {
    match error {
        ureq::Error::StatusCode(400 | 401 | 403) => SyncError::unauthorized(),
        // PostgREST answers 404 (PGRST205) when the table is not in the schema cache.
        ureq::Error::StatusCode(404) => SyncError::schema_missing(),
        ureq::Error::StatusCode(_) => SyncError::server(),
        _ => SyncError::network(),
    }
}

fn session_from_response(body: &str) -> Result<SignIn, SyncError> {
    let value: Value = serde_json::from_str(body).map_err(|_| SyncError::server())?;
    let access_token = value
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(SyncError::unauthorized)?;
    let refresh_token = value
        .get("refresh_token")
        .and_then(Value::as_str)
        .ok_or_else(SyncError::unauthorized)?;
    let expires_in = value
        .get("expires_in")
        .and_then(Value::as_i64)
        .unwrap_or(3_600);
    let user_id = value
        .get("user")
        .and_then(|user| user.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(SyncError::unauthorized)?;
    Ok(SignIn {
        session: SyncSession {
            access_token: access_token.to_owned(),
            refresh_token: refresh_token.to_owned(),
            expires_at: super::time::now_seconds() + expires_in,
        },
        user_id: user_id.to_owned(),
    })
}

fn parse_page(body: &str) -> Result<PullBatch, SyncError> {
    let rows: Vec<Value> = serde_json::from_str(body).map_err(|_| SyncError::server())?;
    let mut batch = PullBatch::default();
    for row in rows {
        let record_type = row
            .get("record_type")
            .and_then(Value::as_str)
            .and_then(SyncRecordType::parse)
            .ok_or_else(SyncError::server)?;
        let id = row
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(SyncError::server)?;
        let client_updated_at = row
            .get("client_updated_at")
            .and_then(Value::as_str)
            .and_then(from_iso8601)
            .ok_or_else(SyncError::server)?;
        let server_updated_at = row
            .get("server_updated_at")
            .and_then(Value::as_str)
            .ok_or_else(SyncError::server)?;
        batch.cursor = Some(server_updated_at.to_owned());
        batch.records.push(SyncRecord {
            id: id.to_owned(),
            record_type,
            client_updated_at,
            is_deleted: row
                .get("is_deleted")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            ciphertext: row
                .get("ciphertext")
                .and_then(Value::as_str)
                .ok_or_else(SyncError::server)?
                .to_owned(),
            nonce: row
                .get("nonce")
                .and_then(Value::as_str)
                .ok_or_else(SyncError::server)?
                .to_owned(),
        });
    }
    Ok(batch)
}

#[cfg(test)]
mod tests {
    use super::{parse_page, session_from_response};

    #[test]
    fn a_delta_page_becomes_records_with_the_last_cursor() {
        let body = r#"[
            {"id":"a","record_type":"folder","ciphertext":"AA==","nonce":"BB==",
             "client_updated_at":"2026-11-18T08:00:00+00:00",
             "server_updated_at":"2026-11-18T08:00:01.5+00:00","is_deleted":false},
            {"id":"b","record_type":"item","ciphertext":"CC==","nonce":"DD==",
             "client_updated_at":"2026-11-18T08:00:00+00:00",
             "server_updated_at":"2026-11-18T08:00:02+00:00","is_deleted":true}
        ]"#;
        let batch = parse_page(body).expect("page");
        assert_eq!(batch.records.len(), 2);
        assert!(batch.records[1].is_deleted);
        assert_eq!(batch.cursor.as_deref(), Some("2026-11-18T08:00:02+00:00"));
    }

    #[test]
    fn a_rejected_login_is_reported_as_unauthorized() {
        let error = session_from_response(r#"{"error":"invalid_grant"}"#).expect_err("error");
        assert_eq!(error.code, "unauthorized");
    }
}
