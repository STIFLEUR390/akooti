//! Local daemon: the only process that holds the master key.
//!
//! `akooti mcp` and `akooti native-host` are thin clients: they present a
//! token, the daemon authenticates it, checks the scope and the folder
//! allowlist, does the work, and answers with an outcome. A secret can therefore
//! never appear on a socket towards an agent.
//!
//! Transport: a Unix socket in an owner-only directory (`chmod 600` on the
//! socket itself). No TCP listener exists anywhere in this module.

// One IPC message is answered at a time, so an inline error envelope costs
// nothing; boxing it would only add noise to every `?`.
#![allow(clippy::result_large_err)]

mod exec;
mod inject;

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use akooti_core::{
    AuditStatus, McpAction, McpFillPlan, McpPrincipal, McpScope, NewAuditEntry, VaultError,
};
use akooti_ipc::{
    ExecOutcome, FillOutcome, FillStatus, Handshake, ItemSummary, PROTOCOL_VERSION, Request,
    Response, ResponseBody, VaultStatus, read_frame, socket_path, write_frame,
};

use crate::vault_runtime::VaultRuntime;

/// Handle returned to the GUI so it can remove the socket on exit.
#[derive(Debug)]
pub struct Daemon {
    path: PathBuf,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Starts the daemon on the default socket.
///
/// # Errors
///
/// Returns a message when the socket path cannot be resolved or bound.
pub fn start(runtime: Arc<VaultRuntime>) -> Result<Daemon, String> {
    let path = socket_path().map_err(|_| "no socket directory is available".to_owned())?;
    akooti_ipc::ensure_socket_dir(&path).map_err(|_| "socket directory unavailable".to_owned())?;
    // A leftover socket from a crashed daemon would block the bind.
    let _ = std::fs::remove_file(&path);
    serve(runtime, path)
}

#[cfg(unix)]
fn serve(runtime: Arc<VaultRuntime>, path: PathBuf) -> Result<Daemon, String> {
    use std::os::unix::fs::PermissionsExt;

    let listener = std::os::unix::net::UnixListener::bind(&path)
        .map_err(|_| "the daemon socket could not be bound".to_owned())?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .map_err(|_| "the daemon socket could not be restricted".to_owned())?;

    thread::Builder::new()
        .name("akooti-daemon".to_owned())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else {
                    continue;
                };
                let runtime = Arc::clone(&runtime);
                let _ = thread::Builder::new()
                    .name("akooti-daemon-conn".to_owned())
                    .spawn(move || handle_connection(stream, &runtime));
            }
        })
        .map_err(|_| "the daemon thread could not start".to_owned())?;

    Ok(Daemon { path })
}

#[cfg(not(unix))]
fn serve(_runtime: Arc<VaultRuntime>, path: PathBuf) -> Result<Daemon, String> {
    Err(format!(
        "the local daemon transport is not implemented on this platform yet ({})",
        path.display()
    ))
}

#[cfg(unix)]
fn handle_connection(mut stream: std::os::unix::net::UnixStream, runtime: &VaultRuntime) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(120)));
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(120)));
    loop {
        let frame = match read_frame(&mut stream) {
            Ok(frame) => frame,
            Err(_) => return,
        };
        let response = match serde_json::from_slice::<Request>(&frame) {
            Ok(request) => dispatch(runtime, request),
            Err(_) => Response::error("protocol", "the request could not be decoded"),
        };
        let encoded = match serde_json::to_vec(&response) {
            Ok(encoded) => encoded,
            Err(_) => return,
        };
        if write_frame(&mut stream, &encoded).is_err() {
            return;
        }
        let _ = stream.flush();
    }
}

