//! System tray: keeps the daemon alive when the window is closed.
//!
//! The tray only exposes three actions — show the vault, lock it immediately,
//! and quit. Locking from the tray goes through the same [`VaultRuntime`] as the
//! UI, so no second code path can hold the master key.

use tauri::{
    AppHandle, Manager,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

use crate::vault_runtime::VaultRuntime;

/// Builds and registers the tray icon.
///
/// # Errors
///
/// Returns a Tauri error when the menu or icon cannot be created.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Afficher Akooti", true, None::<&str>)?;
    let lock = MenuItem::with_id(app, "lock", "Verrouiller le coffre", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quitter", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &lock, &quit])?;

    TrayIconBuilder::with_id("akooti")
        .icon(
            app.default_window_icon()
                .cloned()
                .ok_or_else(|| tauri::Error::AssetNotFound("default window icon".to_owned()))?,
        )
        .tooltip("Akooti")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => reveal_window(app),
            "lock" => lock_vault(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                reveal_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn reveal_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn lock_vault(app: &AppHandle) {
    if let Some(runtime) = app.try_state::<VaultRuntime>() {
        runtime.lock();
    }
    reveal_window(app);
}
