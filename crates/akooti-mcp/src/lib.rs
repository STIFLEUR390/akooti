//! Akooti MCP.
//!
//! Model Context Protocol server exposed to coding agents, plus the
//! Zero-Leak injection engines (browser extension bridge and CDP).
//!
//! Tools never return decrypted secrets: they only expose metadata and action
//! results (see `docs/regles.md` §1.1). This crate holds no key and no vault
//! handle — every call forwards the scoped token to the local daemon, which
//! alone can decrypt.

#![forbid(unsafe_code)]

use akooti_ipc::{
    ClientConfig, DaemonClient, ItemSummary, Request, ResponseBody, SelectorOverride,
};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, ErrorData};
use rmcp::{ServiceExt, tool, tool_router};
use serde::Deserialize;
use serde_json::{Value, json};

/// Arguments of `vault_search`.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SearchArgs {
    /// Free-text query matched against titles, usernames and URLs.
    #[serde(default)]
    pub query: String,
    /// Maximum number of results (1–200, default 25).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Arguments of `vault_blind_autofill` and `vault_cdp_autofill`.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FillArgs {
    /// Identifier returned by `vault_search`.
    pub item_id: String,
    /// Origin of the page being filled (`http://localhost:3000`).
    pub origin: String,
    /// DevTools endpoint; only used by `vault_cdp_autofill`.
    #[serde(default)]
    pub endpoint: Option<String>,
    /// CSS selectors overriding the ones stored on the item.
    #[serde(default)]
    pub selectors: Option<Selectors>,
}

/// Agent-supplied selectors. Never a value.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Selectors {
    /// Username field.
    #[serde(default)]
    pub username: Option<String>,
    /// Password field.
    #[serde(default)]
    pub password: Option<String>,
    /// TOTP field.
    #[serde(default)]
    pub totp: Option<String>,
    /// Submit control.
    #[serde(default)]
    pub submit: Option<String>,
}

impl From<&Selectors> for SelectorOverride {
    fn from(selectors: &Selectors) -> Self {
        Self {
            username: selectors.username.clone(),
            password: selectors.password.clone(),
            totp: selectors.totp.clone(),
            submit: selectors.submit.clone(),
        }
    }
}

/// Arguments of `vault_create_and_fill`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateArgs {
    /// Title of the created item.
    pub title: String,
    /// Signup page to open and fill.
    pub url: String,
    /// Username or email to register.
    #[serde(default)]
    pub username: Option<String>,
    /// Lifetime of the created account, in seconds (60–2592000, default 900).
    #[serde(default)]
    pub ttl_seconds: Option<i64>,
    /// DevTools endpoint used for the fill.
    #[serde(default)]
    pub endpoint: Option<String>,
    /// CSS selectors overriding the ones stored on the created item.
    #[serde(default)]
    pub selectors: Option<Selectors>,
}

/// Arguments of `vault_exec`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExecArgs {
    /// Item exporting the environment variables (`AKOOTI_USERNAME`, `AKOOTI_PASSWORD`, …).
    pub item_id: String,
    /// Command and arguments; `argv[0]` is the program.
    pub command: Vec<String>,
    /// Wall-clock budget in seconds (1–600, default 30).
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

/// MCP server. One instance per agent process, bound to one token.
#[derive(Debug, Clone)]
pub struct AkootiServer {
    client: DaemonClient,
    token: String,
}

impl AkootiServer {
    /// Builds a server for `token` on the default socket.
    ///
    /// # Errors
    ///
    /// Returns a message when no socket directory can be resolved.
    pub fn connect(token: String) -> Result<Self, String> {
        let config =
            ClientConfig::discover().map_err(|_| "no socket directory is available".to_owned())?;
        Ok(Self {
            client: DaemonClient::new(config),
            token,
        })
    }

    /// Builds a server against an explicit socket, used by tests.
    #[must_use]
    pub fn at(path: impl Into<std::path::PathBuf>, token: String) -> Self {
        Self {
            client: DaemonClient::new(ClientConfig::at(path)),
            token,
        }
    }

    /// Sends one request and turns the answer into a tool result.
    ///
    /// A refusal is a *tool* error (`CallToolResult::error`) so the agent sees
    /// the stable code, never a protocol-level failure it cannot read.
    fn exchange(&self, request: Request) -> Result<CallToolResult, ErrorData> {
        let body = match self.client.call(&request) {
            Ok(body) => body,
            Err(akooti_ipc::IpcError::Remote { code, message }) => {
                return Ok(tool_error(&code, &message));
            }
            Err(error) => return Ok(tool_error("daemon_unavailable", &error.to_string())),
        };
        match body {
            ResponseBody::Handshake(handshake) => Ok(json_result(&json!(handshake))),
            ResponseBody::VaultStatus(status) => Ok(json_result(&json!(status))),
            ResponseBody::Search { items } => Ok(search_result(&items)),
            ResponseBody::Fill(outcome) => Ok(json_result(&json!(outcome))),
            ResponseBody::Created {
                item_id,
                title,
                outcome,
            } => Ok(json_result(&json!({
                "itemId": item_id,
                "title": title,
                "outcome": outcome,
            }))),
            ResponseBody::Exec(outcome) => Ok(json_result(&json!(outcome))),
            // The extension payload is the only body that carries a value. No
            // MCP tool maps to it, so this arm is a last-line guard: it drops
            // the payload and answers a refusal (SEC-1, ARCH-6).
            ResponseBody::ExtensionFill(_) => Ok(tool_error(
                "forbidden",
                "this payload belongs to the browser extension",
            )),
            ResponseBody::Error { code, message } => Ok(tool_error(&code, &message)),
        }
    }
}

