use std::collections::HashSet;

use rusqlite::{Connection, params};
use uuid::Uuid;

use crate::crypto::{EncryptedValue, KEY_LEN, custom_field_aad, encrypt_string, secret_field_aad};
use crate::error::VaultError;
use crate::models::{CustomFieldInput, ItemUriInput, SecretString, VaultItem, VaultItemDraft};

use super::query::get;

pub(crate) fn create(
    connection: &mut Connection,
    master_key: &[u8; KEY_LEN],
    draft: VaultItemDraft,
    now: i64,
) -> Result<VaultItem, VaultError> {
    validate_draft(&draft)?;
    let id = Uuid::new_v4().to_string();
    let transaction = connection.transaction()?;
    insert_row(&transaction, master_key, &id, &draft, now)?;
    replace_relations(&transaction, master_key, &id, &draft, now)?;
    transaction.commit()?;
    get(connection, &id, false)
}

pub(crate) fn update(
    connection: &mut Connection,
    master_key: &[u8; KEY_LEN],
    id: &str,
    draft: VaultItemDraft,
    now: i64,
) -> Result<VaultItem, VaultError> {
    validate_draft(&draft)?;
    get(connection, id, false)?;
    let transaction = connection.transaction()?;
    update_row(&transaction, master_key, id, &draft, now)?;
    replace_relations(&transaction, master_key, id, &draft, now)?;
    transaction.commit()?;
    get(connection, id, false)
}

fn insert_row(
    connection: &Connection,
    key: &[u8; KEY_LEN],
    id: &str,
    draft: &VaultItemDraft,
    now: i64,
) -> Result<(), VaultError> {
    let password = encrypt_optional(key, item_field_aad(id, "password"), draft.password.as_ref())?;
    let totp = encrypt_optional(key, item_field_aad(id, "totp"), draft.totp_secret.as_ref())?;
    let notes = encrypt_optional(key, item_field_aad(id, "notes"), draft.notes.as_ref())?;
    connection.execute(
        "INSERT INTO vault_items (
           id, item_type, title, primary_url, username,
           encrypted_password, password_nonce, encrypted_totp_secret, totp_nonce,
           encrypted_notes, notes_nonce, username_selector, password_selector,
           totp_selector, submit_selector, password_updated_at, created_by_agent,
           expires_at, favorite, created_at, updated_at
         ) VALUES (
           ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
           ?16, ?17, ?18, ?19, ?20, ?20
         )",
        params![
            id,
            draft.item_type.as_str(),
            draft.title,
            draft.primary_url,
            draft.username,
            password.as_ref().map(|value| value.ciphertext.as_slice()),
            password.as_ref().map(|value| value.nonce),
            totp.as_ref().map(|value| value.ciphertext.as_slice()),
            totp.as_ref().map(|value| value.nonce),
            notes.as_ref().map(|value| value.ciphertext.as_slice()),
            notes.as_ref().map(|value| value.nonce),
            draft.selectors.username,
            draft.selectors.password,
            draft.selectors.totp,
            draft.selectors.submit,
            draft.password.as_ref().map(|_| now),
            draft.created_by_agent,
            draft.expires_at,
            draft.favorite,
            now,
        ],
    )?;
    Ok(())
}

