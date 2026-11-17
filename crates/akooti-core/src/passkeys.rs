//! Vault-side WebAuthn authenticator.
//!
//! A relying party asks the browser for a credential; the extension relays the
//! ceremony here and the vault answers with a real ES256 signature instead of
//! leaving it to the platform authenticator. What this module deliberately is
//! **not**, because a relying party is entitled to know what it talks to:
//!
//! - **not attested**: `fmt: "none"`. Nothing claims to be a certified
//!   authenticator, so a site stays free to apply its own policy.
//! - **not user-verifying**: the `UV` flag is never set. The vault unlocks with
//!   the user's password and is not a sensor of its own; the extension refuses
//!   a ceremony asking for `userVerification: "required"` before it reaches
//!   this module, and a relying party that looks at the flags sees the truth
//!   (`docs/architecture.md` §4.6).
//! - **not discoverable**: only the credential ids listed in
//!   `allowCredentials` can be answered. There is no account chooser on the
//!   daemon side, so an assertion that lists nothing is refused.
//!
//! What it does keep: one P-256 private scalar per credential, encrypted with
//! the vault master key under the AAD `akooti:v1:passkey:<credential id>`. A
//! stolen row is useless without the master key, and a row moved onto another
//! credential stops authenticating. The public key is never stored: it is
//! re-derived from the scalar, so there is one thing to protect instead of two
//! that could disagree.

use std::net::IpAddr;
use std::time::SystemTime;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use rand_core::{OsRng, RngCore};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use url::Url;
use zeroize::Zeroizing;

use crate::crypto::{NONCE_LEN, decrypt_bytes, encrypt_bytes, passkey_aad};
use crate::error::VaultError;
use crate::mcp::token::{McpPrincipal, McpScope};
use crate::vault::{Vault, unix_seconds};

/// COSE algorithm identifier of ES256: ECDSA over P-256 with SHA-256.
pub const ES256: i64 = -7;

/// Byte length of the credential ids minted here.
pub const CREDENTIAL_ID_LEN: usize = 16;

/// Flag byte of a registration: user present (`0x01`) plus attested data
/// (`0x40`). `UV` (`0x04`) and `ED` (`0x80`) are never set.
const FLAGS_ATTESTED: u8 = 0x41;

/// Flag byte of an assertion: user present only.
const FLAGS_USER_PRESENT: u8 = 0x01;

/// AAGUID of an authenticator that asserts no model, per WebAuthn §6.5.4.
const AAGUID_NONE: [u8; 16] = [0_u8; 16];

/// Shortest challenge accepted, as required by WebAuthn §13.4.3.
const MIN_CHALLENGE_LEN: usize = 16;
/// Longest challenge accepted; a ceremony carrying more is not a browser's.
const MAX_CHALLENGE_LEN: usize = 1024;
/// Longest user handle accepted, as required by WebAuthn §5.4.1.
const MAX_USER_HANDLE_LEN: usize = 64;
/// Longest relying party name stored.
const MAX_RP_NAME_LEN: usize = 128;
/// Longest relying party id accepted; DNS puts the ceiling at 253.
const MAX_RP_ID_LEN: usize = 253;
/// Longest account name stored.
const MAX_USER_NAME_LEN: usize = 253;

/// One registration ceremony, as relayed by the paired extension.
#[derive(Debug, Clone)]
pub struct PasskeyRegistration {
    /// Origin of the page asking, in serialized form (`https://example.test`).
    pub origin: String,
    /// Relying party id; it must belong to `origin`.
    pub rp_id: String,
    /// Human-readable relying party name, stored as given.
    pub rp_name: String,
    /// Opaque user handle chosen by the relying party.
    pub user_handle: Vec<u8>,
    /// Account name the relying party shows, stored for the vault UI.
    pub user_name: String,
    /// Challenge to echo in `clientDataJSON`.
    pub challenge: Vec<u8>,
}

