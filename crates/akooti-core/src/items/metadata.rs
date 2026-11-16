//! Metadata-only edits that never require the caller to hold a plaintext
//! secret.
//!
//! The desktop UI must be able to rename an item, move it between folders, or
//! edit its autofill selectors without learning the stored password. These
//! helpers therefore touch a single column family each and leave every other
//! ciphertext byte-for-byte untouched.

use rusqlite::{Connection, params};
use uuid::Uuid;

use crate::crypto::{EncryptedValue, KEY_LEN, custom_field_aad, encrypt_string, secret_field_aad};
use crate::error::VaultError;
use crate::models::{CustomFieldInput, ItemMetadataDraft, SecretField, SecretString};
use crate::validation;

use super::history::archive_if_changed;

pub(crate) fn update_metadata(
    connection: &Connection,
    id: &str,
    draft: &ItemMetadataDraft,
    now: i64,
) -> Result<(), VaultError> {
    if draft.title.trim().is_empty() {
        return Err(VaultError::InvalidInput("item title is empty"));
    }
    validation::validate_selectors(&draft.selectors)?;
    validation::validate_uris(&draft.uris)?;

    let transaction = connection.unchecked_transaction()?;
    let changed = transaction.execute(
        "UPDATE vault_items SET
           item_type = ?2, title = ?3, primary_url = ?4, username = ?5,
           username_selector = ?6, password_selector = ?7,
           totp_selector = ?8, submit_selector = ?9,
           expires_at = ?10, favorite = ?11, updated_at = ?12
         WHERE id = ?1 AND deleted_at IS NULL",
        params![
            id,
            draft.item_type.as_str(),
            draft.title,
            draft.primary_url,
            draft.username,
            draft.selectors.username,
            draft.selectors.password,
            draft.selectors.totp,
            draft.selectors.submit,
            draft.expires_at,
            draft.favorite,
            now,
        ],
    )?;
    if changed == 0 {
        return Err(VaultError::NotFound);
    }
    replace_folders(&transaction, id, &draft.folder_ids, now)?;
    replace_uris(&transaction, id, &draft.uris)?;
    crate::routing::apply_to_item(&transaction, id, now)?;
    transaction.commit()?;
    Ok(())
}

/// Writes or clears one encrypted field, archiving a replaced password.
pub(crate) fn set_secret(
    connection: &mut Connection,
    key: &[u8; KEY_LEN],
    id: &str,
    field: SecretField,
    value: Option<&SecretString>,
    now: i64,
) -> Result<(), VaultError> {
    let (column, nonce_column) = match field {
        SecretField::Password => ("encrypted_password", "password_nonce"),
        SecretField::Totp => ("encrypted_totp_secret", "totp_nonce"),
        SecretField::Notes => ("encrypted_notes", "notes_nonce"),
        SecretField::CustomField => {
            return Err(VaultError::InvalidInput(
                "custom field values are addressed by identifier",
            ));
        }
    };
    let transaction = connection.transaction()?;
    super::query::get(&transaction, id, false)?;
    if field == SecretField::Password {
        archive_if_changed(&transaction, key, id, value, now)?;
    }
    let encrypted = value
        .map(|secret| encrypt_string(key, secret.expose(), &secret_field_aad(id, field.as_str())))
        .transpose()?;
    let ciphertext = encrypted.as_ref().map(|value| value.ciphertext.as_slice());
    let nonce = encrypted.as_ref().map(|value| value.nonce);
    if field == SecretField::Password {
        transaction.execute(
            &format!(
                "UPDATE vault_items SET {column} = ?2, {nonce_column} = ?3, updated_at = ?4,
                   password_updated_at = ?5, zxcvbn_score = NULL,
                   password_sha1_prefix = NULL, is_compromised = 0
                 WHERE id = ?1 AND deleted_at IS NULL"
            ),
            params![id, ciphertext, nonce, now, value.map(|_| now)],
        )?;
    } else {
        transaction.execute(
            &format!(
                "UPDATE vault_items SET {column} = ?2, {nonce_column} = ?3, updated_at = ?4
                 WHERE id = ?1 AND deleted_at IS NULL"
            ),
            params![id, ciphertext, nonce, now],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

pub(crate) fn add_custom_field(
    connection: &Connection,
    key: &[u8; KEY_LEN],
    item_id: &str,
    field: &CustomFieldInput,
) -> Result<String, VaultError> {
    validation::validate_custom_fields(std::slice::from_ref(field))?;
    super::query::get(connection, item_id, false)?;
    let id = Uuid::new_v4().to_string();
    let encrypted: EncryptedValue =
        encrypt_string(key, field.value.expose(), &custom_field_aad(item_id, &id))?;
    connection.execute(
        "INSERT INTO item_custom_fields (
           id, item_id, field_name, is_secret, encrypted_value, value_nonce, sort_order
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            item_id,
            field.field_name,
            field.is_secret,
            encrypted.ciphertext,
            encrypted.nonce,
            field.sort_order,
        ],
    )?;
    Ok(id)
}

pub(crate) fn remove_custom_field(
    connection: &Connection,
    item_id: &str,
    field_id: &str,
) -> Result<(), VaultError> {
    let changed = connection.execute(
        "DELETE FROM item_custom_fields WHERE id = ?1 AND item_id = ?2",
        params![field_id, item_id],
    )?;
    if changed == 0 {
        return Err(VaultError::NotFound);
    }
    Ok(())
}

fn replace_folders(
    connection: &Connection,
    item_id: &str,
    folder_ids: &[String],
    now: i64,
) -> Result<(), VaultError> {
    connection.execute("DELETE FROM item_folders WHERE item_id = ?1", [item_id])?;
    let mut seen = Vec::new();
    for folder_id in folder_ids {
        if seen.iter().any(|known| known == folder_id) {
            continue;
        }
        crate::folders::get(connection, folder_id)?;
        connection.execute(
            "INSERT INTO item_folders (item_id, folder_id, added_at) VALUES (?1, ?2, ?3)",
            params![item_id, folder_id, now],
        )?;
        seen.push(folder_id.clone());
    }
    Ok(())
}

fn replace_uris(
    connection: &Connection,
    item_id: &str,
    uris: &[crate::models::ItemUriInput],
) -> Result<(), VaultError> {
    connection.execute("DELETE FROM item_uris WHERE item_id = ?1", [item_id])?;
    for uri in uris {
        connection.execute(
            "INSERT INTO item_uris (id, item_id, uri, match_type) VALUES (?1, ?2, ?3, ?4)",
            params![
                Uuid::new_v4().to_string(),
                item_id,
                uri.uri,
                uri.match_type.as_str()
            ],
        )?;
    }
    Ok(())
}
