use aes::Aes256;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use cbc::Decryptor;
use cbc::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use flate2::read::GzDecoder;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;
use std::io::Read;
use zeroize::Zeroizing;

use super::error::ImportError;

type HmacSha256 = Hmac<Sha256>;
type Aes256CbcDecryptor = Decryptor<Aes256>;

pub(crate) fn decode_base64(value: &str) -> Result<Zeroizing<Vec<u8>>, ImportError> {
    STANDARD
        .decode(value.trim())
        .map(Zeroizing::new)
        .map_err(|_| ImportError::CorruptSource)
}

pub(crate) fn decode_hex(value: &str) -> Result<Vec<u8>, ImportError> {
    hex::decode(value).map_err(|_| ImportError::CorruptSource)
}

pub(crate) fn derive_pbkdf2_sha256(
    password: &str,
    salt: &[u8],
    rounds: u32,
    output: usize,
) -> Result<Zeroizing<Vec<u8>>, ImportError> {
    if !(1_000..=10_000_000).contains(&rounds) || output == 0 || output > 128 {
        return Err(ImportError::UnsupportedEncryption);
    }
    let mut key = Zeroizing::new(vec![0_u8; output]);
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, rounds, key.as_mut());
    Ok(key)
}

pub(crate) fn derive_argon2id(
    password: &str,
    salt: &[u8],
    rounds: u32,
    memory_kib: u32,
    parallelism: u32,
) -> Result<Zeroizing<Vec<u8>>, ImportError> {
    if !(2..=100).contains(&rounds)
        || !(16..=1_048_576).contains(&memory_kib)
        || !(1..=64).contains(&parallelism)
    {
        return Err(ImportError::UnsupportedEncryption);
    }
    let params = argon2::Params::new(memory_kib, rounds, parallelism, Some(32))
        .map_err(|_| ImportError::UnsupportedEncryption)?;
    let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut key = Zeroizing::new(vec![0_u8; 32]);
    argon
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|_| ImportError::UnsupportedEncryption)?;
    Ok(key)
}

pub(crate) fn hkdf_expand_32(input: &[u8], info: &[u8]) -> Result<Zeroizing<Vec<u8>>, ImportError> {
    let hkdf = Hkdf::<Sha256>::from_prk(input).map_err(|_| ImportError::UnsupportedEncryption)?;
    let mut output = Zeroizing::new(vec![0_u8; 32]);
    hkdf.expand(info, output.as_mut())
        .map_err(|_| ImportError::UnsupportedEncryption)?;
    Ok(output)
}

pub(crate) fn aes_256_cbc_decrypt(
    key: &[u8],
    iv: &[u8],
    ciphertext: &[u8],
) -> Result<Zeroizing<Vec<u8>>, ImportError> {
    if key.len() != 32
        || iv.len() != 16
        || ciphertext.is_empty()
        || !ciphertext.len().is_multiple_of(16)
    {
        return Err(ImportError::CorruptSource);
    }
    let decryptor = Aes256CbcDecryptor::new_from_slices(key, iv)
        .map_err(|_| ImportError::UnsupportedEncryption)?;
    decryptor
        .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
        .map(Zeroizing::new)
        .map_err(|_| ImportError::InvalidPassword)
}

pub(crate) fn hmac_sha256_matches(
    key: &[u8],
    parts: &[&[u8]],
    expected: &[u8],
) -> Result<bool, ImportError> {
    let mut mac =
        HmacSha256::new_from_slice(key).map_err(|_| ImportError::UnsupportedEncryption)?;
    for part in parts {
        mac.update(part);
    }
    Ok(mac.verify_slice(expected).is_ok())
}

pub(crate) fn gunzip_base64(value: &str) -> Result<Zeroizing<Vec<u8>>, ImportError> {
    let compressed = decode_base64(value)?;
    let mut decoder = GzDecoder::new(compressed.as_slice());
    let mut output = Zeroizing::new(Vec::new());
    decoder
        .read_to_end(output.as_mut())
        .map_err(|_| ImportError::CorruptSource)?;
    Ok(output)
}
