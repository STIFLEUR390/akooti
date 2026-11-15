//! Akooti desktop entrypoint.
//!
//! Bootstraps the Tauri application and routes the three modes of the single
//! `akooti` binary: GUI + tray daemon, `akooti mcp` (headless MCP server) and
//! `akooti native-host` (browser Native Messaging bridge).
//!
//! Tauri wiring lands in task 4.1.

#![forbid(unsafe_code)]

/// Runs the application.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    Err("the Tauri shell is not wired yet (task 4.1)".into())
}
