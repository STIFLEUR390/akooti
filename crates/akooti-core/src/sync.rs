//! End-to-end encrypted record synchronisation.
//!
//! The server only ever sees an opaque envelope: every folder, item, passkey and
//! routing rule is serialized here, encrypted with the vault master key under an
//! AAD bound to its type and identifier, and pushed as `ciphertext` + `nonce`.
//! Conflicts resolve last-writer-wins on the record timestamp. A strictly older
//! record is skipped; on a tie the arriving record applies, so replaying a batch
//! is idempotent and a same-second deletion still propagates. Deletions travel as
//! tombstones (`is_deleted`) so a removal propagates without revealing what was
//! removed.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD as BASE64, URL_SAFE_NO_PAD};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto::{
    EncryptedValue, KEY_LEN, custom_field_aad, decrypt_bytes, encrypt_bytes, encrypt_string,
    secret_field_aad, sync_record_aad, sync_session_aad,
};
use crate::db;
use crate::error::VaultError;
use crate::models::SecretField;

/// Record kinds that are synchronised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyncRecordType {
    /// A folder, including system folders.
    Folder,
    /// A vault item with its relations and secrets.
    Item,
    /// A WebAuthn credential.
    Passkey,
    /// A `Dev Local` routing rule.
    RoutingRule,
}

impl SyncRecordType {
    /// Wire name stored in `record_type`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::Item => "item",
            Self::Passkey => "passkey",
            Self::RoutingRule => "routing_rule",
        }
    }

    /// Parses the wire name of a record type.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "folder" => Some(Self::Folder),
            "item" => Some(Self::Item),
            "passkey" => Some(Self::Passkey),
            "routing_rule" => Some(Self::RoutingRule),
            _ => None,
        }
    }
}

/// One encrypted record, exactly as it travels to and from Supabase.
#[derive(Debug, Clone)]
pub struct SyncRecord {
    /// Same identifier as in SQLCipher.
    pub id: String,
    /// Record kind.
    pub record_type: SyncRecordType,
    /// Last local change, Unix seconds (the LWW key).
    pub client_updated_at: i64,
    /// Tombstone: the record was deleted locally.
    pub is_deleted: bool,
    /// Base64 payload, encrypted with the master key.
    pub ciphertext: String,
    /// Base64 nonce.
    pub nonce: String,
}

/// Outcome of applying a batch of remote records.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncSummary {
    /// Records received.
    pub pulled: usize,
    /// Records actually applied.
    pub applied: usize,
    /// Records skipped because the local copy is newer or equal.
    pub skipped: usize,
    /// Records carrying a tombstone.
    pub tombstones: usize,
}

/// Local sync configuration. Never synced or exported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncSettings {
    /// Whether the scheduler may sync.
    pub enabled: bool,
    /// Supabase project URL.
    pub supabase_url: Option<String>,
    /// Public anon key (not a secret).
    pub anon_key: Option<String>,
    /// Supabase user identifier.
    pub user_id: Option<String>,
    /// This device's identifier, included in diagnostics only.
    pub device_id: String,
    /// Last `server_updated_at` received.
    pub cursor: Option<String>,
    /// Last sync time, Unix seconds.
    pub last_sync_at: Option<i64>,
    /// `success`, `error`, `offline`, ...
    pub last_status: Option<String>,
    /// Short non-secret diagnostic.
    pub last_error: Option<String>,
}

/// Supabase auth tokens, encrypted at rest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncSession {
    /// Bearer token used for REST and Realtime.
    pub access_token: String,
    /// Long-lived token used to refresh the access token.
    pub refresh_token: String,
    /// Access token expiry, Unix seconds.
    pub expires_at: i64,
}

/// The public cryptographic envelope, pushed once per user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncEnvelope {
    /// Vault identifier.
    pub vault_id: String,
    /// Base64 Argon2id salt.
    pub kdf_salt: String,
    /// Argon2id memory, KiB.
    pub kdf_memory_kib: i64,
    /// Argon2id iterations.
    pub kdf_iterations: i64,
    /// Argon2id parallelism.
    pub kdf_parallelism: i64,
    /// Base64 wrapped master key.
    pub wrapped_master_key: String,
    /// Base64 wrapped-key nonce.
    pub wrapped_key_nonce: String,
}