#[tool_router(server_handler)]
impl AkootiServer {
    /// Reports which token is in use, its scopes and its folders. No secret.
    #[tool(description = "Describe the current Akooti token: name, scopes and allowed folders.")]
    fn vault_status(&self) -> Result<CallToolResult, ErrorData> {
        self.exchange(Request::VaultStatus {
            token: self.token.clone(),
        })
    }

    /// Searches item metadata inside the token's folders.
    #[tool(
        description = "Search vault item metadata (title, username, URL) inside the token's folders. Never returns a password or a TOTP seed."
    )]
    fn vault_search(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.exchange(Request::VaultSearch {
            token: self.token.clone(),
            query: args.query,
            limit: page_limit(args.limit),
        })
    }

    /// Fills a login form through the browser extension bridge.
    #[tool(
        description = "Fill a login form in the user's browser through the Akooti extension. Returns which fields were filled, never their values. Requires the blind_fill scope."
    )]
    fn vault_blind_autofill(
        &self,
        Parameters(args): Parameters<FillArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.exchange(Request::VaultBlindAutofill {
            token: self.token.clone(),
            item_id: args.item_id,
            origin: args.origin,
            selectors: args.selectors.as_ref().map(SelectorOverride::from),
        })
    }

    /// Fills a login form in a browser exposing a DevTools (CDP) endpoint.
    #[tool(
        description = "Fill a login form through a Chrome/Chromium DevTools endpoint (default http://127.0.0.1:9222). Returns which fields were filled, never their values. Requires the blind_fill scope."
    )]
    fn vault_cdp_autofill(
        &self,
        Parameters(args): Parameters<FillArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.exchange(Request::VaultCdpAutofill {
            token: self.token.clone(),
            item_id: args.item_id,
            origin: args.origin,
            endpoint: args.endpoint,
            selectors: args.selectors.as_ref().map(SelectorOverride::from),
        })
    }

    /// Creates a throwaway account and fills its signup form.
    #[tool(
        description = "Create an ephemeral account in the token's folder with a generated password and a TTL, then fill the signup form. Returns the new item id and the fill outcome, never the password. Requires the create_and_fill scope."
    )]
    fn vault_create_and_fill(
        &self,
        Parameters(args): Parameters<CreateArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.exchange(Request::VaultCreateAndFill {
            token: self.token.clone(),
            title: args.title,
            url: args.url,
            username: args.username,
            ttl_seconds: args.ttl_seconds,
            endpoint: args.endpoint,
            selectors: args.selectors.as_ref().map(SelectorOverride::from),
        })
    }

    /// Runs a command with an item's secrets in its environment.
    #[tool(
        description = "Run a command with an item's credentials exported as environment variables (AKOOTI_USERNAME, AKOOTI_PASSWORD, AKOOTI_*). Output is returned with every injected value masked. Requires the exec_env scope."
    )]
    fn vault_exec(
        &self,
        Parameters(args): Parameters<ExecArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.exchange(Request::VaultExec {
            token: self.token.clone(),
            item_id: args.item_id,
            command: args.command,
            timeout_seconds: args.timeout_seconds,
        })
    }
}

fn json_result(value: &Value) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(value.to_string())])
}

/// Search results, with the fields an agent may see and nothing else.
fn search_result(items: &[ItemSummary]) -> CallToolResult {
    json_result(&search_payload(items))
}

fn search_payload(items: &[ItemSummary]) -> Value {
    let items = items
        .iter()
        .map(|item| {
            json!({
                "id": item.id,
                "title": item.title,
                "itemType": item.item_type,
                "primaryUrl": item.primary_url,
                "username": item.username,
                "folderIds": item.folder_ids,
                "hasTotp": item.has_totp,
                "expiresAt": item.expires_at,
            })
        })
        .collect::<Vec<_>>();
    json!({ "items": items, "count": items.len() })
}

fn tool_error(code: &str, message: &str) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(
        json!({ "error": code, "message": message }).to_string(),
    )])
}

/// Serves MCP over stdio until the client closes the stream.
///
/// # Errors
///
/// Returns a message when the transport cannot start or the server stops with
/// an error. The token is never included in the message.
pub fn serve(token: String) -> Result<(), String> {
    let server = AkootiServer::connect(token)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "the MCP runtime could not start".to_owned())?;
    runtime.block_on(async move {
        let service = server
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|_| "the MCP transport could not start".to_owned())?;
        service
            .waiting()
            .await
            .map_err(|_| "the MCP server stopped with an error".to_owned())?;
        Ok(())
    })
}

