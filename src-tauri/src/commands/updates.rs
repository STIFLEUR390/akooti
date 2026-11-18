//! Update channel commands.
//!
//! The manifest lives next to the release artifacts (`latest.json`); the public
//! key is baked into `tauri.conf.json`, the private key never enters the repo.
//! Until a first release is published, the check fails with a readable message
//! instead of pretending the app is up to date.

use serde::Serialize;
use tauri::AppHandle;

use crate::error::CommandError;

/// One available update, without the download URL.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfoDto {
    /// Running version.
    pub current_version: String,
    /// Version offered by the manifest.
    pub version: String,
    /// Release notes, when the manifest carries them.
    pub notes: Option<String>,
}

/// Asks the update endpoint whether a newer version exists.
///
/// # Errors
///
/// Returns `update` when the endpoint is unreachable, the manifest is unsigned
/// or the signature does not match the baked-in public key.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<UpdateInfoDto>, CommandError> {
    use tauri_plugin_updater::UpdaterExt;

    let updater = app
        .updater()
        .map_err(|_| CommandError::new("update", "Le canal de mise à jour est mal configuré."))?;
    let update = updater.check().await.map_err(|_| {
        CommandError::new(
            "update",
            "Impossible de vérifier les mises à jour (réseau ou aucune release publiée).",
        )
    })?;
    Ok(update.map(|update| UpdateInfoDto {
        current_version: update.current_version,
        version: update.version,
        notes: update.body,
    }))
}

/// Downloads and installs the pending update, then restarts the app.
///
/// # Errors
///
/// Returns `update` when the download, the signature check or the install fails.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), CommandError> {
    use tauri_plugin_updater::UpdaterExt;

    let updater = app
        .updater()
        .map_err(|_| CommandError::new("update", "Le canal de mise à jour est mal configuré."))?;
    let Some(update) = updater
        .check()
        .await
        .map_err(|_| CommandError::new("update", "Impossible de vérifier les mises à jour."))?
    else {
        return Ok(());
    };
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|_| CommandError::new("update", "La mise à jour n'a pas pu être installée."))?;
    app.restart();
}
