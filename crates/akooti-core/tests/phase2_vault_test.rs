mod common;

use akooti_core::{
    ItemUriInput, SecretString, TotpAlgorithm, UriMatchType, Vault, VaultError, generate_totp_code,
};
use common::{fixture, item_draft};

const SHA1_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
const SHA256_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZA";

#[test]
fn totp_matches_rfc_6238_vectors() {
    let sha1 = SecretString::new(SHA1_SECRET.to_owned());
    let sha256 = SecretString::new(SHA256_SECRET.to_owned());
    assert_eq!(
        generate_totp_code(&sha1, TotpAlgorithm::Sha1, 59)
            .expect("sha1")
            .expose(),
        "287082"
    );
    assert_eq!(
        generate_totp_code(&sha256, TotpAlgorithm::Sha256, 59)
            .expect("sha256")
            .expose(),
        "119246"
    );
}

#[test]
fn vault_totp_uses_encrypted_item_secret() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let mut draft = item_draft("TOTP", "password", "https://example.test");
    draft.totp_secret = Some(SecretString::new(SHA1_SECRET.to_owned()));
    let item = vault.create_item(draft).expect("item");

    let code = vault
        .generate_totp(&item.id, TotpAlgorithm::Sha1, 59)
        .expect("generate");
    assert_eq!(code.expose(), "287082");
    assert!(
        vault
            .verify_totp(&item.id, TotpAlgorithm::Sha1, "287082", 59)
            .expect("verify")
    );
    assert!(
        !vault
            .verify_totp(&item.id, TotpAlgorithm::Sha1, "000000", 59)
            .expect("reject")
    );
}

#[test]
fn an_eighty_bit_secret_still_produces_a_code() {
    // Google Authenticator's default is 80 bits, below the 128 `totp-rs` insists
    // on; the vault must not refuse the secrets services actually issue.
    let short = SecretString::new("JBSWY3DPEHPK3PXP".to_owned());
    let code = generate_totp_code(&short, TotpAlgorithm::Sha1, 59).expect("generate");
    assert_eq!(code.expose().len(), 6);
    assert!(code.expose().bytes().all(|byte| byte.is_ascii_digit()));
}

#[test]
fn a_secret_that_is_not_base32_is_rejected() {
    let broken = SecretString::new("not base32!".to_owned());
    assert!(matches!(
        generate_totp_code(&broken, TotpAlgorithm::Sha1, 59),
        Err(VaultError::InvalidInput(_))
    ));
}

#[test]
fn password_history_only_archives_actual_changes() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let item = vault
        .create_item(item_draft("History", "first", "https://history.test"))
        .expect("item");

    let mut second = item_draft("History", "second", "https://history.test");
    second.folder_ids = item.folder_ids.clone();
    vault.update_item(&item.id, second).expect("second");
    assert_eq!(vault.password_history_count(&item.id).expect("count"), 1);

    let mut same = item_draft("History", "second", "https://history.test");
    same.folder_ids = item.folder_ids.clone();
    vault.update_item(&item.id, same).expect("same");
    assert_eq!(vault.password_history_count(&item.id).expect("count"), 1);

    let mut third = item_draft("History", "third", "https://history.test");
    third.folder_ids = item.folder_ids.clone();
    vault.update_item(&item.id, third).expect("third");
    assert_eq!(vault.password_history_count(&item.id).expect("count"), 2);
}

#[test]
fn ephemeral_items_receive_ttl_and_are_purged() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let mut ephemeral = item_draft("Ephemeral", "password", "https://ephemeral.test");
    ephemeral.created_by_agent = Some("codex".to_owned());
    let item = vault.create_item(ephemeral).expect("ephemeral");
    assert!(item.expires_at.is_some());

    let mut expired = item_draft("Expired", "password", "https://expired.test");
    expired.created_by_agent = Some("codex".to_owned());
    expired.expires_at = Some(1);
    vault.create_item(expired).expect("expired");
    assert_eq!(vault.purge_expired(2).expect("purge"), 1);
    assert!(vault.get_item(&item.id).is_ok());
}

#[test]
fn invalid_selectors_and_regex_uris_are_rejected() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");

    let mut invalid_selector = item_draft("Selector", "password", "https://selector.test");
    invalid_selector.selectors.password = Some("input\n[type=password]".to_owned());
    assert!(matches!(
        vault.create_item(invalid_selector),
        Err(VaultError::InvalidInput(_))
    ));

    let mut invalid_regex = item_draft("Regex", "password", "https://regex.test");
    invalid_regex.uris = vec![ItemUriInput {
        uri: "(".to_owned(),
        match_type: UriMatchType::Regex,
    }];
    assert!(matches!(
        vault.create_item(invalid_regex),
        Err(VaultError::InvalidInput(_))
    ));
}
