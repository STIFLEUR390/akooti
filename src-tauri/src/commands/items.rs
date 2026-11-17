//! Item commands.
//!
//! Reads return metadata only. A plaintext secret is returned exclusively by
//! [`reveal_secret`], which the UI calls from an explicit user gesture and
//! which always carries a 30-second expiry.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use akooti_core::TotpAlgorithm;
use serde::Deserialize;
use tauri::State;

use crate::dto::{
    CreateItemInput, CustomFieldInputDto, RevealedSecretDto, UpdateItemInput, VaultItemDto,
    parse_secret_field,
};
use crate::error::CommandError;
use crate::vault_runtime::VaultRuntime;

/// Item filter accepted by the list command.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemFilterDto {
    /// Restrict the list to one folder.
    pub folder_id: Option<String>,
    #[serde(default)]
    pub include_deleted: bool,
}

/// Lists item metadata, never secrets.
#[tauri::command]
pub fn list_items(
    runtime: State<'_, Arc<VaultRuntime>>,
    filter: Option<ItemFilterDto>,
) -> Result<Vec<VaultItemDto>, CommandError> {
    runtime.touch();
    let filter = filter.unwrap_or_default();
    let core_filter = akooti_core::ItemFilter {
        include_deleted: filter.include_deleted,
        folder_id: filter.folder_id,
    };
    let items = runtime.with_vault(|vault| vault.list_items(&core_filter))?;
    Ok(items.into_iter().map(Into::into).collect())
}

/// Fetches one item's metadata.
#[tauri::command]
pub fn get_item(
    runtime: State<'_, Arc<VaultRuntime>>,
    id: String,
) -> Result<VaultItemDto, CommandError> {
    runtime.touch();
    let item = runtime.with_vault(|vault| vault.get_item(&id))?;
    Ok(item.into())
}

/// Creates an item.
#[tauri::command]
pub fn create_item(
    runtime: State<'_, Arc<VaultRuntime>>,
    draft: CreateItemInput,
) -> Result<VaultItemDto, CommandError> {
    let draft = draft.into_core().map_err(invalid_input)?;
    runtime.touch();
    let item = runtime.with_vault_mut(|vault| vault.create_item(draft))?;
    Ok(item.into())
}

/// Applies a metadata-only edit; stored secrets are left untouched.
#[tauri::command]
pub fn update_item(
    runtime: State<'_, Arc<VaultRuntime>>,
    draft: UpdateItemInput,
) -> Result<VaultItemDto, CommandError> {
    let id = draft.id.clone();
    let metadata = draft.into_core().map_err(invalid_input)?;
    runtime.touch();
    let item = runtime.with_vault_mut(|vault| vault.update_item_metadata(&id, metadata))?;
    Ok(item.into())
}

/// Soft-deletes an item.
#[tauri::command]
pub fn delete_item(runtime: State<'_, Arc<VaultRuntime>>, id: String) -> Result<(), CommandError> {
    runtime.touch();
    runtime.with_vault_mut(|vault| vault.delete_item(&id))?;
    Ok(())
}

/// Writes or clears one encrypted field.
#[tauri::command]
pub fn set_item_secret(
    runtime: State<'_, Arc<VaultRuntime>>,
    id: String,
    field: String,
    value: Option<String>,
) -> Result<(), CommandError> {
    let field = parse_secret_field(&field).map_err(invalid_input)?;
    runtime.touch();
    runtime.with_vault_mut(|vault| {
        vault.set_item_secret(&id, field, value.map(akooti_core::SecretString::new))
    })?;
    Ok(())
}

/// Reveals one secret for an explicit UI action, with a 30-second expiry.
#[tauri::command]
pub fn reveal_secret(
    runtime: State<'_, Arc<VaultRuntime>>,
    id: String,
    field: String,
    custom_field_id: Option<String>,
) -> Result<RevealedSecretDto, CommandError> {
    let field = parse_secret_field(&field).map_err(invalid_input)?;
    runtime.touch();
    let revealed =
        runtime.with_vault(|vault| vault.reveal_secret(&id, field, custom_field_id.as_deref()))?;
    Ok(RevealedSecretDto::new(revealed, now_millis()?))
}

/// Generates the current TOTP code for an item.
///
/// The code is a short-lived secret: it is returned once and never logged.
#[tauri::command]
pub fn totp_code(
    runtime: State<'_, Arc<VaultRuntime>>,
    id: String,
) -> Result<RevealedSecretDto, CommandError> {
    runtime.touch();
    let timestamp = now_millis()? / 1_000;
    let code =
        runtime.with_vault(|vault| vault.generate_totp(&id, TotpAlgorithm::Sha1, timestamp))?;
    Ok(RevealedSecretDto {
        item_id: id,
        field: "totp".to_owned(),
        value: code.expose().to_owned(),
        expires_at: (timestamp + 30) * 1_000,
    })
}

/// Adds one custom field to an item.
#[tauri::command]
pub fn add_item_custom_field(
    runtime: State<'_, Arc<VaultRuntime>>,
    item_id: String,
    field: CustomFieldInputDto,
) -> Result<VaultItemDto, CommandError> {
    runtime.touch();
    runtime.with_vault_mut(|vault| vault.add_item_custom_field(&item_id, field.into()))?;
    let item = runtime.with_vault(|vault| vault.get_item(&item_id))?;
    Ok(item.into())
}

/// Removes one custom field from an item.
#[tauri::command]
pub fn remove_item_custom_field(
    runtime: State<'_, Arc<VaultRuntime>>,
    item_id: String,
    field_id: String,
) -> Result<(), CommandError> {
    runtime.touch();
    runtime.with_vault_mut(|vault| vault.remove_item_custom_field(&item_id, &field_id))?;
    Ok(())
}

fn invalid_input(message: String) -> CommandError {
    CommandError::new("invalid_input", message)
}

fn now_millis() -> Result<u64, CommandError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CommandError::internal())?;
    Ok(u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
}