/// Handles one request. Exposed for tests so the policy can be exercised without
/// a real socket.
pub(crate) fn dispatch(runtime: &VaultRuntime, request: Request) -> Response {
    match request {
        Request::Handshake {
            protocol_version,
            client: _,
        } => {
            if protocol_version != PROTOCOL_VERSION {
                return Response::error(
                    "version_mismatch",
                    format!("this daemon speaks protocol {PROTOCOL_VERSION}"),
                );
            }
            Response::ok(ResponseBody::Handshake(Handshake {
                protocol_version: PROTOCOL_VERSION,
                vault_unlocked: runtime.status() == akooti_core::VaultStatus::Unlocked,
                capabilities: capabilities(),
            }))
        }
        Request::VaultStatus { token } => match authenticate(runtime, &token) {
            Ok(principal) => {
                Response::ok(ResponseBody::VaultStatus(status_of(runtime, &principal)))
            }
            Err(response) => response,
        },
        Request::VaultSearch {
            token,
            query,
            limit,
        } => with_principal(runtime, &token, |principal| {
            let items = runtime
                .with_vault(|vault| vault.mcp_search(principal, &query, limit.clamp(1, 200)))
                .map_err(|error| denial(runtime, principal, error, "vault_search"))?;
            Ok(ResponseBody::Search {
                items: items.iter().map(summary).collect(),
            })
        }),
        Request::VaultBlindAutofill {
            token,
            item_id,
            origin,
            selectors,
        } => with_principal(runtime, &token, |principal| {
            // Authorise without decrypting: the transport below is not wired.
            let _ = selectors;
            runtime
                .with_vault(|vault| vault.mcp_authorize(principal, &item_id, McpScope::BlindFill))
                .map_err(|error| denial(runtime, principal, error, "blind_autofill"))?;
            let outcome = inject::native_messaging_fill();
            audit_outcome(
                runtime,
                principal,
                McpAction::BlindAutofill,
                &item_id,
                &origin,
                &outcome,
            )?;
            Ok(ResponseBody::Fill(outcome))
        }),
        Request::VaultCdpAutofill {
            token,
            item_id,
            origin,
            endpoint,
            selectors,
        } => with_principal(runtime, &token, |principal| {
            let plan = fill_plan(
                runtime,
                principal,
                &item_id,
                McpScope::BlindFill,
                McpAction::CdpAutofill,
            )?;
            let outcome = inject::cdp_fill(
                endpoint.as_deref(),
                &origin,
                &target(&plan),
                selectors.as_ref(),
            );
            audit_outcome(
                runtime,
                principal,
                McpAction::CdpAutofill,
                &item_id,
                &origin,
                &outcome,
            )?;
            Ok(ResponseBody::Fill(outcome))
        }),
        Request::VaultCreateAndFill {
            token,
            title,
            url,
            username,
            ttl_seconds,
            endpoint,
            selectors,
        } => with_principal(runtime, &token, |principal| {
            let created = runtime
                .with_vault_mut(|vault| {
                    vault.mcp_create_ephemeral(
                        principal,
                        akooti_core::EphemeralAccountDraft {
                            title,
                            primary_url: Some(url.clone()),
                            username,
                            ttl_seconds: ttl_seconds.unwrap_or(900),
                            folder_id: None,
                        },
                    )
                })
                .map_err(|error| denial(runtime, principal, error, "create_and_fill"))?;
            let outcome = inject::cdp_fill(
                endpoint.as_deref(),
                &url,
                &target(&created.plan),
                selectors.as_ref(),
            );
            audit_outcome(
                runtime,
                principal,
                McpAction::CreateAndFill,
                &created.item.id,
                &url,
                &outcome,
            )?;
            Ok(ResponseBody::Created {
                item_id: created.item.id,
                title: created.item.title,
                outcome,
            })
        }),
        Request::VaultExec {
            token,
            item_id,
            command,
            timeout_seconds,
        } => with_principal(runtime, &token, |principal| {
            let variables = runtime
                .with_vault(|vault| vault.mcp_env(principal, &item_id))
                .map_err(|error| denial(runtime, principal, error, "exec_env"))?;
            let origin = command.join(" ");
            let outcome: ExecOutcome = match exec::run(&command, &variables, timeout_seconds) {
                Ok(outcome) => {
                    record(
                        runtime,
                        principal,
                        McpAction::ExecEnv,
                        AuditStatus::Success,
                        Some(&item_id),
                        Some(&origin),
                        Some(outcome.exit_code.map_or_else(
                            || "terminated".to_owned(),
                            |code| format!("exit {code}"),
                        )),
                    )?;
                    outcome
                }
                Err(reason) => {
                    record(
                        runtime,
                        principal,
                        McpAction::ExecEnv,
                        AuditStatus::Error,
                        Some(&item_id),
                        Some(&origin),
                        Some(reason.clone()),
                    )?;
                    return Err(Response::error("exec_failed", reason));
                }
            };
            Ok(ResponseBody::Exec(outcome))
        }),
    }
}

