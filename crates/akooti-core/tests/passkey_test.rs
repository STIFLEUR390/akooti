//! End-to-end behaviour of the vault-side WebAuthn authenticator.
//!
//! Every assertion here is checked the way a relying party would check it:
//! the public key is read back out of the attestation object, and the signature
//! is verified against it with an independent implementation (`p256`), not with
//! the code that produced it.

#![expect(clippy::expect_used, reason = "test fixtures fail immediately")]

mod common;

use akooti_core::{
    McpScope, NewMcpToken, PERSONAL_FOLDER_ID, PasskeyAssertion, PasskeyRegistration, Vault,
    VaultError,
};
use common::fixture;
use p256::EncodedPoint;
use p256::ecdsa::{Signature, VerifyingKey, signature::Verifier};
use p256::elliptic_curve::generic_array::GenericArray;
use sha2::{Digest, Sha256};

const ORIGIN: &str = "https://example.test";
const RP_ID: &str = "example.test";

fn token(scopes: Vec<McpScope>) -> NewMcpToken {
    NewMcpToken {
        name: "Akooti browser extension".to_owned(),
        scopes,
        folder_ids: vec![PERSONAL_FOLDER_ID.to_owned()],
        default_create_folder_id: None,
        expires_at: None,
    }
}

fn registration(challenge: &[u8]) -> PasskeyRegistration {
    PasskeyRegistration {
        origin: ORIGIN.to_owned(),
        rp_id: RP_ID.to_owned(),
        rp_name: "Example".to_owned(),
        user_handle: vec![7_u8; 16],
        user_name: "user@example.test".to_owned(),
        challenge: challenge.to_vec(),
    }
}

fn assertion(challenge: &[u8], credential_id: &[u8]) -> PasskeyAssertion {
    PasskeyAssertion {
        origin: ORIGIN.to_owned(),
        rp_id: RP_ID.to_owned(),
        challenge: challenge.to_vec(),
        allow_credentials: vec![credential_id.to_vec()],
    }
}

/// Reads the `authData` back out of the hand-written CBOR attestation object.
fn auth_data_of(attestation_object: &[u8]) -> Vec<u8> {
    let rp_id_hash = Sha256::digest(RP_ID.as_bytes());
    let start = attestation_object
        .windows(rp_id_hash.len())
        .position(|window| window == rp_id_hash.as_slice())
        .expect("the attestation object carries the relying party hash");
    let auth_data = attestation_object[start..].to_vec();
    // 32-byte hash, flags, 4-byte counter, 16-byte AAGUID, 2-byte length,
    // the credential id, then the COSE key.
    assert_eq!(auth_data[32], 0x41, "UP and AT, never UV");
    assert_eq!(&auth_data[33..37], &[0, 0, 0, 0], "a fresh credential");
    assert_eq!(&auth_data[37..53], &[0_u8; 16], "no AAGUID is claimed");
    let id_len = usize::from(u16::from_be_bytes([auth_data[53], auth_data[54]]));
    assert_eq!(auth_data.len(), 55 + id_len + 77);
    auth_data
}

/// Rebuilds the ES256 public key from the COSE map inside `authData`.
fn public_key_of(auth_data: &[u8]) -> VerifyingKey {
    let id_len = usize::from(u16::from_be_bytes([auth_data[53], auth_data[54]]));
    let cose = &auth_data[55 + id_len..];
    assert_eq!(cose[0], 0xa5, "a five-pair COSE map");
    let x = GenericArray::from_slice(&cose[10..42]);
    let y = GenericArray::from_slice(&cose[45..77]);
    let point = EncodedPoint::from_affine_coordinates(x, y, false);
    VerifyingKey::from_encoded_point(&point).expect("a valid P-256 point")
}

