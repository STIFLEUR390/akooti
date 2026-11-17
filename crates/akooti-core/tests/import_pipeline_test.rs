mod common;

use akooti_core::{
    DuplicatePolicy, ImportBundle, ImportCleanup, ImportFormat, ImportItem, ImportOptions,
    SecretString, Selectors, Vault, VaultItemDraft,
};
use common::{fixture, item_draft};

#[test]
fn preview_detects_url_and_fuzzy_title_duplicates_and_policy_skips_them() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let existing = vault
        .create_item(item_draft(
            "Production Login",
            "fixture-password",
            "https://duplicate.example.test/login",
        ))
        .expect("existing");
    let bundle = bundle_with(vec![
        draft_item(
            "Copy",
            "Production Login",
            "https://duplicate.example.test/login",
            "duplicate",
        ),
        draft_item(
            "Copy 2",
            "Production Login!",
            "https://other.example.test",
            "other",
        ),
    ]);
    let preview = vault.preview_import(&bundle).expect("preview");
    assert_eq!(preview.duplicates.len(), 2);
    assert_eq!(preview.duplicates[0].existing_item_id, existing.id);
    let report = vault
        .import_bundle(bundle, ImportOptions::default())
        .expect("skip duplicates");
    assert_eq!(report.imported, 0);
    assert_eq!(report.skipped, 2);
    let forced = bundle_with(vec![draft_item(
        "Copy 3",
        "Production Login",
        "https://duplicate.example.test/login",
        "duplicate",
    )]);
    let options = ImportOptions {
        duplicate_policy: DuplicatePolicy::ImportAnyway,
        ..ImportOptions::default()
    };
    assert_eq!(
        vault
            .import_bundle(forced, options)
            .expect("import anyway")
            .imported,
        1
    );
}

#[test]
fn transactional_import_rolls_back_every_item_after_validation_failure() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let bundle = bundle_with(vec![
        draft_item(
            "First",
            "Valid first",
            "https://first.example.test",
            "first",
        ),
        draft_item("Second", "", "https://second.example.test", "second"),
    ]);
    let result = vault.import_bundle(bundle, ImportOptions::default());
    assert!(result.is_err());
    assert!(
        vault
            .list_items(&Default::default())
            .expect("items")
            .is_empty()
    );
}

#[test]
fn secure_delete_runs_only_after_committed_file_import() {
    let (temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let source = temp.path().join("browser.csv");
    std::fs::write(
        &source,
        b"name,url,username,password\nLocal,http://secure-delete.local,user,fixture-password\n",
    )
    .expect("source");
    let options = ImportOptions {
        secure_delete_source: true,
        ..ImportOptions::default()
    };
    let report = vault
        .import_file(&source, Some(ImportFormat::BrowserCsv), None, options)
        .expect("import file");
    assert_eq!(report.imported, 1);
    assert!(matches!(report.cleanup, ImportCleanup::Completed(_)));
    assert!(!source.exists());
}

#[test]
fn secure_delete_does_not_run_when_parsing_fails() {
    let (temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let source = temp.path().join("broken.kdbx");
    std::fs::write(&source, b"not a kdbx file").expect("source");
    let options = ImportOptions {
        secure_delete_source: true,
        ..ImportOptions::default()
    };
    let password = SecretString::new("fixture-password".to_owned());
    assert!(
        vault
            .import_file(
                &source,
                Some(ImportFormat::KeePass),
                Some(&password),
                options,
            )
            .is_err()
    );
    assert!(source.exists());
}

fn bundle_with(items: Vec<ImportItem>) -> ImportBundle {
    ImportBundle {
        format: ImportFormat::BrowserCsv,
        folders: Vec::new(),
        items,
        issues: Vec::new(),
    }
}

fn draft_item(external_id: &str, title: &str, url: &str, password: &str) -> ImportItem {
    ImportItem {
        external_id: external_id.to_owned(),
        draft: VaultItemDraft {
            item_type: akooti_core::ItemType::Login,
            title: title.to_owned(),
            primary_url: Some(url.to_owned()),
            username: Some("agent@example.test".to_owned()),
            password: Some(SecretString::new(password.to_owned())),
            totp_secret: None,
            notes: None,
            selectors: Selectors::default(),
            created_by_agent: None,
            expires_at: None,
            favorite: false,
            folder_ids: vec![akooti_core::PERSONAL_FOLDER_ID.to_owned()],
            uris: Vec::new(),
            custom_fields: Vec::new(),
        },
    }
}
