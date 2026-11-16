mod common;

use akooti_core::{
    CustomFieldInput, DEV_LOCAL_FOLDER_ID, ItemMetadataDraft, ItemType, PERSONAL_FOLDER_ID,
    SecretField, SecretString, Selectors, UriMatchType, Vault, VaultError,
};
use common::{fixture, item_draft};

fn secret(value: &str) -> Option<SecretString> {
    Some(SecretString::new(value.to_owned()))
}

fn metadata_draft(title: &str) -> ItemMetadataDraft {
    ItemMetadataDraft {
        item_type: ItemType::Login,
        title: title.to_owned(),
        primary_url: Some("https://example.com".to_owned()),
        username: Some("renamed@example.test".to_owned()),
        selectors: Selectors {
            username: Some("#user".to_owned()),
            password: Some("#pass".to_owned()),
            totp: None,
            submit: Some("button[type=submit]".to_owned()),
        },
        expires_at: None,
        favorite: true,
        folder_ids: vec![PERSONAL_FOLDER_ID.to_owned()],
        uris: vec![akooti_core::ItemUriInput {
            uri: "https://example.com".to_owned(),
            match_type: UriMatchType::Host,
        }],
    }
}

#[test]
fn metadata_edit_preserves_every_stored_secret() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");

    let mut draft = item_draft("Original", "hunter2-very-secret", "https://example.com");
    draft.totp_secret = secret("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
    draft.custom_fields = vec![CustomFieldInput {
        field_name: "api token".to_owned(),
        value: SecretString::new("token-value".to_owned()),
        is_secret: true,
        sort_order: 0,
    }];
    let item = vault.create_item(draft).expect("item");
    let custom_field_id = item.custom_fields[0].id.clone();
    let totp_before = vault
        .generate_totp(&item.id, akooti_core::TotpAlgorithm::Sha1, 1_700_000_000)
        .expect("totp")
        .expose()
        .to_owned();

    let updated = vault
        .update_item_metadata(&item.id, metadata_draft("Renamed"))
        .expect("metadata");
    assert_eq!(updated.title, "Renamed");
    assert!(updated.favorite);
    assert_eq!(updated.selectors.username.as_deref(), Some("#user"));
    assert_eq!(updated.uris.len(), 1);
    assert_eq!(updated.custom_fields.len(), 1);
    assert_eq!(updated.custom_fields[0].id, custom_field_id);

    assert_eq!(
        vault
            .reveal_secret(&item.id, SecretField::Password, None)
            .expect("password")
            .value
            .expose(),
        "hunter2-very-secret"
    );
    assert_eq!(
        vault
            .reveal_secret(&item.id, SecretField::CustomField, Some(&custom_field_id))
            .expect("custom field")
            .value
            .expose(),
        "token-value"
    );
    assert_eq!(
        vault
            .generate_totp(&item.id, akooti_core::TotpAlgorithm::Sha1, 1_700_000_000)
            .expect("totp")
            .expose(),
        totp_before
    );
}

#[test]
fn secret_writes_archive_history_and_clear_secrets() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let item = vault
        .create_item(item_draft(
            "Rotate",
            "first-password",
            "https://example.com",
        ))
        .expect("item");
    assert_eq!(vault.password_history_count(&item.id).expect("count"), 0);

    vault
        .set_item_secret(&item.id, SecretField::Password, secret("second-password"))
        .expect("rotate");
    assert_eq!(vault.password_history_count(&item.id).expect("count"), 1);
    assert_eq!(
        vault
            .reveal_secret(&item.id, SecretField::Password, None)
            .expect("password")
            .value
            .expose(),
        "second-password"
    );

    vault
        .set_item_secret(&item.id, SecretField::Notes, secret("memorable note"))
        .expect("notes");
    assert_eq!(
        vault
            .reveal_secret(&item.id, SecretField::Notes, None)
            .expect("notes")
            .value
            .expose(),
        "memorable note"
    );
    vault
        .set_item_secret(&item.id, SecretField::Notes, None)
        .expect("clear notes");
    assert!(matches!(
        vault.reveal_secret(&item.id, SecretField::Notes, None),
        Err(VaultError::Database(_))
    ));

    assert!(matches!(
        vault.set_item_secret("missing-item", SecretField::Password, secret("x")),
        Err(VaultError::NotFound)
    ));
    assert!(matches!(
        vault.set_item_secret(&item.id, SecretField::CustomField, secret("x")),
        Err(VaultError::InvalidInput(_))
    ));
}

#[test]
fn custom_fields_can_be_added_and_removed_without_touching_folders() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let item = vault
        .create_item(item_draft("Fields", "password", "https://example.com"))
        .expect("item");

    let field_id = vault
        .add_item_custom_field(
            &item.id,
            CustomFieldInput {
                field_name: "region".to_owned(),
                value: SecretString::new("eu-west-3".to_owned()),
                is_secret: false,
                sort_order: 1,
            },
        )
        .expect("field");
    let reloaded = vault.get_item(&item.id).expect("reload");
    assert_eq!(reloaded.custom_fields.len(), 1);
    assert_eq!(reloaded.custom_fields[0].field_name, "region");
    assert_eq!(reloaded.folder_ids, vec![PERSONAL_FOLDER_ID]);

    vault
        .remove_item_custom_field(&item.id, &field_id)
        .expect("remove");
    assert!(
        vault
            .get_item(&item.id)
            .expect("reload")
            .custom_fields
            .is_empty()
    );
    assert!(matches!(
        vault.remove_item_custom_field(&item.id, &field_id),
        Err(VaultError::NotFound)
    ));
}

#[test]
fn metadata_edit_reapplies_local_routing_without_removing_folders() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let item = vault
        .create_item(item_draft("Moved", "password", "https://example.com"))
        .expect("item");
    assert_eq!(item.folder_ids, vec![PERSONAL_FOLDER_ID]);

    let mut draft = metadata_draft("Moved");
    draft.primary_url = Some("http://api.service.internal".to_owned());
    let routed = vault
        .update_item_metadata(&item.id, draft.clone())
        .expect("metadata");
    assert_eq!(routed.folder_ids, vec![PERSONAL_FOLDER_ID]);

    vault
        .add_routing_rule(akooti_core::NewRoutingRule {
            pattern: ".internal".to_owned(),
            pattern_type: akooti_core::RoutingPatternType::Suffix,
            target_folder_id: DEV_LOCAL_FOLDER_ID.to_owned(),
            enabled: true,
        })
        .expect("rule");
    let routed = vault
        .update_item_metadata(&item.id, draft)
        .expect("metadata");
    assert!(routed.folder_ids.contains(&DEV_LOCAL_FOLDER_ID.to_owned()));
    assert!(routed.folder_ids.contains(&PERSONAL_FOLDER_ID.to_owned()));
}