#[test]
fn a_registration_yields_a_credential_the_relying_party_can_verify() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let issued = vault
        .create_mcp_token(token(vec![McpScope::BlindFill]))
        .expect("issue");
    let principal = vault
        .authenticate_mcp_token(&issued.secret)
        .expect("authenticate");

    let challenge = [3_u8; 32];
    let registered = vault
        .passkey_register(&principal, &registration(&challenge))
        .expect("register");

    assert_eq!(registered.credential_id.len(), 16);
    assert_eq!(registered.public_key_algorithm, -7, "ES256");
    assert_eq!(registered.transports, vec!["internal".to_owned()]);
    let client_data = String::from_utf8(registered.client_data_json.clone()).expect("utf-8");
    assert!(client_data.contains("\"type\":\"webauthn.create\""));
    assert!(client_data.contains("\"origin\":\"https://example.test\""));
    assert!(client_data.contains("\"crossOrigin\":false"));

    let auth_data = auth_data_of(&registered.attestation_object);
    let id_len = usize::from(u16::from_be_bytes([auth_data[53], auth_data[54]]));
    assert_eq!(
        &auth_data[55..55 + id_len],
        registered.credential_id.as_slice(),
        "the attested id is the returned id"
    );
    // A usable key: the point parses, which is all a relying party needs to
    // start accepting assertions from this credential.
    let _ = public_key_of(&auth_data);
}

#[test]
fn an_assertion_is_a_signature_over_the_challenge() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let issued = vault
        .create_mcp_token(token(vec![McpScope::BlindFill]))
        .expect("issue");
    let principal = vault
        .authenticate_mcp_token(&issued.secret)
        .expect("authenticate");

    let registered = vault
        .passkey_register(&principal, &registration(&[1_u8; 32]))
        .expect("register");
    let public_key = public_key_of(&auth_data_of(&registered.attestation_object));

    let challenge = [9_u8; 32];
    let signed = vault
        .passkey_assert(
            &principal,
            &assertion(&challenge, &registered.credential_id),
        )
        .expect("assert");

    assert_eq!(signed.credential_id, registered.credential_id);
    assert_eq!(signed.user_handle, Some(vec![7_u8; 16]));
    let client_data = String::from_utf8(signed.client_data_json.clone()).expect("utf-8");
    assert!(client_data.contains("\"type\":\"webauthn.get\""));
    assert!(client_data.contains("\"origin\":\"https://example.test\""));

    // What WebAuthn signs: authenticatorData || SHA-256(clientDataJSON).
    let mut message = signed.authenticator_data.clone();
    message.extend_from_slice(&Sha256::digest(&signed.client_data_json));
    let signature = Signature::from_der(&signed.signature).expect("DER signature");
    public_key
        .verify(&message, &signature)
        .expect("the relying party accepts the signature");

    // The signature covers the challenge: a different one would not verify.
    let mut other = signed.authenticator_data.clone();
    other.extend_from_slice(&Sha256::digest(b"{\"type\":\"webauthn.get\"}"));
    assert!(public_key.verify(&other, &signature).is_err());
}

#[test]
fn the_counter_advances_so_a_relying_party_can_spot_a_clone() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let issued = vault
        .create_mcp_token(token(vec![McpScope::BlindFill]))
        .expect("issue");
    let principal = vault
        .authenticate_mcp_token(&issued.secret)
        .expect("authenticate");
    let registered = vault
        .passkey_register(&principal, &registration(&[2_u8; 32]))
        .expect("register");

    let first = vault
        .passkey_assert(
            &principal,
            &assertion(&[4_u8; 32], &registered.credential_id),
        )
        .expect("first");
    let second = vault
        .passkey_assert(
            &principal,
            &assertion(&[5_u8; 32], &registered.credential_id),
        )
        .expect("second");

    let counter = |data: &[u8]| u32::from_be_bytes([data[33], data[34], data[35], data[36]]);
    assert_eq!(counter(&first.authenticator_data), 1);
    assert_eq!(counter(&second.authenticator_data), 2);
    assert_eq!(second.authenticator_data[32], 0x01, "UP only, never UV");
}

