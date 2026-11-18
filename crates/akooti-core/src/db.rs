use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use zeroize::Zeroizing;

use crate::crypto::{EncryptedValue, KEY_LEN, KdfParams, SALT_LEN};
use crate::error::VaultError;

const MIGRATION_V1: &str = include_str!("../migrations/001_init.sql");
const MIGRATION_V2: &str = include_str!("../migrations/002_sync_backup.sql");
const CURRENT_SCHEMA_VERSION: i64 = 2;

pub(crate) struct VaultMeta {
    pub(crate) vault_id: String,
    pub(crate) salt: [u8; SALT_LEN],
    pub(crate) kdf: KdfParams,
    pub(crate) wrapped_key: EncryptedValue,
    pub(crate) auto_lock_minutes: u32,
}

pub(crate) fn open_encrypted(path: &Path, key: &[u8; KEY_LEN]) -> Result<Connection, VaultError> {
    let connection = Connection::open(path)?;
    let encoded = Zeroizing::new(hex::encode(key));
    let pragma = Zeroizing::new(format!("PRAGMA key = \"x'{}'\";", encoded.as_str()));
    connection.execute_batch(&pragma)?;
    configure(&connection)?;
    Ok(connection)
}

fn configure(connection: &Connection) -> Result<(), VaultError> {
    connection.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA cipher_memory_security = ON;
         PRAGMA busy_timeout = 5000;",
    )?;
    let _mode: String = connection.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
    Ok(())
}

pub(crate) fn migrate(connection: &mut Connection) -> Result<(), VaultError> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > CURRENT_SCHEMA_VERSION {
        return Err(VaultError::CorruptVault);
    }
    if version < 1 {
        apply(connection, MIGRATION_V1, 1)?;
    }
    if version < 2 {
        apply(connection, MIGRATION_V2, 2)?;
    }
    ensure_system_folders(connection)?;
    Ok(())
}

fn apply(connection: &mut Connection, migration: &str, version: i64) -> Result<(), VaultError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(migration)?;
    transaction.pragma_update(None, "user_version", version)?;
    transaction.commit()?;
    Ok(())
}

fn ensure_system_folders(connection: &Connection) -> Result<(), VaultError> {
    connection.execute_batch(
        "INSERT OR IGNORE INTO folders
           (id, name, slug, icon, color, is_system, created_at, updated_at)
         VALUES
           ('00000000-0000-4000-8000-000000000001', 'Personnel', 'personal', 'user', '#3b82f6', 1, strftime('%s','now'), strftime('%s','now')),
           ('00000000-0000-4000-8000-000000000002', 'Dev Local', 'dev-local', 'terminal', '#10b981', 1, strftime('%s','now'), strftime('%s','now')),
           ('00000000-0000-4000-8000-000000000003', 'Agents & MCP', 'agents-mcp', 'bot', '#8b5cf6', 1, strftime('%s','now'), strftime('%s','now'));",
    )?;
    Ok(())
}

pub(crate) fn insert_meta(
    connection: &Connection,
    vault_id: &str,
    salt: &[u8; SALT_LEN],
    kdf: KdfParams,
    wrapped_key: &EncryptedValue,
    auto_lock_minutes: u32,
    now: i64,
) -> Result<(), VaultError> {
    connection.execute(
        "INSERT INTO vault_meta (
           id, vault_id, kdf_algorithm, kdf_salt, kdf_memory_kib, kdf_iterations,
           kdf_parallelism, wrapped_master_key, wrapped_key_nonce, auto_lock_minutes,
           created_at, updated_at
         ) VALUES (1, ?1, 'argon2id', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
        params![
            vault_id,
            salt.as_slice(),
            kdf.memory_kib,
            kdf.iterations,
            kdf.parallelism,
            wrapped_key.ciphertext,
            wrapped_key.nonce,
            auto_lock_minutes,
            now,
        ],
    )?;
    Ok(())
}

