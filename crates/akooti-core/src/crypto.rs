use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand_core::{OsRng, RngCore};
use zeroize::Zeroizing;

use crate::error::VaultError;
use crate::models::SecretString;

/// XChaCha20-Poly1305 nonce length in bytes.
pub const NONCE_LEN: usize = 24;
/// Argon2id salt length in bytes.
pub const SALT_LEN: usize = 16;
/// Symmetric key length in bytes.
pub const KEY_LEN: usize = 32;

/// Argon2id parameters stored in the public vault header and database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    /// Memory cost in KiB.
    pub memory_kib: u32,
    /// Iteration count.
    pub iterations: u32,
    /// Parallel lanes.
    pub parallelism: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        Self {
            memory_kib: 65_536,
            iterations: 3,
            parallelism: 4,
        }
    }
}

impl KdfParams {
    pub(crate) fn validate(self) -> Result<(), VaultError> {
        if self.memory_kib < 8 || self.memory_kib > 1_048_576 {
            return Err(VaultError::CorruptVault);
        }
        if self.iterations == 0 || self.iterations > 100 {
            return Err(VaultError::CorruptVault);
        }
        if self.parallelism == 0 || self.parallelism > 64 {
            return Err(VaultError::CorruptVault);
        }
        Ok(())
    }
}

/// Encrypted value with its independently stored nonce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedValue {
    /// Authenticated ciphertext.
    pub ciphertext: Vec<u8>,
    /// Unique 24-byte nonce.
    pub nonce: [u8; NONCE_LEN],
}

/// Derives the 32-byte key-encryption key from a password.
pub fn derive_key(
    password: &str,
    salt: &[u8; SALT_LEN],
    params: KdfParams,
) -> Result<Zeroizing<[u8; KEY_LEN]>, VaultError> {
    params.validate()?;
    let argon_params = Params::new(
        params.memory_kib,
        params.iterations,
        params.parallelism,
        Some(KEY_LEN),
    )
    .map_err(|_| VaultError::Crypto)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);
    let mut key = Zeroizing::new([0_u8; KEY_LEN]);
    argon
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|_| VaultError::Crypto)?;
    Ok(key)
}

/// Generates a cryptographically random key.
pub fn generate_key() -> Zeroizing<[u8; KEY_LEN]> {
    let mut key = Zeroizing::new([0_u8; KEY_LEN]);
    OsRng.fill_bytes(key.as_mut());
    key
}

/// Generates a fresh salt.
pub fn generate_salt() -> [u8; SALT_LEN] {
    let mut salt = [0_u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    salt
}

/// Encrypts UTF-8 data with XChaCha20-Poly1305 and associated data.
pub(crate) fn encrypt_string(
    key: &[u8; KEY_LEN],
    value: &str,
    associated_data: &[u8],
) -> Result<EncryptedValue, VaultError> {
    encrypt_bytes(key, value.as_bytes(), associated_data)
}

/// Decrypts UTF-8 data with XChaCha20-Poly1305 and associated data.
pub(crate) fn decrypt_string(
    key: &[u8; KEY_LEN],
    encrypted: &EncryptedValue,
    associated_data: &[u8],
) -> Result<SecretString, VaultError> {
    let plaintext = decrypt_bytes(key, encrypted, associated_data)?;
    let text = std::str::from_utf8(plaintext.as_slice()).map_err(|_| VaultError::Crypto)?;
    Ok(SecretString::new(text.to_owned()))
}

pub(crate) fn unwrap_master_key(
    key: &[u8; KEY_LEN],
    encrypted: &EncryptedValue,
    associated_data: &[u8],
) -> Result<Zeroizing<[u8; KEY_LEN]>, VaultError> {
    let plaintext = decrypt_bytes(key, encrypted, associated_data)?;
    if plaintext.len() != KEY_LEN {
        return Err(VaultError::CorruptVault);
    }
    let mut master_key = Zeroizing::new([0_u8; KEY_LEN]);
    master_key.copy_from_slice(plaintext.as_slice());
    Ok(master_key)
}

/// Encrypts arbitrary bytes with XChaCha20-Poly1305 and associated data.
pub(crate) fn encrypt_bytes(
    key: &[u8; KEY_LEN],
    value: &[u8],
    associated_data: &[u8],
) -> Result<EncryptedValue, VaultError> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut nonce = [0_u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce);
    encrypt_with_nonce(&cipher, nonce, value, associated_data)
}

