mod common;

use std::io::Write;

use akooti_core::{ImportFormat, ImportOptions, Vault};
use common::fixture;
use zip::write::SimpleFileOptions;

#[test]
fn proton_zip_json_maps_vault_login_totp_and_custom_fields() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let json = r#"{"version":"1.0","encrypted":false,"vaults":{"vault-1":{"name":"Personal","items":[{"itemId":"item-1","state":1,"pinned":true,"aliasEmail":null,"data":{"metadata":{"name":"Proton Login","note":"note","itemUuid":"uuid-1"},"extraFields":[{"fieldName":"API_TOKEN","type":"hidden","data":{"content":"fixture-token"}}],"type":"login","content":{"itemEmail":"email@example.test","itemUsername":"proton@example.test","password":"fixture-password","totpUri":"otpauth://totp/Test?secret=JBSWY3DPEHPK3PXP","urls":["http://proton.local"]}}},{"itemId":"item-2","state":1,"pinned":false,"aliasEmail":null,"data":{"metadata":{"name":"Proton Card","note":"card note","itemUuid":"uuid-2"},"extraFields":[],"type":"creditCard","content":{"number":"4111111111111111","verificationNumber":"123","expirationDate":"2030-01","pin":"4321"}}}]}}}"#;
    let archive = zip_bytes("Proton Pass/data.json", json.as_bytes());
    let report = vault
        .import_bytes(
            ImportFormat::ProtonPass,
            &archive,
            None,
            ImportOptions::default(),
        )
        .expect("import");
    assert_eq!(report.imported, 2);
    let items = vault.list_items(&Default::default()).expect("items");
    let item = items
        .iter()
        .find(|item| item.title == "Proton Login")
        .expect("login");
    assert!(
        item.custom_fields
            .iter()
            .any(|field| field.field_name == "API_TOKEN")
    );
    let card = items
        .iter()
        .find(|item| item.title == "Proton Card")
        .expect("card");
    assert!(
        card.custom_fields
            .iter()
            .any(|field| field.field_name == "number" && field.is_secret)
    );
}

#[test]
fn onepassword_pux_maps_vaults_fields_and_totp() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let json = r#"{"accounts":[{"vaults":[{"attrs":{"uuid":"vault-1","name":"Work"},"items":[{"uuid":"op-item","favIndex":1,"categoryUuid":"001","details":{"loginFields":[{"value":"op@example.test","name":"username","designation":"username"},{"value":"fixture-password","name":"password","designation":"password"}],"notesPlain":"note","sections":[{"title":"Security","fields":[{"title":"one-time password","id":"TOTP_1","value":{"totp":"JBSWY3DPEHPK3PXP"},"guarded":true}]}]},"overview":{"title":"OnePassword Login","urls":[{"url":"http://onepassword.local"}]}}]}]}]}"#;
    let archive = zip_bytes("export.data", json.as_bytes());
    let report = vault
        .import_bytes(
            ImportFormat::OnePasswordPux,
            &archive,
            None,
            ImportOptions::default(),
        )
        .expect("import");
    assert_eq!(report.imported, 1);
    assert_eq!(report.routed, 1);
}

#[expect(
    clippy::expect_used,
    reason = "synthetic ZIP fixture construction fails the test immediately"
)]
fn zip_bytes(name: &str, contents: &[u8]) -> Vec<u8> {
    let mut output = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut output);
        writer
            .start_file(name, SimpleFileOptions::default())
            .expect("start file");
        writer.write_all(contents).expect("write");
        writer.finish().expect("finish");
    }
    output.into_inner()
}
