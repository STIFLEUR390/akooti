use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::crypto::{EncryptedValue, KEY_LEN, decrypt_string};
use crate::error::VaultError;
use crate::models::SecretString;

type StoredPassword = (Option<Vec<u8>>, Option<Vec<u8>>);

pub(crate) fn archive_if_changed(
    connection: &Connection,
    key: &[u8; KEY_LEN],
    item_id: &str,
    new_password: Option<&SecretString>,
    now: i64,
) -> Result<(), VaultError> {
    let Some(new_password) = new_password else {
        return Ok(());
    };
    let previous: Option<StoredPassword> = connection
        .query_row(
            "SELECT encrypted_password, password_nonce
             FROM vault_items WHERE id = ?1 AND deleted_at IS NULL",
            [item_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((ciphertext, nonce)) = previous else {
        return Ok(());
    };
    let (Some(ciphertext), Some(nonce)) = (ciphertext, nonce) else {
        return Ok(());
    };
    let nonce = nonce.try_into().map_err(|_| VaultError::CorruptVault)?;
    let old_password = decrypt_string(
        key,
        &EncryptedValue { ciphertext, nonce },
        &crate::crypto::secret_field_aad(item_id, "password"),
    )?;
    if old_password.expose() == new_password.expose() {
        return Ok(());
    }

    let history_password =
        crate::crypto::encrypt_string(key, old_password.expose(), &history_aad(item_id, now))?;
    connection.execute(
        "INSERT INTO password_history (
           id, item_id, encrypted_password, password_nonce, changed_at
         ) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            Uuid::new_v4().to_string(),
            item_id,
            history_password.ciphertext,
            history_password.nonce,
            now,
        ],
    )?;
    Ok(())
}

fn history_aad(item_id: &str, changed_at: i64) -> Vec<u8> {
    format!("akooti:v1:item:{item_id}:password-history:{changed_at}").into_bytes()
}

pub(crate) fn count(connection: &Connection, item_id: &str) -> Result<usize, VaultError> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM password_history WHERE item_id = ?1",
        [item_id],
        |row| row.get(0),
    )?;
    usize::try_from(count).map_err(|_| VaultError::CorruptVault)
}
