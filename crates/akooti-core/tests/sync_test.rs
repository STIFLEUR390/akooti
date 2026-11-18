mod common;

use akooti_core::{ItemFilter, SecretField, SyncRecord, Vault, VaultPaths};
use common::{fixture, item_draft};

#[allow(clippy::expect_used, reason = "test fixture ")]
fn second_device(temp: &tempfile::TempDir, vault: &Vault, snapshot_name: &str) -> Vault {
    let snapshot = temp.path().join(snapshot_name);
    vault
        .write_snapshot(&snapshot, 1_800_000_000)
        .expect("snapshot");
    let paths =
        VaultPaths::under_app_data(&temp.path().join(snapshot_name.trim_end_matches(".akooti")));
    akooti_core::restore_snapshot(&snapshot, &paths, false).expect("restore");
    Vault::open(&paths, "master-password").expect("open restored")
}

#[test]
fn records_are_opaque_and_round_trip_to_another_device() {
    let (temp, paths) = fixture();
    let mut device_a = Vault::create(&paths, "master-password").expect("create");
    device_a
        .create_item(item_draft(
            "Portail",
            "s3cret-value",
            "https://example.test",
        ))
        .expect("item");
    let mut device_b = second_device(&temp, &device_a, "b.akooti");

    let records = device_a.sync_records().expect("records");
    assert!(!records.is_empty());
    let item = records
        .iter()
        .find(|record| record.record_type.as_str() == "item")
        .expect("item record");
    // The payload is encrypted: neither the title nor the password leaks.
    assert!(!item.ciphertext.contains("Portail"));
    assert!(!item.ciphertext.contains("s3cret-value"));
    assert!(!item.is_deleted);

    let summary = device_b.apply_sync_records(&records).expect("apply");
    assert_eq!(summary.pulled, records.len());
    assert!(summary.applied > 0);
    assert_eq!(summary.pulled, summary.applied + summary.skipped);

    // A record created after the snapshot reaches the other device.
    let created = device_a
        .create_item(item_draft(
            "Sync only",
            "another-secret",
            "https://sync.test",
        ))
        .expect("item")
        .id;
    let summary = device_b
        .apply_sync_records(&device_a.sync_records().expect("records"))
        .expect("apply");
    assert!(summary.applied > 0);
    let items = device_b.list_items(&ItemFilter::default()).expect("items");
    assert!(items.iter().any(|item| item.id == created));
    assert_eq!(
        device_b
            .reveal_secret(&created, SecretField::Password, None)
            .expect("reveal")
            .value
            .expose(),
        "another-secret"
    );
}

#[test]
fn an_older_record_never_overwrites_a_newer_local_one() {
    let (temp, paths) = fixture();
    let mut device_a = Vault::create(&paths, "master-password").expect("create");
    let item = device_a
        .create_item(item_draft("Local", "local-secret", "https://example.test"))
        .expect("item")
        .id;
    let mut device_b = second_device(&temp, &device_a, "b.akooti");

    let records = device_a.sync_records().expect("records");
    let stale: Vec<SyncRecord> = records
        .into_iter()
        .map(|mut record| {
            record.client_updated_at = 0;
            record
        })
        .collect();
    let summary = device_b.apply_sync_records(&stale).expect("apply");
    assert_eq!(summary.skipped, stale.len());
    assert!(
        device_b
            .list_items(&ItemFilter::default())
            .expect("items")
            .iter()
            .any(|existing| existing.id == item)
    );
}

#[test]
fn tombstones_propagate_a_deletion() {
    let (temp, paths) = fixture();
    let mut device_a = Vault::create(&paths, "master-password").expect("create");
    let item = device_a
        .create_item(item_draft("Doomed", "secret", "https://example.test"))
        .expect("item")
        .id;
    let mut device_b = second_device(&temp, &device_a, "b.akooti");
    device_b
        .apply_sync_records(&device_a.sync_records().expect("records"))
        .expect("apply");

    device_a.delete_item(&item).expect("delete");
    let records = device_a.sync_records().expect("records");
    let tombstone = records
        .iter()
        .find(|record| record.id == item)
        .expect("tombstone");
    assert!(tombstone.is_deleted);
    let summary = device_b.apply_sync_records(&records).expect("apply");
    assert!(summary.tombstones > 0);
    assert!(
        !device_b
            .list_items(&ItemFilter::default())
            .expect("items")
            .iter()
            .any(|existing| existing.id == item)
    );
}

#[test]
fn a_tampered_record_is_refused() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "master-password").expect("create");
    vault
        .create_item(item_draft("Portail", "secret", "https://example.test"))
        .expect("item");
    let records = vault.sync_records().expect("records");
    let mut tampered = records
        .into_iter()
        .find(|record| record.record_type.as_str() == "item")
        .expect("item");
    tampered.id = "00000000-0000-4000-8000-00000000dead".to_owned();
    assert!(vault.apply_sync_records(&[tampered]).is_err());
}
