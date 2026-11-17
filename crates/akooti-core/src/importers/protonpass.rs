use std::collections::HashMap;

use serde::Deserialize;

use crate::models::{ItemType, PERSONAL_FOLDER_ID};

use super::archive_support::read_zip_entry;
use super::csv_support::parse_csv as parse_csv_document;
use super::error::ImportError;
use super::support::{
    add_field, add_folder_path, add_uri, generated_external_id, new_item, normalize_totp, secret,
    set_notes,
};
use super::types::{ImportBundle, ImportFormat, ImportIssue, ImportIssueKind};

#[derive(Deserialize)]
struct File {
    #[serde(default)]
    encrypted: bool,
    #[serde(default)]
    vaults: HashMap<String, Vault>,
}

#[derive(Deserialize)]
struct Vault {
    #[serde(default)]
    name: String,
    #[serde(default)]
    items: Vec<Item>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    item_id: Option<String>,
    alias_email: Option<String>,
    #[serde(default)]
    pinned: bool,
    #[serde(default)]
    state: u32,
    data: Data,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Data {
    metadata: Metadata,
    #[serde(default)]
    extra_fields: Vec<ExtraField>,
    #[serde(rename = "type")]
    item_type: String,
    #[serde(default)]
    content: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    #[serde(default)]
    name: String,
    #[serde(default)]
    note: String,
    item_uuid: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Content {
    item_email: Option<String>,
    item_username: Option<String>,
    password: Option<String>,
    totp_uri: Option<String>,
    #[serde(default)]
    urls: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtraField {
    field_name: String,
    #[serde(rename = "type", default)]
    field_type: String,
    data: FieldData,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FieldData {
    content: Option<String>,
    totp_uri: Option<String>,
    timestamp: Option<String>,
}

pub(crate) fn parse(bytes: &[u8]) -> Result<ImportBundle, ImportError> {
    let json = if bytes.starts_with(b"PK") {
        read_zip_entry(bytes, "/data.json")?
    } else {
        zeroize::Zeroizing::new(bytes.to_vec())
    };
    let file: File = serde_json::from_slice(json.as_slice())?;
    if file.encrypted {
        return Err(ImportError::UnsupportedEncryption);
    }
    convert(file)
}

fn convert(file: File) -> Result<ImportBundle, ImportError> {
    let mut bundle = ImportBundle {
        format: ImportFormat::ProtonPass,
        folders: Vec::new(),
        items: Vec::new(),
        issues: Vec::new(),
    };
    for (vault_id, vault) in file.vaults {
        let name = if vault.name.trim().is_empty() {
            "Proton Pass".to_owned()
        } else {
            vault.name
        };
        bundle.folders.push(super::types::ImportFolder {
            external_id: vault_id.clone(),
            name,
            parent_external_id: None,
        });
        for (index, item) in vault.items.into_iter().enumerate() {
            if item.state == 2 {
                continue;
            }
            let external_id = item
                .item_id
                .clone()
                .or(item.data.metadata.item_uuid.clone())
                .unwrap_or_else(|| format!("proton:{vault_id}:{index}"));
            let is_login = item.data.item_type == "login" || item.data.item_type == "alias";
            let item_type = if is_login {
                ItemType::Login
            } else {
                ItemType::Note
            };
            let content =
                serde_json::from_value::<Content>(item.data.content.clone()).unwrap_or_default();
            let primary_url = content.urls.first().cloned();
            let username = login_username(&content, item.alias_email.as_deref());
            let mut imported = new_item(
                &external_id,
                item_type,
                &item.data.metadata.name,
                primary_url,
                username,
                vec![vault_id.clone()],
            );
            imported.draft.favorite = item.pinned;
            if is_login {
                imported.draft.password = content.password.as_deref().and_then(secret);
                if let Some(totp) = content.totp_uri.as_deref() {
                    imported.draft.totp_secret = normalize_totp(totp);
                    if imported.draft.totp_secret.is_none() {
                        bundle.issues.push(ImportIssue {
                            line: None,
                            kind: ImportIssueKind::InvalidTotp,
                        });
                    }
                }
                for url in &content.urls {
                    add_uri(&mut imported.draft, url);
                }
            }
            if let (Some(username), Some(email)) = (
                content.item_username.as_deref(),
                content.item_email.as_deref(),
            ) {
                add_field(&mut imported.draft, "email", email, false);
                imported.draft.username = Some(username.to_owned());
            } else if !is_login {
                add_dynamic_fields(&mut imported, &item.data.content);
            }
            set_notes(&mut imported.draft, Some(&item.data.metadata.note));
            add_extra_fields(&mut imported, &item.data.extra_fields);
            bundle.items.push(imported);
        }
    }
    Ok(bundle)
}

fn login_username(content: &Content, alias_email: Option<&str>) -> Option<String> {
    content
        .item_username
        .clone()
        .or_else(|| content.item_email.clone())
        .or_else(|| alias_email.map(str::to_owned))
}

fn add_dynamic_fields(item: &mut super::types::ImportItem, value: &serde_json::Value) {
    let Some(object) = value.as_object() else {
        return;
    };
    for (name, value) in object {
        let rendered = match value {
            serde_json::Value::String(value) => Some(value.clone()),
            serde_json::Value::Number(value) => Some(value.to_string()),
            serde_json::Value::Bool(value) => Some(value.to_string()),
            _ => None,
        };
        if let Some(value) = rendered {
            let secret_field = ["password", "number", "pin", "secret", "key"]
                .iter()
                .any(|term| name.to_ascii_lowercase().contains(term));
            add_field(&mut item.draft, name, &value, secret_field);
        }
    }
}

fn add_extra_fields(item: &mut super::types::ImportItem, fields: &[ExtraField]) {
    for field in fields {
        let value = field
            .data
            .content
            .as_deref()
            .or(field.data.timestamp.as_deref());
        if let Some(value) = value {
            add_field(
                &mut item.draft,
                &field.field_name,
                value,
                field.field_type == "hidden",
            );
        }
        if let Some(totp) = field.data.totp_uri.as_deref()
            && item.draft.totp_secret.is_none()
        {
            item.draft.totp_secret = normalize_totp(totp);
        }
    }
}

pub(crate) fn parse_csv(bytes: &[u8]) -> Result<ImportBundle, ImportError> {
    let document = parse_csv_document(bytes)?;
    let mut folders = Vec::new();
    let mut known = HashMap::new();
    let mut bundle = ImportBundle {
        format: ImportFormat::ProtonPass,
        folders: Vec::new(),
        items: Vec::new(),
        issues: document.issues,
    };
    for (index, row) in document.rows.into_iter().enumerate() {
        let folder = row.value(&["vault", "folder", "group"]).unwrap_or_default();
        let folder_id = add_folder_path(&mut folders, &mut known, folder)
            .unwrap_or_else(|| PERSONAL_FOLDER_ID.to_owned());
        let title = row.value(&["name", "title"]).unwrap_or_default();
        if title.is_empty() {
            bundle.issues.push(ImportIssue {
                line: row.line,
                kind: ImportIssueKind::MissingTitle,
            });
            continue;
        }
        let is_note = row
            .value(&["type"])
            .is_some_and(|value| value.eq_ignore_ascii_case("note"));
        let external_id = generated_external_id("proton", row.line, index);
        let mut item = new_item(
            &external_id,
            if is_note {
                ItemType::Note
            } else {
                ItemType::Login
            },
            title,
            row.value(&["url", "uri"]).map(str::to_owned),
            row.value(&["username", "email"]).map(str::to_owned),
            vec![folder_id],
        );
        item.draft.password = row.value(&["password"]).and_then(secret);
        item.draft.totp_secret = row.value(&["totp", "otp"]).and_then(normalize_totp);
        if let Some(url) = row.value(&["url", "uri"]) {
            add_uri(&mut item.draft, url);
        }
        set_notes(&mut item.draft, row.value(&["note", "notes"]));
        bundle.items.push(item);
    }
    bundle.folders = folders;
    Ok(bundle)
}
