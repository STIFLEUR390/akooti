mod common;

use akooti_core::{
    DEV_LOCAL_FOLDER_ID, ImportFormat, ImportIssueKind, ImportOptions, PERSONAL_FOLDER_ID, Vault,
};
use common::fixture;

#[test]
fn browser_csv_handles_bom_semicolon_notes_and_invalid_rows() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let csv = b"\xEF\xBB\xBFname;url;username;password;note\r\n\
Local App;http://browser.local:3000;browser@example.test;fixture-password;fixture note\r\n\
Broken;;;;fixture only\r\n";
    let report = vault
        .import_bytes(
            ImportFormat::BrowserCsv,
            csv,
            None,
            ImportOptions::default(),
        )
        .expect("import");
    assert_eq!(report.imported, 1);
    assert_eq!(report.routed, 1);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.kind == ImportIssueKind::InvalidRow)
    );
    let items = vault.list_items(&Default::default()).expect("items");
    assert_eq!(items.len(), 1);
    assert!(items[0].folder_ids.contains(&PERSONAL_FOLDER_ID.to_owned()));
    assert!(
        items[0]
            .folder_ids
            .contains(&DEV_LOCAL_FOLDER_ID.to_owned())
    );
}

#[test]
fn buttercup_csv_maps_nested_groups_and_custom_fields() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let csv = b"!group_id,!group_name,title,username,password,URL,note,id,API_TOKEN\n\
1,Work/Projects,Buttercup CSV,csv@example.test,fixture-password,https://csv.example.test,fixture note,item-1,fixture-token\n";
    let report = vault
        .import_bytes(
            ImportFormat::ButtercupCsv,
            csv,
            None,
            ImportOptions::default(),
        )
        .expect("import");
    assert_eq!(report.imported, 1);
    let folders = vault.list_folders(false).expect("folders");
    assert!(folders.iter().any(|folder| folder.name == "Work"));
    assert!(folders.iter().any(|folder| folder.name == "Projects"));
    let item = vault
        .list_items(&Default::default())
        .expect("items")
        .pop()
        .expect("item");
    assert!(
        item.custom_fields
            .iter()
            .any(|field| field.field_name == "api_token")
    );
}

#[test]
fn keepass_csv_and_onepassword_csv_are_tolerated() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let keepass = b"Title,Username,Password,URL,Notes,TOTP,Group\n\
KeePass CSV,keepass@example.test,fixture-password,http://keepass.local,note,otpauth://totp/Test?secret=JBSWY3DPEHPK3PXP,Root/Dev\n";
    vault
        .import_bytes(
            ImportFormat::KeePassCsv,
            keepass,
            None,
            ImportOptions::default(),
        )
        .expect("keepass csv");
    let onepassword = b"Title,URL,Username,Password,Notes,TOTP\n\
1Password CSV,https://onepassword.example.test,onepassword@example.test,fixture-password,note,JBSWY3DPEHPK3PXP\n";
    vault
        .import_bytes(
            ImportFormat::OnePasswordCsv,
            onepassword,
            None,
            ImportOptions::default(),
        )
        .expect("1password csv");
    assert_eq!(
        vault.list_items(&Default::default()).expect("items").len(),
        2
    );
}

#[test]
fn proton_csv_maps_aliases_and_vaults() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let csv = b"vault,type,name,email,username,password,url,note\n\
Personal,alias,Alias Item,alias@example.test,alias@example.test,fixture-password,http://proton.local,note\n";
    let report = vault
        .import_bytes(
            ImportFormat::ProtonPass,
            csv,
            None,
            ImportOptions::default(),
        )
        .expect("proton csv");
    assert_eq!(report.imported, 1);
    assert_eq!(report.routed, 1);
}

#[test]
fn encrypted_parser_requires_a_password() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let result = vault.import_bytes(
        ImportFormat::Buttercup,
        b"b~>buttercup/binvalid",
        None,
        ImportOptions::default(),
    );
    assert!(matches!(
        result,
        Err(akooti_core::ImportError::PasswordRequired)
    ));
}
