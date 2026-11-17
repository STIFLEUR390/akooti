//! Wire protocol of the local daemon.
//!
//! Every request carries the token it acts with: the socket proves *which OS
//! user* is calling, the token proves *what* that caller may reach. Nothing here
//! carries a plaintext secret towards the client.

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Protocol version.
///
/// History: `1` was the first shipped shape (variant names camelCase, variant
/// fields snake_case); `2` renamed the fields to camelCase so one casing rule
/// covers the whole wire, including the browser extension.
pub const PROTOCOL_VERSION: u32 = 2;

/// One request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "method"
)]
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
    /// Values released to the paired browser extension.
    ///
    /// This is the only request that answers with a secret: it exists for the
    /// extension, which fills a page (or copies a value) the user explicitly
    /// asked for. The daemon answers it only on a connection that declared
    /// `client: "native-host"`, only for a token holding `blind_fill`, and only
    /// when the requested origin matches the item. `akooti mcp` never sends it.
    VaultExtensionFill {
        /// Presented token secret.
        token: String,
        /// Item to read values from.
        item_id: String,
        /// Origin of the page the values are released for.
        origin: String,
        /// Selectors supplied by the extension (a capture, for instance).
        selectors: Option<SelectorOverride>,
        /// What the extension will do: `fill` or `clipboard` (audit only).
        target: Option<String>,
    },
    /// WebAuthn registration served by the vault (`navigator.credentials.create`).
    ///
    /// Like [`Request::VaultExtensionFill`], this verb exists for the paired
    /// extension only: the daemon refuses it on any connection that did not
    /// declare `client: "native-host"`. Binary parameters travel base64url
    /// encoded, so the extension never has to agree on a second encoding.
    VaultPasskeyRegister {
        /// Presented token secret.
        token: String,
        /// Origin of the page asking, in serialized form.
        origin: String,
        /// Relying party id; it must belong to `origin`.
        rp_id: String,
        /// Relying party display name.
        rp_name: String,
        /// Opaque user handle, base64url.
        user_handle: String,
        /// Account name shown to the user.
        user_name: String,
        /// Challenge to echo in `clientDataJSON`, base64url.
        challenge: String,
    },
    /// WebAuthn assertion served by the vault (`navigator.credentials.get`).
    VaultPasskeyAssert {
        /// Presented token secret.
        token: String,
        /// Origin of the page asking, in serialized form.
        origin: String,
        /// Relying party id; it must belong to `origin`.
        rp_id: String,
        /// Challenge to echo in `clientDataJSON`, base64url.
        challenge: String,
        /// Credential ids the relying party accepts, base64url.
        allow_credentials: Vec<String>,
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
        /// Selectors overriding the ones stored on the created item.
        #[serde(default)]
        selectors: Option<SelectorOverride>,
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
    /// Values released to the paired browser extension.
    ExtensionFill(FillPayload),
    /// Credential produced by a registration ceremony.
    PasskeyRegistration(PasskeyCredential),
    /// Signature produced by an assertion ceremony.
    PasskeyAssertion(PasskeySignature),
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

/// Values released to the paired browser extension.
///
/// This is the only payload that carries a secret across the local socket. It
/// is produced exclusively for a connection that declared `client:
/// "native-host"`, for a token holding `blind_fill`, and for an origin that
/// matches the item; `akooti mcp` exposes no tool that maps to it
/// (`docs/regles.md` SEC-1). Values live in [`Zeroizing`] and `Debug` redacts
/// them, so a stray trace or a log line cannot leak one.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillPayload {
    /// Origin the values are released for.
    pub origin: String,
    /// Selectors to use: stored on the item, overridden by the caller's.
    pub selectors: SelectorOverride,
    /// Account identifier, when the item has one.
    pub username: Option<Zeroizing<String>>,
    /// Password, when the item has one.
    pub password: Option<Zeroizing<String>>,
    /// Current TOTP code, when the item has a seed.
    pub totp: Option<Zeroizing<String>>,
}

