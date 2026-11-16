//! Headless entrypoints for the `mcp` and `native-host` subcommands.
//!
//! Phase 4 only establishes the process contract: correct exit codes, clean
//! stderr, and no window. The MCP transport and the Native Messaging bridge
//! themselves land in phase 5 (`crates/akooti-mcp` and `native_host.rs`).
//!
//! Neither function reads the vault, the master key, or a secret: they only
//! validate that the process was invoked correctly.

use std::io::Write;

/// Exit code used when a headless mode is invoked without its required token.
pub const EXIT_MISSING_TOKEN: u8 = 2;
/// Exit code used when the headless surface is not implemented yet.
pub const EXIT_NOT_IMPLEMENTED: u8 = 3;

/// Runs `akooti mcp --token …`.
///
/// The token is validated for presence only; it is never logged, echoed, or
/// stored. Returns the process exit code.
pub fn run_mcp(token: Option<&str>) -> u8 {
    let Some(_token) = token.filter(|value| !value.trim().is_empty()) else {
        let _ = writeln!(
            std::io::stderr(),
            "akooti mcp: a scoped token is required (--token <TOKEN>)"
        );
        return EXIT_MISSING_TOKEN;
    };
    let _ = writeln!(
        std::io::stderr(),
        "akooti mcp: the MCP stdio server is not available in this build (phase 5)"
    );
    EXIT_NOT_IMPLEMENTED
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
        assert_eq!(run_mcp(Some("ak_mcp_test")), EXIT_NOT_IMPLEMENTED);
    }

    #[test]
    fn native_host_reports_the_phase_gap() {
        assert_eq!(run_native_host(), EXIT_NOT_IMPLEMENTED);
    }
}