/// Everything a relying party needs back from a registration: no secret.
#[derive(Debug, Clone)]
pub struct RegisteredPasskey {
    /// Identifier the relying party will store and present later.
    pub credential_id: Vec<u8>,
    /// `clientDataJSON` the credential is bound to.
    pub client_data_json: Vec<u8>,
    /// CBOR `attestationObject`, with `fmt: "none"`.
    pub attestation_object: Vec<u8>,
    /// COSE algorithm of the credential key; always [`ES256`].
    pub public_key_algorithm: i64,
    /// Transports reported to the page.
    pub transports: Vec<String>,
}

/// One assertion ceremony, as relayed by the paired extension.
#[derive(Debug, Clone)]
pub struct PasskeyAssertion {
    /// Origin of the page asking, in serialized form.
    pub origin: String,
    /// Relying party id; it must belong to `origin`.
    pub rp_id: String,
    /// Challenge to echo in `clientDataJSON`.
    pub challenge: Vec<u8>,
    /// Credential ids the relying party accepts; an empty list is refused.
    pub allow_credentials: Vec<Vec<u8>>,
}

/// Signature produced for one assertion: no secret either.
#[derive(Debug, Clone)]
pub struct SignedAssertion {
    /// Credential that signed.
    pub credential_id: Vec<u8>,
    /// `clientDataJSON` the signature covers.
    pub client_data_json: Vec<u8>,
    /// `authenticatorData` the signature covers.
    pub authenticator_data: Vec<u8>,
    /// DER-encoded ECDSA signature, as WebAuthn requires.
    pub signature: Vec<u8>,
    /// User handle stored with the credential, echoed back to the page.
    pub user_handle: Option<Vec<u8>>,
}

