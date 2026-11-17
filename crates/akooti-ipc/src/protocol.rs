//! Wire protocol of the local daemon.
//!
//! Every request carries the token it acts with: the socket proves *which OS
//! user* is calling, the token proves *what* that caller may reach. Nothing here
//! carries a plaintext secret towards the client.

use serde::{Deserialize, Serialize};

/// Protocol version; bumped when a field changes meaning.
pub const PROTOCOL_VERSION: u32 = 1;

/// One request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "method")]
pub enum Request {
    /// Version and capability negotiation.
    Handshake {
        /// Protocol version spoken by the client.
        protocol_version: u32,
        /// Client kind (`mcp`, `native-host`, `ui`).
        client: String,
    },
    /// Token introspection: name, scopes and folders.
    VaultStatus {
        /// Presented token secret.
        token: String,
    },
    /// Metadata-only search.
    VaultSearch {
        /// Presented token secret.
        token: String,
        /// Free-text query.
        query: String,
        /// Maximum number of results.
        limit: usize,
    },
    /// Selector-based fill through the browser extension bridge.
    VaultBlindAutofill {
        /// Presented token secret.
        token: String,
        /// Item to fill from.
        item_id: String,
        /// Origin of the page.
        origin: String,
        /// Extra selectors supplied by the agent.
        selectors: Option<SelectorOverride>,
    },
    /// Selector-based fill through a CDP endpoint.
    VaultCdpAutofill {
        /// Presented token secret.
        token: String,
        /// Item to fill from.
        item_id: String,
        /// Origin of the page.
        origin: String,
        /// CDP HTTP endpoint (`http://127.0.0.1:9222`).
        endpoint: Option<String>,
        /// Extra selectors supplied by the agent.
        selectors: Option<SelectorOverride>,
    },
    /// Ephemeral account creation plus signup fill.
    VaultCreateAndFill {
        /// Presented token secret.
        token: String,
        /// Title of the created item.
        title: String,
        /// Signup page.
        url: String,
        /// Username to register.
        username: Option<String>,
        /// Lifetime in seconds.
        ttl_seconds: Option<i64>,
        /// CDP endpoint used for the fill.
        endpoint: Option<String>,
    },
    /// Secret injection into a child process environment.
    VaultExec {
        /// Presented token secret.
        token: String,
        /// Item exporting the variables.
        item_id: String,
        /// Command and arguments (`argv[0]` is the program).
        command: Vec<String>,
        /// Wall-clock budget in seconds.
        timeout_seconds: Option<u64>,
    },
}

/// Agent-supplied selector overrides (never a value).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectorOverride {
    /// Username field.
    pub username: Option<String>,
    /// Password field.
    pub password: Option<String>,
    /// TOTP field.
    pub totp: Option<String>,
    /// Submit control.
    pub submit: Option<String>,
}

/// One response envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    /// Protocol version spoken by the daemon.
    pub protocol_version: u32,
    /// Payload, or an error.
    #[serde(flatten)]
    pub body: ResponseBody,
}

impl Response {
    /// Successful payload.
    #[must_use]
    pub fn ok(body: ResponseBody) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            body,
        }
    }

    /// Error envelope with a stable code.
    #[must_use]
    pub fn error(code: &str, message: impl Into<String>) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            body: ResponseBody::Error {
                code: code.to_owned(),
                message: message.into(),
            },
        }
    }
}

/// Response payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ResponseBody {
    /// Handshake answer.
    Handshake(Handshake),
    /// Token introspection answer.
    VaultStatus(VaultStatus),
    /// Search results: metadata only.
    Search {
        /// Matching items, already filtered by the token's folders.
        items: Vec<ItemSummary>,
    },
    /// Injection outcome.
    Fill(FillOutcome),
    /// Ephemeral account creation.
    Created {
        /// Identifier of the created item.
        item_id: String,
        /// Title of the created item.
        title: String,
        /// Injection outcome for the signup form.
        outcome: FillOutcome,
    },
    /// Child process outcome, with masked output.
    Exec(ExecOutcome),
    /// Failure.
    Error {
        /// Stable code (`forbidden`, `locked`, `not_found`, …).
        code: String,
        /// Safe message.
        message: String,
    },
}

/// Handshake answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Handshake {
    /// Protocol version spoken by the daemon.
    pub protocol_version: u32,
    /// Whether a vault is currently unlocked in the daemon.
    pub vault_unlocked: bool,
    /// Capabilities available right now.
    pub capabilities: Vec<String>,
}

