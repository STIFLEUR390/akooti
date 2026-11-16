#![expect(clippy::expect_used, reason = "test fixtures fail immediately")]

use std::fs;
use std::time::{Duration, SystemTime};

use akooti_core::{
    AGENTS_MCP_FOLDER_ID, CustomFieldInput, DEV_LOCAL_FOLDER_ID, FolderDraft, ItemFilter, ItemType,
    ItemUriInput, PERSONAL_FOLDER_ID, SecretField, SecretString, Selectors, UriMatchType, Vault,
    VaultError, VaultItemDraft, VaultPaths, VaultSession, VaultStatus,
};
use tempfile::TempDir;

fn fixture() -> (TempDir, VaultPaths) {
    let temp = tempfile::tempdir().expect("temporary directory");
    let paths = VaultPaths::under_app_data(temp.path());
    (temp, paths)
}

fn item_draft(title: &str, password: &str) -> VaultItemDraft {
    VaultItemDraft {
        item_type: ItemType::Login,
        title: title.to_owned(),
        primary_url: Some("https://example.test/login".to_owned()),
        username: Some("agent@example.test".to_owned()),
        password: Some(SecretString::new(password.to_owned())),
        totp_secret: Some(SecretString::new("JBSWY3DPEHPK3PXP".to_owned())),
        notes: Some(SecretString::new("private note".to_owned())),
        selectors: Selectors {
            username: Some("input[name=email]".to_owned()),
            password: Some("input[name=password]".to_owned()),
            ..Selectors::default()
        },
        created_by_agent: Some("codex".to_owned()),
        expires_at: None,
        favorite: false,
        folder_ids: vec![
            PERSONAL_FOLDER_ID.to_owned(),
            DEV_LOCAL_FOLDER_ID.to_owned(),
        ],
        uris: vec![ItemUriInput {
            uri: "example.test".to_owned(),
            match_type: UriMatchType::Host,
        }],
        custom_fields: vec![CustomFieldInput {
            field_name: "API_TOKEN".to_owned(),
            value: SecretString::new("custom-secret-value".to_owned()),
            is_secret: true,
            sort_order: 1,
        }],
    }
}

#[test]
fn vault_reopens_with_password_and_rejects_wrong_password() {
    let (_temp, paths) = fixture();
    drop(Vault::create(&paths, "correct horse battery staple").expect("create"));

    let reopened = Vault::open(&paths, "correct horse battery staple").expect("reopen");
    assert_eq!(reopened.list_folders(false).expect("folders").len(), 3);
    assert!(matches!(
        Vault::open(&paths, "wrong password"),
        Err(VaultError::InvalidCredentials)
    ));
}

#[test]
fn system_folders_are_seeded_and_protected() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let folders = vault.list_folders(false).expect("folders");
    assert_eq!(folders.len(), 3);
    assert!(folders.iter().all(|folder| folder.is_system));

    assert!(matches!(
        vault.rename_folder(PERSONAL_FOLDER_ID, "Changed"),
        Err(VaultError::SystemFolderProtected)
    ));
    assert!(matches!(
        vault.delete_folder(DEV_LOCAL_FOLDER_ID),
        Err(VaultError::SystemFolderProtected)
    ));

    let folder = vault
        .create_folder(FolderDraft {
            parent_id: None,
            name: "Work".to_owned(),
            icon: "briefcase".to_owned(),
            color: None,
            require_biometric_for_mcp: true,
        })
        .expect("user folder");
    let renamed = vault
        .rename_folder(&folder.id, "Customers")
        .expect("rename");
    assert_eq!(renamed.name, "Customers");
    vault.delete_folder(&folder.id).expect("soft delete");
    assert_eq!(vault.list_folders(false).expect("visible").len(), 3);
    assert_eq!(vault.list_folders(true).expect("all").len(), 4);
}

