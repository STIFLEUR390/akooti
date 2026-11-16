use std::fmt;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto::{
    KEY_LEN, derive_key, encrypt_bytes, generate_key, master_key_aad, unwrap_master_key,
};
use crate::db;
use crate::error::VaultError;
use crate::folders;
use crate::header::{VaultHeader, VaultPaths};
use crate::items;
use crate::models::{
    Folder, FolderDraft, ItemFilter, RevealedSecret, SecretField, VaultItem, VaultItemDraft,
};

/// An unlocked SQLCipher vault.
pub struct Vault {
    paths: VaultPaths,
    connection: Connection,
    master_key: Zeroizing<[u8; KEY_LEN]>,
    auto_lock_minutes: u32,
}

impl fmt::Debug for Vault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Vault")
            .field("paths", &self.paths)
            .field("master_key", &"[REDACTED]")
            .field("auto_lock_minutes", &self.auto_lock_minutes)
            .finish()
    }
}

impl Vault {
    /// Creates a new encrypted vault and returns it unlocked.
    ///
    /// # Errors
    ///
    /// Returns an error when the paths already exist, derivation fails, or the
    /// initial schema cannot be persisted.
    pub fn create(paths: &VaultPaths, password: &str) -> Result<Self, VaultError> {
        Self::create_at(paths, password, SystemTime::now())
    }

    /// Opens an existing vault with its password.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::InvalidCredentials`] for a wrong password and
    /// [`VaultError::CorruptVault`] for inconsistent metadata.
    pub fn open(paths: &VaultPaths, password: &str) -> Result<Self, VaultError> {
        let header = VaultHeader::read(&paths.header)?;
        let database_key = derive_key(password, &header.salt, header.kdf)?;
        let mut connection = db::open_encrypted(&paths.database, &database_key)
            .map_err(|_| VaultError::InvalidCredentials)?;
        db::migrate(&mut connection).map_err(|_| VaultError::CorruptVault)?;
        let meta = db::read_meta(&connection)
            .map_err(|_| VaultError::InvalidCredentials)?
            .ok_or(VaultError::CorruptVault)?;
        if meta.salt != header.salt || meta.kdf != header.kdf {
            return Err(VaultError::CorruptVault);
        }
        let master_key = unwrap_master_key(
            &database_key,
            &meta.wrapped_key,
            &master_key_aad(&meta.vault_id),
        )?;
        Ok(Self {
            paths: paths.clone(),
            connection,
            master_key,
            auto_lock_minutes: meta.auto_lock_minutes,
        })
    }

    fn create_at(paths: &VaultPaths, password: &str, now: SystemTime) -> Result<Self, VaultError> {
        if paths.header.exists() || paths.database.exists() {
            return Err(VaultError::InvalidInput("vault paths already exist"));
        }
        if let Some(parent) = paths.header.parent() {
            fs::create_dir_all(parent)?;
        }
        let header = VaultHeader::new();
        let database_key = derive_key(password, &header.salt, header.kdf)?;
        let mut connection = db::open_encrypted(&paths.database, &database_key)?;
        let initialized = (|| {
            db::migrate(&mut connection)?;
            let vault_id = Uuid::new_v4().to_string();
            let master_key = generate_key();
            let wrapped_key = encrypt_bytes(
                &database_key,
                master_key.as_slice(),
                &master_key_aad(&vault_id),
            )?;
            db::insert_meta(
                &connection,
                &vault_id,
                &header.salt,
                header.kdf,
                &wrapped_key,
                15,
                unix_seconds(now)?,
            )?;
            header.create_new(&paths.header)?;
            Ok::<_, VaultError>(master_key)
        })();
        match initialized {
            Ok(master_key) => Ok(Self {
                paths: paths.clone(),
                connection,
                master_key,
                auto_lock_minutes: 15,
            }),
            Err(error) => {
                drop(connection);
                cleanup_failed_creation(paths, error)
            }
        }
    }

    /// Returns the vault paths.
    pub fn paths(&self) -> &VaultPaths {
        &self.paths
    }

    pub(crate) fn connection(&self) -> &Connection {
        &self.connection
    }

    pub(crate) fn master_key(&self) -> &[u8; KEY_LEN] {
        &self.master_key
    }

    /// Returns the configured inactivity timeout.
    pub fn auto_lock_minutes(&self) -> u32 {
        self.auto_lock_minutes
    }