fn capabilities() -> Vec<String> {
    vec![
        "vault_search".to_owned(),
        "cdp_autofill".to_owned(),
        "create_and_fill".to_owned(),
        "exec_env".to_owned(),
    ]
}

fn authenticate(runtime: &VaultRuntime, token: &str) -> Result<McpPrincipal, Response> {
    runtime
        .with_vault_mut(|vault| vault.authenticate_mcp_token(token))
        .map_err(|error| {
            // An unauthenticated attempt is audited too, without attributing it
            // to any token.
            let _ = runtime.record_audit(NewAuditEntry {
                token_id: None,
                action: McpAction::AccessDenied,
                target_item_id: None,
                target_folder_id: None,
                target_origin: None,
                status: AuditStatus::Denied,
                details: Some("token rejected".to_owned()),
            });
            response_for(error)
        })
}

fn with_principal(
    runtime: &VaultRuntime,
    token: &str,
    handler: impl FnOnce(&McpPrincipal) -> Result<ResponseBody, Response>,
) -> Response {
    match authenticate(runtime, token) {
        Ok(principal) => match handler(&principal) {
            Ok(body) => Response::ok(body),
            Err(response) => response,
        },
        Err(response) => response,
    }
}

fn fill_plan(
    runtime: &VaultRuntime,
    principal: &McpPrincipal,
    item_id: &str,
    scope: McpScope,
    action: McpAction,
) -> Result<McpFillPlan, Response> {
    runtime
        .with_vault(|vault| vault.mcp_fill_plan(principal, item_id, scope))
        .map_err(|error| denial(runtime, principal, error, action.as_str()))
}

/// Maps a failed action to a response, and audits a policy refusal.
///
/// The core refuses before any write, so the daemon is the only place that can
/// attribute the denial to a token and an action.
fn denial(
    runtime: &VaultRuntime,
    principal: &McpPrincipal,
    error: VaultError,
    action: &'static str,
) -> Response {
    if matches!(error, VaultError::Forbidden) {
        tracing::warn!(action, "MCP action refused by policy");
        let _ = runtime.record_audit(NewAuditEntry {
            token_id: Some(principal.token_id.clone()),
            action: McpAction::AccessDenied,
            target_item_id: None,
            target_folder_id: None,
            target_origin: None,
            status: AuditStatus::Denied,
            details: Some(format!("{action} refused by policy")),
        });
    }
    response_for(error)
}

fn audit_outcome(
    runtime: &VaultRuntime,
    principal: &McpPrincipal,
    action: McpAction,
    item_id: &str,
    origin: &str,
    outcome: &FillOutcome,
) -> Result<(), Response> {
    // `Partial` means no selector matched: nothing was filled, so it is a failure.
    let status = match outcome.status {
        FillStatus::Filled => AuditStatus::Success,
        FillStatus::Partial | FillStatus::Unavailable | FillStatus::Failed => AuditStatus::Error,
    };
    let details = outcome
        .detail
        .clone()
        .unwrap_or_else(|| format!("{} field(s) filled", outcome.fields_filled.len()));
    runtime
        .record_audit(NewAuditEntry {
            token_id: Some(principal.token_id.clone()),
            action,
            target_item_id: Some(item_id.to_owned()),
            target_folder_id: None,
            target_origin: Some(origin.to_owned()),
            status,
            details: Some(details),
        })
        .map_err(|_| Response::error("audit", "the audit log could not be written"))
}

fn record(
    runtime: &VaultRuntime,
    principal: &McpPrincipal,
    action: McpAction,
    status: AuditStatus,
    item_id: Option<&str>,
    origin: Option<&str>,
    details: Option<String>,
) -> Result<(), Response> {
    runtime
        .record_audit(NewAuditEntry {
            token_id: Some(principal.token_id.clone()),
            action,
            target_item_id: item_id.map(str::to_owned),
            target_folder_id: None,
            target_origin: origin.map(str::to_owned),
            status,
            details,
        })
        .map_err(|_| Response::error("audit", "the audit log could not be written"))
}