fn update_row(
    connection: &Connection,
    key: &[u8; KEY_LEN],
    id: &str,
    draft: &VaultItemDraft,
    now: i64,
) -> Result<(), VaultError> {
    let password = encrypt_optional(key, item_field_aad(id, "password"), draft.password.as_ref())?;
    let totp = encrypt_optional(key, item_field_aad(id, "totp"), draft.totp_secret.as_ref())?;
    let notes = encrypt_optional(key, item_field_aad(id, "notes"), draft.notes.as_ref())?;
    connection.execute(
        "UPDATE vault_items SET
           item_type = ?2, title = ?3, primary_url = ?4, username = ?5,
           encrypted_password = ?6, password_nonce = ?7,
           encrypted_totp_secret = ?8, totp_nonce = ?9,
           encrypted_notes = ?10, notes_nonce = ?11,
           username_selector = ?12, password_selector = ?13,
           totp_selector = ?14, submit_selector = ?15,
           zxcvbn_score = NULL, password_sha1_prefix = NULL, is_compromised = 0,
           password_updated_at = ?16, created_by_agent = ?17,
           expires_at = ?18, favorite = ?19, updated_at = ?20
         WHERE id = ?1 AND deleted_at IS NULL",
        params![
            id,
            draft.item_type.as_str(),
            draft.title,
            draft.primary_url,
            draft.username,
            password.as_ref().map(|value| value.ciphertext.as_slice()),
            password.as_ref().map(|value| value.nonce),
            totp.as_ref().map(|value| value.ciphertext.as_slice()),
            totp.as_ref().map(|value| value.nonce),
            notes.as_ref().map(|value| value.ciphertext.as_slice()),
            notes.as_ref().map(|value| value.nonce),
            draft.selectors.username,
            draft.selectors.password,
            draft.selectors.totp,
            draft.selectors.submit,
            draft.password.as_ref().map(|_| now),
            draft.created_by_agent,
            draft.expires_at,
            draft.favorite,
            now,
        ],
    )?;
    Ok(())
}

fn replace_relations(
    connection: &Connection,
    key: &[u8; KEY_LEN],
    item_id: &str,
    draft: &VaultItemDraft,
    now: i64,
) -> Result<(), VaultError> {
    connection.execute("DELETE FROM item_folders WHERE item_id = ?1", [item_id])?;
    connection.execute("DELETE FROM item_uris WHERE item_id = ?1", [item_id])?;
    connection.execute(
        "DELETE FROM item_custom_fields WHERE item_id = ?1",
        [item_id],
    )?;

    let mut folders = HashSet::new();
    for folder_id in &draft.folder_ids {
        if !folders.insert(folder_id) {
            continue;
        }
        crate::folders::get(connection, folder_id)?;
        connection.execute(
            "INSERT INTO item_folders (item_id, folder_id, added_at) VALUES (?1, ?2, ?3)",
            params![item_id, folder_id, now],
        )?;
    }
    for uri in &draft.uris {
        insert_uri(connection, item_id, uri)?;
    }
    for field in &draft.custom_fields {
        insert_custom_field(connection, key, item_id, field)?;
    }
    Ok(())
}

fn insert_uri(
    connection: &Connection,
    item_id: &str,
    uri: &ItemUriInput,
) -> Result<(), VaultError> {
    connection.execute(
        "INSERT INTO item_uris (id, item_id, uri, match_type) VALUES (?1, ?2, ?3, ?4)",
        params![
            Uuid::new_v4().to_string(),
            item_id,
            uri.uri,
            uri.match_type.as_str()
        ],
    )?;
    Ok(())
}

fn insert_custom_field(
    connection: &Connection,
    key: &[u8; KEY_LEN],
    item_id: &str,
    field: &CustomFieldInput,
) -> Result<(), VaultError> {
    let id = Uuid::new_v4().to_string();
    let aad = custom_field_aad(item_id, &id);
    let encrypted = encrypt_string(key, field.value.expose(), &aad)?;
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
    Ok(())
}

fn encrypt_optional(
    key: &[u8; KEY_LEN],
    aad: Vec<u8>,
    value: Option<&SecretString>,
) -> Result<Option<EncryptedValue>, VaultError> {
    value
        .map(|secret| encrypt_string(key, secret.expose(), &aad))
        .transpose()
}

fn item_field_aad(item_id: &str, field: &str) -> Vec<u8> {
    secret_field_aad(item_id, field)
}

fn validate_draft(draft: &VaultItemDraft) -> Result<(), VaultError> {
    if draft.title.trim().is_empty() {
        return Err(VaultError::InvalidInput("item title is empty"));
    }
    Ok(())
}