fn encrypt_with_nonce(
    cipher: &XChaCha20Poly1305,
    nonce: [u8; NONCE_LEN],
    value: &[u8],
    associated_data: &[u8],
) -> Result<EncryptedValue, VaultError> {
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: value,
                aad: associated_data,
            },
        )
        .map_err(|_| VaultError::Crypto)?;
    Ok(EncryptedValue { ciphertext, nonce })
}

pub(crate) fn decrypt_bytes(
    key: &[u8; KEY_LEN],
    encrypted: &EncryptedValue,
    associated_data: &[u8],
) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    let cipher = XChaCha20Poly1305::new(key.into());
    cipher
        .decrypt(
            XNonce::from_slice(&encrypted.nonce),
            Payload {
                msg: &encrypted.ciphertext,
                aad: associated_data,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| VaultError::Crypto)
}

pub(crate) fn master_key_aad(vault_id: &str) -> Vec<u8> {
    format!("akooti:v1:master-key:{vault_id}").into_bytes()
}

pub(crate) fn secret_field_aad(item_id: &str, field: &str) -> Vec<u8> {
    format!("akooti:v1:item:{item_id}:field:{field}").into_bytes()
}

pub(crate) fn custom_field_aad(item_id: &str, field_id: &str) -> Vec<u8> {
    format!("akooti:v1:item:{item_id}:custom:{field_id}").into_bytes()
}

/// Associated data of the private key of one passkey.
///
/// Binding the ciphertext to the credential id means a row cannot be copied
/// onto another credential: the tag stops authenticating.
pub(crate) fn passkey_aad(credential_id: &[u8]) -> Vec<u8> {
    format!("akooti:v1:passkey:{}", hex::encode(credential_id)).into_bytes()
}

/// Associated data of a backup target configuration.
pub(crate) fn backup_config_aad(target_id: &str) -> Vec<u8> {
    format!("akooti:v1:backup-target:{target_id}").into_bytes()
}

/// Associated data of the Supabase session (access and refresh tokens).
pub(crate) fn sync_session_aad() -> Vec<u8> {
    b"akooti:v1:sync-session".to_vec()
}

/// Associated data of one synced record, bound to its type and identifier.
pub(crate) fn sync_record_aad(record_type: &str, id: &str) -> Vec<u8> {
    format!("akooti:v1:sync:{record_type}:{id}").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_vector_authenticates_aad() {
        let key = [7_u8; KEY_LEN];
        let cipher = XChaCha20Poly1305::new((&key).into());
        let encrypted =
            encrypt_with_nonce(&cipher, [9_u8; NONCE_LEN], b"secret", b"field-a").expect("vector");
        let decrypted = decrypt_bytes(&key, &encrypted, b"field-a").expect("decrypt");
        assert_eq!(decrypted.as_slice(), b"secret");

        let wrong_aad = decrypt_bytes(&key, &encrypted, b"field-b");
        assert!(matches!(wrong_aad, Err(VaultError::Crypto)));
    }

    #[test]
    fn fresh_encryptions_use_distinct_nonces() {
        let key = [3_u8; KEY_LEN];
        let first = encrypt_string(&key, "secret", b"aad").expect("first");
        let second = encrypt_string(&key, "secret", b"aad").expect("second");
        assert_ne!(first.nonce, second.nonce);
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let key = [5_u8; KEY_LEN];
        let mut encrypted = encrypt_string(&key, "secret", b"aad").expect("encrypt");
        encrypted.ciphertext[0] ^= 1;
        assert!(matches!(
            decrypt_string(&key, &encrypted, b"aad"),
            Err(VaultError::Crypto)
        ));
    }
}