impl Vault {
    /// Registered a new credential for one relying party.
    ///
    /// The credential belongs to the vault, not to an item: a passkey has no
    /// folder, so the token's folder allowlist cannot narrow it. The gates are
    /// therefore the `blind_fill` scope, the `native-host` connection declared
    /// by the caller, and the relying party id belonging to the origin — the
    /// last one is checked here and not only in the extension.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Forbidden`] without `blind_fill`, and
    /// [`VaultError::InvalidInput`] for a relying party that does not belong to
    /// the origin or a parameter outside the WebAuthn bounds.
    pub fn passkey_register(
        &self,
        principal: &McpPrincipal,
        request: &PasskeyRegistration,
    ) -> Result<RegisteredPasskey, VaultError> {
        if !principal.has_scope(McpScope::BlindFill) {
            return Err(VaultError::Forbidden);
        }
        let rp_id = checked_relying_party(&request.origin, &request.rp_id)?;
        check_registration(request)?;

        let mut credential_id = vec![0_u8; CREDENTIAL_ID_LEN];
        OsRng.fill_bytes(&mut credential_id);
        let key = SigningKey::random(&mut OsRng);
        let point = key.verifying_key().to_encoded_point(false);
        let (x, y) = match (point.x(), point.y()) {
            (Some(x), Some(y)) => (x, y),
            _ => return Err(VaultError::Crypto),
        };

        let client_data_json =
            client_data_json("webauthn.create", &request.challenge, &request.origin);
        // AAGUID, credential id length, credential id, COSE key.
        let mut attested = Vec::with_capacity(16 + 2 + CREDENTIAL_ID_LEN + 77);
        attested.extend_from_slice(&AAGUID_NONE);
        attested.extend_from_slice(
            &u16::try_from(credential_id.len())
                .map_err(|_| VaultError::CorruptVault)?
                .to_be_bytes(),
        );
        attested.extend_from_slice(&credential_id);
        attested.extend_from_slice(&cose_key(x, y));
        let auth_data = authenticator_data(&rp_id, FLAGS_ATTESTED, 0, &attested);

        let scalar = Zeroizing::new(key.to_bytes().to_vec());
        let encrypted = encrypt_bytes(
            self.master_key(),
            scalar.as_slice(),
            &passkey_aad(&credential_id),
        )?;
        let now = unix_seconds(SystemTime::now())?;
        self.connection().execute(
            "INSERT INTO passkeys (
                 id, item_id, credential_id, rp_id, rp_name, user_handle, user_name,
                 encrypted_private_key, private_key_nonce, sign_count, created_at,
                 last_used_at, deleted_at
             ) VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9, NULL, NULL)",
            params![
                uuid::Uuid::new_v4().to_string(),
                credential_id,
                rp_id,
                request.rp_name,
                request.user_handle,
                request.user_name,
                encrypted.ciphertext,
                encrypted.nonce.as_slice(),
                now,
            ],
        )?;

        Ok(RegisteredPasskey {
            credential_id,
            client_data_json,
            attestation_object: attestation_object(&auth_data),
            public_key_algorithm: ES256,
            transports: vec!["internal".to_owned()],
        })
    }

    /// Signs one assertion with a credential stored in the vault.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Forbidden`] without `blind_fill`,
    /// [`VaultError::InvalidInput`] for a relying party that does not belong to
    /// the origin, and [`VaultError::NotFound`] when none of the listed
    /// credentials belongs to this relying party.
    pub fn passkey_assert(
        &self,
        principal: &McpPrincipal,
        request: &PasskeyAssertion,
    ) -> Result<SignedAssertion, VaultError> {
        if !principal.has_scope(McpScope::BlindFill) {
            return Err(VaultError::Forbidden);
        }
        let rp_id = checked_relying_party(&request.origin, &request.rp_id)?;
        check_challenge(&request.challenge)?;
        if request.allow_credentials.is_empty() {
            // Without a listed credential there is nothing to select: this
            // authenticator is not discoverable and has no account chooser.
            return Err(VaultError::InvalidInput(
                "the ceremony lists no credential to assert with",
            ));
        }

        let stored = find_passkey(self.connection(), &rp_id, &request.allow_credentials)?
            .ok_or(VaultError::NotFound)?;
        let scalar = decrypt_bytes(
            self.master_key(),
            &stored.encrypted,
            &passkey_aad(&stored.credential_id),
        )?;
        let key =
            SigningKey::from_slice(scalar.as_slice()).map_err(|_| VaultError::CorruptVault)?;

        let sign_count = stored.sign_count.wrapping_add(1);
        let auth_data = authenticator_data(&rp_id, FLAGS_USER_PRESENT, sign_count, &[]);
        let client_data_json =
            client_data_json("webauthn.get", &request.challenge, &request.origin);
        let mut signed = Vec::with_capacity(auth_data.len() + 32);
        signed.extend_from_slice(&auth_data);
        signed.extend_from_slice(&Sha256::digest(&client_data_json));
        let signature: Signature = key.sign(&signed);

        let now = unix_seconds(SystemTime::now())?;
        self.connection().execute(
            "UPDATE passkeys SET sign_count = ?2, last_used_at = ?3 WHERE id = ?1",
            params![stored.row_id, sign_count, now],
        )?;

        Ok(SignedAssertion {
            credential_id: stored.credential_id,
            client_data_json,
            authenticator_data: auth_data,
            signature: signature.to_der().as_bytes().to_vec(),
            user_handle: Some(stored.user_handle),
        })
    }
}

/// Row of `passkeys` needed to answer one assertion.
struct StoredPasskey {
    row_id: String,
    credential_id: Vec<u8>,
    user_handle: Vec<u8>,
    encrypted: crate::crypto::EncryptedValue,
    sign_count: u32,
}

/// Finds the first listed credential that belongs to this relying party.
fn find_passkey(
    connection: &Connection,
    rp_id: &str,
    candidates: &[Vec<u8>],
) -> Result<Option<StoredPasskey>, VaultError> {
    let mut statement = connection.prepare(
        "SELECT id, credential_id, user_handle, encrypted_private_key, private_key_nonce,
                sign_count
         FROM passkeys
         WHERE credential_id = ?1 AND rp_id = ?2 AND deleted_at IS NULL",
    )?;
    for candidate in candidates {
        let row = statement
            .query_row(params![candidate.as_slice(), rp_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })
            .optional()?;
        let Some((row_id, credential_id, user_handle, ciphertext, nonce, sign_count)) = row else {
            continue;
        };
        let nonce: [u8; NONCE_LEN] = nonce.try_into().map_err(|_| VaultError::CorruptVault)?;
        let sign_count = u32::try_from(sign_count).map_err(|_| VaultError::CorruptVault)?;
        return Ok(Some(StoredPasskey {
            row_id,
            credential_id,
            user_handle,
            encrypted: crate::crypto::EncryptedValue { ciphertext, nonce },
            sign_count,
        }));
    }
    Ok(None)
}

