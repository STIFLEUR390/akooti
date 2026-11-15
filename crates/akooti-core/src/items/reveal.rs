use rusqlite::{Connection, OptionalExtension, params};

use crate::crypto::{EncryptedValue, KEY_LEN, custom_field_aad, decrypt_string, secret_field_aad};
use crate::error::VaultError;
use crate::models::{RevealedSecret, SecretField};

pub(crate) fn reveal(
    connection: &Connection,
    master_key: &[u8; KEY_LEN],
    item_id: &str,
    field: SecretField,
    custom_field_id: Option<&str>,
) -> Result<RevealedSecret, VaultError> {
    match field {
        SecretField::CustomField => reveal_custom_field(
            connection,
            master_key,
            item_id,
            custom_field_id.ok_or(VaultError::InvalidInput(
                "custom field identifier is required",
            ))?,
        ),
        SecretField::Password | SecretField::Totp | SecretField::Notes => {
            reveal_item_field(connection, master_key, item_id, field)
        }
    }
}

fn reveal_item_field(
    connection: &Connection,
    master_key: &[u8; KEY_LEN],
    item_id: &str,
    field: SecretField,
) -> Result<RevealedSecret, VaultError> {
    let (cipher_column, nonce_column, field_name) = match field {
        SecretField::Password => ("encrypted_password", "password_nonce", "password"),
        SecretField::Totp => ("encrypted_totp_secret", "totp_nonce", "totp"),
        SecretField::Notes => ("encrypted_notes", "notes_nonce", "notes"),
        SecretField::CustomField => {
            return Err(VaultError::InvalidInput(
                "custom field must use its own lookup",
            ));
        }
    };
    let sql = format!(
        "SELECT {cipher_column}, {nonce_column} FROM vault_items
         WHERE id = ?1 AND deleted_at IS NULL"
    );
    let row: Option<(Vec<u8>, Vec<u8>)> = connection
        .query_row(&sql, [item_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .optional()?;
    let (ciphertext, nonce) = row.ok_or(VaultError::NotFound)?;
    let encrypted = encrypted_value(ciphertext, nonce)?;
    let aad = secret_field_aad(item_id, field_name);
    Ok(RevealedSecret {
        item_id: item_id.to_owned(),
        field,
        value: decrypt_string(master_key, &encrypted, &aad)?,
    })
}

fn reveal_custom_field(
    connection: &Connection,
    master_key: &[u8; KEY_LEN],
    item_id: &str,
    field_id: &str,
) -> Result<RevealedSecret, VaultError> {
    let row: Option<(Vec<u8>, Vec<u8>)> = connection
        .query_row(
            "SELECT f.encrypted_value, f.value_nonce
             FROM item_custom_fields f
             JOIN vault_items i ON i.id = f.item_id
             WHERE f.id = ?1 AND f.item_id = ?2 AND i.deleted_at IS NULL",
            params![field_id, item_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (ciphertext, nonce) = row.ok_or(VaultError::NotFound)?;
    let encrypted = encrypted_value(ciphertext, nonce)?;
    let aad = custom_field_aad(item_id, field_id);
    Ok(RevealedSecret {
        item_id: item_id.to_owned(),
        field: SecretField::CustomField,
        value: decrypt_string(master_key, &encrypted, &aad)?,
    })
}

fn encrypted_value(ciphertext: Vec<u8>, nonce: Vec<u8>) -> Result<EncryptedValue, VaultError> {
    let nonce = nonce.try_into().map_err(|_| VaultError::CorruptVault)?;
    Ok(EncryptedValue { ciphertext, nonce })
}
