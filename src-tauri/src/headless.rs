//! Headless entrypoints: `mcp`, `native-host` and `native-register`.
//!
//! Only `mcp` and `native-host` speak on stdout — JSON-RPC and Native Messaging
//! frames respectively — so every log goes to stderr. `native-register` is an
//! ordinary command: it prints one line per browser on stdout and never touches
//! the vault, a token or a secret.

use std::io::Write;

use crate::cli::NativeRegisterOptions;
use crate::native_register::{self, Browser, ManifestState, RegisterError, RegistrationRequest};

/// Exit code used when a headless mode is invoked without its required token.
pub const EXIT_MISSING_TOKEN: u8 = 2;
/// Exit code used when a mode cannot run on this platform yet.
pub const EXIT_NOT_IMPLEMENTED: u8 = 3;
/// Exit code used when the headless surface could not start or stopped badly.
pub const EXIT_FAILURE: u8 = 1;

/// Runs `akooti mcp --token …`.
///
/// The token is never logged, echoed, or stored. Returns the process exit code.
pub fn run_mcp(token: Option<&str>) -> u8 {
    let Some(token) = token.filter(|value| !value.trim().is_empty()) else {
        let _ = writeln!(
            std::io::stderr(),
            "akooti mcp: a scoped token is required (--token <TOKEN>)"
        );
        return EXIT_MISSING_TOKEN;
    };
    match akooti_mcp::serve(token.to_owned()) {
        Ok(()) => 0,
        Err(reason) => {
            // The reason is built by `akooti-mcp` and never carries the token.
            let _ = writeln!(std::io::stderr(), "akooti mcp: {reason}");
            EXIT_FAILURE
        }
    }
}

/// Runs `akooti native-host`: the Native Messaging frame pump.
pub fn run_native_host() -> u8 {
    crate::native_host::run()
}

/// Runs `akooti native-register`.
///
/// Every browser is handled independently: one failure does not stop the
/// others, and the exit code reports whether all of them succeeded.
pub fn run_native_register(options: &NativeRegisterOptions) -> u8 {
    let Some(home) = options
        .home
        .clone()
        .map(std::path::PathBuf::from)
        .or_else(user_home)
    else {
        let _ = writeln!(
            std::io::stderr(),
            "akooti native-register: no home directory is available (pass --home)"
        );
        return EXIT_FAILURE;
    };
    let host_path = match options.host_path.clone().map(std::path::PathBuf::from) {
        Some(path) => path,
        None => match std::env::current_exe() {
            Ok(path) => path,
            Err(_) => {
                let _ = writeln!(
                    std::io::stderr(),
                    "akooti native-register: the running binary path is unknown (pass --host)"
                );
                return EXIT_FAILURE;
            }
        },
    };
    let browsers = match selected_browsers(&options.browsers) {
        Ok(browsers) => browsers,
        Err(unknown) => {
            let _ = writeln!(
                std::io::stderr(),
                "akooti native-register: unknown browser `{unknown}` \
                 (chrome, chromium, edge, brave, firefox)"
            );
            return EXIT_FAILURE;
        }
    };
    let config_home = std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from);
    let mut failed = false;
    let mut unsupported = false;
    for browser in browsers {
        let request = RegistrationRequest {
            browser,
            home: &home,
            config_home: config_home.as_deref(),
            host_path: &host_path,
            chromium_extension_id: options.extension_id.as_deref(),
            firefox_extension_id: options.firefox_id.as_deref(),
        };
        match apply(options, &request) {
            Ok((line, ok)) => {
                println!("{line}");
                failed |= !ok;
            }
            Err(error) => {
                let _ = writeln!(
                    std::io::stderr(),
                    "akooti native-register: {}: {}",
                    browser.as_str(),
                    error.message()
                );
                unsupported |= error == RegisterError::UnsupportedPlatform;
                failed = true;
            }
        }
    }
    match (failed, unsupported) {
        (false, _) => 0,
        (true, true) => EXIT_NOT_IMPLEMENTED,
        (true, false) => EXIT_FAILURE,
    }
}

/// Applies the requested action to one browser.
///
/// Returns the line to print and whether it counts as a success: `--check` is a
/// gate, so anything that is not `current` fails.
fn apply(
    options: &NativeRegisterOptions,
    request: &RegistrationRequest<'_>,
) -> Result<(String, bool), RegisterError> {
    let manifest = native_register::plan(request)?;
    if options.check {
        let state = native_register::check(&manifest);
        return Ok((
            format!(
                "{}: {} ({})",
                manifest.browser.as_str(),
                state_name(state),
                manifest.path.display()
            ),
            state == ManifestState::Current,
        ));
    }
    if options.uninstall {
        let removed = native_register::uninstall(&manifest)?;
        return Ok((
            format!(
                "{}: {} ({})",
                manifest.browser.as_str(),
                if removed { "removed" } else { "absent" },
                manifest.path.display()
            ),
            true,
        ));
    }
    let outcome = native_register::install(&manifest)?;
    Ok((
        format!(
            "{}: {} ({})",
            manifest.browser.as_str(),
            outcome.as_str(),
            manifest.path.display()
        ),
        true,
    ))
}

fn state_name(state: ManifestState) -> &'static str {
    match state {
        ManifestState::Missing => "missing",
        ManifestState::Current => "current",
        ManifestState::Outdated => "outdated",
        ManifestState::Unreadable => "unreadable",
    }
}

fn selected_browsers(names: &[String]) -> Result<Vec<Browser>, String> {
    if names.is_empty() {
        return Ok(Browser::ALL.to_vec());
    }
    names
        .iter()
        .map(|name| Browser::parse(name).ok_or_else(|| name.clone()))
        .collect()
}

fn user_home() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

#[cfg(test)]
mod tests {
    use super::{EXIT_MISSING_TOKEN, EXIT_NOT_IMPLEMENTED, run_mcp, selected_browsers, state_name};
    use crate::cli::NativeRegisterOptions;
    use crate::native_register::{Browser, ManifestState};

    #[test]
    fn mcp_requires_a_token() {
        assert_eq!(run_mcp(None), EXIT_MISSING_TOKEN);
        assert_eq!(run_mcp(Some("   ")), EXIT_MISSING_TOKEN);
    }

    #[test]
    fn the_not_implemented_code_is_reserved_for_unsupported_platforms() {
        assert_eq!(EXIT_NOT_IMPLEMENTED, 3);
    }

    #[test]
    fn no_browser_flag_means_every_browser() {
        assert_eq!(
            selected_browsers(&[]).expect("all browsers"),
            Browser::ALL.to_vec()
        );
        assert_eq!(
            selected_browsers(&["chrome".to_owned()]),
            Ok(vec![Browser::Chrome])
        );
        assert_eq!(
            selected_browsers(&["safari".to_owned()]),
            Err("safari".to_owned())
        );
    }

    #[test]
    fn states_have_stable_names() {
        assert_eq!(state_name(ManifestState::Current), "current");
        assert_eq!(state_name(ManifestState::Outdated), "outdated");
    }

    #[test]
    fn a_registration_against_a_sandbox_home_fails_without_an_extension_id() {
        let directory = tempfile::tempdir().expect("tempdir");
        let options = NativeRegisterOptions {
            browsers: vec!["chrome".to_owned()],
            home: Some(directory.path().to_string_lossy().into_owned()),
            host_path: Some("/usr/local/bin/akooti".to_owned()),
            ..NativeRegisterOptions::default()
        };
        // Without `--extension-id` the Chromium manifest cannot be expressed.
        assert_eq!(super::run_native_register(&options), super::EXIT_FAILURE);
    }
}
