mod common;

use akooti_core::{
    BackupConfig, BackupProvider, BackupTargetDraft, ItemFilter, NewBackupRun, Vault, VaultPaths,
    expired_snapshots,
};
use common::{fixture, item_draft};

#[test]
fn local_target_round_trip_hides_its_credentials() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "master-password").expect("create");
    let draft = BackupTargetDraft {
        name: "NAS".to_owned(),
        config: BackupConfig::Local {
            path: "/tmp/akooti-backups".to_owned(),
        },
        interval_minutes: 60,
        retention_count: 7,
        enabled: true,
    };
    let target = vault.create_backup_target(draft).expect("create target");
    assert_eq!(target.provider, BackupProvider::Local);
    assert_eq!(target.summary, "/tmp/akooti-backups");
    assert_eq!(target.interval_minutes, 60);
    assert_eq!(target.retention_count, 7);

    let listed = vault.backup_targets().expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, target.id);

    // The decrypted configuration is only reachable through the explicit call.
    match vault.backup_config(&target.id).expect("config") {
        BackupConfig::Local { path } => assert_eq!(path, "/tmp/akooti-backups"),
        other => panic!("unexpected config: {other:?}"),
    }

    // Incomplete configurations are refused before any write.
    let incomplete = BackupTargetDraft {
        name: "S3".to_owned(),
        config: BackupConfig::S3 {
            endpoint: "https://s3.example.test".to_owned(),
            region: String::new(),
            bucket: "bucket".to_owned(),
            access_key_id: "key".to_owned(),
            secret_access_key: "secret".to_owned(),
            prefix: None,
        },
        interval_minutes: 60,
        retention_count: 3,
        enabled: false,
    };
    assert!(vault.create_backup_target(incomplete).is_err());
}

#[test]
fn schedule_due_and_history_are_recorded_together() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "master-password").expect("create");
    let target = vault
        .create_backup_target(BackupTargetDraft {
            name: "Local".to_owned(),
            config: BackupConfig::Local {
                path: "/tmp/akooti-backups".to_owned(),
            },
            interval_minutes: 60,
            retention_count: 2,
            enabled: true,
        })
        .expect("target");

    // Never run: due immediately.
    assert_eq!(vault.due_backup_targets(1_000).expect("due").len(), 1);

    vault
        .record_backup_run(
            &target.id,
            NewBackupRun {
                status: "success".to_owned(),
                detail: None,
                size_bytes: Some(4096),
                snapshot_name: Some("akooti-1000.akooti".to_owned()),
            },
            1_000,
        )
        .expect("run");
    assert!(vault.due_backup_targets(1_000).expect("due").is_empty());
    assert!(vault.due_backup_targets(4_599).expect("due").is_empty());
    assert_eq!(vault.due_backup_targets(4_600).expect("due").len(), 1);

    let runs = vault.backup_runs(Some(&target.id), 10).expect("runs");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, "success");
    assert_eq!(runs[0].size_bytes, Some(4096));
    assert_eq!(
        vault.backup_target(&target.id).expect("target").last_status,
        Some("success".to_owned())
    );

    // An unknown target is refused, not silently ignored.
    assert!(
        vault
            .record_backup_run(
                "missing",
                NewBackupRun {
                    status: "error".to_owned(),
                    detail: None,
                    size_bytes: None,
                    snapshot_name: None,
                },
                1_000,
            )
            .is_err()
    );
    assert!(vault.delete_backup_target(&target.id).is_ok());
    assert!(vault.backup_runs(None, 10).expect("runs").is_empty());
}

#[test]
fn retention_keeps_the_newest_snapshots_only() {
    let names = vec![
        "akooti-100.akooti".to_owned(),
        "akooti-300.akooti".to_owned(),
        "akooti-200.akooti".to_owned(),
        "notes.txt".to_owned(),
    ];
    let mut expired = expired_snapshots(&names, 2);
    expired.sort();
    assert_eq!(expired, vec!["akooti-100.akooti".to_owned()]);
    assert!(expired_snapshots(&names, 5).is_empty());
}

#[test]
fn snapshot_restores_into_a_fresh_location() {
    let (temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "master-password").expect("create");
    vault
        .create_item(item_draft(
            "Portail",
            "s3cret-value",
            "https://example.test",
        ))
        .expect("item");
    let snapshot = temp.path().join("backup.akooti");
    let size = vault
        .write_snapshot(&snapshot, 1_800_000_000)
        .expect("snapshot");
    assert!(size > 1_000);
    drop(vault);

    let restored_dir = temp.path().join("restored");
    let restored_paths = VaultPaths::under_app_data(&restored_dir);
    let info = akooti_core::restore_snapshot(&snapshot, &restored_paths, false).expect("restore");
    assert_eq!(info.header.created_at, 1_800_000_000);
    assert!(info.database_bytes > 0);

    let vault = Vault::open(&restored_paths, "master-password").expect("open restored");
    let items = vault.list_items(&ItemFilter::default()).expect("items");
    assert_eq!(items.len(), 1);
    assert_eq!(
        vault
            .reveal_secret(&items[0].id, akooti_core::SecretField::Password, None)
            .expect("reveal")
            .value
            .expose(),
        "s3cret-value"
    );

    // A second restore onto the same files needs an explicit overwrite.
    assert!(akooti_core::restore_snapshot(&snapshot, &restored_paths, false).is_err());
    assert!(akooti_core::restore_snapshot(&snapshot, &restored_paths, true).is_ok());
}

#[test]
fn plaintext_export_requires_the_master_password() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "master-password").expect("create");
    vault
        .create_item(item_draft(
            "Portail",
            "s3cret-value",
            "https://example.test",
        ))
        .expect("item");

    assert!(vault.export_json("wrong-password", 1_800_000_000).is_err());
    let exported = vault
        .export_json("master-password", 1_800_000_000)
        .expect("export");
    assert!(exported.contains("\"format\": \"akooti-export\""));
    assert!(exported.contains("s3cret-value"));
    assert!(exported.contains("Portail"));
    assert!(!exported.contains("ak_mcp_"));
    assert!(!exported.contains("private_key_exported\": true"));
}

#[test]
fn snapshot_refuses_a_file_that_is_not_a_snapshot() {
    let (temp, _) = fixture();
    let bogus = temp.path().join("bogus.akooti");
    std::fs::write(&bogus, b"not a snapshot").expect("write");
    let restored_paths = VaultPaths::under_app_data(&temp.path().join("restored"));
    assert!(akooti_core::restore_snapshot(&bogus, &restored_paths, false).is_err());
}
