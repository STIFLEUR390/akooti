use std::fs;
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::json;
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use crate::crypto::{derive_key, master_key_aad, unwrap_master_key};
use crate::db;
use crate::error::VaultError;
use crate::header::VaultHeader;
use crate::models::{ItemFilter, SecretField, VaultItem};
use crate::snapshot::{SNAPSHOT_VERSION, SnapshotHeader, encode};
use crate::vault::Vault;

impl Vault {
    /// Writes a `.akooti` snapshot next to the vault files it copies.
    ///
    /// The database is checkpointed first so the single file carries every
    /// committed transaction. The result is encrypted exactly like the vault:
    /// there is no second key to protect.
    ///
    /// # Errors
    ///
    /// Returns a database or IO error when the checkpoint or the copy fails.
    pub fn write_snapshot(&self, path: &Path, now: i64) -> Result<u64, VaultError> {
        self.connection()
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
        let meta = db::read_meta(self.connection())?.ok_or(VaultError::CorruptVault)?;
        let header = SnapshotHeader {
            version: SNAPSHOT_VERSION,
            created_at: now,
            core_version: crate::VERSION.to_owned(),
            vault_id: meta.vault_id,
        };
        let bytes = encode(
            &header,
            &fs::read(&self.paths().header)?,
            &fs::read(&self.paths().database)?,
        );
        let temp = path.with_extension("akooti-tmp");
        fs::write(&temp, &bytes)?;
        fs::rename(&temp, path)?;
        Ok(u64::try_from(bytes.len()).unwrap_or(u64::MAX))
    }

    /// Checks a master password against the bootstrap header and wrapped key.
    ///
    /// Used before a plaintext export: the caller must prove it knows the
    /// password, not merely that the vault is unlocked.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::InvalidCredentials`] for a wrong password.
    pub fn verify_master_password(&self, password: &str) -> Result<(), VaultError> {
        let header = VaultHeader::read(&self.paths().header)?;
        let database_key = derive_key(password, &header.salt, header.kdf)?;
        let meta = db::read_meta(self.connection())?.ok_or(VaultError::CorruptVault)?;
        let unwrapped = unwrap_master_key(
            &database_key,
            &meta.wrapped_key,
            &master_key_aad(&meta.vault_id),
        )
        .map_err(|_| VaultError::InvalidCredentials)?;
        if unwrapped.ct_eq(&self.master_key()[..]).unwrap_u8() == 1 {
            Ok(())
        } else {
            Err(VaultError::InvalidCredentials)
        }
    }

