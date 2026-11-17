//! Headless entrypoints for the `mcp` and `native-host` subcommands.
//!
//! Neither function reads the vault, the master key, or a secret: `mcp` forwards
//! its scoped token to the local daemon, `native-host` is still a phase 6 stub.

use std::io::Write;

/// Exit code used when a headless mode is invoked without its required token.
pub const EXIT_MISSING_TOKEN: u8 = 2;
/// Exit code used when the headless surface is not implemented yet.
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

/// Runs `akooti native-host`.
pub fn run_native_host() -> u8 {
    let _ = writeln!(
        std::io::stderr(),
        "akooti native-host: the Native Messaging bridge is not available in this build (phase 6)"
    );
    EXIT_NOT_IMPLEMENTED
}

#[cfg(test)]
mod tests {
    use super::{EXIT_MISSING_TOKEN, EXIT_NOT_IMPLEMENTED, run_mcp, run_native_host};

    #[test]
    fn mcp_requires_a_token() {
        assert_eq!(run_mcp(None), EXIT_MISSING_TOKEN);
        assert_eq!(run_mcp(Some("   ")), EXIT_MISSING_TOKEN);
    }

    #[test]
    fn native_host_reports_the_phase_gap() {
        assert_eq!(run_native_host(), EXIT_NOT_IMPLEMENTED);
    }
}