impl SyncEnvelope {
    /// JSON body accepted by the `akooti_vault_envelopes` upsert.
    #[must_use]
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "vault_id": self.vault_id,
            "kdf_algorithm": "argon2id",
            "kdf_salt": self.kdf_salt,
            "kdf_memory_kib": self.kdf_memory_kib,
            "kdf_iterations": self.kdf_iterations,
            "kdf_parallelism": self.kdf_parallelism,
            "wrapped_master_key": self.wrapped_master_key,
            "wrapped_key_nonce": self.wrapped_key_nonce,
        })
    }
}

pub(crate) fn settings(connection: &Connection) -> Result<SyncSettings, VaultError> {
    let row = connection.query_row(
        "SELECT enabled, supabase_url, anon_key, user_id, device_id, cursor,
                last_sync_at, last_status, last_error
         FROM sync_config WHERE id = 1",
        [],
        |row| {
            Ok(SyncSettings {
                enabled: row.get(0)?,
                supabase_url: row.get(1)?,
                anon_key: row.get(2)?,
                user_id: row.get(3)?,
                device_id: row.get(4)?,
                cursor: row.get(5)?,
                last_sync_at: row.get(6)?,
                last_status: row.get(7)?,
                last_error: row.get(8)?,
            })
        },
    )?;
    Ok(row)
}

pub(crate) fn configure(
    connection: &Connection,
    enabled: bool,
    supabase_url: Option<&str>,
    anon_key: Option<&str>,
    user_id: Option<&str>,
) -> Result<(), VaultError> {
    connection.execute(
        "UPDATE sync_config SET enabled = ?1, supabase_url = ?2, anon_key = ?3, user_id = ?4
         WHERE id = 1",
        params![enabled, supabase_url, anon_key, user_id],
    )?;
    Ok(())
}

pub(crate) fn session(
    connection: &Connection,
    master_key: &[u8; KEY_LEN],
) -> Result<Option<SyncSession>, VaultError> {
    let row: Option<(Vec<u8>, Vec<u8>)> = connection
        .query_row(
            "SELECT encrypted_session, session_nonce FROM sync_config WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((ciphertext, nonce)) = row else {
        return Ok(None);
    };
    if ciphertext.is_empty() {
        return Ok(None);
    }
    let decrypted = decrypt_bytes(
        master_key,
        &EncryptedValue {
            ciphertext,
            nonce: nonce.try_into().map_err(|_| VaultError::CorruptVault)?,
        },
        &sync_session_aad(),
    )?;
    serde_json::from_slice(decrypted.as_slice())
        .map(Some)
        .map_err(|_| VaultError::CorruptVault)
}

pub(crate) fn save_session(
    connection: &Connection,
    master_key: &[u8; KEY_LEN],
    tokens: &SyncSession,
) -> Result<(), VaultError> {
    let serialized =
        Zeroizing::new(serde_json::to_vec(tokens).map_err(|_| VaultError::CorruptVault)?);
    let encrypted = encrypt_bytes(master_key, &serialized, &sync_session_aad())?;
    connection.execute(
        "UPDATE sync_config SET encrypted_session = ?1, session_nonce = ?2 WHERE id = 1",
        params![encrypted.ciphertext, encrypted.nonce],
    )?;
    Ok(())
}

pub(crate) fn mark(
    connection: &Connection,
    cursor: Option<&str>,
    status: &str,
    error: Option<&str>,
    now: i64,
) -> Result<(), VaultError> {
    connection.execute(
        "UPDATE sync_config SET cursor = COALESCE(?1, cursor), last_sync_at = ?2,
                last_status = ?3, last_error = ?4
         WHERE id = 1",
        params![cursor, now, status, error],
    )?;
    Ok(())
}

pub(crate) fn envelope(connection: &Connection) -> Result<SyncEnvelope, VaultError> {
    let meta = db::read_meta(connection)?.ok_or(VaultError::CorruptVault)?;
    Ok(SyncEnvelope {
        vault_id: meta.vault_id,
        kdf_salt: BASE64.encode(meta.salt),
        kdf_memory_kib: i64::from(meta.kdf.memory_kib),
        kdf_iterations: i64::from(meta.kdf.iterations),
        kdf_parallelism: i64::from(meta.kdf.parallelism),
        wrapped_master_key: BASE64.encode(meta.wrapped_key.ciphertext),
        wrapped_key_nonce: BASE64.encode(meta.wrapped_key.nonce),
    })
}

