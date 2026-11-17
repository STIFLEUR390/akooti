//! MCP token management and audit trail commands.
//!
//! Two invariants drive this module (`docs/regles.md` §2):
//!
//! * a secret token is returned **once**, by [`create_mcp_token`], and never
//!   again — the listing only ever carries the display prefix;
//! * every AI-driven action is visible: the audit page reads the rows written by
//!   the daemon, and a row never contains a value.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use akooti_core::{
    AuditStatus, IssuedMcpToken, McpAction, McpAuditEntry, McpScope, McpToken, NewMcpToken,
};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::CommandError;
use crate::vault_runtime::VaultRuntime;

/// Upper bound accepted for a custom TTL, in days.
const MAX_TTL_DAYS: i64 = 365;

/// One token as shown in the settings screen. Never carries the secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTokenDto {
    /// Identifier, needed to revoke.
    pub id: String,
    /// Human label.
    pub name: String,
    /// Display prefix (`ak_mcp_8f9a`).
    pub token_prefix: String,
    /// Granted scopes.
    pub scopes: Vec<String>,
    /// Allowed folders.
    pub folder_ids: Vec<String>,
    /// Folder receiving agent-created items.
    pub default_create_folder_id: Option<String>,
    /// Expiry in Unix seconds.
    pub expires_at: Option<i64>,
    /// Revocation time in Unix seconds.
    pub revoked_at: Option<i64>,
    /// Last successful authentication.
    pub last_used_at: Option<i64>,
    /// Creation time in Unix seconds.
    pub created_at: i64,
    /// Whether the token can still authenticate right now.
    pub active: bool,
}

impl McpTokenDto {
    fn from_token(token: McpToken, now: i64) -> Self {
        let active = token.is_active(now);
        Self {
            scopes: token
                .scopes
                .iter()
                .map(|scope| scope.as_str().to_owned())
                .collect(),
            folder_ids: token.folder_ids,
            default_create_folder_id: token.default_create_folder_id,
            expires_at: token.expires_at,
            revoked_at: token.revoked_at,
            last_used_at: token.last_used_at,
            created_at: token.created_at,
            id: token.id,
            name: token.name,
            token_prefix: token.token_prefix,
            active,
        }
    }
}

/// The one and only time a secret is returned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuedMcpTokenDto {
    /// Token metadata.
    pub token: McpTokenDto,
    /// Plaintext secret; the UI shows it once, then drops it.
    pub secret: String,
}

/// Creation payload sent by the UI.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTokenDraftDto {
    /// Human label.
    pub name: String,
    /// Requested scopes (`read_metadata`, `blind_fill`, …).
    pub scopes: Vec<String>,
    /// Folders the token may reach.
    pub folder_ids: Vec<String>,
    /// Folder receiving agent-created items.
    #[serde(default)]
    pub default_create_folder_id: Option<String>,
    /// Lifetime in days; `0` or absent means "no expiry".
    #[serde(default)]
    pub ttl_days: Option<i64>,
}

/// One audit row shown in the activity screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAuditEntryDto {
    /// Identifier.
    pub id: String,
    /// Token that acted, when authentication succeeded.
    pub token_id: Option<String>,
    /// Action name.
    pub action: String,
    /// Item targeted.
    pub target_item_id: Option<String>,
    /// Folder targeted.
    pub target_folder_id: Option<String>,
    /// Origin or command.
    pub target_origin: Option<String>,
    /// `success`, `denied` or `error`.
    pub status: String,
    /// Non-revealing diagnostic.
    pub details: Option<String>,
    /// Unix seconds.
    pub created_at: i64,
}

impl From<McpAuditEntry> for McpAuditEntryDto {
    fn from(entry: McpAuditEntry) -> Self {
        Self {
            id: entry.id,
            token_id: entry.token_id,
            action: action_name(entry.action).to_owned(),
            target_item_id: entry.target_item_id,
            target_folder_id: entry.target_folder_id,
            target_origin: entry.target_origin,
            status: status_name(entry.status).to_owned(),
            details: entry.details,
            created_at: entry.created_at,
        }
    }
}

/// State of the local daemon, for the settings screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonStatusDto {
    /// Whether the socket exists and is reachable right now.
    pub running: bool,
    /// Socket path, or `null` when no runtime directory exists.
    pub socket_path: Option<String>,
    /// Capabilities the daemon announced.
    pub capabilities: Vec<String>,
}

/// Lists every token; a revoked one stays visible for the audit trail.
///
/// # Errors
///
/// Returns `locked` when the vault is closed.
#[tauri::command]
pub fn list_mcp_tokens(
    runtime: State<'_, Arc<VaultRuntime>>,
) -> Result<Vec<McpTokenDto>, CommandError> {
    let now = unix_now();
    runtime
        .with_vault(|vault| vault.mcp_tokens())
        .map_err(CommandError::from)
        .map(|tokens| {
            tokens
                .into_iter()
                .map(|token| McpTokenDto::from_token(token, now))
                .collect()
        })
}

