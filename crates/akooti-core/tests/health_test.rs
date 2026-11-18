#![expect(clippy::expect_used, reason = "test fixtures fail immediately")]

mod common;

use std::collections::HashSet;

use akooti_core::{
    HealthFinding, HealthReport, ItemFilter, ItemType, SecretString, Selectors, Vault,
    VaultItemDraft,
};
use common::{fixture, item_draft};
use sha1::{Digest, Sha1};

fn finding<'a>(report: &'a HealthReport, item_id: &str) -> &'a HealthFinding {
    report
        .findings
        .iter()
        .find(|finding| finding.item_id == item_id)
        .expect("finding")
}

fn sha1_hex(password: &str) -> String {
    Sha1::digest(password.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect()
}

#[test]
fn audit_persists_scores_and_flags_reuse() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "master-password").expect("create");
    let weak = vault
        .create_item(item_draft("Weak", "password123", "https://weak.test"))
        .expect("weak item")
        .id;
    let first = vault
        .create_item(item_draft(
            "Duplicate A",
            "Tr0ub4dor&3-horse-battery",
            "https://a.test",
        ))
        .expect("first duplicate")
        .id;
    let second = vault
        .create_item(item_draft(
            "Duplicate B",
            "Tr0ub4dor&3-horse-battery",
            "https://b.test",
        ))
        .expect("second duplicate")
        .id;
    let note = vault
        .create_item(VaultItemDraft {
            item_type: ItemType::Note,
            title: "Note".to_owned(),
            primary_url: None,
            username: None,
            password: None,
            totp_secret: None,
            notes: Some(SecretString::new("memo".to_owned())),
            selectors: Selectors::default(),
            created_by_agent: None,
            expires_at: None,
            favorite: false,
            folder_ids: Vec::new(),
            uris: Vec::new(),
            custom_fields: Vec::new(),
        })
        .expect("note")
        .id;
    assert!(!note.is_empty());

    let now = 1_800_000_000;
    let mut steps = Vec::new();
    let report = vault
        .audit_health(now, |done, total| steps.push((done, total)))
        .expect("audit");

    assert_eq!(report.findings.len(), 3);
    assert_eq!(report.without_password, 1);
    assert_eq!(steps.last(), Some(&(3, 3)));
    assert!(finding(&report, &weak).score <= 1);
    assert!(!finding(&report, &weak).reused);
    assert!(finding(&report, &first).reused);
    assert!(finding(&report, &second).reused);
    assert_eq!(
        [&first, &second]
            .iter()
            .filter(|id| finding(&report, id).reused)
            .count(),
        2
    );

    // Scores and prefixes are persisted, once per item.
    let items = vault.list_items(&ItemFilter::default()).expect("items");
    let scored: Vec<_> = items
        .iter()
        .filter(|item| item.zxcvbn_score.is_some())
        .collect();
    assert_eq!(scored.len(), 3);
    for item in scored {
        let prefix = item
            .password_sha1_prefix
            .as_deref()
            .expect("prefix is stored");
        assert_eq!(prefix.len(), akooti_core::SHA1_PREFIX_LEN);
        assert!(prefix.chars().all(|value| value.is_ascii_hexdigit()));
        assert!(prefix.chars().all(|value| !value.is_ascii_lowercase()));
    }

    // The report never carries the plaintext, even through `Debug`.
    let rendered = format!("{report:?}");
    assert!(!rendered.contains("Tr0ub4dor"));
    assert!(!rendered.contains("password123"));
}

#[test]
fn audit_reports_age_from_the_last_password_change() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "master-password").expect("create");
    let item = vault
        .create_item(item_draft(
            "Old",
            "a-long-unique-passphrase-42",
            "https://old.test",
        ))
        .expect("item")
        .id;
    let changed_at = vault.get_item(&item).expect("item").password_updated_at;
    let changed_at = changed_at.expect("password change time is stamped");

    let report = vault
        .audit_health(changed_at + 90 * 86_400, |_, _| {})
        .expect("audit");
    assert_eq!(finding(&report, &item).age_days, Some(90));

    let fresh = vault.audit_health(changed_at, |_, _| {}).expect("audit");
    assert_eq!(finding(&fresh, &item).age_days, Some(0));
}

#[test]
fn breach_range_marks_then_clears_items() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "master-password").expect("create");
    let leaked = vault
        .create_item(item_draft("Leaked", "hunter2", "https://leaked.test"))
        .expect("item")
        .id;
    assert!(vault.audit_health(1_800_000_000, |_, _| {}).is_ok());

    let targets = vault.breach_targets().expect("targets");
    assert_eq!(targets.len(), 1);
    let digest = sha1_hex("hunter2");
    assert_eq!(targets[0].prefix, digest[..5]);
    assert_eq!(targets[0].items, 1);

    let suffixes = HashSet::from([digest[5..].to_owned()]);
    assert_eq!(
        vault
            .apply_breach_range(&targets[0].prefix, &suffixes)
            .expect("apply"),
        1
    );
    assert!(vault.get_item(&leaked).expect("item").is_compromised);

    // A clean range clears the stale flag: the password is checked, not unknown.
    assert_eq!(
        vault
            .apply_breach_range(&targets[0].prefix, &HashSet::new())
            .expect("apply"),
        0
    );
    assert!(!vault.get_item(&leaked).expect("item").is_compromised);
}