pub(crate) fn read_meta(connection: &Connection) -> Result<Option<VaultMeta>, VaultError> {
    connection
        .query_row(
            "SELECT vault_id, kdf_salt, kdf_memory_kib, kdf_iterations, kdf_parallelism,
                    wrapped_master_key, wrapped_key_nonce, auto_lock_minutes
             FROM vault_meta WHERE id = 1",
            [],
            |row| {
                let salt: Vec<u8> = row.get(1)?;
                let nonce: Vec<u8> = row.get(6)?;
                let salt = salt.try_into().map_err(|_| rusqlite::Error::InvalidQuery)?;
                let nonce = nonce
                    .try_into()
                    .map_err(|_| rusqlite::Error::InvalidQuery)?;
                Ok(VaultMeta {
                    vault_id: row.get(0)?,
                    salt,
                    kdf: KdfParams {
                        memory_kib: row.get(2)?,
                        iterations: row.get(3)?,
                        parallelism: row.get(4)?,
                    },
                    wrapped_key: EncryptedValue {
                        ciphertext: row.get(5)?,
                        nonce,
                    },
                    auto_lock_minutes: row.get(7)?,
                })
            },
        )
        .optional()
        .map_err(VaultError::from)
}

pub(crate) fn set_auto_lock_minutes(
    connection: &Connection,
    minutes: u32,
    now: i64,
) -> Result<(), VaultError> {
    connection.execute(
        "UPDATE vault_meta SET auto_lock_minutes = ?1, updated_at = ?2 WHERE id = 1",
        params![minutes, now],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn connection_enables_required_pragmas() {
        let temp = tempdir().expect("temporary directory");
        let path = temp.path().join("vault.db");
        let connection = open_encrypted(&path, &[3_u8; KEY_LEN]).expect("open");
        let foreign_keys: i64 = connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .expect("foreign keys");
        let journal: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("journal mode");
        assert_eq!(foreign_keys, 1);
        assert_eq!(journal, "wal");
    }

    #[test]
    fn migrations_are_idempotent_and_seed_once() {
        let temp = tempdir().expect("temporary directory");
        let path = temp.path().join("vault.db");
        let mut connection = open_encrypted(&path, &[5_u8; KEY_LEN]).expect("open");
        migrate(&mut connection).expect("first migration");
        migrate(&mut connection).expect("second migration");
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("version");
        let folders: i64 = connection
            .query_row("SELECT COUNT(*) FROM folders", [], |row| row.get(0))
            .expect("folders");
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        assert_eq!(folders, 3);
    }

    #[test]
    fn deleting_item_cascades_relationship_rows() {
        let temp = tempdir().expect("temporary directory");
        let path = temp.path().join("vault.db");
        let mut connection = open_encrypted(&path, &[7_u8; KEY_LEN]).expect("open");
        migrate(&mut connection).expect("migrate");
        connection
            .execute(
                "INSERT INTO vault_items (
                   id, item_type, title, created_at, updated_at
                 ) VALUES ('item', 'login', 'Title', 1, 1)",
                [],
            )
            .expect("item");
        connection
            .execute(
                "INSERT INTO item_folders (item_id, folder_id, added_at)
                 VALUES ('item', '00000000-0000-4000-8000-000000000001', 1)",
                [],
            )
            .expect("folder link");
        connection
            .execute(
                "INSERT INTO item_uris (id, item_id, uri) VALUES ('uri', 'item', 'example.test')",
                [],
            )
            .expect("uri");
        connection
            .execute(
                "INSERT INTO item_custom_fields (
                   id, item_id, field_name, encrypted_value, value_nonce
                 ) VALUES ('field', 'item', 'TOKEN', X'01', X'020202020202020202020202020202020202020202020202')",
                [],
            )
            .expect("field");

        connection
            .execute("DELETE FROM vault_items WHERE id = 'item'", [])
            .expect("delete");

        for table in ["item_folders", "item_uris", "item_custom_fields"] {
            let count: i64 = connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .expect("count");
            assert_eq!(count, 0, "{table}");
        }
    }
}
