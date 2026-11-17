//! Folder commands.

use tauri::State;

use crate::dto::{FolderDraftDto, FolderDto};
use crate::error::CommandError;
use crate::vault_runtime::VaultRuntime;

/// Lists folders, protected system folders first.
#[tauri::command]
pub fn list_folders(
    runtime: State<'_, VaultRuntime>,
    include_deleted: Option<bool>,
) -> Result<Vec<FolderDto>, CommandError> {
    runtime.touch();
    let folders =
        runtime.with_vault(|vault| vault.list_folders(include_deleted.unwrap_or(false)))?;
    Ok(folders.into_iter().map(Into::into).collect())
}

/// Creates a user folder.
#[tauri::command]
pub fn create_folder(
    runtime: State<'_, VaultRuntime>,
    draft: FolderDraftDto,
) -> Result<FolderDto, CommandError> {
    runtime.touch();
    let folder = runtime.with_vault_mut(|vault| vault.create_folder(draft.into()))?;
    Ok(folder.into())
}

/// Renames a user folder.
#[tauri::command]
pub fn rename_folder(
    runtime: State<'_, VaultRuntime>,
    id: String,
    name: String,
) -> Result<FolderDto, CommandError> {
    runtime.touch();
    let folder = runtime.with_vault_mut(|vault| vault.rename_folder(&id, &name))?;
    Ok(folder.into())
}

/// Soft-deletes a user folder. System folders are rejected by the core.
#[tauri::command]
pub fn delete_folder(runtime: State<'_, VaultRuntime>, id: String) -> Result<(), CommandError> {
    runtime.touch();
    runtime.with_vault_mut(|vault| vault.delete_folder(&id))?;
    Ok(())
}

/// Adds one existing item to one folder without duplicating the item.
#[tauri::command]
pub fn add_item_to_folder(
    runtime: State<'_, VaultRuntime>,
    item_id: String,
    folder_id: String,
) -> Result<(), CommandError> {
    runtime.touch();
    runtime.with_vault_mut(|vault| vault.add_item_to_folder(&item_id, &folder_id))?;
    Ok(())
}

/// Removes one item/folder link.
#[tauri::command]
pub fn remove_item_from_folder(
    runtime: State<'_, VaultRuntime>,
    item_id: String,
    folder_id: String,
) -> Result<(), CommandError> {
    runtime.touch();
    runtime.with_vault_mut(|vault| vault.remove_item_from_folder(&item_id, &folder_id))?;
    Ok(())
}