#[derive(Debug, Serialize, Deserialize)]
struct SelectorsPayload {
    username: Option<String>,
    password: Option<String>,
    totp: Option<String>,
    submit: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UriPayload {
    uri: String,
    match_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CustomFieldPayload {
    id: String,
    field_name: String,
    is_secret: bool,
    sort_order: i64,
    value: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct FolderPayload {
    id: String,
    parent_id: Option<String>,
    name: String,
    slug: Option<String>,
    icon: String,
    color: Option<String>,
    is_system: bool,
    require_biometric_for_mcp: bool,
    created_at: i64,
    updated_at: i64,
    deleted_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ItemPayload {
    id: String,
    item_type: String,
    title: String,
    primary_url: Option<String>,
    username: Option<String>,
    password: Option<String>,
    totp_secret: Option<String>,
    notes: Option<String>,
    selectors: SelectorsPayload,
    favorite: bool,
    expires_at: Option<i64>,
    created_by_agent: Option<String>,
    zxcvbn_score: Option<i64>,
    is_compromised: bool,
    password_updated_at: Option<i64>,
    created_at: i64,
    updated_at: i64,
    deleted_at: Option<i64>,
    folder_ids: Vec<String>,
    uris: Vec<UriPayload>,
    custom_fields: Vec<CustomFieldPayload>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PasskeyPayload {
    id: String,
    item_id: Option<String>,
    credential_id: String,
    rp_id: String,
    rp_name: String,
    user_handle: String,
    user_name: String,
    encrypted_private_key: String,
    private_key_nonce: String,
    sign_count: i64,
    created_at: i64,
    last_used_at: Option<i64>,
    deleted_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RoutingRulePayload {
    id: String,
    pattern: String,
    pattern_type: String,
    target_folder_id: String,
    is_default: bool,
    enabled: bool,
    created_at: i64,
}

/// Serializes and encrypts every syncable record.
///
/// This is a full push on purpose: folder membership lives inside the item
/// payload, so pushing only changed rows would miss a membership change. Vaults
/// are small (SCOPE-3), and the upsert is idempotent.
pub(crate) fn records(
    connection: &Connection,
    master_key: &[u8; KEY_LEN],
) -> Result<Vec<SyncRecord>, VaultError> {
    let mut records = Vec::new();

    let folders = {
        let mut statement = connection.prepare(
            "SELECT id, parent_id, name, slug, icon, color, is_system,
                    require_biometric_for_mcp, created_at, updated_at, deleted_at
             FROM folders ORDER BY id",
        )?;
        statement
            .query_map([], |row| {
                Ok(FolderPayload {
                    id: row.get(0)?,
                    parent_id: row.get(1)?,
                    name: row.get(2)?,
                    slug: row.get(3)?,
                    icon: row.get(4)?,
                    color: row.get(5)?,
                    is_system: row.get(6)?,
                    require_biometric_for_mcp: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                    deleted_at: row.get(10)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    for folder in folders {
        records.push(seal(
            master_key,
            SyncRecordType::Folder,
            &folder.id,
            folder.deleted_at.unwrap_or(folder.updated_at),
            folder.deleted_at.is_some(),
            &folder,
        )?);
    }

    let item_rows = {
        let mut statement = connection.prepare(
            "SELECT id, item_type, title, primary_url, username, favorite, expires_at,
                    created_by_agent, zxcvbn_score, is_compromised, password_updated_at,
                    created_at, updated_at, deleted_at, username_selector,
                    password_selector, totp_selector, submit_selector
             FROM vault_items ORDER BY id",
        )?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, bool>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<i64>>(8)?,
                    row.get::<_, bool>(9)?,
                    row.get::<_, Option<i64>>(10)?,
                    row.get::<_, i64>(11)?,
                    row.get::<_, i64>(12)?,
                    row.get::<_, Option<i64>>(13)?,
                    (
                        row.get::<_, Option<String>>(14)?,
                        row.get::<_, Option<String>>(15)?,
                        row.get::<_, Option<String>>(16)?,
                        row.get::<_, Option<String>>(17)?,
                    ),
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    for row in item_rows {
        let (
            id,
            item_type,
            title,
            primary_url,
            username,
            favorite,
            expires_at,
            created_by_agent,
            zxcvbn_score,
            is_compromised,
            password_updated_at,
            created_at,
            updated_at,
            deleted_at,
            selectors,
        ) = row;
        let deleted = deleted_at.is_some();
        let (password, totp_secret, notes) = if deleted {
            (None, None, None)
        } else {
            item_secrets(connection, master_key, &id)?
        };
        let folder_ids = if deleted {
            Vec::new()
        } else {
            folder_ids(connection, &id)?
        };
        let uris = if deleted {
            Vec::new()
        } else {
            uris(connection, &id)?
        };
        let custom_fields = if deleted {
            Vec::new()
        } else {
            custom_fields(connection, master_key, &id)?
        };
        let payload = ItemPayload {
            id: id.clone(),
            item_type,
            title,
            primary_url,
            username,
            password,
            totp_secret,
            notes,
            selectors: SelectorsPayload {
                username: selectors.0,
                password: selectors.1,
                totp: selectors.2,
                submit: selectors.3,
            },
            favorite,
            expires_at,
            created_by_agent,
            zxcvbn_score,
            is_compromised,
            password_updated_at,
            created_at,
            updated_at,
            deleted_at,
            folder_ids,
            uris,
            custom_fields,
        };
        records.push(seal(
            master_key,
            SyncRecordType::Item,
            &id,
            deleted_at.unwrap_or(updated_at),
            deleted,
            &payload,
        )?);
    }

    let passkeys = {
        let mut statement = connection.prepare(
            "SELECT id, item_id, credential_id, rp_id, rp_name, user_handle, user_name,
                    encrypted_private_key, private_key_nonce, sign_count, created_at,
                    last_used_at, deleted_at
             FROM passkeys ORDER BY id",
        )?;
        statement
            .query_map([], |row| {
                let credential_id: Vec<u8> = row.get(2)?;
                let user_handle: Vec<u8> = row.get(5)?;
                let key: Vec<u8> = row.get(7)?;
                let nonce: Vec<u8> = row.get(8)?;
                Ok(PasskeyPayload {
                    id: row.get(0)?,
                    item_id: row.get(1)?,
                    credential_id: URL_SAFE_NO_PAD.encode(credential_id),
                    rp_id: row.get(3)?,
                    rp_name: row.get(4)?,
                    user_handle: URL_SAFE_NO_PAD.encode(user_handle),
                    user_name: row.get(6)?,
                    encrypted_private_key: BASE64.encode(key),
                    private_key_nonce: BASE64.encode(nonce),
                    sign_count: row.get(9)?,
                    created_at: row.get(10)?,
                    last_used_at: row.get(11)?,
                    deleted_at: row.get(12)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    for passkey in passkeys {
        records.push(seal(
            master_key,
            SyncRecordType::Passkey,
            &passkey.id,
            passkey.deleted_at.unwrap_or(passkey.created_at),
            passkey.deleted_at.is_some(),
            &passkey,
        )?);
    }

    let rules = {
        let mut statement = connection.prepare(
            "SELECT id, pattern, pattern_type, target_folder_id, is_default, enabled, created_at
             FROM local_routing_rules ORDER BY id",
        )?;
        statement
            .query_map([], |row| {
                Ok(RoutingRulePayload {
                    id: row.get(0)?,
                    pattern: row.get(1)?,
                    pattern_type: row.get(2)?,
                    target_folder_id: row.get(3)?,
                    is_default: row.get(4)?,
                    enabled: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    for rule in rules {
        records.push(seal(
            master_key,
            SyncRecordType::RoutingRule,
            &rule.id,
            rule.created_at,
            false,
            &rule,
        )?);
    }

    Ok(records)
}

/// Applies remote records in dependency order (folders first), LWW per record.
pub(crate) fn apply_records(
    connection: &mut Connection,
    master_key: &[u8; KEY_LEN],
    incoming: &[SyncRecord],
) -> Result<SyncSummary, VaultError> {
    let mut summary = SyncSummary {
        pulled: incoming.len(),
        ..SyncSummary::default()
    };
    let mut ordered = incoming.to_vec();
    ordered.sort_by_key(|record| match record.record_type {
        SyncRecordType::Folder => 0,
        SyncRecordType::Item => 1,
        SyncRecordType::RoutingRule => 2,
        SyncRecordType::Passkey => 3,
    });

    let transaction = connection.transaction()?;
    for record in &ordered {
        if record.is_deleted {
            summary.tombstones += 1;
        }
        let plaintext = open(master_key, record)?;
        let applied = match record.record_type {
            SyncRecordType::Folder => apply_folder(&transaction, record, &plaintext)?,
            SyncRecordType::Item => apply_item(&transaction, master_key, record, &plaintext)?,
            SyncRecordType::Passkey => apply_passkey(&transaction, record, &plaintext)?,
            SyncRecordType::RoutingRule => apply_rule(&transaction, record, &plaintext)?,
        };
        if applied {
            summary.applied += 1;
        } else {
            summary.skipped += 1;
        }
    }
    transaction.commit()?;
    Ok(summary)
}

fn seal<T: Serialize>(
    master_key: &[u8; KEY_LEN],
    record_type: SyncRecordType,
    id: &str,
    client_updated_at: i64,
    is_deleted: bool,
    payload: &T,
) -> Result<SyncRecord, VaultError> {
    let serialized =
        Zeroizing::new(serde_json::to_vec(payload).map_err(|_| VaultError::CorruptVault)?);
    let encrypted = encrypt_bytes(
        master_key,
        &serialized,
        &sync_record_aad(record_type.as_str(), id),
    )?;
    Ok(SyncRecord {
        id: id.to_owned(),
        record_type,
        client_updated_at,
        is_deleted,
        ciphertext: BASE64.encode(encrypted.ciphertext),
        nonce: BASE64.encode(encrypted.nonce),
    })
}

fn open(master_key: &[u8; KEY_LEN], record: &SyncRecord) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    let ciphertext = BASE64
        .decode(&record.ciphertext)
        .map_err(|_| VaultError::InvalidInput("corrupt sync record"))?;
    let nonce: [u8; crate::crypto::NONCE_LEN] = BASE64
        .decode(&record.nonce)
        .map_err(|_| VaultError::InvalidInput("corrupt sync record"))?
        .try_into()
        .map_err(|_| VaultError::InvalidInput("corrupt sync record"))?;
    decrypt_bytes(
        master_key,
        &EncryptedValue { ciphertext, nonce },
        &sync_record_aad(record.record_type.as_str(), &record.id),
    )
}

fn local_timestamp(
    connection: &Connection,
    record: &SyncRecord,
) -> Result<Option<i64>, VaultError> {
    let sql = match record.record_type {
        SyncRecordType::Folder | SyncRecordType::Item => {
            "SELECT COALESCE(deleted_at, updated_at) FROM {table} WHERE id = ?1"
        }
        SyncRecordType::Passkey => {
            "SELECT COALESCE(deleted_at, created_at) FROM {table} WHERE id = ?1"
        }
        SyncRecordType::RoutingRule => "SELECT created_at FROM {table} WHERE id = ?1",
    };
    let table = match record.record_type {
        SyncRecordType::Folder => "folders",
        SyncRecordType::Item => "vault_items",
        SyncRecordType::Passkey => "passkeys",
        SyncRecordType::RoutingRule => "local_routing_rules",
    };
    let sql = sql.replace("{table}", table);
    connection
        .query_row(&sql, [&record.id], |row| row.get::<_, i64>(0))
        .optional()
        .map_err(VaultError::from)
}

fn apply_folder(
    connection: &Connection,
    record: &SyncRecord,
    plaintext: &[u8],
) -> Result<bool, VaultError> {
    let Some(local) = local_timestamp(connection, record)? else {
        insert_folder(connection, plaintext)?;
        return Ok(true);
    };
    if record.client_updated_at < local {
        return Ok(false);
    }
    let payload: FolderPayload =
        serde_json::from_slice(plaintext).map_err(|_| VaultError::CorruptVault)?;
    // A tombstone keeps the row so the deletion can keep propagating.
    connection.execute(
        "UPDATE folders SET parent_id = ?2, name = ?3, slug = ?4, icon = ?5, color = ?6,
                is_system = ?7, require_biometric_for_mcp = ?8, created_at = ?9,
                updated_at = ?10, deleted_at = ?11
         WHERE id = ?1",
        params![
            payload.id,
            payload.parent_id,
            payload.name,
            payload.slug,
            payload.icon,
            payload.color,
            payload.is_system,
            payload.require_biometric_for_mcp,
            payload.created_at,
            payload.updated_at,
            payload.deleted_at,
        ],
    )?;
    Ok(true)
}

fn insert_folder(connection: &Connection, plaintext: &[u8]) -> Result<(), VaultError> {
    let payload: FolderPayload =
        serde_json::from_slice(plaintext).map_err(|_| VaultError::CorruptVault)?;
    connection.execute(
        "INSERT INTO folders (
           id, parent_id, name, slug, icon, color, is_system, require_biometric_for_mcp,
           created_at, updated_at, deleted_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(id) DO UPDATE SET parent_id = excluded.parent_id, name = excluded.name,
           slug = excluded.slug, icon = excluded.icon, color = excluded.color,
           is_system = excluded.is_system,
           require_biometric_for_mcp = excluded.require_biometric_for_mcp,
           created_at = excluded.created_at, updated_at = excluded.updated_at,
           deleted_at = excluded.deleted_at",
        params![
            payload.id,
            payload.parent_id,
            payload.name,
            payload.slug,
            payload.icon,
            payload.color,
            payload.is_system,
            payload.require_biometric_for_mcp,
            payload.created_at,
            payload.updated_at,
            payload.deleted_at,
        ],
    )?;
    Ok(())
}

fn apply_item(
    connection: &Connection,
    master_key: &[u8; KEY_LEN],
    record: &SyncRecord,
    plaintext: &[u8],
) -> Result<bool, VaultError> {
    let Some(local) = local_timestamp(connection, record)? else {
        write_item(connection, master_key, plaintext, false)?;
        return Ok(true);
    };
    if record.client_updated_at < local {
        return Ok(false);
    }
    write_item(connection, master_key, plaintext, true)?;
    Ok(true)
}

fn write_item(
    connection: &Connection,
    master_key: &[u8; KEY_LEN],
    plaintext: &[u8],
    exists: bool,
) -> Result<(), VaultError> {
    let payload: ItemPayload =
        serde_json::from_slice(plaintext).map_err(|_| VaultError::CorruptVault)?;
    let password = encrypt_optional(master_key, &payload.id, "password", payload.password)?;
    let totp = encrypt_optional(master_key, &payload.id, "totp", payload.totp_secret)?;
    let notes = encrypt_optional(master_key, &payload.id, "notes", payload.notes)?;
    let values = params![
        payload.id,
        payload.item_type,
        payload.title,
        payload.primary_url,
        payload.username,
        password.as_ref().map(|value| value.ciphertext.as_slice()),
        password.as_ref().map(|value| value.nonce),
        totp.as_ref().map(|value| value.ciphertext.as_slice()),
        totp.as_ref().map(|value| value.nonce),
        notes.as_ref().map(|value| value.ciphertext.as_slice()),
        notes.as_ref().map(|value| value.nonce),
        payload.selectors.username,
        payload.selectors.password,
        payload.selectors.totp,
        payload.selectors.submit,
        payload.zxcvbn_score,
        payload.is_compromised,
        payload.password_updated_at,
        payload.created_by_agent,
        payload.expires_at,
        payload.favorite,
        payload.created_at,
        payload.updated_at,
        payload.deleted_at,
    ];
    let sql = if exists {
        "UPDATE vault_items SET item_type = ?2, title = ?3, primary_url = ?4, username = ?5,
                encrypted_password = ?6, password_nonce = ?7, encrypted_totp_secret = ?8,
                totp_nonce = ?9, encrypted_notes = ?10, notes_nonce = ?11,
                username_selector = ?12, password_selector = ?13, totp_selector = ?14,
                submit_selector = ?15, zxcvbn_score = ?16, is_compromised = ?17,
                password_updated_at = ?18, created_by_agent = ?19, expires_at = ?20,
                favorite = ?21, created_at = ?22, updated_at = ?23, deleted_at = ?24
         WHERE id = ?1"
    } else {
        "INSERT INTO vault_items (
           id, item_type, title, primary_url, username, encrypted_password, password_nonce,
           encrypted_totp_secret, totp_nonce, encrypted_notes, notes_nonce, username_selector,
           password_selector, totp_selector, submit_selector, zxcvbn_score, is_compromised,
           password_updated_at, created_by_agent, expires_at, favorite, created_at, updated_at,
           deleted_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                   ?18, ?19, ?20, ?21, ?22, ?23, ?24)"
    };
    connection.execute(sql, values)?;

    if payload.deleted_at.is_none() {
        for folder_id in &payload.folder_ids {
            connection.execute(
                "INSERT OR IGNORE INTO item_folders (item_id, folder_id, added_at)
                 VALUES (?1, ?2, ?3)",
                params![payload.id, folder_id, payload.updated_at],
            )?;
        }
        connection.execute("DELETE FROM item_uris WHERE item_id = ?1", [&payload.id])?;
        for uri in &payload.uris {
            connection.execute(
                "INSERT INTO item_uris (id, item_id, uri, match_type) VALUES (?1, ?2, ?3, ?4)",
                params![
                    Uuid::new_v4().to_string(),
                    payload.id,
                    uri.uri,
                    uri.match_type,
                ],
            )?;
        }
        connection.execute(
            "DELETE FROM item_custom_fields WHERE item_id = ?1",
            [&payload.id],
        )?;
        for field in &payload.custom_fields {
            let encrypted = encrypt_string(
                master_key,
                &field.value,
                &custom_field_aad(&payload.id, &field.id),
            )?;
            connection.execute(
                "INSERT INTO item_custom_fields (
                   id, item_id, field_name, is_secret, encrypted_value, value_nonce, sort_order
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    field.id,
                    payload.id,
                    field.field_name,
                    field.is_secret,
                    encrypted.ciphertext,
                    encrypted.nonce,
                    field.sort_order,
                ],
            )?;
        }
    }
    Ok(())
}

fn encrypt_optional(
    master_key: &[u8; KEY_LEN],
    item_id: &str,
    field: &str,
    value: Option<String>,
) -> Result<Option<EncryptedValue>, VaultError> {
    let Some(value) = value else {
        return Ok(None);
    };
    encrypt_string(master_key, &value, &secret_field_aad(item_id, field)).map(Some)
}

fn apply_passkey(
    connection: &Connection,
    record: &SyncRecord,
    plaintext: &[u8],
) -> Result<bool, VaultError> {
    let payload: PasskeyPayload =
        serde_json::from_slice(plaintext).map_err(|_| VaultError::CorruptVault)?;
    let local: Option<(i64, i64, Option<i64>)> = connection
        .query_row(
            "SELECT sign_count, created_at, deleted_at FROM passkeys WHERE id = ?1",
            [&record.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    if let Some((sign_count, created_at, deleted_at)) = local {
        let local_key = deleted_at.unwrap_or(created_at);
        if payload.sign_count <= sign_count && record.client_updated_at < local_key {
            return Ok(false);
        }
    }
    let credential_id = URL_SAFE_NO_PAD
        .decode(&payload.credential_id)
        .map_err(|_| VaultError::CorruptVault)?;
    let user_handle = URL_SAFE_NO_PAD
        .decode(&payload.user_handle)
        .map_err(|_| VaultError::CorruptVault)?;
    let key = BASE64
        .decode(&payload.encrypted_private_key)
        .map_err(|_| VaultError::CorruptVault)?;
    let nonce: [u8; crate::crypto::NONCE_LEN] = BASE64
        .decode(&payload.private_key_nonce)
        .map_err(|_| VaultError::CorruptVault)?
        .try_into()
        .map_err(|_| VaultError::CorruptVault)?;
    connection.execute(
        "INSERT INTO passkeys (
           id, item_id, credential_id, rp_id, rp_name, user_handle, user_name,
           encrypted_private_key, private_key_nonce, sign_count, created_at, last_used_at,
           deleted_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(id) DO UPDATE SET item_id = excluded.item_id,
           credential_id = excluded.credential_id, rp_id = excluded.rp_id,
           rp_name = excluded.rp_name, user_handle = excluded.user_handle,
           user_name = excluded.user_name,
           encrypted_private_key = excluded.encrypted_private_key,
           private_key_nonce = excluded.private_key_nonce, sign_count = excluded.sign_count,
           created_at = excluded.created_at, last_used_at = excluded.last_used_at,
           deleted_at = excluded.deleted_at",
        params![
            payload.id,
            payload.item_id,
            credential_id,
            payload.rp_id,
            payload.rp_name,
            user_handle,
            payload.user_name,
            key,
            nonce,
            payload.sign_count,
            payload.created_at,
            payload.last_used_at,
            payload.deleted_at,
        ],
    )?;
    Ok(true)
}

fn apply_rule(
    connection: &Connection,
    record: &SyncRecord,
    plaintext: &[u8],
) -> Result<bool, VaultError> {
    let Some(local) = local_timestamp(connection, record)? else {
        insert_rule(connection, plaintext)?;
        return Ok(true);
    };
    if record.client_updated_at < local {
        return Ok(false);
    }
    update_rule(connection, plaintext)?;
    Ok(true)
}

fn insert_rule(connection: &Connection, plaintext: &[u8]) -> Result<(), VaultError> {
    let payload: RoutingRulePayload =
        serde_json::from_slice(plaintext).map_err(|_| VaultError::CorruptVault)?;
    connection.execute(
        "INSERT INTO local_routing_rules (
           id, pattern, pattern_type, target_folder_id, is_default, enabled, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET pattern = excluded.pattern,
           pattern_type = excluded.pattern_type,
           target_folder_id = excluded.target_folder_id,
           is_default = excluded.is_default, enabled = excluded.enabled,
           created_at = excluded.created_at",
        params![
            payload.id,
            payload.pattern,
            payload.pattern_type,
            payload.target_folder_id,
            payload.is_default,
            payload.enabled,
            payload.created_at,
        ],
    )?;
    Ok(())
}

fn update_rule(connection: &Connection, plaintext: &[u8]) -> Result<(), VaultError> {
    let payload: RoutingRulePayload =
        serde_json::from_slice(plaintext).map_err(|_| VaultError::CorruptVault)?;
    connection.execute(
        "UPDATE local_routing_rules SET pattern = ?2, pattern_type = ?3,
                target_folder_id = ?4, is_default = ?5, enabled = ?6, created_at = ?7
         WHERE id = ?1",
        params![
            payload.id,
            payload.pattern,
            payload.pattern_type,
            payload.target_folder_id,
            payload.is_default,
            payload.enabled,
            payload.created_at,
        ],
    )?;
    Ok(())
}

/// Decrypted password, TOTP seed and notes of one item.
type ItemSecrets = (Option<String>, Option<String>, Option<String>);

fn item_secrets(
    connection: &Connection,
    master_key: &[u8; KEY_LEN],
    item_id: &str,
) -> Result<ItemSecrets, VaultError> {
    let present: (bool, bool, bool) = connection.query_row(
        "SELECT encrypted_password IS NOT NULL, encrypted_totp_secret IS NOT NULL,
                encrypted_notes IS NOT NULL
         FROM vault_items WHERE id = ?1",
        [item_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let mut values = (None, None, None);
    if present.0 {
        values.0 = Some(
            crate::items::reveal(connection, master_key, item_id, SecretField::Password, None)?
                .value
                .expose()
                .to_owned(),
        );
    }
    if present.1 {
        values.1 = Some(
            crate::items::reveal(connection, master_key, item_id, SecretField::Totp, None)?
                .value
                .expose()
                .to_owned(),
        );
    }
    if present.2 {
        values.2 = Some(
            crate::items::reveal(connection, master_key, item_id, SecretField::Notes, None)?
                .value
                .expose()
                .to_owned(),
        );
    }
    Ok(values)
}

fn folder_ids(connection: &Connection, item_id: &str) -> Result<Vec<String>, VaultError> {
    let mut statement = connection
        .prepare("SELECT folder_id FROM item_folders WHERE item_id = ?1 ORDER BY folder_id")?;
    let rows = statement.query_map([item_id], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn uris(connection: &Connection, item_id: &str) -> Result<Vec<UriPayload>, VaultError> {
    let mut statement = connection
        .prepare("SELECT uri, match_type FROM item_uris WHERE item_id = ?1 ORDER BY uri")?;
    let rows = statement.query_map([item_id], |row| {
        Ok(UriPayload {
            uri: row.get(0)?,
            match_type: row.get(1)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn custom_fields(
    connection: &Connection,
    master_key: &[u8; KEY_LEN],
    item_id: &str,
) -> Result<Vec<CustomFieldPayload>, VaultError> {
    let fields = {
        let mut statement = connection.prepare(
            "SELECT id, field_name, is_secret, sort_order FROM item_custom_fields
             WHERE item_id = ?1 ORDER BY sort_order, id",
        )?;
        statement
            .query_map([item_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, bool>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut payloads = Vec::with_capacity(fields.len());
    for (id, field_name, is_secret, sort_order) in fields {
        let value = crate::items::reveal(
            connection,
            master_key,
            item_id,
            SecretField::CustomField,
            Some(&id),
        )?
        .value
        .expose()
        .to_owned();
        payloads.push(CustomFieldPayload {
            id,
            field_name,
            is_secret,
            sort_order,
            value,
        });
    }
    Ok(payloads)
}