#[test]
fn a_passkey_survives_a_lock_because_its_key_is_encrypted() {
    let (_temp, paths) = fixture();
    let credential_id = {
        let mut vault = Vault::create(&paths, "password").expect("create");
        let issued = vault
            .create_mcp_token(token(vec![McpScope::BlindFill]))
            .expect("issue");
        let principal = vault
            .authenticate_mcp_token(&issued.secret)
            .expect("authenticate");
        vault
            .passkey_register(&principal, &registration(&[6_u8; 32]))
            .expect("register")
            .credential_id
    };

    // Reopening derives the master key again: a private key stored in clear
    // would still work, but a row encrypted under the wrong AAD would not.
    let mut vault = Vault::open(&paths, "password").expect("open");
    let issued = vault
        .create_mcp_token(token(vec![McpScope::BlindFill]))
        .expect("issue");
    let principal = vault
        .authenticate_mcp_token(&issued.secret)
        .expect("authenticate");
    let signed = vault
        .passkey_assert(&principal, &assertion(&[8_u8; 32], &credential_id))
        .expect("assert");
    assert_eq!(signed.credential_id, credential_id);
}

#[test]
fn a_credential_belonging_to_another_relying_party_is_never_found() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let issued = vault
        .create_mcp_token(token(vec![McpScope::BlindFill]))
        .expect("issue");
    let principal = vault
        .authenticate_mcp_token(&issued.secret)
        .expect("authenticate");
    let registered = vault
        .passkey_register(&principal, &registration(&[1_u8; 32]))
        .expect("register");

    // The right credential id, presented for another relying party.
    let elsewhere = PasskeyAssertion {
        origin: "https://other.test".to_owned(),
        rp_id: "other.test".to_owned(),
        challenge: vec![1_u8; 32],
        allow_credentials: vec![registered.credential_id.clone()],
    };
    assert!(matches!(
        vault.passkey_assert(&principal, &elsewhere),
        Err(VaultError::NotFound)
    ));

    // The right relying party, presented for another credential id.
    assert!(matches!(
        vault.passkey_assert(&principal, &assertion(&[1_u8; 32], &[0xaa_u8; 16])),
        Err(VaultError::NotFound)
    ));
}

#[test]
fn a_ceremony_cannot_claim_another_sites_relying_party() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let issued = vault
        .create_mcp_token(token(vec![McpScope::BlindFill]))
        .expect("issue");
    let principal = vault
        .authenticate_mcp_token(&issued.secret)
        .expect("authenticate");

    let mut stolen = registration(&[1_u8; 32]);
    stolen.rp_id = "bank.test".to_owned();
    assert!(matches!(
        vault.passkey_register(&principal, &stolen),
        Err(VaultError::InvalidInput(_))
    ));

    let mut sibling = registration(&[1_u8; 32]);
    sibling.rp_id = "notexample.test".to_owned();
    assert!(matches!(
        vault.passkey_register(&principal, &sibling),
        Err(VaultError::InvalidInput(_))
    ));

    // An insecurely served page has no WebAuthn at all.
    let mut insecure = registration(&[1_u8; 32]);
    insecure.origin = "http://example.test".to_owned();
    assert!(matches!(
        vault.passkey_register(&principal, &insecure),
        Err(VaultError::InvalidInput(_))
    ));
}

#[test]
fn a_token_without_blind_fill_cannot_touch_a_passkey() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let issued = vault
        .create_mcp_token(token(vec![McpScope::ReadMetadata]))
        .expect("issue");
    let principal = vault
        .authenticate_mcp_token(&issued.secret)
        .expect("authenticate");

    assert!(matches!(
        vault.passkey_register(&principal, &registration(&[1_u8; 32])),
        Err(VaultError::Forbidden)
    ));
    assert!(matches!(
        vault.passkey_assert(&principal, &assertion(&[1_u8; 32], &[0xbb_u8; 16])),
        Err(VaultError::Forbidden)
    ));
}

#[test]
fn an_assertion_without_a_listed_credential_is_refused() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let issued = vault
        .create_mcp_token(token(vec![McpScope::BlindFill]))
        .expect("issue");
    let principal = vault
        .authenticate_mcp_token(&issued.secret)
        .expect("authenticate");

    // No account chooser here: an empty `allowCredentials` is not a request
    // this authenticator can serve, and saying so is better than guessing.
    let discoverable = PasskeyAssertion {
        origin: ORIGIN.to_owned(),
        rp_id: RP_ID.to_owned(),
        challenge: vec![1_u8; 32],
        allow_credentials: Vec::new(),
    };
    assert!(matches!(
        vault.passkey_assert(&principal, &discoverable),
        Err(VaultError::InvalidInput(_))
    ));
}
