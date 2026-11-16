//! Akooti desktop entrypoint.
//!
//! Bootstraps the Tauri application and routes the three modes of the single
//! `akooti` binary: GUI + tray daemon, `akooti mcp` (headless MCP server) and
//! `akooti native-host` (browser Native Messaging bridge).
//!
//! CLI routing (`tauri-plugin-cli`) and the tray land in tasks 4.1 / 5.1.

#![forbid(unsafe_code)]

use akooti_core::VaultPaths;
use tauri::{Manager, RunEvent, WindowEvent};

mod vault_runtime;

use vault_runtime::VaultRuntime;

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

    let app = builder
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                window.set_title("Akooti")?;
            }
            let data_dir = app.path().app_data_dir()?;
            if !app.manage(VaultRuntime::new(VaultPaths::under_app_data(&data_dir))) {
                return Err(std::io::Error::other("vault runtime is already managed").into());
            }
            Ok(())
        })
        .build(tauri::generate_context!())?;

    app.run(|app, event| {
        let runtime = app.state::<VaultRuntime>();
        match event {
            RunEvent::Resumed => runtime.resume(),
            RunEvent::ExitRequested { .. } | RunEvent::Exit => runtime.lock(),
            RunEvent::WindowEvent {
                event: WindowEvent::Focused(true),
                ..
            } => runtime.focus_gained(),
            RunEvent::MainEventsCleared => {
                runtime.enforce_auto_lock();
                runtime.run_maintenance();
            }
            _ => {}
        }
    });
    Ok(())
}