impl std::fmt::Debug for FillPayload {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn presence(value: &Option<Zeroizing<String>>) -> Option<&'static str> {
            value.as_ref().map(|_| "<set>")
        }
        formatter
            .debug_struct("FillPayload")
            .field("origin", &self.origin)
            .field("selectors", &self.selectors)
            .field("username", &presence(&self.username))
            .field("password", &presence(&self.password))
            .field("totp", &presence(&self.totp))
            .finish()
    }
}

/// Credential produced by a WebAuthn registration ceremony.
///
/// The shape is exactly what `PublicKeyCredential` needs on the page, which is
/// the one place this payload is consumed (`apps/extension/lib/webauthn/
/// credential.ts`). It carries no secret: a credential is public by design, and
/// the private key it was made from never leaves the vault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasskeyCredential {
    /// Credential id, base64url.
    pub id: String,
    /// `clientDataJSON` the credential is bound to, base64url.
    #[serde(rename = "clientDataJSON")]
    pub client_data_json: String,
    /// CBOR `attestationObject`, base64url.
    pub attestation_object: String,
    /// COSE algorithm of the key (`-7` for ES256).
    pub public_key_algorithm: i64,
    /// Transports the authenticator reports (`["internal"]`).
    pub transports: Vec<String>,
}

/// Signature produced by a WebAuthn assertion ceremony.
///
/// Same rule as [`PasskeyCredential`]: public data only, shaped for the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasskeySignature {
    /// Credential that signed, base64url.
    pub id: String,
    /// `clientDataJSON` the signature covers, base64url.
    #[serde(rename = "clientDataJSON")]
    pub client_data_json: String,
    /// `authenticatorData` the signature covers, base64url.
    pub authenticator_data: String,
    /// DER-encoded ECDSA signature, base64url.
    pub signature: String,
    /// User handle stored with the credential, base64url.
    pub user_handle: Option<String>,
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
        FillOutcome, FillPayload, FillStatus, PROTOCOL_VERSION, PasskeyCredential,
        PasskeySignature, Request, Response, ResponseBody, SelectorOverride,
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

    #[test]
    fn the_extension_wire_shape_is_stable() {
        // `apps/extension/lib/native/protocol.ts` speaks these exact shapes.
        let request = Request::VaultExtensionFill {
            token: "ak_mcp_probe".to_owned(),
            item_id: "item-1".to_owned(),
            origin: "https://app.example.test".to_owned(),
            selectors: Some(SelectorOverride {
                password: Some("#pass".to_owned()),
                ..SelectorOverride::default()
            }),
            target: Some("fill".to_owned()),
        };
        let encoded: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&request).expect("encode"))
                .expect("decode");
        assert_eq!(
            encoded,
            serde_json::json!({
                "method": "vaultExtensionFill",
                "token": "ak_mcp_probe",
                "itemId": "item-1",
                "origin": "https://app.example.test",
                "selectors": {
                    "username": null,
                    "password": "#pass",
                    "totp": null,
                    "submit": null,
                },
                "target": "fill",
            })
        );

        let response = Response::ok(ResponseBody::ExtensionFill(FillPayload {
            origin: "https://app.example.test".to_owned(),
            selectors: SelectorOverride {
                username: Some("#email".to_owned()),
                ..SelectorOverride::default()
            },
            username: Some("agent@example.test".to_owned().into()),
            password: Some("s3cret-value".to_owned().into()),
            totp: None,
        }));
        let encoded: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&response).expect("encode"))
                .expect("decode");
        assert_eq!(encoded["kind"], "extensionFill");
        assert_eq!(encoded["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(encoded["password"], "s3cret-value");
        assert_eq!(encoded["totp"], serde_json::Value::Null);
    }

    #[test]
    fn a_fill_payload_never_prints_a_value() {
        let payload = FillPayload {
            origin: "https://app.example.test".to_owned(),
            selectors: SelectorOverride::default(),
            username: Some("agent@example.test".to_owned().into()),
            password: Some("s3cret-value".to_owned().into()),
            totp: Some("123456".to_owned().into()),
        };
        let rendered = format!("{payload:?}");
        assert!(rendered.contains("https://app.example.test"));
        assert!(!rendered.contains("s3cret-value"));
        assert!(!rendered.contains("123456"));
        assert!(rendered.contains("<set>"));
    }

    #[test]
    fn the_passkey_wire_shape_is_stable() {
        // `apps/extension/lib/protocol.ts` speaks these exact shapes, and the
        // page consumes the answers through `webauthn/credential.ts`.
        let request = Request::VaultPasskeyRegister {
            token: "ak_mcp_probe".to_owned(),
            origin: "https://example.test".to_owned(),
            rp_id: "example.test".to_owned(),
            rp_name: "Example".to_owned(),
            user_handle: "BwcHBwc".to_owned(),
            user_name: "user@example.test".to_owned(),
            challenge: "AQID".to_owned(),
        };
        let encoded: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&request).expect("encode"))
                .expect("decode");
        assert_eq!(
            encoded,
            serde_json::json!({
                "method": "vaultPasskeyRegister",
                "token": "ak_mcp_probe",
                "origin": "https://example.test",
                "rpId": "example.test",
                "rpName": "Example",
                "userHandle": "BwcHBwc",
                "userName": "user@example.test",
                "challenge": "AQID",
            })
        );

        let assert = Request::VaultPasskeyAssert {
            token: "ak_mcp_probe".to_owned(),
            origin: "https://example.test".to_owned(),
            rp_id: "example.test".to_owned(),
            challenge: "AQID".to_owned(),
            allow_credentials: vec!["BwcHBwc".to_owned()],
        };
        let encoded: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&assert).expect("encode")).expect("decode");
        assert_eq!(encoded["method"], "vaultPasskeyAssert");
        assert_eq!(encoded["allowCredentials"], serde_json::json!(["BwcHBwc"]));

        let response = Response::ok(ResponseBody::PasskeyRegistration(PasskeyCredential {
            id: "BwcHBwc".to_owned(),
            client_data_json: "e30".to_owned(),
            attestation_object: "o2NmbXRkbm9uZQ".to_owned(),
            public_key_algorithm: -7,
            transports: vec!["internal".to_owned()],
        }));
        let encoded: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&response).expect("encode"))
                .expect("decode");
        assert_eq!(encoded["kind"], "passkeyRegistration");
        assert_eq!(encoded["protocolVersion"], PROTOCOL_VERSION);
        // WebAuthn spells it `clientDataJSON`, acronym included.
        assert_eq!(encoded["clientDataJSON"], "e30");
        assert_eq!(encoded["publicKeyAlgorithm"], -7);

        let response = Response::ok(ResponseBody::PasskeyAssertion(PasskeySignature {
            id: "BwcHBwc".to_owned(),
            client_data_json: "e30".to_owned(),
            authenticator_data: "AAAA".to_owned(),
            signature: "MEUCIQ".to_owned(),
            user_handle: None,
        }));
        let encoded: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&response).expect("encode"))
                .expect("decode");
        assert_eq!(encoded["kind"], "passkeyAssertion");
        assert_eq!(encoded["clientDataJSON"], "e30");
        assert_eq!(encoded["userHandle"], serde_json::Value::Null);
    }

    #[test]
    fn a_passkey_answer_carries_no_private_key() {
        // The private scalar stays in the vault: only the public credential and
        // the signature travel, both of which the relying party already sees.
        let signature = PasskeySignature {
            id: "BwcHBwc".to_owned(),
            client_data_json: "e30".to_owned(),
            authenticator_data: "AAAA".to_owned(),
            signature: "MEUCIQ".to_owned(),
            user_handle: Some("BwcHBwc".to_owned()),
        };
        let rendered = format!("{signature:?}");
        assert!(rendered.contains("BwcHBwc"));
        assert!(!rendered.to_lowercase().contains("private"));
    }
}
