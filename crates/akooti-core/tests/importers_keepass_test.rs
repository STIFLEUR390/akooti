mod common;

use akooti_core::{ImportError, ImportFormat, ImportOptions, SecretString, Vault};
use common::fixture;
use keepass::db::fields;

#[test]
fn kdbx4_opens_with_the_export_password_and_rejects_invalid_or_missing_password() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let database = kdbx_fixture();
    let password = SecretString::new("kdbx-password".to_owned());
    let report = vault
        .import_bytes(
            ImportFormat::KeePass,
            &database,
            Some(&password),
            ImportOptions::default(),
        )
        .expect("valid password");
    assert_eq!(report.imported, 1);

    let wrong = SecretString::new("wrong password".to_owned());
    assert!(matches!(
        vault.import_bytes(
            ImportFormat::KeePass,
            &database,
            Some(&wrong),
            ImportOptions::default(),
        ),
        Err(ImportError::InvalidPassword)
    ));
    assert!(matches!(
        vault.import_bytes(
            ImportFormat::KeePass,
            &database,
            None,
            ImportOptions::default(),
        ),
        Err(ImportError::PasswordRequired)
    ));
}

#[expect(
    clippy::expect_used,
    reason = "synthetic KDBX fixture construction fails the test immediately"
)]
fn kdbx_fixture() -> Vec<u8> {
    let mut database = keepass::Database::new();
    {
        let mut root = database.root_mut();
        let mut group = root.add_group();
        group.name = "Work".to_owned();
        let mut entry = group.add_entry();
        entry.set_unprotected(fields::TITLE, "KeePass Entry");
        entry.set_unprotected(fields::USERNAME, "keepass@example.test");
        entry.set_protected(fields::PASSWORD, "fixture-password");
        entry.set_unprotected(fields::URL, "http://keepass.local");
        entry.set_unprotected(fields::OTP, "JBSWY3DPEHPK3PXP");
        entry.set_unprotected("API_TOKEN", "fixture-token");
    }
    let mut output = Vec::new();
    database
        .save(
            &mut output,
            keepass::DatabaseKey::new().with_password("kdbx-password"),
        )
        .expect("save kdbx");
    output
}
