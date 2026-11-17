//! Blind injection engines.
//!
//! Two transports, one contract: the daemon pushes values straight into the
//! page, and the caller only learns *what was filled*. The secret never appears
//! in a response, an error or a log.
//!
//! * **CDP** — attach to a Chrome/Chromium DevTools endpoint, evaluate a fill
//!   program in the page, dispatch `input`/`change` so reactive frameworks see
//!   the new value, then optionally click the submit control.
//! * **Native Messaging** — the extension bridge, scheduled for phase 6. Until
//!   then the daemon answers `unavailable`, never a fake success.

use akooti_core::{SecretString, Selectors};
use akooti_ipc::{FillOutcome, FillStatus, SelectorOverride};
use serde::Deserialize;

/// Default DevTools endpoint.
pub const DEFAULT_CDP_ENDPOINT: &str = "http://127.0.0.1:9222";

/// Values to inject for one item.
#[derive(Debug, Default)]
pub struct FillTarget {
    /// Selectors memorised on the item.
    pub selectors: Selectors,
    /// Username, when stored.
    pub username: Option<SecretString>,
    /// Password, when stored.
    pub password: Option<SecretString>,
    /// Current TOTP code, when the item has a seed.
    pub totp: Option<SecretString>,
}

/// Fills a page through the extension bridge.
///
/// The bridge lands in phase 6 (`akooti native-host`); until then this is an
/// explicit `unavailable`, so no agent believes a form was filled.
#[must_use]
pub fn native_messaging_fill() -> FillOutcome {
    FillOutcome::unavailable(
        "native-messaging",
        "the browser extension bridge is not available in this build; use vault_cdp_autofill",
    )
}

/// Fills a page through a CDP endpoint.
///
/// # Errors
///
/// Never returns the values; failures become a [`FillOutcome`] with a stable
/// `detail` code.
#[must_use]
pub fn cdp_fill(
    endpoint: Option<&str>,
    origin: &str,
    target: &FillTarget,
    overrides: Option<&SelectorOverride>,
) -> FillOutcome {
    let endpoint = endpoint.unwrap_or(DEFAULT_CDP_ENDPOINT);
    let selectors = merge_selectors(&target.selectors, overrides);
    let program = fill_program(&selectors, target);

    let page = match page_target(endpoint, origin) {
        Ok(page) => page,
        Err(reason) => return failed("cdp", reason),
    };
    let reply = match evaluate(&page.websocket_url, &program) {
        Ok(reply) => reply,
        Err(reason) => return failed("cdp", reason),
    };
    match reply {
        ScriptReply::Filled { fields, submitted } => {
            let empty = fields.is_empty();
            FillOutcome {
                status: if empty {
                    FillStatus::Partial
                } else {
                    FillStatus::Filled
                },
                transport: "cdp".to_owned(),
                fields_filled: fields,
                submitted,
                detail: if empty {
                    Some("no selector matched".to_owned())
                } else {
                    None
                },
            }
        }
        ScriptReply::SelectorError => failed("cdp", "a selector matched no field or is invalid"),
    }
}

fn failed(transport: &str, detail: &str) -> FillOutcome {
    FillOutcome {
        status: FillStatus::Failed,
        transport: transport.to_owned(),
        fields_filled: Vec::new(),
        submitted: false,
        detail: Some(detail.to_owned()),
    }
}

fn merge_selectors(base: &Selectors, overrides: Option<&SelectorOverride>) -> Selectors {
    let Some(overrides) = overrides else {
        return base.clone();
    };
    Selectors {
        username: overrides.username.clone().or_else(|| base.username.clone()),
        password: overrides.password.clone().or_else(|| base.password.clone()),
        totp: overrides.totp.clone().or_else(|| base.totp.clone()),
        submit: overrides.submit.clone().or_else(|| base.submit.clone()),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageTarget {
    #[serde(rename = "webSocketDebuggerUrl")]
    websocket_url: String,
    url: String,
}

fn page_target(endpoint: &str, origin: &str) -> Result<PageTarget, &'static str> {
    let list_url = format!("{}/json/list", endpoint.trim_end_matches('/'));
    let mut response = ureq::get(&list_url)
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(5)))
        .build()
        .call()
        .map_err(|_| "the CDP endpoint did not answer")?;
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|_| "the CDP target list is unreadable")?;
    let pages: Vec<PageTarget> =
        serde_json::from_str(&body).map_err(|_| "the CDP target list is malformed")?;
    // The origin must match: filling the first page on the endpoint would send
    // credentials into whatever tab happens to be focused.
    let matching = pages
        .iter()
        .find(|page| page.url.starts_with(origin) && page.websocket_url.starts_with("ws://"))
        .ok_or("no page target matches the requested origin")?;
    Ok(PageTarget {
        websocket_url: matching.websocket_url.clone(),
        url: matching.url.clone(),
    })
}

enum ScriptReply {
    Filled {
        fields: Vec<String>,
        submitted: bool,
    },
    SelectorError,
}

