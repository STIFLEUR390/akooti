use zeroize::Zeroizing;

use super::crypto_support::{
    aes_256_cbc_decrypt, decode_base64, derive_argon2id, derive_pbkdf2_sha256, hkdf_expand_32,
    hmac_sha256_matches,
};
use super::error::ImportError;

pub(crate) struct ExportKey<'a> {
    pub(crate) password: &'a str,
    pub(crate) salt: &'a str,
    pub(crate) kdf_type: u32,
    pub(crate) iterations: u32,
    pub(crate) memory_kib: Option<u32>,
    pub(crate) parallelism: Option<u32>,
}

pub(crate) fn decrypt_export(
    config: ExportKey<'_>,
    validation: &str,
    data: &str,
) -> Result<Zeroizing<Vec<u8>>, ImportError> {
    let master_key = match config.kdf_type {
        0 => derive_pbkdf2_sha256(
            config.password,
            config.salt.as_bytes(),
            config.iterations,
            32,
        )?,
        1 => derive_argon2id(
            config.password,
            config.salt.as_bytes(),
            config.iterations,
            config
                .memory_kib
                .ok_or(ImportError::UnsupportedEncryption)?,
            config
                .parallelism
                .ok_or(ImportError::UnsupportedEncryption)?,
        )?,
        _ => return Err(ImportError::UnsupportedEncryption),
    };
    let mut stretched = Zeroizing::new(Vec::with_capacity(64));
    stretched.extend_from_slice(hkdf_expand_32(&master_key, b"enc")?.as_slice());
    stretched.extend_from_slice(hkdf_expand_32(&master_key, b"mac")?.as_slice());
    decrypt_enc_string(validation, &stretched)?;
    decrypt_enc_string(data, &stretched)
}

fn decrypt_enc_string(value: &str, key: &[u8]) -> Result<Zeroizing<Vec<u8>>, ImportError> {
    let (kind, payload) = value.split_once('.').ok_or(ImportError::CorruptSource)?;
    let kind = kind
        .parse::<u32>()
        .map_err(|_| ImportError::CorruptSource)?;
    let parts = payload.split('|').collect::<Vec<_>>();
    match kind {
        0 => {
            if parts.len() != 2 {
                return Err(ImportError::CorruptSource);
            }
            let iv = decode_base64(parts[0])?;
            let ciphertext = decode_base64(parts[1])?;
            aes_256_cbc_decrypt(&key[..32], iv.as_slice(), ciphertext.as_slice())
        }
        2 => {
            if parts.len() != 3 {
                return Err(ImportError::CorruptSource);
            }
            let iv = decode_base64(parts[0])?;
            let ciphertext = decode_base64(parts[1])?;
            let mac = decode_base64(parts[2])?;
            if !hmac_sha256_matches(
                &key[32..64],
                &[iv.as_slice(), ciphertext.as_slice()],
                mac.as_slice(),
            )? {
                return Err(ImportError::InvalidPassword);
            }
            aes_256_cbc_decrypt(&key[..32], iv.as_slice(), ciphertext.as_slice())
        }
        _ => Err(ImportError::UnsupportedEncryption),
    }
}
