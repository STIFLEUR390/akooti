use std::collections::{BTreeMap, HashMap};

use crate::models::{ItemType, PERSONAL_FOLDER_ID};

use super::archive_support::read_zip_entry;
use super::csv_support::parse_csv as parse_csv_document;
use super::error::ImportError;
use super::onepassword_types::{File, Item, Section};
use super::support::{
    add_field, add_folder_path, add_uri, generated_external_id, new_item, normalize_totp, secret,
    set_notes,
};
use super::types::{ImportBundle, ImportFormat, ImportIssue, ImportIssueKind};

pub(crate) fn parse_pux(bytes: &[u8]) -> Result<ImportBundle, ImportError> {
    let json = if bytes.starts_with(b"PK") {
        read_zip_entry(bytes, "export.data")?
    } else {
        zeroize::Zeroizing::new(bytes.to_vec())
    };
    let file: File = serde_json::from_slice(json.as_slice())?;
    convert(file)
}

fn convert(file: File) -> Result<ImportBundle, ImportError> {
    let mut bundle = ImportBundle {
        format: ImportFormat::OnePasswordPux,
        folders: Vec::new(),
        items: Vec::new(),
        issues: Vec::new(),
    };
    for account in file.accounts {
        for vault in account.vaults {
            bundle.folders.push(super::types::ImportFolder {
                external_id: vault.attrs.uuid.clone(),
                name: if vault.attrs.name.trim().is_empty() {
                    "Imported vault".to_owned()
                } else {
                    vault.attrs.name
                },
                parent_external_id: None,
            });
            for item in vault.items {
                let imported = convert_item(&item, &vault.attrs.uuid, &mut bundle.issues);
                bundle.items.push(imported);
            }
        }
    }
    Ok(bundle)
}

fn convert_item(
    item: &Item,
    vault_id: &str,
    issues: &mut Vec<ImportIssue>,
) -> super::types::ImportItem {
    let category = item.category_uuid.as_str();
    let item_type = match category {
        "001" | "005" | "102" | "105" | "109" | "110" => ItemType::Login,
        "112" => ItemType::ApiKey,
        _ => ItemType::Note,
    };
    let username = item
        .details
        .login_fields
        .iter()
        .find(|field| field.designation.as_deref() == Some("username"))
        .map(|field| field.value.clone())
        .or_else(|| field_value(&item.details.sections, &["username"]));
    let password = item
        .details
        .login_fields
        .iter()
        .find(|field| field.designation.as_deref() == Some("password"))
        .map(|field| field.value.clone())
        .or_else(|| item.details.password.clone())
        .or_else(|| field_value(&item.details.sections, &["credential", "password"]));
    let url = item
        .overview
        .urls
        .iter()
        .find(|url| !url.url.trim().is_empty())
        .map(|url| url.url.clone())
        .or_else(|| item.overview.url.clone());
    let mut imported = new_item(
        &item.uuid,
        item_type,
        &item.overview.title,
        url.clone(),
        username,
        vec![vault_id.to_owned()],
    );
    imported.draft.favorite = item.fav_index == 1;
    imported.draft.password = password.as_deref().and_then(secret);
    for field in &item.details.login_fields {
        if field.designation.is_none() {
            add_field(&mut imported.draft, &field.name, &field.value, true);
        }
    }
    if let Some(url) = url.as_deref() {
        add_uri(&mut imported.draft, url);
    }
    set_notes(&mut imported.draft, item.details.notes_plain.as_deref());
    for section in &item.details.sections {
        for field in &section.fields {
            let Some(value) = field_first_value(&field.value) else {
                continue;
            };
            let name = if field.title.is_empty() {
                section.title.as_str()
            } else {
                field.title.as_str()
            };
            if field.id.starts_with("TOTP_") || name.eq_ignore_ascii_case("one-time password") {
                imported.draft.totp_secret = normalize_totp(&value);
                if imported.draft.totp_secret.is_none() {
                    issues.push(ImportIssue {
                        line: None,
                        kind: ImportIssueKind::InvalidTotp,
                    });
                }
            } else {
                add_field(&mut imported.draft, name, &value, field.guarded);
            }
        }
    }
    imported
}

fn field_value(sections: &[Section], names: &[&str]) -> Option<String> {
    sections
        .iter()
        .flat_map(|section| &section.fields)
        .find_map(|field| {
            names
                .iter()
                .any(|name| {
                    field.title.eq_ignore_ascii_case(name) || field.id.eq_ignore_ascii_case(name)
                })
                .then(|| field_first_value(&field.value))
                .flatten()
        })
}

fn field_first_value(value: &BTreeMap<String, serde_json::Value>) -> Option<String> {
    let value = value.values().find(|value| !value.is_null())?;
    match value {
        serde_json::Value::String(value) => (!value.is_empty()).then(|| value.clone()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        serde_json::Value::Object(object) => object
            .get("email_address")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                object
                    .values()
                    .filter_map(serde_json::Value::as_str)
                    .collect::<Vec<_>>()
                    .first()
                    .map(|value| (*value).to_owned())
            }),
        _ => None,
    }
}

pub(crate) fn parse_csv(bytes: &[u8]) -> Result<ImportBundle, ImportError> {
    let document = parse_csv_document(bytes)?;
    let mut folders = Vec::new();
    let mut known = HashMap::new();
    let mut bundle = ImportBundle {
        format: ImportFormat::OnePasswordCsv,
        folders: Vec::new(),
        items: Vec::new(),
        issues: document.issues,
    };
    for (index, row) in document.rows.into_iter().enumerate() {
        let title = row.value(&["title", "name"]).unwrap_or_default();
        if title.is_empty() {
            bundle.issues.push(ImportIssue {
                line: row.line,
                kind: ImportIssueKind::MissingTitle,
            });
            continue;
        }
        let folder = row.value(&["vault", "folder", "group"]).unwrap_or_default();
        let folder_id = add_folder_path(&mut folders, &mut known, folder)
            .unwrap_or_else(|| PERSONAL_FOLDER_ID.to_owned());
        let external_id = generated_external_id("1password", row.line, index);
        let mut item = new_item(
            &external_id,
            ItemType::Login,
            title,
            row.value(&["url", "website"]).map(str::to_owned),
            row.value(&["username", "email"]).map(str::to_owned),
            vec![folder_id],
        );
        item.draft.password = row
            .value(&["password", "password(password)", "password(pop_password)"])
            .and_then(secret);
        item.draft.totp_secret = row
            .value(&["totp", "otp", "one-time password"])
            .and_then(normalize_totp);
        if let Some(url) = row.value(&["url", "website"]) {
            add_uri(&mut item.draft, url);
        }
        set_notes(&mut item.draft, row.value(&["notes", "notesplain"]));
        for (name, value) in row.entries() {
            if !is_official(name) {
                add_field(
                    &mut item.draft,
                    name,
                    value,
                    name.contains("password") || name.contains("secret"),
                );
            }
        }
        bundle.items.push(item);
    }
    bundle.folders = folders;
    Ok(bundle)
}

fn is_official(name: &str) -> bool {
    matches!(
        name,
        "title"
            | "name"
            | "url"
            | "website"
            | "username"
            | "email"
            | "password"
            | "notes"
            | "vault"
            | "folder"
            | "group"
            | "totp"
            | "otp"
    )
}
