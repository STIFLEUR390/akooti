use base32::Alphabet;
use totp_rs::{Algorithm, TOTP};
use zeroize::Zeroizing;

use crate::error::VaultError;
use crate::models::SecretString;

/// Hash algorithm used by a TOTP secret.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TotpAlgorithm {
    /// RFC 6238 default.
    #[default]
    Sha1,
    /// Stronger compatible algorithm.
    Sha256,
}

impl TotpAlgorithm {
    const fn crate_algorithm(self) -> Algorithm {
        match self {
            Self::Sha1 => Algorithm::SHA1,
            Self::Sha256 => Algorithm::SHA256,
        }
    }
}

/// Generates a six-digit TOTP code for a Unix timestamp.
///
/// # Errors
///
/// Returns [`VaultError::InvalidInput`] for a Base32 secret that does not
/// decode. A short secret is accepted: services still issue 80-bit ones.
pub fn generate(
    secret: &SecretString,
    algorithm: TotpAlgorithm,
    timestamp: u64,
) -> Result<SecretString, VaultError> {
    let totp = build(secret, algorithm)?;
    Ok(SecretString::new(totp.generate(timestamp)))
}

/// Verifies a six-digit TOTP code with a one-step clock skew.
///
/// # Errors
///
/// Returns [`VaultError::InvalidInput`] for a Base32 secret that does not
/// decode.
pub fn verify(
    secret: &SecretString,
    algorithm: TotpAlgorithm,
    token: &str,
    timestamp: u64,
) -> Result<bool, VaultError> {
    if token.len() != 6 || !token.bytes().all(|byte| byte.is_ascii_digit()) {
        return Ok(false);
    }
    let totp = build(secret, algorithm)?;
    Ok(totp.check(token, timestamp))
}

fn build(secret: &SecretString, algorithm: TotpAlgorithm) -> Result<TOTP, VaultError> {
    let normalized = Zeroizing::new(
        secret
            .expose()
            .chars()
            .filter(|character| !character.is_ascii_whitespace() && *character != '-')
            .flat_map(char::to_uppercase)
            .collect::<String>(),
    );
    let bytes = base32::decode(Alphabet::Rfc4648 { padding: false }, &normalized)
        .map(Zeroizing::new)
        .ok_or(VaultError::InvalidInput("TOTP secret is not valid Base32"))?;
    // `TOTP::new` refuses secrets under 128 bits; RFC 4226 recommends 160 but
    // many services still issue 80-bit ones, so the vault accepts what they give.
    Ok(TOTP::new_unchecked(
        algorithm.crate_algorithm(),
        6,
        1,
        30,
        bytes.to_vec(),
    ))
}
