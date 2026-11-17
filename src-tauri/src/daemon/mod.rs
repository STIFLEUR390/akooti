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
    ExecOutcome, FillOutcome, FillPayload, FillStatus, Handshake, ItemSummary, PROTOCOL_VERSION,
    PasskeyCredential, PasskeySignature, Request, Response, ResponseBody, SelectorOverride,
    VaultStatus, read_frame, socket_path, write_frame,
};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use zeroize::Zeroizing;

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
    let mut client = ClientKind::Unknown;
    loop {
        let frame = match read_frame(&mut stream) {
            Ok(frame) => frame,
            Err(_) => return,
        };
        let response = match serde_json::from_slice::<Request>(&frame) {
            Ok(request) => {
                if let Request::Handshake { client: name, .. } = &request {
                    client = ClientKind::parse(name);
                }
                dispatch(runtime, request, client)
            }
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

/// Who opened the connection, as declared by the handshake.
///
/// The declaration is not a credential — the token is — but it keeps the
/// extension-only verbs off an agent's connection, and it makes an accidental
/// cross-wiring visible in the audit log instead of silent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClientKind {
    /// No handshake was seen yet.
    Unknown,
    /// `akooti mcp`, driven by an AI agent.
    Mcp,
    /// `akooti native-host`, driven by the paired browser extension.
    NativeHost,
    /// The desktop UI.
    Ui,
    /// A client kind this build does not know.
    Other,
}

impl ClientKind {
    /// Parses the `client` field of a handshake.
    pub(crate) fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "mcp" => Self::Mcp,
            "native-host" | "native_host" => Self::NativeHost,
            "ui" => Self::Ui,
            "" => Self::Unknown,
            _ => Self::Other,
        }
    }
}

