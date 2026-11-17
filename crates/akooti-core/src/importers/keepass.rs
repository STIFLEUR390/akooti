use std::collections::HashMap;

use keepass::db::{DatabaseOpenError, fields};
use keepass::{Database, DatabaseKey};
use serde::Deserialize;

use crate::models::{ItemType, PERSONAL_FOLDER_ID, SecretString};

use super::csv_support::parse_csv as parse_csv_document;
use super::error::ImportError;
use super::support::{
    add_field, add_folder_path, add_uri, generated_external_id, new_item, normalize_totp, secret,
    set_notes,
};
use super::types::{ImportBundle, ImportFormat, ImportIssue, ImportIssueKind};

pub(crate) fn parse_kdbx(
    bytes: &[u8],
    password: Option<&SecretString>,
) -> Result<ImportBundle, ImportError> {
    let password = password.ok_or(ImportError::PasswordRequired)?;
    let database = Database::parse(bytes, DatabaseKey::new().with_password(password.expose()))
        .map_err(|error| match error {
            DatabaseOpenError::Key(_) | DatabaseOpenError::Cryptography(_) => {
                ImportError::InvalidPassword
            }
            _ => ImportError::CorruptSource,
        })?;
    let mut bundle = ImportBundle {
        format: ImportFormat::KeePass,
        folders: database
            .iter_all_groups()
            .filter(|group| !group.name.trim().is_empty())
            .map(|group| super::types::ImportFolder {
                external_id: group.id().to_string(),
                name: group.name.clone(),
                parent_external_id: group
                    .parent()
                    .filter(|parent| !parent.name.trim().is_empty())
                    .map(|parent| parent.id().to_string()),
            })
            .collect(),
        items: Vec::new(),
        issues: Vec::new(),
    };
    for entry in database.iter_all_entries() {
        let title = entry.get_title().unwrap_or("Imported entry");
        let parent = entry.parent();
        let folder_id = if parent.name.trim().is_empty() {
            PERSONAL_FOLDER_ID.to_owned()
        } else {
            parent.id().to_string()
        };
        let mut item = new_item(
            &entry.id().to_string(),
            ItemType::Login,
            title,
            entry.get_url().map(str::to_owned),
            entry.get_username().map(str::to_owned),
            vec![folder_id],
        );
        item.draft.password = entry.get_password().and_then(secret);
        item.draft.totp_secret = entry.get_raw_otp_value().and_then(normalize_totp);
        if let Some(url) = entry.get_url() {
            add_uri(&mut item.draft, url);
        }
        set_notes(&mut item.draft, entry.get(fields::NOTES));
        for (name, value) in &entry.fields {
            if !is_standard(name) {
                add_field(&mut item.draft, name, value.as_str(), true);
            }
        }
        bundle.items.push(item);
    }
    Ok(bundle)
}

#[derive(Deserialize)]
struct XmlFile {
    #[serde(rename = "Root")]
    root: XmlRoot,
}

#[derive(Deserialize)]
struct XmlRoot {
    #[serde(rename = "Group")]
    group: XmlGroup,
}

#[derive(Deserialize)]
struct XmlGroup {
    #[serde(rename = "UUID", default)]
    uuid: String,
    #[serde(rename = "Name", default)]
    name: String,
    #[serde(rename = "Group", default)]
    groups: Vec<XmlGroup>,
    #[serde(rename = "Entry", default)]
    entries: Vec<XmlEntry>,
}

#[derive(Deserialize)]
struct XmlEntry {
    #[serde(rename = "UUID", default)]
    uuid: String,
    #[serde(rename = "String", default)]
    strings: Vec<XmlString>,
}

#[derive(Deserialize)]
struct XmlString {
    #[serde(rename = "Key", default)]
    key: String,
    #[serde(rename = "Value", default)]
    value: String,
    #[serde(rename = "@Protected")]
    protected: Option<String>,
}

pub(crate) fn parse_xml(bytes: &[u8]) -> Result<ImportBundle, ImportError> {
    let text = super::csv_support::decode_text(bytes);
    let file: XmlFile = quick_xml::de::from_str(&text)?;
    let mut bundle = ImportBundle {
        format: ImportFormat::KeePassXml,
        folders: Vec::new(),
        items: Vec::new(),
        issues: Vec::new(),
    };
    flatten_group(&file.root.group, None, "root", &mut bundle);
    Ok(bundle)
}