    /// Persists a new inactivity timeout.
    ///
    /// # Errors
    ///
    /// Returns a database error when metadata cannot be updated.
    pub fn set_auto_lock_minutes(&mut self, minutes: u32) -> Result<(), VaultError> {
        db::set_auto_lock_minutes(&self.connection, minutes, unix_seconds(SystemTime::now())?)?;
        self.auto_lock_minutes = minutes;
        Ok(())
    }

    /// Lists folders with protected system folders first.
    pub fn list_folders(&self, include_deleted: bool) -> Result<Vec<Folder>, VaultError> {
        folders::list(&self.connection, include_deleted)
    }

    /// Creates a user folder.
    pub fn create_folder(&mut self, draft: FolderDraft) -> Result<Folder, VaultError> {
        folders::create(&self.connection, draft, unix_seconds(SystemTime::now())?)
    }

    /// Replaces a user folder's editable metadata.
    pub fn update_folder(&mut self, id: &str, draft: FolderDraft) -> Result<Folder, VaultError> {
        folders::update(
            &self.connection,
            id,
            draft,
            unix_seconds(SystemTime::now())?,
        )
    }

    /// Renames a user folder.
    pub fn rename_folder(&mut self, id: &str, name: &str) -> Result<Folder, VaultError> {
        folders::rename(&self.connection, id, name, unix_seconds(SystemTime::now())?)
    }

    /// Soft-deletes a user folder.
    pub fn delete_folder(&mut self, id: &str) -> Result<(), VaultError> {
        folders::soft_delete(&self.connection, id, unix_seconds(SystemTime::now())?)
    }

    /// Lists item metadata without secret plaintext.
    pub fn list_items(&self, filter: &ItemFilter) -> Result<Vec<VaultItem>, VaultError> {
        items::list(&self.connection, filter)
    }

    /// Fetches one item without secret plaintext.
    pub fn get_item(&self, id: &str) -> Result<VaultItem, VaultError> {
        items::get(&self.connection, id, false)
    }

    /// Creates an item and its relationships transactionally.
    pub fn create_item(&mut self, draft: VaultItemDraft) -> Result<VaultItem, VaultError> {
        items::create(
            &mut self.connection,
            &self.master_key,
            draft,
            unix_seconds(SystemTime::now())?,
        )
    }

    /// Replaces an item and its relationships transactionally.
    pub fn update_item(
        &mut self,
        id: &str,
        draft: VaultItemDraft,
    ) -> Result<VaultItem, VaultError> {
        items::update(
            &mut self.connection,
            &self.master_key,
            id,
            draft,
            unix_seconds(SystemTime::now())?,
        )
    }

    /// Soft-deletes an item.
    pub fn delete_item(&mut self, id: &str) -> Result<(), VaultError> {
        items::soft_delete(&self.connection, id, unix_seconds(SystemTime::now())?)
    }

    /// Permanently removes an item and cascades relationship rows.
    pub fn delete_item_permanently(&mut self, id: &str) -> Result<(), VaultError> {
        items::delete_permanently(&self.connection, id)
    }

    /// Adds an item to a folder without creating a duplicate link.
    pub fn add_item_to_folder(&mut self, item_id: &str, folder_id: &str) -> Result<(), VaultError> {
        items::add_to_folder(
            &self.connection,
            item_id,
            folder_id,
            unix_seconds(SystemTime::now())?,
        )
    }

    /// Removes one item/folder link.
    pub fn remove_item_from_folder(
        &mut self,
        item_id: &str,
        folder_id: &str,
    ) -> Result<(), VaultError> {
        items::remove_from_folder(&self.connection, item_id, folder_id)
    }

    /// Reveals a secret for an explicit local UI operation.
    pub fn reveal_secret(
        &self,
        item_id: &str,
        field: SecretField,
        custom_field_id: Option<&str>,
    ) -> Result<RevealedSecret, VaultError> {
        items::reveal(
            &self.connection,
            &self.master_key,
            item_id,
            field,
            custom_field_id,
        )
    }
}

pub(crate) fn unix_seconds(time: SystemTime) -> Result<i64, VaultError> {
    let duration = time
        .duration_since(UNIX_EPOCH)
        .map_err(|_| VaultError::InvalidInput("system clock is before Unix epoch"))?;
    i64::try_from(duration.as_secs()).map_err(|_| VaultError::InvalidInput("timestamp overflow"))
}

fn cleanup_failed_creation(paths: &VaultPaths, original: VaultError) -> Result<Vault, VaultError> {
    for path in [&paths.header, &paths.database] {
        if path.exists()
            && let Err(error) = fs::remove_file(path)
        {
            return Err(VaultError::Io(error));
        }
    }
    Err(original)
}
