mod common;

use akooti_core::{
    DEV_LOCAL_FOLDER_ID, NewRoutingRule, PERSONAL_FOLDER_ID, RoutingPatternType, RoutingRule,
    Vault, VaultError,
};
use common::{fixture, item_draft};

#[test]
fn default_routing_adds_dev_local_without_false_positives() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    assert_eq!(vault.routing_rules().expect("rules").len(), 5);

    let local = vault
        .create_item(item_draft("Local", "password", "http://localhost:3000"))
        .expect("local");
    assert!(local.folder_ids.contains(&PERSONAL_FOLDER_ID.to_owned()));
    assert!(local.folder_ids.contains(&DEV_LOCAL_FOLDER_ID.to_owned()));

    let remote = vault
        .create_item(item_draft(
            "Remote",
            "password",
            "https://notlocalhost.example.com",
        ))
        .expect("remote");
    assert!(remote.folder_ids.contains(&PERSONAL_FOLDER_ID.to_owned()));
    assert!(!remote.folder_ids.contains(&DEV_LOCAL_FOLDER_ID.to_owned()));
}

#[test]
fn retroactive_routing_only_adds_folder_memberships() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let item = vault
        .create_item(item_draft(
            "Internal",
            "password",
            "http://api.service.internal",
        ))
        .expect("item");
    assert_eq!(item.folder_ids, vec![PERSONAL_FOLDER_ID]);

    vault
        .add_routing_rule(NewRoutingRule {
            pattern: ".internal".to_owned(),
            pattern_type: RoutingPatternType::Suffix,
            target_folder_id: DEV_LOCAL_FOLDER_ID.to_owned(),
            enabled: true,
        })
        .expect("rule");
    assert_eq!(vault.apply_routing_rules().expect("apply"), 1);
    let routed = vault.get_item(&item.id).expect("routed");
    assert!(routed.folder_ids.contains(&PERSONAL_FOLDER_ID.to_owned()));
    assert!(routed.folder_ids.contains(&DEV_LOCAL_FOLDER_ID.to_owned()));
    assert_eq!(vault.apply_routing_rules().expect("idempotent"), 0);
}

#[test]
fn routing_supports_cidr_regex_and_exact_host_matching() {
    let cidr = rule("192.168.0.0/24", RoutingPatternType::Cidr);
    assert!(cidr.matches("http://192.168.0.42:8080").expect("inside"));
    assert!(!cidr.matches("http://192.168.1.42:8080").expect("outside"));

    let regex = rule(r"^https://[^.]+\.svc\.local/", RoutingPatternType::Regex);
    assert!(regex.matches("https://api.svc.local/login").expect("regex"));
    assert!(
        !regex
            .matches("https://api.example.com/login")
            .expect("regex miss")
    );

    let exact = rule("localhost", RoutingPatternType::ExactHost);
    assert!(exact.matches("http://localhost:1420").expect("exact"));
    assert!(
        !exact
            .matches("https://notlocalhost.example.com")
            .expect("exact miss")
    );
}

#[test]
fn malformed_candidates_are_ignored_but_missing_items_are_errors() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let item = vault
        .create_item(item_draft("Malformed", "password", "not a url"))
        .expect("malformed candidate");
    assert_eq!(item.folder_ids, vec![PERSONAL_FOLDER_ID]);
    assert!(matches!(
        vault.route_item("missing-item"),
        Err(VaultError::NotFound)
    ));
}

fn rule(pattern: &str, pattern_type: RoutingPatternType) -> RoutingRule {
    RoutingRule {
        id: "rule".to_owned(),
        pattern: pattern.to_owned(),
        pattern_type,
        target_folder_id: DEV_LOCAL_FOLDER_ID.to_owned(),
        is_default: false,
        enabled: true,
        created_at: 0,
    }
}