fn flatten_group(
    group: &XmlGroup,
    parent: Option<&str>,
    fallback_id: &str,
    bundle: &mut ImportBundle,
) {
    let external_id = if group.uuid.is_empty() {
        fallback_id.to_owned()
    } else {
        group.uuid.clone()
    };
    let folder_parent = if group.name.is_empty() { None } else { parent };
    if !group.name.is_empty() {
        bundle.folders.push(super::types::ImportFolder {
            external_id: external_id.clone(),
            name: group.name.clone(),
            parent_external_id: folder_parent.map(str::to_owned),
        });
    }
    for (index, entry) in group.entries.iter().enumerate() {
        let values = entry
            .strings
            .iter()
            .map(|value| (value.key.as_str(), value))
            .collect::<HashMap<_, _>>();
        let entry_id = if entry.uuid.is_empty() {
            format!("{external_id}:entry:{index}")
        } else {
            entry.uuid.clone()
        };
        let mut item = new_item(
            &entry_id,
            ItemType::Login,
            values
                .get("Title")
                .map_or("Imported entry", |value| value.value.as_str()),
            values.get("URL").map(|value| value.value.clone()),
            values.get("UserName").map(|value| value.value.clone()),
            vec![if group.name.is_empty() {
                PERSONAL_FOLDER_ID.to_owned()
            } else {
                external_id.clone()
            }],
        );
        item.draft.password = values
            .get("Password")
            .filter(|value| !value.is_protected())
            .and_then(|value| secret(&value.value));
        item.draft.totp_secret = values
            .get("otp")
            .filter(|value| !value.is_protected())
            .and_then(|value| normalize_totp(&value.value));
        if let Some(url) = values.get("URL") {
            add_uri(&mut item.draft, &url.value);
        }
        set_notes(
            &mut item.draft,
            values.get("Notes").map(|value| value.value.as_str()),
        );
        for (name, value) in values {
            if !is_standard(name) {
                if value.is_protected() {
                    bundle.issues.push(ImportIssue {
                        line: None,
                        kind: ImportIssueKind::ProtectedXmlValue,
                    });
                } else {
                    add_field(&mut item.draft, name, &value.value, true);
                }
            }
        }
        bundle.items.push(item);
    }
    for (index, child) in group.groups.iter().enumerate() {
        let child_fallback = format!("{external_id}:group:{index}");
        flatten_group(
            child,
            (!group.name.is_empty()).then_some(external_id.as_str()),
            &child_fallback,
            bundle,
        );
    }
}

impl XmlString {
    fn is_protected(&self) -> bool {
        self.protected
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
    }
}

pub(crate) fn parse_csv(bytes: &[u8]) -> Result<ImportBundle, ImportError> {
    let document = parse_csv_document(bytes)?;
    let mut folders = Vec::new();
    let mut known = HashMap::new();
    let mut bundle = ImportBundle {
        format: ImportFormat::KeePassCsv,
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
        let group = row.value(&["group"]).unwrap_or_default();
        let group = group.strip_prefix("Root/").unwrap_or(group);
        let folder_id = add_folder_path(&mut folders, &mut known, group)
            .unwrap_or_else(|| PERSONAL_FOLDER_ID.to_owned());
        let external_id = generated_external_id("keepass", row.line, index);
        let mut item = new_item(
            &external_id,
            ItemType::Login,
            title,
            row.value(&["url"]).map(str::to_owned),
            row.value(&["username"]).map(str::to_owned),
            vec![folder_id],
        );
        item.draft.password = row.value(&["password"]).and_then(secret);
        item.draft.totp_secret = row.value(&["totp"]).and_then(normalize_totp);
        if let Some(url) = row.value(&["url"]) {
            add_uri(&mut item.draft, url);
        }
        set_notes(&mut item.draft, row.value(&["notes"]));
        bundle.items.push(item);
    }
    bundle.folders = folders;
    Ok(bundle)
}

fn is_standard(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "title" | "username" | "password" | "url" | "notes" | "otp"
    )
}
