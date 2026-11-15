//! Akooti desktop entrypoint.
//!
//! Bootstraps the Tauri application and routes the three modes of the single
//! `akooti` binary: GUI + tray daemon, `akooti mcp` (headless MCP server) and
//! `akooti native-host` (browser Native Messaging bridge).
//!
//! CLI routing (`tauri-plugin-cli`) and the tray land in tasks 4.1 / 5.1.

#![forbid(unsafe_code)]

use tauri::Manager;

/// Runs the desktop application.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Debug-only automation bridge used by MCP Server Tauri. It is compiled
    // out of release builds and only listens on the loopback interface.
    #[cfg(debug_assertions)]
    let builder = tauri::Builder::default().plugin(
        tauri_plugin_mcp_bridge::Builder::new()
            .bind_address("127.0.0.1")
            .build(),
    );

    #[cfg(not(debug_assertions))]
    let builder = tauri::Builder::default();

    builder
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                window.set_title("Akooti")?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())?;

    Ok(())
}
