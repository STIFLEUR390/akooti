//! Immutable audit trail for every AI-driven action.
//!
//! `details` never carries a secret, a selector value or a decrypted field: it
//! is free-form diagnostic text written by the daemon, e.g. the number of
//! matched items. The Zero-Leak tests scan this table for secret patterns.

use rusqlite::{Connection, params};

use crate::error::VaultError;

/// Action recorded in `mcp_audit_logs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpAction {
    /// Metadata-only search.
    VaultSearch,
    /// Selector-based fill through the extension bridge.
    BlindAutofill,
    /// Selector-based fill through a CDP endpoint.
    CdpAutofill,
    /// Ephemeral account creation plus signup fill.
    CreateAndFill,
    /// Secret injection into a child process environment.
    ExecEnv,
    /// A WebAuthn ceremony served from a stored credential.
    PasskeyUse,
    /// A request refused before touching the vault.
    AccessDenied,
}

impl McpAction {
    /// Wire name stored in the database.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VaultSearch => "vault_search",
            Self::BlindAutofill => "blind_autofill",
            Self::CdpAutofill => "cdp_autofill",
            Self::CreateAndFill => "create_and_fill",
            Self::ExecEnv => "exec_env",
            Self::PasskeyUse => "passkey_use",
            Self::AccessDenied => "access_denied",
        }
    }
}

/// Outcome of an audited action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditStatus {
    /// The action completed.
    Success,
    /// The token was not allowed (scope, folder, expiry, revocation).
    Denied,
    /// The action was allowed but failed.
    Error,
}

impl AuditStatus {
    /// Wire name stored in the database.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Denied => "denied",
            Self::Error => "error",
        }
    }
}

/// One audit row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpAuditEntry {
    /// Identifier.
    pub id: String,
    /// Token that performed the action, when authentication succeeded.
    pub token_id: Option<String>,
    /// Action performed.
    pub action: McpAction,
    /// Item targeted, when relevant.
    pub target_item_id: Option<String>,
    /// Folder targeted, when relevant.
    pub target_folder_id: Option<String>,
    /// Origin or command, when relevant.
    pub target_origin: Option<String>,
    /// Outcome.
    pub status: AuditStatus,
    /// Non-revealing diagnostic text.
    pub details: Option<String>,
    /// Unix seconds.
    pub created_at: i64,
}

/// Payload used to append an audit row.
#[derive(Debug, Clone)]
pub struct NewAuditEntry {
    /// Token performing the action.
    pub token_id: Option<String>,
    /// Action performed.
    pub action: McpAction,
    /// Item targeted.
    pub target_item_id: Option<String>,
    /// Folder targeted.
    pub target_folder_id: Option<String>,
    /// Origin or command.
    pub target_origin: Option<String>,
    /// Outcome.
    pub status: AuditStatus,
    /// Non-revealing diagnostic text.
    pub details: Option<String>,
}

pub(crate) fn insert(
    connection: &Connection,
    entry: &NewAuditEntry,
    now: i64,
) -> Result<(), VaultError> {
    connection.execute(
        "INSERT INTO mcp_audit_logs (
           id, token_id, action, target_item_id, target_folder_id,
           target_origin, status, details, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            uuid::Uuid::new_v4().to_string(),
            entry.token_id,
            entry.action.as_str(),
            entry.target_item_id,
            entry.target_folder_id,
            entry.target_origin,
            entry.status.as_str(),
            entry.details,
            now,
        ],
    )?;
    Ok(())
}

pub(crate) fn list(
    connection: &Connection,
    limit: usize,
) -> Result<Vec<McpAuditEntry>, VaultError> {
    let mut statement = connection.prepare(
        "SELECT id, token_id, action, target_item_id, target_folder_id,
                target_origin, status, details, created_at
         FROM mcp_audit_logs
         ORDER BY created_at DESC, rowid DESC
         LIMIT ?1",
    )?;
    let rows = statement.query_map([i64::try_from(limit).unwrap_or(i64::MAX)], |row| {
        Ok(McpAuditEntry {
            id: row.get(0)?,
            token_id: row.get(1)?,
            action: parse_action(&row.get::<_, String>(2)?),
            target_item_id: row.get(3)?,
            target_folder_id: row.get(4)?,
            target_origin: row.get(5)?,
            status: parse_status(&row.get::<_, String>(6)?),
            details: row.get(7)?,
            created_at: row.get(8)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn parse_action(value: &str) -> McpAction {
    match value {
        "blind_autofill" => McpAction::BlindAutofill,
        "cdp_autofill" => McpAction::CdpAutofill,
        "create_and_fill" => McpAction::CreateAndFill,
        "exec_env" => McpAction::ExecEnv,
        "passkey_use" => McpAction::PasskeyUse,
        "access_denied" => McpAction::AccessDenied,
        _ => McpAction::VaultSearch,
    }
}

fn parse_status(value: &str) -> AuditStatus {
    match value {
        "denied" => AuditStatus::Denied,
        "error" => AuditStatus::Error,
        _ => AuditStatus::Success,
    }
}