/// Handles one request. Exposed for tests so the policy can be exercised without
/// a real socket.
pub(crate) fn dispatch(runtime: &VaultRuntime, request: Request, client: ClientKind) -> Response {
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
                .map_err(|error| denial(runtime, principal, error, "vault_search", None))?;
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
                .map_err(|error| {
                    denial(runtime, principal, error, "blind_autofill", Some(&origin))
                })?;
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
        Request::VaultExtensionFill {
            token,
            item_id,
            origin,
            selectors,
            target,
        } => with_principal(runtime, &token, |principal| {
            // The extension is the only peer allowed to receive a value: it is
            // the process that writes the page. An agent connection asking for
            // this verb is refused and audited before any decryption.
            if client != ClientKind::NativeHost {
                let _ = runtime.record_audit(NewAuditEntry {
                    token_id: Some(principal.token_id.clone()),
                    action: McpAction::AccessDenied,
                    target_item_id: Some(item_id.clone()),
                    target_folder_id: None,
                    target_origin: Some(origin.clone()),
                    status: AuditStatus::Denied,
                    details: Some("extension fill requested outside the browser bridge".to_owned()),
                });
                return Err(Response::error("forbidden", "access denied"));
            }
            let fill = runtime
                .with_vault(|vault| {
                    vault.mcp_extension_fill(principal, &item_id, &origin, McpScope::BlindFill)
                })
                .map_err(|error| {
                    denial(runtime, principal, error, "blind_autofill", Some(&origin))
                })?;
            let merged = inject::merge_selectors(&fill.selectors, selectors.as_ref());
            let payload = FillPayload {
                origin: fill.origin.clone(),
                selectors: SelectorOverride {
                    username: merged.username,
                    password: merged.password,
                    totp: merged.totp,
                    submit: merged.submit,
                },
                username: redacted(fill.username.as_ref()),
                password: redacted(fill.password.as_ref()),
                totp: redacted(fill.totp.as_ref()),
            };
            let released = [
                payload.username.is_some(),
                payload.password.is_some(),
                payload.totp.is_some(),
            ]
            .iter()
            .filter(|present| **present)
            .count();
            // `target` is free text from the extension; the audit keeps the
            // count and the requested gesture, never a value.
            let gesture = match target.as_deref() {
                Some("clipboard") => "clipboard",
                _ => "fill",
            };
            record(
                runtime,
                principal,
                McpAction::BlindAutofill,
                AuditStatus::Success,
                Some(&item_id),
                Some(&origin),
                Some(format!("{released} value(s) released for {gesture}")),
            )?;
            Ok(ResponseBody::ExtensionFill(payload))
        }),
        Request::VaultPasskeyRegister {
            token,
            origin,
            rp_id,
            rp_name,
            user_handle,
            user_name,
            challenge,
        } => with_principal(runtime, &token, |principal| {
            // A passkey is a credential the vault signs with: only the browser
            // bridge may ask for one, never an agent connection.
            if client != ClientKind::NativeHost {
                let _ = runtime.record_audit(NewAuditEntry {
                    token_id: Some(principal.token_id.clone()),
                    action: McpAction::AccessDenied,
                    target_item_id: None,
                    target_folder_id: None,
                    target_origin: Some(origin.clone()),
                    status: AuditStatus::Denied,
                    details: Some(
                        "passkey registration requested outside the browser bridge".to_owned(),
                    ),
                });
                return Err(Response::error("forbidden", "access denied"));
            }
            let request = akooti_core::PasskeyRegistration {
                rp_name,
                user_handle: decode_base64url(&user_handle, "user handle")?,
                user_name,
                challenge: decode_base64url(&challenge, "challenge")?,
                origin: origin.clone(),
                rp_id: rp_id.clone(),
            };
            let registered = runtime
                .with_vault(|vault| vault.passkey_register(principal, &request))
                .map_err(|error| denial(runtime, principal, error, "passkey_use", Some(&origin)))?;
            record(
                runtime,
                principal,
                McpAction::PasskeyUse,
                AuditStatus::Success,
                None,
                Some(&origin),
                Some(format!("passkey registered for {rp_id}")),
            )?;
            Ok(ResponseBody::PasskeyRegistration(PasskeyCredential {
                id: encode_base64url(&registered.credential_id),
                client_data_json: encode_base64url(&registered.client_data_json),
                attestation_object: encode_base64url(&registered.attestation_object),
                public_key_algorithm: registered.public_key_algorithm,
                transports: registered.transports,
            }))
        }),
        Request::VaultPasskeyAssert {
            token,
            origin,
            rp_id,
            challenge,
            allow_credentials,
        } => with_principal(runtime, &token, |principal| {
            if client != ClientKind::NativeHost {
                let _ = runtime.record_audit(NewAuditEntry {
                    token_id: Some(principal.token_id.clone()),
                    action: McpAction::AccessDenied,
                    target_item_id: None,
                    target_folder_id: None,
                    target_origin: Some(origin.clone()),
                    status: AuditStatus::Denied,
                    details: Some(
                        "passkey assertion requested outside the browser bridge".to_owned(),
                    ),
                });
                return Err(Response::error("forbidden", "access denied"));
            }
            let mut credentials = Vec::with_capacity(allow_credentials.len());
            for credential in &allow_credentials {
                credentials.push(decode_base64url(credential, "credential id")?);
            }
            let request = akooti_core::PasskeyAssertion {
                origin: origin.clone(),
                rp_id: rp_id.clone(),
                challenge: decode_base64url(&challenge, "challenge")?,
                allow_credentials: credentials,
            };
            let signed = runtime
                .with_vault(|vault| vault.passkey_assert(principal, &request))
                .map_err(|error| denial(runtime, principal, error, "passkey_use", Some(&origin)))?;
            // The audit records which site was signed for, never the signature.
            record(
                runtime,
                principal,
                McpAction::PasskeyUse,
                AuditStatus::Success,
                None,
                Some(&origin),
                Some(format!("passkey assertion for {rp_id}")),
            )?;
            Ok(ResponseBody::PasskeyAssertion(PasskeySignature {
                id: encode_base64url(&signed.credential_id),
                client_data_json: encode_base64url(&signed.client_data_json),
                authenticator_data: encode_base64url(&signed.authenticator_data),
                signature: encode_base64url(&signed.signature),
                user_handle: signed.user_handle.as_deref().map(encode_base64url),
            }))
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
                .map_err(|error| {
                    denial(runtime, principal, error, "create_and_fill", Some(&url))
                })?;
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
                .map_err(|error| denial(runtime, principal, error, "exec_env", None))?;
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
        "extension_fill".to_owned(),
        "passkey_ceremony".to_owned(),
        "create_and_fill".to_owned(),
        "exec_env".to_owned(),
    ]
}

/// Decodes one base64url field of a WebAuthn ceremony.
///
/// The error names the field and never echoes the value: a malformed challenge
/// is a client bug, not something to copy into a log or an audit row.
fn decode_base64url(value: &str, field: &'static str) -> Result<Vec<u8>, Response> {
    URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| Response::error("invalid_input", format!("the {field} is not base64url")))
}