    /// Exports the vault as plaintext JSON, after re-checking the password.
    ///
    /// The result holds passwords in the clear: it is zeroized when dropped and
    /// must never be logged. Passkeys are listed for recovery, but their private
    /// scalar is not: the `.akooti` snapshot is the restorable backup.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::InvalidCredentials`] for a wrong password.
    pub fn export_json(&self, password: &str, now: i64) -> Result<Zeroizing<String>, VaultError> {
        self.verify_master_password(password)?;
        let meta = db::read_meta(self.connection())?.ok_or(VaultError::CorruptVault)?;
        let folders = self.list_folders(false)?;
        let items = self.list_items(&ItemFilter::default())?;

        let folder_values = folders
            .into_iter()
            .map(|folder| {
                json!({
                    "id": folder.id,
                    "parent_id": folder.parent_id,
                    "name": folder.name,
                    "slug": folder.slug,
                    "icon": folder.icon,
                    "color": folder.color,
                    "is_system": folder.is_system,
                    "created_at": folder.created_at,
                    "updated_at": folder.updated_at,
                })
            })
            .collect::<Vec<_>>();

        let mut item_values = Vec::with_capacity(items.len());
        for item in items {
            let password = if item.has_password {
                self.reveal_secret(&item.id, SecretField::Password, None)
                    .ok()
                    .map(|secret| secret.value.expose().to_owned())
            } else {
                None
            };
            let totp_secret = if item.has_totp {
                self.reveal_secret(&item.id, SecretField::Totp, None)
                    .ok()
                    .map(|secret| secret.value.expose().to_owned())
            } else {
                None
            };
            let notes = if item.has_notes {
                self.reveal_secret(&item.id, SecretField::Notes, None)
                    .ok()
                    .map(|secret| secret.value.expose().to_owned())
            } else {
                None
            };
            let mut custom_fields = Vec::with_capacity(item.custom_fields.len());
            for field in &item.custom_fields {
                let value = self
                    .reveal_secret(&item.id, SecretField::CustomField, Some(&field.id))
                    .ok()
                    .map(|secret| secret.value.expose().to_owned());
                custom_fields.push(json!({
                    "field_name": field.field_name,
                    "is_secret": field.is_secret,
                    "sort_order": field.sort_order,
                    "value": value,
                }));
            }
            item_values.push(item_json(
                &item,
                password,
                totp_secret,
                notes,
                custom_fields,
            ));
        }

        let mut rules = Vec::new();
        let mut statement = self.connection().prepare(
            "SELECT id, pattern, pattern_type, target_folder_id, is_default, enabled, created_at
             FROM local_routing_rules ORDER BY created_at, id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "pattern": row.get::<_, String>(1)?,
                "pattern_type": row.get::<_, String>(2)?,
                "target_folder_id": row.get::<_, String>(3)?,
                "is_default": row.get::<_, bool>(4)?,
                "enabled": row.get::<_, bool>(5)?,
                "created_at": row.get::<_, i64>(6)?,
            }))
        })?;
        for rule in rows {
            rules.push(rule?);
        }

        let mut passkeys = Vec::new();
        let mut statement = self.connection().prepare(
            "SELECT rp_id, rp_name, user_name, credential_id, sign_count, created_at, last_used_at
             FROM passkeys WHERE deleted_at IS NULL ORDER BY created_at, id",
        )?;
        let rows = statement.query_map([], |row| {
            let credential_id: Vec<u8> = row.get(3)?;
            Ok(json!({
                "rp_id": row.get::<_, String>(0)?,
                "rp_name": row.get::<_, String>(1)?,
                "user_name": row.get::<_, String>(2)?,
                "credential_id": URL_SAFE_NO_PAD.encode(credential_id),
                "sign_count": row.get::<_, i64>(4)?,
                "created_at": row.get::<_, i64>(5)?,
                "last_used_at": row.get::<_, Option<i64>>(6)?,
                "private_key_exported": false,
            }))
        })?;
        for passkey in rows {
            passkeys.push(passkey?);
        }

        let document = json!({
            "format": "akooti-export",
            "version": 1,
            "exported_at": now,
            "vault_id": meta.vault_id,
            "folders": folder_values,
            "items": item_values,
            "routing_rules": rules,
            "passkeys": passkeys,
        });
        Ok(Zeroizing::new(
            serde_json::to_string_pretty(&document).map_err(|_| VaultError::CorruptVault)?,
        ))
    }
}

fn item_json(
    item: &VaultItem,
    password: Option<String>,
    totp_secret: Option<String>,
    notes: Option<String>,
    custom_fields: Vec<serde_json::Value>,
) -> serde_json::Value {
    json!({
        "id": item.id,
        "item_type": item.item_type.as_str(),
        "title": item.title,
        "primary_url": item.primary_url,
        "username": item.username,
        "password": password,
        "totp_secret": totp_secret,
        "notes": notes,
        "selectors": {
            "username": item.selectors.username,
            "password": item.selectors.password,
            "totp": item.selectors.totp,
            "submit": item.selectors.submit,
        },
        "favorite": item.favorite,
        "expires_at": item.expires_at,
        "created_by_agent": item.created_by_agent,
        "created_at": item.created_at,
        "updated_at": item.updated_at,
        "folder_ids": item.folder_ids,
        "uris": item.uris.iter().map(|uri| json!({
            "uri": uri.uri,
            "match_type": uri.match_type.as_str(),
        })).collect::<Vec<_>>(),
        "custom_fields": custom_fields,
    })
}
