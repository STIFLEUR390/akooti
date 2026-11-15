use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::crypto::{KdfParams, SALT_LEN, generate_salt};
use crate::error::VaultError;

const MAGIC: &[u8; 8] = b"AKOOTI01";
const FORMAT_VERSION: u8 = 1;
const ALGORITHM_ARGON2ID: u8 = 1;
const HEADER_LEN: usize = 8 + 1 + 1 + 2 + SALT_LEN + 4 + 4 + 4;

/// Paths that make up one vault on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultPaths {
    /// Public bootstrap header.
    pub header: PathBuf,
    /// Encrypted SQLCipher database.
    pub database: PathBuf,
}

impl VaultPaths {
    /// Builds the standard two-file layout below a platform data directory.
    pub fn under_app_data(base: &Path) -> Self {
        Self {
            header: base.join("vault.meta"),
            database: base.join("vault.db"),
        }
    }
}

/// Public bootstrap data needed to derive the SQLCipher key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultHeader {
    /// Argon2id salt.
    pub salt: [u8; SALT_LEN],
    /// Argon2id parameters.
    pub kdf: KdfParams,
}

impl VaultHeader {
    /// Generates a header with a fresh salt and the approved defaults.
    pub fn new() -> Self {
        Self {
            salt: generate_salt(),
            kdf: KdfParams::default(),
        }
    }

    /// Serializes the header to its fixed-size binary representation.
    pub fn encode(&self) -> [u8; HEADER_LEN] {
        let mut output = [0_u8; HEADER_LEN];
        output[..8].copy_from_slice(MAGIC);
        output[8] = FORMAT_VERSION;
        output[9] = ALGORITHM_ARGON2ID;
        output[12..28].copy_from_slice(&self.salt);
        output[28..32].copy_from_slice(&self.kdf.memory_kib.to_le_bytes());
        output[32..36].copy_from_slice(&self.kdf.iterations.to_le_bytes());
        output[36..40].copy_from_slice(&self.kdf.parallelism.to_le_bytes());
        output
    }

    /// Parses and validates a binary header.
    pub fn decode(bytes: &[u8]) -> Result<Self, VaultError> {
        if bytes.len() != HEADER_LEN || bytes.get(..8) != Some(MAGIC) {
            return Err(VaultError::CorruptVault);
        }
        if bytes[8] != FORMAT_VERSION || bytes[9] != ALGORITHM_ARGON2ID {
            return Err(VaultError::CorruptVault);
        }
        let mut salt = [0_u8; SALT_LEN];
        salt.copy_from_slice(&bytes[12..28]);
        let kdf = KdfParams {
            memory_kib: u32::from_le_bytes(
                bytes[28..32]
                    .try_into()
                    .map_err(|_| VaultError::CorruptVault)?,
            ),
            iterations: u32::from_le_bytes(
                bytes[32..36]
                    .try_into()
                    .map_err(|_| VaultError::CorruptVault)?,
            ),
            parallelism: u32::from_le_bytes(
                bytes[36..40]
                    .try_into()
                    .map_err(|_| VaultError::CorruptVault)?,
            ),
        };
        kdf.validate()?;
        Ok(Self { salt, kdf })
    }

    /// Reads a header from disk.
    pub fn read(path: &Path) -> Result<Self, VaultError> {
        Self::decode(&fs::read(path)?)
    }

    /// Writes a new header without overwriting an existing vault.
    pub fn create_new(&self, path: &Path) -> Result<(), VaultError> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        file.write_all(&self.encode())?;
        file.sync_all()?;
        Ok(())
    }
}

impl Default for VaultHeader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trips() {
        let header = VaultHeader::new();
        assert_eq!(VaultHeader::decode(&header.encode()).ok(), Some(header));
    }

    #[test]
    fn header_rejects_truncation() {
        assert!(matches!(
            VaultHeader::decode(b"AKOOTI01"),
            Err(VaultError::CorruptVault)
        ));
    }

    #[test]
    fn header_rejects_unknown_algorithm() {
        let mut bytes = VaultHeader::new().encode();
        bytes[9] = 99;
        assert!(matches!(
            VaultHeader::decode(&bytes),
            Err(VaultError::CorruptVault)
        ));
    }
}