#[test]
fn item_flow_encrypts_secrets_and_preserves_n_to_n_links() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let item = vault
        .create_item(item_draft("Example", "never-store-this-password"))
        .expect("item");

    assert_eq!(item.folder_ids.len(), 2);
    let password = vault
        .reveal_secret(&item.id, SecretField::Password, None)
        .expect("password");
    assert_eq!(password.value.expose(), "never-store-this-password");
    let custom_id = item.custom_fields[0].id.clone();
    let custom = vault
        .reveal_secret(&item.id, SecretField::CustomField, Some(&custom_id))
        .expect("custom field");
    assert_eq!(custom.value.expose(), "custom-secret-value");

    vault
        .add_item_to_folder(&item.id, AGENTS_MCP_FOLDER_ID)
        .expect("add link");
    vault
        .add_item_to_folder(&item.id, AGENTS_MCP_FOLDER_ID)
        .expect("idempotent add");
    assert_eq!(vault.get_item(&item.id).expect("item").folder_ids.len(), 3);
    vault
        .remove_item_from_folder(&item.id, DEV_LOCAL_FOLDER_ID)
        .expect("remove link");
    assert_eq!(vault.get_item(&item.id).expect("item").folder_ids.len(), 2);

    let mut updated = item_draft("Updated", "updated-password");
    updated.primary_url = Some("https://example.com/login".to_owned());
    updated.uris.clear();
    updated.folder_ids = vec![AGENTS_MCP_FOLDER_ID.to_owned()];
    let updated_item = vault.update_item(&item.id, updated).expect("update");
    assert_eq!(updated_item.title, "Updated");
    assert_eq!(updated_item.folder_ids, vec![AGENTS_MCP_FOLDER_ID]);

    vault.delete_item(&item.id).expect("soft delete");
    assert!(
        vault
            .list_items(&ItemFilter::default())
            .expect("visible")
            .is_empty()
    );
    let all = vault
        .list_items(&ItemFilter {
            include_deleted: true,
            folder_id: None,
        })
        .expect("all");
    assert_eq!(all.len(), 1);
    assert!(all[0].deleted_at.is_some());
}

#[test]
fn database_bytes_do_not_contain_plaintext_secrets() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let password = "plaintext-must-not-hit-disk";
    let item = vault
        .create_item(item_draft("Plaintext Marker", password))
        .expect("item");
    vault
        .reveal_secret(&item.id, SecretField::Notes, None)
        .expect("flush reads");
    drop(vault);

    let mut bytes = fs::read(&paths.database).expect("database");
    for suffix in ["-wal", "-shm"] {
        let sidecar = paths.database.with_file_name(format!("vault.db{suffix}"));
        if sidecar.exists() {
            bytes.extend(fs::read(sidecar).expect("sidecar"));
        }
    }
    assert!(
        !bytes
            .windows(password.len())
            .any(|window| window == password.as_bytes())
    );
    assert!(
        !bytes
            .windows(b"Plaintext Marker".len())
            .any(|window| window == b"Plaintext Marker")
    );
}

#[test]
fn inactivity_locks_and_requires_a_new_unlock() {
    let (_temp, paths) = fixture();
    let mut session = VaultSession::new(paths.clone(), 15);
    assert_eq!(session.status(), VaultStatus::Locked);
    assert!(matches!(session.vault_mut(), Err(VaultError::Locked)));

    session.create("password").expect("create");
    assert_eq!(session.status(), VaultStatus::Unlocked);
    session
        .vault_mut()
        .expect("unlocked")
        .set_auto_lock_minutes(15)
        .expect("timeout");

    let future = SystemTime::now() + Duration::from_secs(16 * 60);
    assert!(session.enforce_auto_lock(future));
    assert_eq!(session.status(), VaultStatus::Locked);
    assert!(matches!(session.vault(), Err(VaultError::Locked)));

    session.unlock("password").expect("unlock");
    assert_eq!(session.status(), VaultStatus::Unlocked);
}