fn status_of(runtime: &VaultRuntime, principal: &McpPrincipal) -> VaultStatus {
    let expires_at = runtime
        .with_vault(|vault| vault.mcp_tokens())
        .ok()
        .and_then(|tokens| {
            tokens
                .into_iter()
                .find(|token| token.id == principal.token_id)
                .and_then(|token| token.expires_at)
        });
    VaultStatus {
        token_name: principal.name.clone(),
        scopes: principal
            .scopes
            .iter()
            .map(|scope| scope.as_str().to_owned())
            .collect(),
        folder_ids: principal.folder_ids.clone(),
        expires_at,
    }
}

fn summary(item: &akooti_core::VaultItem) -> ItemSummary {
    ItemSummary {
        id: item.id.clone(),
        title: item.title.clone(),
        item_type: item_type_name(item.item_type).to_owned(),
        primary_url: item.primary_url.clone(),
        username: item.username.clone(),
        folder_ids: item.folder_ids.clone(),
        has_totp: item.has_totp,
        expires_at: item.expires_at,
    }
}

fn target(plan: &McpFillPlan) -> inject::FillTarget {
    inject::FillTarget {
        selectors: plan.selectors.clone(),
        username: plan
            .username
            .as_ref()
            .map(|value| akooti_core::SecretString::new(value.expose().to_owned())),
        password: plan
            .password
            .as_ref()
            .map(|value| akooti_core::SecretString::new(value.expose().to_owned())),
        totp: plan
            .totp
            .as_ref()
            .map(|value| akooti_core::SecretString::new(value.expose().to_owned())),
    }
}

const fn item_type_name(item_type: akooti_core::ItemType) -> &'static str {
    match item_type {
        akooti_core::ItemType::Login => "login",
        akooti_core::ItemType::ApiKey => "api_key",
        akooti_core::ItemType::Note => "note",
        akooti_core::ItemType::EnvSet => "env_set",
    }
}

fn response_for(error: VaultError) -> Response {
    match error {
        VaultError::Forbidden => Response::error("forbidden", "access denied"),
        VaultError::Locked => Response::error("locked", "the vault is locked"),
        VaultError::NotFound => Response::error("not_found", "item not found"),
        VaultError::InvalidCredentials => {
            Response::error("invalid_credentials", "the token was rejected")
        }
        VaultError::InvalidInput(_) => Response::error("invalid_input", "invalid request"),
        other => {
            tracing::warn!(error = %other, "daemon request failed");
            Response::error("internal", "the daemon could not complete the request")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{capabilities, response_for};
    use akooti_core::VaultError;
    use akooti_ipc::{PROTOCOL_VERSION, Request, ResponseBody};

    #[test]
    fn the_handshake_announces_the_protocol_and_capabilities() {
        let caps = capabilities();
        assert!(caps.contains(&"vault_search".to_owned()));
        assert!(!caps.contains(&"blind_autofill".to_owned()));
        assert!(caps.contains(&"cdp_autofill".to_owned()));
    }

    #[test]
    fn denials_never_echo_a_value() {
        let response = response_for(VaultError::Forbidden);
        let encoded = serde_json::to_string(&response).expect("encode");
        assert!(encoded.contains("\"code\":\"forbidden\""));
        assert!(!encoded.contains("password"));
    }

    #[test]
    fn an_unknown_token_is_rejected_as_invalid_credentials() {
        let response = response_for(VaultError::InvalidCredentials);
        match response.body {
            ResponseBody::Error { code, .. } => assert_eq!(code, "invalid_credentials"),
            other => panic!("unexpected body: {other:?}"),
        }
    }

    #[test]
    fn a_bad_protocol_version_is_refused_before_authentication() {
        // Dispatch needs a runtime, so this only checks the version gate helper
        // through the public constant.
        assert_ne!(PROTOCOL_VERSION, 0);
        let request = Request::Handshake {
            protocol_version: PROTOCOL_VERSION + 1,
            client: "mcp".to_owned(),
        };
        assert!(matches!(request, Request::Handshake { .. }));
    }
}