/// Token introspection answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    /// Token label, used as `created_by_agent`.
    pub token_name: String,
    /// Granted scopes.
    pub scopes: Vec<String>,
    /// Allowed folders.
    pub folder_ids: Vec<String>,
    /// Expiry in Unix seconds.
    pub expires_at: Option<i64>,
}

/// Metadata returned by a search: no secret of any kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemSummary {
    /// Item identifier.
    pub id: String,
    /// Title.
    pub title: String,
    /// Kind (`login`, `api_key`, …).
    pub item_type: String,
    /// Primary URL.
    pub primary_url: Option<String>,
    /// Username.
    pub username: Option<String>,
    /// Folders the item belongs to.
    pub folder_ids: Vec<String>,
    /// Whether a TOTP seed exists (presence only).
    pub has_totp: bool,
    /// Expiry in Unix seconds.
    pub expires_at: Option<i64>,
}

/// Status of an injection attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FillStatus {
    /// Every requested field was filled.
    Filled,
    /// The page was reached but a selector matched nothing.
    Partial,
    /// The transport is not available.
    Unavailable,
    /// The transport failed.
    Failed,
}

/// Outcome of an injection, never including a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillOutcome {
    /// What happened.
    pub status: FillStatus,
    /// Transport used (`cdp`, `native-messaging`).
    pub transport: String,
    /// Field names filled (`username`, `password`, `totp`).
    pub fields_filled: Vec<String>,
    /// True when the submit control was clicked.
    pub submitted: bool,
    /// Non-revealing diagnostic.
    pub detail: Option<String>,
}

impl FillOutcome {
    /// An outcome for a transport that is not wired yet.
    #[must_use]
    pub fn unavailable(transport: &str, detail: &str) -> Self {
        Self {
            status: FillStatus::Unavailable,
            transport: transport.to_owned(),
            fields_filled: Vec::new(),
            submitted: false,
            detail: Some(detail.to_owned()),
        }
    }
}

/// Result of `vault_exec`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecOutcome {
    /// Exit code, when the process exited normally.
    pub exit_code: Option<i32>,
    /// Signal name, when the process was killed.
    pub signal: Option<String>,
    /// True when the timeout fired.
    pub timed_out: bool,
    /// Captured stdout, with every injected value masked.
    pub stdout: String,
    /// Captured stderr, with every injected value masked.
    pub stderr: String,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::{
        FillOutcome, FillStatus, PROTOCOL_VERSION, Request, Response, ResponseBody,
        SelectorOverride,
    };

    #[test]
    fn requests_serialise_with_their_method_tag() {
        let request = Request::VaultSearch {
            token: "ak_mcp_test".to_owned(),
            query: "docker".to_owned(),
            limit: 5,
        };
        let encoded = serde_json::to_string(&request).expect("serialise");
        assert!(encoded.contains("\"method\":\"vaultSearch\""));
        let decoded: Request = serde_json::from_str(&encoded).expect("deserialise");
        assert!(matches!(decoded, Request::VaultSearch { limit: 5, .. }));
    }

    #[test]
    fn selector_overrides_are_optional() {
        let request = Request::VaultCdpAutofill {
            token: "t".to_owned(),
            item_id: "i".to_owned(),
            origin: "http://localhost:3000".to_owned(),
            endpoint: None,
            selectors: Some(SelectorOverride {
                password: Some("#pass".to_owned()),
                ..SelectorOverride::default()
            }),
        };
        let encoded = serde_json::to_string(&request).expect("serialise");
        assert!(encoded.contains("#pass"));
        assert!(encoded.contains("\"method\":\"vaultCdpAutofill\""));
    }

    #[test]
    fn responses_flatten_their_kind() {
        let response = Response::ok(ResponseBody::Fill(FillOutcome::unavailable(
            "native-messaging",
            "extension not connected",
        )));
        let encoded = serde_json::to_string(&response).expect("serialise");
        assert!(encoded.contains("\"kind\":\"fill\""));
        assert!(encoded.contains("\"status\":\"unavailable\""));
        assert_eq!(response.protocol_version, PROTOCOL_VERSION);

        let error = Response::error("forbidden", "access denied");
        let json = serde_json::to_string(&error).expect("serialise");
        assert!(json.contains("\"code\":\"forbidden\""));
    }

    #[test]
    fn an_unavailable_fill_carries_no_field() {
        let outcome = FillOutcome::unavailable("cdp", "no page target");
        assert_eq!(outcome.status, FillStatus::Unavailable);
        assert!(outcome.fields_filled.is_empty());
        assert!(!outcome.submitted);
    }
}
