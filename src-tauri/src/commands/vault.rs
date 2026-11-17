//! Vault lifecycle commands: status, creation and unlock.
//!
//! Authentication failures always return the same `invalid_credentials` code
//! and the same message so the UI cannot be used as a timing or existence
//! oracle.

use akooti_core::{VaultError, VaultStatus};
use serde::Serialize;
use tauri::State;

use crate::dto::FolderDto;
use crate::error::CommandError;
use crate::vault_runtime::VaultRuntime;

/// State reported to the unlock screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatusDto {
    /// `"locked"` or `"unlocked"`.
    pub status: &'static str,
    /// Whether a vault file already exists on disk.
    pub has_vault: bool,
    /// Configured inactivity timeout, in minutes.
    pub auto_lock_minutes: u32,
}

/// Returns the current lock state without touching any secret.
#[tauri::command]
pub fn vault_status(runtime: State<'_, VaultRuntime>) -> Result<VaultStatusDto, CommandError> {
    let status = runtime.status();
    let auto_lock_minutes = runtime
        .with_vault(|vault| Ok(vault.auto_lock_minutes()))
        .unwrap_or(15);
    Ok(VaultStatusDto {
        status: match status {
            VaultStatus::Unlocked => "unlocked",
            VaultStatus::Locked => "locked",
        },
        has_vault: runtime.is_initialized(),
        auto_lock_minutes,
    })
}

/// Creates the vault with a master password and leaves it unlocked.
///
/// Returns the three protected system folders so the UI can render the sidebar
/// immediately.
///
/// # Errors
///
/// Returns `invalid_input` for a password shorter than 12 characters and
/// `vault_exists` when a vault file is already present.
#[tauri::command]
pub fn create_vault(
    runtime: State<'_, VaultRuntime>,
    password: String,
) -> Result<Vec<FolderDto>, CommandError> {
    if password.chars().count() < 12 {
        return Err(CommandError::new(
            "invalid_input",
            "Le mot de passe maître doit contenir au moins 12 caractères.",
        ));
    }
    if runtime.is_initialized() {
        return Err(CommandError::new(
            "vault_exists",
            "Un coffre existe déjà sur cet appareil.",
        ));
    }
    runtime.create(&password)?;
    runtime.touch();
    let folders = runtime.with_vault(|vault| vault.list_folders(false))?;
    Ok(folders.into_iter().map(Into::into).collect())
}

/// Unlocks an existing vault.
///
/// # Errors
///
/// Returns `invalid_credentials` for a wrong password and `no_vault` when no
/// vault file exists yet.
#[tauri::command]
pub fn unlock_vault(
    runtime: State<'_, VaultRuntime>,
    password: String,
) -> Result<Vec<FolderDto>, CommandError> {
    if !runtime.is_initialized() {
        return Err(CommandError::new(
            "no_vault",
            "Aucun coffre n'a été trouvé sur cet appareil.",
        ));
    }
    match runtime.unlock(&password) {
        Ok(()) => {}
        Err(VaultError::InvalidCredentials) | Err(VaultError::Crypto) => {
            return Err(CommandError::new(
                "invalid_credentials",
                "Mot de passe maître incorrect.",
            ));
        }
        Err(error) => return Err(error.into()),
    }
    runtime.touch();
    let folders = runtime.with_vault(|vault| vault.list_folders(false))?;
    Ok(folders.into_iter().map(Into::into).collect())
}
