//! `.akooti` snapshots: encrypted, self-contained copies of the vault files.
//!
//! A snapshot is not a second encryption scheme: it carries the public
//! bootstrap header and the SQLCipher database as they are, so the file stays
//! encrypted at rest and is restored by opening it with the master password.
//! The container only adds a magic, a version and a small JSON header so a
//! wrong file is refused before anything is written.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::VaultError;
use crate::header::VaultPaths;

/// Magic of a `.akooti` snapshot, version included.
pub const MAGIC: &[u8; 12] = b"AKOOTI-SNAP1";
/// Current container version.
pub const SNAPSHOT_VERSION: u32 = 1;
/// Largest header accepted (JSON metadata only).
const MAX_HEADER_BYTES: u32 = 64 * 1024;

/// Metadata stored next to the encrypted files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotHeader {
    /// Container version.
    pub version: u32,
    /// Creation time, Unix seconds.
    pub created_at: i64,
    /// Core version that produced the file.
    pub core_version: String,
    /// Vault identifier, useful to tell two vaults apart.
    pub vault_id: String,
}

/// What a restored snapshot was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotInfo {
    /// Parsed header.
    pub header: SnapshotHeader,
    /// Size of the embedded database, in bytes.
    pub database_bytes: usize,
}

pub(crate) fn encode(header: &SnapshotHeader, meta: &[u8], database: &[u8]) -> Vec<u8> {
    let json = serde_json::to_vec(header).unwrap_or_default();
    let mut output =
        Vec::with_capacity(MAGIC.len() + 4 + json.len() + 4 + meta.len() + database.len());
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&(json.len() as u32).to_le_bytes());
    output.extend_from_slice(&json);
    output.extend_from_slice(&(meta.len() as u32).to_le_bytes());
    output.extend_from_slice(meta);
    output.extend_from_slice(database);
    output
}

/// Restores a snapshot into `paths`, overwriting only when told to.
///
/// # Errors
///
/// Returns [`VaultError::InvalidInput`] for a malformed container, an existing
/// vault without `overwrite`, or a path that cannot be written.
pub fn restore(
    snapshot: &Path,
    paths: &VaultPaths,
    overwrite: bool,
) -> Result<SnapshotInfo, VaultError> {
    if !overwrite && (paths.header.exists() || paths.database.exists()) {
        return Err(VaultError::InvalidInput(
            "a vault already exists at this location",
        ));
    }
    let bytes = fs::read(snapshot)?;
    let (header, meta, database) = decode(&bytes)?;
    if let Some(parent) = paths.header.parent() {
        fs::create_dir_all(parent)?;
    }
    write_atomic(&paths.header, meta)?;
    write_atomic(&paths.database, database)?;
    // Stale WAL files from a previous vault would be replayed onto the new one.
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = paths.database.clone().into_os_string();
        sidecar.push(suffix);
        let sidecar = std::path::PathBuf::from(sidecar);
        if sidecar.exists() {
            fs::remove_file(sidecar)?;
        }
    }
    Ok(SnapshotInfo {
        header,
        database_bytes: database.len(),
    })
}

fn decode(bytes: &[u8]) -> Result<(SnapshotHeader, &[u8], &[u8]), VaultError> {
    let malformed = || VaultError::InvalidInput("this file is not an Akooti snapshot");
    if !bytes.starts_with(MAGIC) {
        return Err(malformed());
    }
    let mut offset = MAGIC.len();
    let header_len = read_len(bytes, &mut offset).ok_or_else(malformed)?;
    if header_len > MAX_HEADER_BYTES {
        return Err(malformed());
    }
    let header_end = offset
        .checked_add(header_len as usize)
        .filter(|end| *end <= bytes.len())
        .ok_or_else(malformed)?;
    let header: SnapshotHeader =
        serde_json::from_slice(&bytes[offset..header_end]).map_err(|_| malformed())?;
    if header.version != SNAPSHOT_VERSION {
        return Err(VaultError::InvalidInput("unsupported snapshot version"));
    }
    offset = header_end;
    let meta_len = read_len(bytes, &mut offset).ok_or_else(malformed)?;
    let meta_end = offset
        .checked_add(meta_len as usize)
        .filter(|end| *end <= bytes.len())
        .ok_or_else(malformed)?;
    let database = &bytes[meta_end..];
    if database.is_empty() {
        return Err(malformed());
    }
    Ok((header, &bytes[offset..meta_end], database))
}

fn read_len(bytes: &[u8], offset: &mut usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let slice = bytes.get(*offset..end)?;
    *offset = end;
    Some(u32::from_le_bytes(slice.try_into().ok()?))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), VaultError> {
    let temp = path.with_extension("akooti-tmp");
    fs::write(&temp, bytes)?;
    fs::rename(&temp, path)?;
    Ok(())
}