/// Clamps a requested page size to the daemon's accepted range.
#[must_use]
pub fn page_limit(limit: Option<usize>) -> usize {
    limit.unwrap_or(25).clamp(1, 200)
}

#[cfg(test)]
mod tests {
    use super::{AkootiServer, Selectors, page_limit};
    use akooti_ipc::{ItemSummary, Response, ResponseBody, VaultStatus};
    use std::io::Write;
    use std::os::unix::net::UnixListener;

    #[test]
    fn a_page_size_is_clamped_to_the_daemon_range() {
        assert_eq!(page_limit(None), 25);
        assert_eq!(page_limit(Some(0)), 1);
        assert_eq!(page_limit(Some(500)), 200);
        assert_eq!(page_limit(Some(12)), 12);
    }

    #[test]
    fn a_denial_is_a_tool_error_with_a_stable_code() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("daemon.sock");
        let listener = UnixListener::bind(&path).expect("bind");
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            akooti_ipc::read_frame(&mut stream).expect("request");
            let reply = Response::error("forbidden", "access denied");
            let encoded = serde_json::to_vec(&reply).expect("encode");
            akooti_ipc::write_frame(&mut stream, &encoded).expect("reply");
            stream.flush().expect("flush");
        });

        let server = AkootiServer::at(&path, "ak_mcp_test".to_owned());
        let result = server
            .exchange(akooti_ipc::Request::VaultStatus {
                token: "ak_mcp_test".to_owned(),
            })
            .expect("result");
        let rendered = serde_json::to_string(&result).expect("render");
        assert!(rendered.contains("forbidden"));
        handle.join().expect("join");
    }

    #[test]
    fn a_value_released_to_the_extension_never_reaches_a_tool_result() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("daemon.sock");
        let listener = UnixListener::bind(&path).expect("bind");
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            akooti_ipc::read_frame(&mut stream).expect("request");
            let reply = Response::ok(ResponseBody::ExtensionFill(akooti_ipc::FillPayload {
                origin: "https://app.example.test".to_owned(),
                selectors: akooti_ipc::SelectorOverride::default(),
                username: Some("agent@example.test".to_owned().into()),
                password: Some("s3cret-value".to_owned().into()),
                totp: Some("123456".to_owned().into()),
            }));
            let encoded = serde_json::to_vec(&reply).expect("encode");
            akooti_ipc::write_frame(&mut stream, &encoded).expect("reply");
            stream.flush().expect("flush");
        });

        let server = AkootiServer::at(&path, "ak_mcp_test".to_owned());
        let result = server
            .exchange(akooti_ipc::Request::VaultStatus {
                token: "ak_mcp_test".to_owned(),
            })
            .expect("result");
        let rendered = serde_json::to_string(&result).expect("render");
        assert!(rendered.contains("forbidden"));
        assert!(!rendered.contains("s3cret-value"));
        assert!(!rendered.contains("123456"));
        assert!(!rendered.contains("agent@example.test"));
        handle.join().expect("join");
    }

    #[test]
    fn selector_overrides_only_carry_selectors() {
        let selectors = Selectors {
            password: Some("#pass".to_owned()),
            ..Selectors::default()
        };
        let overrides = akooti_ipc::SelectorOverride::from(&selectors);
        assert_eq!(overrides.password.as_deref(), Some("#pass"));
        assert!(overrides.username.is_none());
    }

    #[test]
    fn a_search_result_never_carries_a_secret_field() {
        let items = vec![ItemSummary {
            id: "i".to_owned(),
            title: "Docker".to_owned(),
            item_type: "login".to_owned(),
            primary_url: Some("http://localhost:2375".to_owned()),
            username: Some("agent".to_owned()),
            folder_ids: vec!["f".to_owned()],
            has_totp: true,
            expires_at: None,
        }];
        let rendered = super::search_payload(&items).to_string();
        assert!(rendered.contains("\"hasTotp\":true"));
        assert!(!rendered.contains("password"));
        assert!(!rendered.contains("secret"));
    }

    #[test]
    fn a_status_reply_is_forwarded_verbatim() {
        let json = serde_json::to_value(VaultStatus {
            token_name: "agent".to_owned(),
            scopes: vec!["read_metadata".to_owned()],
            folder_ids: vec!["f".to_owned()],
            expires_at: None,
        })
        .expect("json");
        assert_eq!(json["tokenName"], "agent");
        let body = ResponseBody::VaultStatus(VaultStatus {
            token_name: "agent".to_owned(),
            scopes: Vec::new(),
            folder_ids: Vec::new(),
            expires_at: None,
        });
        assert!(matches!(body, ResponseBody::VaultStatus(_)));
        assert_eq!(
            Response::ok(body).protocol_version,
            akooti_ipc::PROTOCOL_VERSION
        );
    }
}
