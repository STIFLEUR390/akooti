#![expect(clippy::expect_used, reason = "test fixtures fail immediately")]

use akooti_core::{
    ItemType, PERSONAL_FOLDER_ID, SecretString, Selectors, VaultItemDraft, VaultPaths,
};
use tempfile::TempDir;

pub fn fixture() -> (TempDir, VaultPaths) {
    let temp = tempfile::tempdir().expect("temporary directory");
    let paths = VaultPaths::under_app_data(temp.path());
    (temp, paths)
}

#[allow(dead_code)]
pub fn item_draft(title: &str, password: &str, primary_url: &str) -> VaultItemDraft {
    VaultItemDraft {
        item_type: ItemType::Login,
        title: title.to_owned(),
        primary_url: Some(primary_url.to_owned()),
        username: Some("agent@example.test".to_owned()),
        password: Some(SecretString::new(password.to_owned())),
        totp_secret: None,
        notes: None,
        selectors: Selectors::default(),
        created_by_agent: None,
        expires_at: None,
        favorite: false,
        folder_ids: vec![PERSONAL_FOLDER_ID.to_owned()],
        uris: Vec::new(),
        custom_fields: Vec::new(),
    }
}