/// Validates the relying party id and returns it lowercased.
///
/// The WebAuthn rule is "equal to, or a registrable suffix of, the origin host".
/// It is checked here because the page controls the `rpId` it puts in the
/// options: the extension checks it too, but only the daemon can make it a gate.
fn checked_relying_party(origin: &str, rp_id: &str) -> Result<String, VaultError> {
    let host = checked_origin_host(origin)?;
    let rp_id = rp_id.trim().trim_end_matches('.').to_ascii_lowercase();
    if rp_id.is_empty() || rp_id.len() > MAX_RP_ID_LEN {
        return Err(VaultError::InvalidInput(
            "the relying party id is empty or too long",
        ));
    }
    if rp_id.parse::<IpAddr>().is_ok() {
        return Err(VaultError::InvalidInput(
            "a relying party id cannot be an IP address",
        ));
    }
    // A single label is never a registrable suffix: `com` must not be accepted.
    let allowed = host == rp_id || (rp_id.contains('.') && host.ends_with(&format!(".{rp_id}")));
    if !allowed {
        return Err(VaultError::InvalidInput(
            "the relying party does not belong to the origin",
        ));
    }
    Ok(rp_id)
}

/// Checks that `origin` is a serialized origin in a secure context, and returns
/// its host lowercased.
fn checked_origin_host(origin: &str) -> Result<String, VaultError> {
    let parsed = Url::parse(origin)
        .map_err(|_| VaultError::InvalidInput("the ceremony origin is not a URL"))?;
    // Only the serialized form is accepted: no path, no query, no uppercase
    // host. That is what `location.origin` produces, and it leaves nothing to
    // normalise before it is echoed into `clientDataJSON`.
    if parsed.origin().ascii_serialization() != origin {
        return Err(VaultError::InvalidInput(
            "the ceremony origin is not a serialized origin",
        ));
    }
    let host = parsed
        .host_str()
        .ok_or(VaultError::InvalidInput("the ceremony origin has no host"))?
        .to_ascii_lowercase();
    let secure = parsed.scheme() == "https"
        || (parsed.scheme() == "http"
            && matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1"));
    if !secure {
        return Err(VaultError::InvalidInput(
            "the ceremony origin is not a secure context",
        ));
    }
    Ok(host)
}

/// Validates the registration parameters the relying party supplied.
fn check_registration(request: &PasskeyRegistration) -> Result<(), VaultError> {
    check_challenge(&request.challenge)?;
    if request.user_handle.is_empty() || request.user_handle.len() > MAX_USER_HANDLE_LEN {
        return Err(VaultError::InvalidInput(
            "the user handle must be between 1 and 64 bytes",
        ));
    }
    if request.user_name.is_empty() || request.user_name.len() > MAX_USER_NAME_LEN {
        return Err(VaultError::InvalidInput(
            "the account name is empty or too long",
        ));
    }
    if request.rp_name.is_empty() || request.rp_name.len() > MAX_RP_NAME_LEN {
        return Err(VaultError::InvalidInput(
            "the relying party name is empty or too long",
        ));
    }
    Ok(())
}

/// Validates the challenge, which the relying party picks.
fn check_challenge(challenge: &[u8]) -> Result<(), VaultError> {
    if challenge.len() < MIN_CHALLENGE_LEN || challenge.len() > MAX_CHALLENGE_LEN {
        return Err(VaultError::InvalidInput(
            "the challenge must be between 16 and 1024 bytes",
        ));
    }
    Ok(())
}

/// Builds the `clientDataJSON` the ceremony is bound to.
///
/// Nothing here needs escaping: the origin is a serialized origin (validated
/// above), the challenge is base64url, and the ceremony type is a literal.
fn client_data_json(ceremony: &str, challenge: &[u8], origin: &str) -> Vec<u8> {
    let challenge = URL_SAFE_NO_PAD.encode(challenge);
    format!(
        "{{\"type\":\"{ceremony}\",\"challenge\":\"{challenge}\",\"origin\":\"{origin}\",\
         \"crossOrigin\":false}}"
    )
    .into_bytes()
}

/// Builds `authenticatorData`, with attested data when registering.
fn authenticator_data(rp_id: &str, flags: u8, sign_count: u32, attested: &[u8]) -> Vec<u8> {
    let mut data = Vec::with_capacity(37 + attested.len());
    data.extend_from_slice(&Sha256::digest(rp_id.as_bytes()));
    data.push(flags);
    data.extend_from_slice(&sign_count.to_be_bytes());
    data.extend_from_slice(attested);
    data
}

/// Builds the CBOR `attestationObject` of a `fmt: "none"` attestation.
///
/// Written by hand rather than with a CBOR crate: it is three keys and one
/// byte string, all known at compile time, and a decoder would be a dependency
/// to trust for nothing.
fn attestation_object(auth_data: &[u8]) -> Vec<u8> {
    let mut cbor = Vec::with_capacity(auth_data.len() + 32);
    // Keys in canonical order (shorter first): "fmt", "attStmt", "authData".
    cbor_head(&mut cbor, 5, 3);
    cbor_text(&mut cbor, "fmt");
    cbor_text(&mut cbor, "none");
    cbor_text(&mut cbor, "attStmt");
    cbor_head(&mut cbor, 5, 0);
    cbor_text(&mut cbor, "authData");
    cbor_bytes(&mut cbor, auth_data);
    cbor
}

/// Builds the COSE key of an ES256 credential (EC2, P-256, x and y).
fn cose_key(x: &[u8], y: &[u8]) -> Vec<u8> {
    let mut cbor = Vec::with_capacity(77);
    cbor_head(&mut cbor, 5, 5);
    // 1: kty, 2: EC2
    cbor_head(&mut cbor, 0, 1);
    cbor_head(&mut cbor, 0, 2);
    // 3: alg, -7: ES256
    cbor_head(&mut cbor, 0, 3);
    cbor_negative(&mut cbor, ES256);
    // -1: crv, 1: P-256
    cbor_negative(&mut cbor, -1);
    cbor_head(&mut cbor, 0, 1);
    // -2: x, -3: y
    cbor_negative(&mut cbor, -2);
    cbor_bytes(&mut cbor, x);
    cbor_negative(&mut cbor, -3);
    cbor_bytes(&mut cbor, y);
    cbor
}

/// Appends one CBOR head: major type plus a length or a small value.
fn cbor_head(out: &mut Vec<u8>, major: u8, value: u64) {
    let major = major << 5;
    if value < 24 {
        out.push(major | u8::try_from(value).unwrap_or(23));
    } else if let Ok(small) = u8::try_from(value) {
        out.push(major | 24);
        out.push(small);
    } else if let Ok(small) = u16::try_from(value) {
        out.push(major | 25);
        out.extend_from_slice(&small.to_be_bytes());
    } else if let Ok(small) = u32::try_from(value) {
        out.push(major | 26);
        out.extend_from_slice(&small.to_be_bytes());
    } else {
        out.push(major | 27);
        out.extend_from_slice(&value.to_be_bytes());
    }
}

/// Appends one CBOR byte string.
fn cbor_bytes(out: &mut Vec<u8>, value: &[u8]) {
    cbor_head(out, 2, u64::try_from(value.len()).unwrap_or(u64::MAX));
    out.extend_from_slice(value);
}

/// Appends one CBOR text string.
fn cbor_text(out: &mut Vec<u8>, value: &str) {
    cbor_head(out, 3, u64::try_from(value.len()).unwrap_or(u64::MAX));
    out.extend_from_slice(value.as_bytes());
}

/// Appends one CBOR negative integer.
fn cbor_negative(out: &mut Vec<u8>, value: i64) {
    let encoded = value.unsigned_abs().saturating_sub(1);
    cbor_head(out, 1, encoded);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cose_key_is_a_five_pair_ec2_map() {
        let key = cose_key(&[1_u8; 32], &[2_u8; 32]);
        assert_eq!(key.len(), 77);
        assert_eq!(key[0], 0xa5, "map of five pairs");
        assert_eq!(
            &key[1..10],
            &[0x01, 0x02, 0x03, 0x26, 0x20, 0x01, 0x21, 0x58, 0x20]
        );
        assert_eq!(&key[10..42], &[1_u8; 32], "x");
        assert_eq!(
            &key[42..45],
            &[0x22, 0x58, 0x20],
            "-3 then a 32-byte string"
        );
        assert_eq!(&key[45..], &[2_u8; 32], "y");
    }

    #[test]
    fn an_attestation_object_states_no_attestation() {
        let cbor = attestation_object(&[9_u8; 37]);
        assert_eq!(cbor[0], 0xa3, "map of three pairs");
        assert!(cbor.windows(3).any(|window| window == b"fmt"));
        assert!(cbor.windows(4).any(|window| window == b"none"));
        assert!(cbor.windows(7).any(|window| window == b"attStmt"));
        assert!(cbor.windows(8).any(|window| window == b"authData"));
        // The empty statement is an empty map, not a missing key.
        assert!(cbor.windows(2).any(|window| window == [0xa0, 0x68]));
    }

    #[test]
    fn authenticator_data_carries_the_relying_party_hash_and_flags() {
        let data = authenticator_data("example.test", FLAGS_USER_PRESENT, 7, &[]);
        assert_eq!(data.len(), 37);
        assert_eq!(&data[..32], Sha256::digest(b"example.test").as_slice());
        assert_eq!(data[32], 0x01, "UP only: no UV was performed");
        assert_eq!(&data[33..], &7_u32.to_be_bytes());
    }

    #[test]
    fn client_data_json_binds_type_challenge_and_origin() {
        let data = client_data_json("webauthn.get", &[0xff_u8; 2], "https://example.test");
        let text = String::from_utf8(data).expect("utf-8");
        assert_eq!(
            text,
            "{\"type\":\"webauthn.get\",\"challenge\":\"__8\",\
             \"origin\":\"https://example.test\",\"crossOrigin\":false}"
        );
    }

    #[test]
    fn a_relying_party_must_belong_to_the_origin() {
        assert_eq!(
            checked_relying_party("https://example.test", "example.test").expect("equal"),
            "example.test"
        );
        assert_eq!(
            checked_relying_party("https://app.example.test", "Example.Test").expect("suffix"),
            "example.test"
        );
        for (origin, rp_id) in [
            ("https://example.test", "other.test"),
            ("https://example.test", "com"),
            ("https://notexample.test", "example.test"),
            ("https://example.test", "192.0.2.1"),
            ("https://example.test", ""),
        ] {
            assert!(
                matches!(
                    checked_relying_party(origin, rp_id),
                    Err(VaultError::InvalidInput(_))
                ),
                "{origin} / {rp_id} should be refused"
            );
        }
    }

    #[test]
    fn an_origin_must_be_serialized_and_secure() {
        assert!(checked_origin_host("https://example.test").is_ok());
        assert!(checked_origin_host("http://127.0.0.1:8798").is_ok());
        assert!(checked_origin_host("http://localhost:3000").is_ok());
        // Everything else is refused: plain HTTP off-loopback, a path or a
        // query in what claims to be an origin, a host that is not lowercase.
        for origin in [
            "http://example.test",
            "https://example.test/path",
            "https://example.test?query",
            "https://Example.Test",
            "file://example.test",
            "not a url",
        ] {
            assert!(
                matches!(
                    checked_origin_host(origin),
                    Err(VaultError::InvalidInput(_))
                ),
                "{origin} should be refused"
            );
        }
    }

    #[test]
    fn a_short_challenge_is_refused() {
        assert!(check_challenge(&[0_u8; 15]).is_err());
        assert!(check_challenge(&[0_u8; 16]).is_ok());
        assert!(check_challenge(&[0_u8; 1025]).is_err());
    }
}