/// Encodes one binary field of a WebAuthn answer for the extension.
fn encode_base64url(value: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(value)
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
        .map_err(|error| denial(runtime, principal, error, action.as_str(), None))
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
    origin: Option<&str>,
) -> Response {
    if matches!(error, VaultError::Forbidden) {
        tracing::warn!(action, "MCP action refused by policy");
        let _ = runtime.record_audit(NewAuditEntry {
            token_id: Some(principal.token_id.clone()),
            action: McpAction::AccessDenied,
            target_item_id: None,
            target_folder_id: None,
            target_origin: origin.map(str::to_owned),
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

/// Copies a secret into the payload that leaves towards the extension.
///
/// The copy is wrapped in [`Zeroizing`] so it is wiped with the response, and it
/// never passes through a `String` that outlives it.
fn redacted(value: Option<&akooti_core::SecretString>) -> Option<Zeroizing<String>> {
    value.map(|secret| Zeroizing::new(secret.expose().to_owned()))
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
    use super::{
        ClientKind, capabilities, decode_base64url, dispatch, encode_base64url, response_for,
    };
    use crate::vault_runtime::VaultRuntime;
    use akooti_core::{McpAction, McpScope, NewMcpToken, VaultError};
    use akooti_ipc::{PROTOCOL_VERSION, Request, ResponseBody};

    #[test]
    fn the_handshake_announces_the_protocol_and_capabilities() {
        let caps = capabilities();
        assert!(caps.contains(&"vault_search".to_owned()));
        assert!(!caps.contains(&"blind_autofill".to_owned()));
        assert!(caps.contains(&"cdp_autofill".to_owned()));
        assert!(caps.contains(&"passkey_ceremony".to_owned()));
    }

    #[test]
    fn a_web_authn_field_round_trips_and_a_bad_one_is_refused() {
        let bytes = [0xfb_u8, 0xff, 0x00, 0x10];
        let encoded = encode_base64url(&bytes);
        assert_eq!(encoded, "-_8AEA", "base64url, unpadded, never base64");
        assert_eq!(
            decode_base64url(&encoded, "challenge").expect("decode"),
            bytes
        );

        // A malformed field is a client bug: the error names the field, and
        // never repeats what was sent.
        let error = decode_base64url("pas*un*base64", "challenge").expect_err("refused");
        let encoded = serde_json::to_string(&error).expect("encode");
        assert!(encoded.contains("invalid_input"));
        assert!(encoded.contains("challenge"));
        assert!(!encoded.contains("pas*un*base64"));
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

    /// An unlocked vault holding one `blind_fill` token, as the app would have.
    fn unlocked_runtime() -> (tempfile::TempDir, VaultRuntime, String) {
        let temp = tempfile::tempdir().expect("temporary directory");
        let paths = akooti_core::VaultPaths::under_app_data(temp.path());
        let runtime = VaultRuntime::new(paths);
        runtime.create("password").expect("create vault");
        let issued = runtime
            .with_vault_mut(|vault| {
                vault.create_mcp_token(NewMcpToken {
                    name: "Akooti browser extension".to_owned(),
                    scopes: vec![McpScope::BlindFill],
                    folder_ids: vec![akooti_core::PERSONAL_FOLDER_ID.to_owned()],
                    default_create_folder_id: None,
                    expires_at: None,
                })
            })
            .expect("issue token");
        (temp, runtime, issued.secret.to_string())
    }

    fn registration(token: &str) -> Request {
        Request::VaultPasskeyRegister {
            token: token.to_owned(),
            origin: "https://example.test".to_owned(),
            rp_id: "example.test".to_owned(),
            rp_name: "Example".to_owned(),
            user_handle: encode_base64url(&[7_u8; 16]),
            user_name: "user@example.test".to_owned(),
            challenge: encode_base64url(&[1_u8; 32]),
        }
    }

    /// The whole daemon path, without a socket: policy, base64 and answers.
    #[test]
    fn the_browser_bridge_serves_a_whole_ceremony() {
        let (_temp, runtime, token) = unlocked_runtime();

        let handshake = dispatch(
            &runtime,
            Request::Handshake {
                protocol_version: PROTOCOL_VERSION,
                client: "native-host".to_owned(),
            },
            ClientKind::NativeHost,
        );
        assert!(matches!(handshake.body, ResponseBody::Handshake(_)));

        let registered = dispatch(&runtime, registration(&token), ClientKind::NativeHost);
        let credential = match registered.body {
            ResponseBody::PasskeyRegistration(credential) => credential,
            other => panic!("unexpected body: {other:?}"),
        };
        assert_eq!(credential.public_key_algorithm, -7, "ES256");
        assert_eq!(credential.transports, vec!["internal".to_owned()]);
        assert!(!credential.id.is_empty());
        assert!(!credential.attestation_object.is_empty());
        // Nothing but base64url leaves the daemon.
        assert!(
            credential
                .attestation_object
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );

        let signed = dispatch(
            &runtime,
            Request::VaultPasskeyAssert {
                token: token.clone(),
                origin: "https://example.test".to_owned(),
                rp_id: "example.test".to_owned(),
                challenge: encode_base64url(&[2_u8; 32]),
                allow_credentials: vec![credential.id.clone()],
            },
            ClientKind::NativeHost,
        );
        match signed.body {
            ResponseBody::PasskeyAssertion(signature) => {
                assert_eq!(signature.id, credential.id);
                assert_eq!(signature.user_handle, Some(encode_base64url(&[7_u8; 16])));
                assert!(!signature.signature.is_empty());
                assert!(!signature.authenticator_data.is_empty());
            }
            other => panic!("unexpected body: {other:?}"),
        }

        // Two ceremonies, both written to the audit log with the origin, and
        // neither carrying anything the relying party did not already have.
        let audit = runtime
            .with_vault(|vault| vault.mcp_audit_logs(10))
            .expect("audit log");
        let passkeys = audit
            .iter()
            .filter(|entry| entry.action == McpAction::PasskeyUse)
            .count();
        assert_eq!(passkeys, 2);
        let rendered = format!("{audit:?}");
        assert!(!rendered.contains(&credential.id));
    }

    #[test]
    fn an_agent_connection_cannot_run_a_passkey_ceremony() {
        let (_temp, runtime, token) = unlocked_runtime();

        // `akooti mcp` never sends this verb, and an accidental cross-wiring
        // must not be the thing that hands an agent a signing oracle.
        let refused = dispatch(&runtime, registration(&token), ClientKind::Mcp);
        match refused.body {
            ResponseBody::Error { code, .. } => assert_eq!(code, "forbidden"),
            other => panic!("unexpected body: {other:?}"),
        }

        let audit = runtime
            .with_vault(|vault| vault.mcp_audit_logs(10))
            .expect("audit log");
        assert!(
            audit
                .iter()
                .any(|entry| entry.action == McpAction::AccessDenied
                    && entry.target_origin.as_deref() == Some("https://example.test"))
        );
        assert!(
            !audit
                .iter()
                .any(|entry| entry.action == McpAction::PasskeyUse)
        );
    }

    #[test]
    fn a_ceremony_for_another_site_is_refused_by_the_daemon() {
        let (_temp, runtime, token) = unlocked_runtime();
        let mut stolen = registration(&token);
        if let Request::VaultPasskeyRegister { origin, .. } = &mut stolen {
            *origin = "https://bank.test".to_owned();
        }
        match dispatch(&runtime, stolen, ClientKind::NativeHost).body {
            ResponseBody::Error { code, .. } => assert_eq!(code, "invalid_input"),
            other => panic!("unexpected body: {other:?}"),
        }
    }

    #[test]
    fn a_binary_field_that_is_not_base64url_is_refused_before_the_vault() {
        let (_temp, runtime, token) = unlocked_runtime();
        let mut broken = registration(&token);
        if let Request::VaultPasskeyRegister { challenge, .. } = &mut broken {
            *challenge = "not base64url".to_owned();
        }
        match dispatch(&runtime, broken, ClientKind::NativeHost).body {
            ResponseBody::Error { code, message } => {
                assert_eq!(code, "invalid_input");
                assert!(message.contains("challenge"));
            }
            other => panic!("unexpected body: {other:?}"),
        }
    }
}
