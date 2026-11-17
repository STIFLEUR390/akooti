//! Akooti desktop entrypoint.
//!
//! The single `akooti` binary routes three modes before any window exists
//! (see [`cli`]):
//!
//! * `akooti` → GUI + background daemon + tray icon;
//! * `akooti mcp --token …` → headless MCP server (phase 5);
//! * `akooti native-host` → browser Native Messaging bridge (phase 6).
//!
//! Headless modes never build a Tauri application, so they cannot create a
//! webview, a tray icon, or a window.

#![forbid(unsafe_code)]

mod biometrics;
mod cli;
mod commands;
mod daemon;
mod dto;
mod error;
mod headless;
mod native_host;
mod native_register;
mod tray;
mod vault_runtime;

use commands::clipboard::ClipboardGuard;

use std::sync::Arc;

use akooti_core::VaultPaths;
use tauri::{Manager, RunEvent, WindowEvent};

use vault_runtime::VaultRuntime;

/// Runs the binary, routing the requested mode.
///
/// # Errors
///
/// Returns an error when the desktop application cannot be built.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    match cli::parse_mode(std::env::args_os()) {
        cli::CliMode::Help => {
            print!("{}", cli::USAGE);
            Ok(())
        }
        cli::CliMode::Version => {
            println!("akooti {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        cli::CliMode::Mcp { token } => {
            std::process::exit(i32::from(headless::run_mcp(token.as_deref())));
        }
        cli::CliMode::NativeHost => {
            std::process::exit(i32::from(headless::run_native_host()));
        }
        cli::CliMode::NativeRegister(options) => {
            let code = headless::run_native_register(&options);
            std::process::exit(i32::from(code));
        }
        cli::CliMode::Gui => run_gui(),
    }
}

fn run_gui() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_cli::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init());

    // Debug-only automation bridge used by the Tauri MCP server. It is compiled
    // out of release builds and only listens on the loopback interface.
    #[cfg(debug_assertions)]
    let builder = builder.plugin(
        tauri_plugin_mcp_bridge::Builder::new()
            .bind_address("127.0.0.1")
            .build(),
    );

    let app = builder
        .invoke_handler(tauri::generate_handler![
            commands::vault::vault_status,
            commands::vault::create_vault,
            commands::vault::unlock_vault,
            commands::system::lock_vault,
            commands::system::biometrics_status,
            commands::system::build_info,
            commands::folders::list_folders,
            commands::folders::create_folder,
            commands::folders::rename_folder,
            commands::folders::delete_folder,
            commands::folders::add_item_to_folder,
            commands::folders::remove_item_from_folder,
            commands::items::list_items,
            commands::items::get_item,
            commands::items::create_item,
            commands::items::update_item,
            commands::items::delete_item,
            commands::items::set_item_secret,
            commands::items::reveal_secret,
            commands::items::totp_code,
            commands::items::add_item_custom_field,
            commands::items::remove_item_custom_field,
            commands::generator::generate_password,
            commands::generator::generate_passphrase,
            commands::routing::list_routing_rules,
            commands::routing::add_routing_rule,
            commands::routing::set_routing_rule_enabled,
            commands::routing::apply_routing_rules,
            commands::clipboard::copy_secret,
            commands::import_export::list_import_formats,
            commands::import_export::pick_import_file,
            commands::import_export::preview_import_file,
            commands::import_export::import_file,
            commands::mcp::list_mcp_tokens,
            commands::mcp::create_mcp_token,
            commands::mcp::revoke_mcp_token,
            commands::mcp::delete_mcp_token,
            commands::mcp::list_mcp_audit_log,
            commands::mcp::daemon_status,
        ])
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                window.set_title("Akooti")?;
            }
            let data_dir = app.path().app_data_dir()?;
            let runtime = Arc::new(VaultRuntime::new(VaultPaths::under_app_data(&data_dir)));
            if !app.manage(Arc::clone(&runtime)) {
                return Err(std::io::Error::other("vault runtime is already managed").into());
            }
            app.manage(ClipboardGuard::default());
            // The daemon is the only holder of the master key: headless
            // entrypoints talk to it over an owner-only local socket.
            match daemon::start(Arc::clone(&runtime)) {
                Ok(handle) => {
                    app.manage(handle);
                }
                Err(reason) => {
                    tracing::warn!(%reason, "local daemon did not start");
                }
            }
            tray::install(app.handle())?;
            Ok(())
        })
        .build(tauri::generate_context!())?;

    app.run(|app, event| {
        // Early events (window creation, first ready tick) can arrive before
        // `setup` has registered the runtime: skip them instead of panicking.
        let Some(runtime) = app.try_state::<Arc<VaultRuntime>>() else {
            return;
        };
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