/// Issues a token. The secret is returned exactly once.
///
/// # Errors
///
/// Returns `invalid_input` for an empty name or scope list, `not_found` for an
/// unknown folder, and `locked` when the vault is closed.
#[tauri::command]
pub fn create_mcp_token(
    runtime: State<'_, Arc<VaultRuntime>>,
    draft: McpTokenDraftDto,
) -> Result<IssuedMcpTokenDto, CommandError> {
    let scopes = draft
        .scopes
        .iter()
        .map(|name| {
            McpScope::parse(name)
                .ok_or_else(|| CommandError::new("invalid_input", "Portée inconnue."))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let expires_at = draft
        .ttl_days
        .filter(|days| *days > 0)
        .map(|days| unix_now() + days.clamp(1, MAX_TTL_DAYS) * 86_400);
    let token = NewMcpToken {
        name: draft.name,
        scopes,
        folder_ids: draft.folder_ids,
        default_create_folder_id: draft.default_create_folder_id,
        expires_at,
    };
    let now = unix_now();
    runtime
        .with_vault_mut(|vault| vault.create_mcp_token(token))
        .map_err(CommandError::from)
        .map(|issued| issued_dto(issued, now))
}

/// Revokes a token immediately; the corresponding row stays for the audit.
///
/// # Errors
///
/// Returns `not_found` for an unknown or already revoked identifier.
#[tauri::command]
pub fn revoke_mcp_token(
    runtime: State<'_, Arc<VaultRuntime>>,
    id: String,
) -> Result<McpTokenDto, CommandError> {
    let now = unix_now();
    runtime
        .with_vault_mut(|vault| vault.revoke_mcp_token(&id))
        .map_err(CommandError::from)
        .map(|token| McpTokenDto::from_token(token, now))
}

/// Deletes a token and its folder allowlist. Audit rows are kept.
///
/// # Errors
///
/// Returns `not_found` for an unknown identifier.
#[tauri::command]
pub fn delete_mcp_token(
    runtime: State<'_, Arc<VaultRuntime>>,
    id: String,
) -> Result<(), CommandError> {
    runtime
        .with_vault_mut(|vault| vault.delete_mcp_token(&id))
        .map_err(CommandError::from)
}

/// Reads the newest audit rows.
///
/// # Errors
///
/// Returns `locked` when the vault is closed.
#[tauri::command]
pub fn list_mcp_audit_log(
    runtime: State<'_, Arc<VaultRuntime>>,
    limit: Option<usize>,
) -> Result<Vec<McpAuditEntryDto>, CommandError> {
    runtime
        .with_vault(|vault| vault.mcp_audit_logs(limit.unwrap_or(50).clamp(1, 500)))
        .map_err(CommandError::from)
        .map(|entries| entries.into_iter().map(McpAuditEntryDto::from).collect())
}

/// Reports whether the local daemon answers, without needing an unlocked vault.
#[tauri::command]
pub fn daemon_status() -> DaemonStatusDto {
    let path = akooti_ipc::socket_path().ok();
    let running = path.as_deref().is_some_and(|path| path.exists());
    DaemonStatusDto {
        running,
        socket_path: path.map(|path| path.display().to_string()),
        capabilities: if running {
            vec![
                "vault_search".to_owned(),
                "cdp_autofill".to_owned(),
                "create_and_fill".to_owned(),
                "exec_env".to_owned(),
            ]
        } else {
            Vec::new()
        },
    }
}

fn issued_dto(issued: IssuedMcpToken, now: i64) -> IssuedMcpTokenDto {
    IssuedMcpTokenDto {
        token: McpTokenDto::from_token(issued.token, now),
        secret: issued.secret.to_string(),
    }
}

const fn action_name(action: McpAction) -> &'static str {
    action.as_str()
}

const fn status_name(status: AuditStatus) -> &'static str {
    status.as_str()
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::{McpAuditEntryDto, McpTokenDraftDto, unix_now};
    use akooti_core::{AuditStatus, McpAction, McpAuditEntry};

    #[test]
    fn an_audit_row_serialises_its_wire_names() {
        let dto = McpAuditEntryDto::from(McpAuditEntry {
            id: "a".to_owned(),
            token_id: Some("t".to_owned()),
            action: McpAction::CdpAutofill,
            target_item_id: None,
            target_folder_id: None,
            target_origin: Some("http://localhost:3000".to_owned()),
            status: AuditStatus::Denied,
            details: None,
            created_at: 1,
        });
        assert_eq!(dto.action, "cdp_autofill");
        assert_eq!(dto.status, "denied");
        let encoded = serde_json::to_string(&dto).expect("serialise");
        assert!(encoded.contains("\"action\":\"cdp_autofill\""));
        assert!(encoded.contains("\"createdAt\":1"));
    }

    #[test]
    fn a_draft_decodes_camel_case_defaults() {
        let draft: McpTokenDraftDto = serde_json::from_str(
            r#"{"name":"Agent","scopes":["read_metadata"],"folderIds":["f"]}"#,
        )
        .expect("decode");
        assert_eq!(draft.name, "Agent");
        assert!(draft.ttl_days.is_none());
        assert!(draft.default_create_folder_id.is_none());
    }

    #[test]
    fn the_clock_is_plausible() {
        assert!(unix_now() > 1_700_000_000);
    }
}
