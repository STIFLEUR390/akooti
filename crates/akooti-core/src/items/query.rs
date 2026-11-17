use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::error::VaultError;
use crate::models::{
    CustomField, ItemFilter, ItemType, ItemUri, Selectors, UriMatchType, VaultItem,
};

const ITEM_COLUMNS: &str = "id, item_type, title, primary_url, username,
    username_selector, password_selector, totp_selector, submit_selector,
    zxcvbn_score, password_sha1_prefix, is_compromised, password_updated_at,
    created_by_agent, expires_at, favorite, created_at, updated_at, deleted_at";

/// Secret **presence** flags. Booleans only: the ciphertext never leaves the
/// row, so the UI can decide whether to offer a reveal control without holding
/// — or probing for — a secret.
const SECRET_FLAGS: &str = "(encrypted_password IS NOT NULL) AS has_password,
    (encrypted_totp_secret IS NOT NULL) AS has_totp,
    (encrypted_notes IS NOT NULL) AS has_notes";

/// Same flags, qualified for the `vault_items v` alias used by [`list`].
const SECRET_FLAGS_ALIASED: &str = "(v.encrypted_password IS NOT NULL) AS has_password,
    (v.encrypted_totp_secret IS NOT NULL) AS has_totp,
    (v.encrypted_notes IS NOT NULL) AS has_notes";

pub(crate) fn list(
    connection: &Connection,
    filter: &ItemFilter,
) -> Result<Vec<VaultItem>, VaultError> {
    let deleted_clause = if filter.include_deleted {
        ""
    } else {
        " AND v.deleted_at IS NULL"
    };
    let folder_clause = if filter.folder_id.is_some() {
        " AND EXISTS (
            SELECT 1 FROM item_folders f
            WHERE f.item_id = v.id AND f.folder_id = ?1
          )"
    } else {
        ""
    };
    let sql = format!(
        "SELECT v.{columns}, {SECRET_FLAGS_ALIASED} FROM vault_items v
         WHERE 1 = 1{deleted_clause}{folder_clause}
         ORDER BY v.updated_at DESC, v.id",
        columns = ITEM_COLUMNS.replace(", ", ", v.")
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = match filter.folder_id.as_deref() {
        Some(folder_id) => statement.query_map([folder_id], item_from_row)?,
        None => statement.query_map([], item_from_row)?,
    };
    let metadata = rows.collect::<Result<Vec<_>, _>>()?;
    metadata
        .into_iter()
        .map(|item| hydrate(connection, item))
        .collect()
}

pub(crate) fn get(
    connection: &Connection,
    id: &str,
    include_deleted: bool,
) -> Result<VaultItem, VaultError> {
    let sql = if include_deleted {
        format!("SELECT {ITEM_COLUMNS}, {SECRET_FLAGS} FROM vault_items WHERE id = ?1")
    } else {
        format!(
            "SELECT {ITEM_COLUMNS}, {SECRET_FLAGS} FROM vault_items \
             WHERE id = ?1 AND deleted_at IS NULL"
        )
    };
    let item = connection
        .query_row(&sql, [id], item_from_row)
        .optional()?
        .ok_or(VaultError::NotFound)?;
    hydrate(connection, item)
}

pub(crate) fn soft_delete(connection: &Connection, id: &str, now: i64) -> Result<(), VaultError> {
    let changed = connection.execute(
        "UPDATE vault_items SET deleted_at = ?2, updated_at = ?2
         WHERE id = ?1 AND deleted_at IS NULL",
        params![id, now],
    )?;
    if changed == 0 {
        return Err(VaultError::NotFound);
    }
    Ok(())
}

pub(crate) fn delete_permanently(connection: &Connection, id: &str) -> Result<(), VaultError> {
    let changed = connection.execute("DELETE FROM vault_items WHERE id = ?1", [id])?;
    if changed == 0 {
        return Err(VaultError::NotFound);
    }
    Ok(())
}

pub(crate) fn add_to_folder(
    connection: &Connection,
    item_id: &str,
    folder_id: &str,
    now: i64,
) -> Result<(), VaultError> {
    get(connection, item_id, false)?;
    crate::folders::get(connection, folder_id)?;
    connection.execute(
        "INSERT OR IGNORE INTO item_folders (item_id, folder_id, added_at)
         VALUES (?1, ?2, ?3)",
        params![item_id, folder_id, now],
    )?;
    Ok(())
}

pub(crate) fn remove_from_folder(
    connection: &Connection,
    item_id: &str,
    folder_id: &str,
) -> Result<(), VaultError> {
    connection.execute(
        "DELETE FROM item_folders WHERE item_id = ?1 AND folder_id = ?2",
        params![item_id, folder_id],
    )?;
    Ok(())
}

fn item_from_row(row: &Row<'_>) -> rusqlite::Result<VaultItem> {
    let item_type: String = row.get(1)?;
    Ok(VaultItem {
        id: row.get(0)?,
        item_type: ItemType::parse(&item_type).ok_or(rusqlite::Error::InvalidQuery)?,
        title: row.get(2)?,
        primary_url: row.get(3)?,
        username: row.get(4)?,
        selectors: Selectors {
            username: row.get(5)?,
            password: row.get(6)?,
            totp: row.get(7)?,
            submit: row.get(8)?,
        },
        zxcvbn_score: row.get(9)?,
        password_sha1_prefix: row.get(10)?,
        is_compromised: row.get(11)?,
        password_updated_at: row.get(12)?,
        created_by_agent: row.get(13)?,
        expires_at: row.get(14)?,
        favorite: row.get(15)?,
        folder_ids: Vec::new(),
        uris: Vec::new(),
        custom_fields: Vec::new(),
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
        deleted_at: row.get(18)?,
        has_password: row.get(19)?,
        has_totp: row.get(20)?,
        has_notes: row.get(21)?,
    })
}

fn hydrate(connection: &Connection, mut item: VaultItem) -> Result<VaultItem, VaultError> {
    let mut folder_statement = connection.prepare(
        "SELECT folder_id FROM item_folders WHERE item_id = ?1 ORDER BY added_at, folder_id",
    )?;
    item.folder_ids = folder_statement
        .query_map([&item.id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut uri_statement =
        connection.prepare("SELECT id, uri, match_type FROM item_uris WHERE item_id = ?1")?;
    item.uris = uri_statement
        .query_map([&item.id], |row| {
            let match_type: String = row.get(2)?;
            Ok(ItemUri {
                id: row.get(0)?,
                uri: row.get(1)?,
                match_type: UriMatchType::parse(&match_type)
                    .ok_or(rusqlite::Error::InvalidQuery)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut field_statement = connection.prepare(
        "SELECT id, field_name, is_secret, sort_order
         FROM item_custom_fields WHERE item_id = ?1 ORDER BY sort_order, field_name",
    )?;
    item.custom_fields = field_statement
        .query_map([&item.id], |row| {
            Ok(CustomField {
                id: row.get(0)?,
                field_name: row.get(1)?,
                is_secret: row.get(2)?,
                sort_order: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(item)
}