fn evaluate(websocket_url: &str, program: &str) -> Result<ScriptReply, &'static str> {
    use tungstenite::Message;

    let (mut socket, _) =
        tungstenite::connect(websocket_url).map_err(|_| "the CDP page could not be reached")?;
    if let tungstenite::stream::MaybeTlsStream::Plain(stream) = socket.get_ref() {
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
    }
    let request = serde_json::json!({
        "id": 1,
        "method": "Runtime.evaluate",
        "params": {
            "expression": program,
            "returnByValue": true,
            "awaitPromise": false,
        }
    });
    socket
        .send(Message::Text(request.to_string().into()))
        .map_err(|_| "the CDP request was rejected")?;

    loop {
        let message = socket
            .read()
            .map_err(|_| "the CDP page stopped answering")?;
        let text = match message {
            Message::Text(text) => text.to_string(),
            Message::Binary(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Message::Close(_) => return Err("the CDP page closed the connection"),
            _ => continue,
        };
        let value: serde_json::Value =
            serde_json::from_str(&text).map_err(|_| "the CDP reply is malformed")?;
        if value.get("id").and_then(serde_json::Value::as_u64) != Some(1) {
            continue;
        }
        let result = value
            .get("result")
            .and_then(|result| result.get("result"))
            .and_then(|result| result.get("value"))
            .and_then(serde_json::Value::as_str)
            .ok_or("the CDP reply carries no result")?;
        let parsed: serde_json::Value =
            serde_json::from_str(result).map_err(|_| "the fill program returned garbage")?;
        if parsed.get("error").is_some_and(|error| !error.is_null()) {
            return Ok(ScriptReply::SelectorError);
        }
        let fields = parsed
            .get("filled")
            .and_then(serde_json::Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let submitted = parsed
            .get("submitted")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        return Ok(ScriptReply::Filled { fields, submitted });
    }
}

/// Builds the in-page program. Values are JSON-encoded, so a secret containing
/// quotes or newlines cannot break out of the string literal.
fn fill_program(selectors: &Selectors, target: &FillTarget) -> String {
    let json = |value: Option<&str>| match value {
        Some(value) => serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_owned()),
        None => "null".to_owned(),
    };
    format!(
        r#"(() => {{
  const plan = [
    ["username", {username_selector}, {username_value}],
    ["password", {password_selector}, {password_value}],
    ["totp", {totp_selector}, {totp_value}],
  ];
  const filled = [];
  const apply = (element, value) => {{
    const setter = Object.getOwnPropertyDescriptor(
      Object.getPrototypeOf(element), "value")?.set;
    if (setter) setter.call(element, value); else element.value = value;
    element.dispatchEvent(new Event("input", {{ bubbles: true }}));
    element.dispatchEvent(new Event("change", {{ bubbles: true }}));
  }};
  try {{
    for (const [name, selector, value] of plan) {{
      if (!selector || value === null) continue;
      const element = document.querySelector(selector);
      if (!element) {{ if (name !== "totp") return JSON.stringify({{ error: "selector" }}); continue; }}
      apply(element, value);
      filled.push(name);
    }}
    let submitted = false;
    const submit = {submit_selector};
    if (submit) {{
      const button = document.querySelector(submit);
      if (button) {{ button.click(); submitted = true; }}
    }}
    return JSON.stringify({{ filled, submitted, error: null }});
  }} catch (error) {{
    return JSON.stringify({{ error: "runtime", filled }});
  }}
}})()"#,
        username_selector = json(selectors.username.as_deref()),
        username_value = json(target.username.as_ref().map(SecretString::expose)),
        password_selector = json(selectors.password.as_deref()),
        password_value = json(target.password.as_ref().map(SecretString::expose)),
        totp_selector = json(selectors.totp.as_deref()),
        totp_value = json(target.totp.as_ref().map(SecretString::expose)),
        submit_selector = json(selectors.submit.as_deref()),
    )
}

#[cfg(test)]
mod tests {
    use super::{FillTarget, SelectorOverride, cdp_fill, fill_program, native_messaging_fill};
    use akooti_core::{SecretString, Selectors};
    use akooti_ipc::FillStatus;

    fn target() -> FillTarget {
        FillTarget {
            selectors: Selectors {
                username: Some("#email".to_owned()),
                password: Some("#password".to_owned()),
                totp: None,
                submit: Some("button[type=submit]".to_owned()),
            },
            username: Some(SecretString::new("agent@example.test".to_owned())),
            password: Some(SecretString::new("s3cret-value".to_owned())),
            totp: None,
        }
    }

    #[test]
    fn the_program_embeds_escaped_values_and_no_stray_quote() {
        let program = fill_program(&target().selectors, &target());
        assert!(program.contains("#email"));
        assert!(program.contains("\"s3cret-value\""));
        assert!(program.contains("dispatchEvent"));
        assert!(!program.contains("Runtime"));
    }

    #[test]
    fn a_quote_in_a_secret_cannot_break_the_literal() {
        let mut target = target();
        target.password = Some(SecretString::new("pa\"ss');alert(1);//".to_owned()));
        let program = fill_program(&target.selectors, &target);
        assert!(program.contains(r#""pa\"ss');alert(1);//""#));
    }

    #[test]
    fn overrides_win_over_stored_selectors() {
        let mut selectors = target().selectors;
        selectors.password = Some("#old".to_owned());
        let overrides = SelectorOverride {
            password: Some("#new".to_owned()),
            ..SelectorOverride::default()
        };
        let merged = super::merge_selectors(&selectors, Some(&overrides));
        assert_eq!(merged.password.as_deref(), Some("#new"));
        assert_eq!(merged.username.as_deref(), Some("#email"));
    }

    #[test]
    fn an_unreachable_endpoint_is_a_failure_not_a_silent_success() {
        let outcome = cdp_fill(
            Some("http://127.0.0.1:1"),
            "http://localhost:3000",
            &target(),
            None,
        );
        assert_eq!(outcome.status, FillStatus::Failed);
        assert!(outcome.fields_filled.is_empty());
        assert!(!outcome.submitted);
        assert!(!format!("{outcome:?}").contains("s3cret-value"));
    }

    #[test]
    fn the_extension_bridge_reports_itself_as_unavailable() {
        let outcome = native_messaging_fill();
        assert_eq!(outcome.status, FillStatus::Unavailable);
        assert_eq!(outcome.transport, "native-messaging");
        assert!(
            outcome
                .detail
                .is_some_and(|detail| detail.contains("not available"))
        );
    }
}
