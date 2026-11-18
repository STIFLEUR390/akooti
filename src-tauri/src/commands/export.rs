//! Reversibility commands: `.akooti` snapshots, plaintext export and restore.
//!
//! The snapshot is the complete backup (vault files as they are, encrypted). The
//! plaintext JSON export is a deliberate, password-confirmed exception used for
//! migration; it is written once and never kept by the app.

use std::sync::Arc;

use akooti_core::VaultStatus;
use serde::Serialize;
use tauri::State;

use crate::error::CommandError;
use crate::sync::time::now_seconds;
use crate::vault_runtime::VaultRuntime;

/// Result of a written export.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportDto {
    /// Absolute path written.
    pub path: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// `snapshot` or `plaintext`.
    pub format: &'static str,
}

/// Writes an encrypted `.akooti` snapshot through a save dialog.
///
/// # Errors
///
/// Returns `dialog` when the picker fails, `io` when the file cannot be written,
/// or `locked` when the vault is locked.
#[tauri::command(async)]
pub fn export_snapshot(
    app: tauri::AppHandle,
    runtime: State<'_, Arc<VaultRuntime>>,
) -> Result<Option<ExportDto>, CommandError> {
    use tauri_plugin_dialog::DialogExt;

    runtime.touch();
    let default_name = format!("{}{}.akooti", akooti_core::SNAPSHOT_PREFIX, now_seconds());
    let picked = app
        .dialog()
        .file()
        .set_title("Exporter un instantané chiffré")
        .set_file_name(&default_name)
        .add_filter("Instantané Akooti", &["akooti"])
        .blocking_save_file();
    let Some(path) = picked else {
        return Ok(None);
    };
    let path = path
        .into_path()
        .map_err(|_| CommandError::new("dialog", "Ce fichier n'est pas accessible."))?;
    let size = runtime.with_vault(|vault| vault.write_snapshot(&path, now_seconds()))?;
    Ok(Some(ExportDto {
        path: path.to_string_lossy().into_owned(),
        size_bytes: size,
        format: "snapshot",
    }))
}

/// Writes the vault as plaintext JSON, after re-checking the master password.
///
/// # Errors
///
/// Returns `invalid_credentials` for a wrong password, `dialog`/`io` for the
/// file, or `locked` when the vault is locked.
#[tauri::command(async)]
pub fn export_plaintext(
    app: tauri::AppHandle,
    runtime: State<'_, Arc<VaultRuntime>>,
    password: String,
) -> Result<Option<ExportDto>, CommandError> {
    use tauri_plugin_dialog::DialogExt;

    runtime.touch();
    runtime.with_vault(|vault| vault.verify_master_password(&password))?;
    let default_name = format!("akooti-export-{}.json", now_seconds());
    let picked = app
        .dialog()
        .file()
        .set_title("Exporter en clair (non chiffré)")
        .set_file_name(&default_name)
        .add_filter("JSON", &["json"])
        .blocking_save_file();
    let Some(path) = picked else {
        return Ok(None);
    };
    let path = path
        .into_path()
        .map_err(|_| CommandError::new("dialog", "Ce fichier n'est pas accessible."))?;
    let contents = runtime.with_vault(|vault| vault.export_json(&password, now_seconds()))?;
    std::fs::write(&path, contents.as_bytes())
        .map_err(|_| CommandError::new("io", "Le fichier n'a pas pu être écrit."))?;
    Ok(Some(ExportDto {
        path: path.to_string_lossy().into_owned(),
        size_bytes: u64::try_from(contents.len()).unwrap_or(0),
        format: "plaintext",
    }))
}

/// Restores a snapshot into the vault location, replacing what is there.
///
/// The vault must be locked: replacing the files under an open SQLCipher
/// connection would leave the app holding a key to a file that no longer exists.
///
/// # Errors
///
/// Returns `vault_unlocked`, `invalid_input` for a foreign file, `dialog` or
/// `io` for the paths.
#[tauri::command(async)]
pub fn restore_snapshot(
    app: tauri::AppHandle,
    runtime: State<'_, Arc<VaultRuntime>>,
) -> Result<Option<String>, CommandError> {
    use tauri_plugin_dialog::DialogExt;

    if runtime.status() == VaultStatus::Unlocked {
        return Err(CommandError::new(
            "vault_unlocked",
            "Verrouillez le coffre avant de restaurer un instantané.",
        ));
    }
    let picked = app
        .dialog()
        .file()
        .set_title("Restaurer un instantané Akooti")
        .add_filter("Instantané Akooti", &["akooti"])
        .blocking_pick_file();
    let Some(path) = picked else {
        return Ok(None);
    };
    let path = path
        .into_path()
        .map_err(|_| CommandError::new("dialog", "Ce fichier n'est pas accessible."))?;
    akooti_core::restore_snapshot(&path, runtime.paths(), true)?;
    Ok(Some(path.to_string_lossy().into_owned()))
}
