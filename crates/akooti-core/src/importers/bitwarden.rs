use std::collections::HashSet;

use serde::Deserialize;

use crate::models::{ItemType, ItemUriInput, PERSONAL_FOLDER_ID, SecretString, UriMatchType};

use super::bitwarden_crypto::{ExportKey, decrypt_export};
use super::error::ImportError;
use super::support::{add_field, new_item, normalize_totp, secret, set_notes, uri_match_from_code};
use super::types::{ImportBundle, ImportFormat, ImportIssue, ImportIssueKind};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Export {
    encrypted: Option<bool>,
    #[serde(default)]
    password_protected: bool,
    salt: Option<String>,
    #[serde(default)]
    kdf_iterations: u32,
    #[serde(default)]
    kdf_type: u32,
    kdf_memory: Option<u32>,
    kdf_parallelism: Option<u32>,
    #[serde(rename = "encKeyValidation_DO_NOT_EDIT")]
    validation: Option<String>,
    data: Option<String>,
    #[serde(default)]
    folders: Vec<Folder>,
    #[serde(default)]
    items: Vec<Item>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Folder {
    id: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    id: Option<String>,
    name: Option<String>,
    #[serde(rename = "type")]
    item_type: Option<u32>,
    notes: Option<String>,
    favorite: Option<bool>,
    folder_id: Option<String>,
    #[serde(default)]
    fields: Vec<CustomField>,
    login: Option<Login>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CustomField {
    name: Option<String>,
    value: Option<String>,
    #[serde(rename = "type")]
    field_type: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Login {
    username: Option<String>,
    password: Option<String>,
    totp: Option<String>,
    #[serde(default)]
    uris: Vec<LoginUri>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginUri {
    uri: Option<String>,
    #[serde(rename = "match")]
    match_type: Option<i64>,
}

pub(crate) fn parse(
    bytes: &[u8],
    password: Option<&SecretString>,
) -> Result<ImportBundle, ImportError> {
    let export: Export = serde_json::from_slice(bytes)?;
    if export.password_protected {
        let password = password.ok_or(ImportError::PasswordRequired)?;
        let clear = decrypt_export(
            ExportKey {
                password: password.expose(),
                salt: export.salt.as_deref().ok_or(ImportError::CorruptSource)?,
                kdf_type: export.kdf_type,
                iterations: export.kdf_iterations,
                memory_kib: export.kdf_memory,
                parallelism: export.kdf_parallelism,
            },
            export
                .validation
                .as_deref()
                .ok_or(ImportError::CorruptSource)?,
            export.data.as_deref().ok_or(ImportError::CorruptSource)?,
        )?;
        let decrypted: Export = serde_json::from_slice(clear.as_slice())?;
        return convert(decrypted);
    }
    if export.encrypted == Some(true) {
        return Err(ImportError::UnsupportedEncryption);
    }
    convert(export)
}

fn convert(export: Export) -> Result<ImportBundle, ImportError> {
    let mut bundle = ImportBundle {
        format: ImportFormat::BitwardenJson,
        folders: export
            .folders
            .into_iter()
            .map(|folder| super::types::ImportFolder {
                external_id: folder.id,
                name: folder.name,
                parent_external_id: None,
            })
            .collect(),
        items: Vec::new(),
        issues: Vec::new(),
    };
    let folder_ids = bundle
        .folders
        .iter()
        .map(|folder| folder.external_id.clone())
        .collect::<HashSet<_>>();
    for (index, item) in export.items.into_iter().enumerate() {
        let external_id = item
            .id
            .clone()
            .unwrap_or_else(|| format!("bitwarden:{index}"));
        let title = item.name.as_deref().unwrap_or("Imported entry");
        let login = item.login.as_ref();
        let item_type = match item.item_type {
            Some(1) => ItemType::Login,
            Some(2) => ItemType::Note,
            _ => ItemType::Note,
        };
        let folders = item
            .folder_id
            .filter(|folder| folder_ids.contains(folder))
            .map_or_else(
                || vec![PERSONAL_FOLDER_ID.to_owned()],
                |folder| vec![folder],
            );
        let mut imported = new_item(
            &external_id,
            item_type,
            title,
            login
                .and_then(|login| login.uris.first())
                .and_then(|uri| uri.uri.clone()),
            login.and_then(|login| login.username.clone()),
            folders,
        );
        imported.draft.favorite = item.favorite.unwrap_or(false);
        imported.draft.password = login
            .and_then(|login| login.password.as_deref())
            .and_then(secret);
        if let Some(totp) = login.and_then(|login| login.totp.as_deref()) {
            imported.draft.totp_secret = normalize_totp(totp);
            if imported.draft.totp_secret.is_none() {
                bundle.issues.push(ImportIssue {
                    line: None,
                    kind: ImportIssueKind::InvalidTotp,
                });
            }
        }
        if let Some(login) = login {
            for uri in &login.uris {
                if let Some(value) = uri.uri.as_deref() {
                    add_typed_uri(&mut imported, value, uri.match_type);
                }
            }
        }
        set_notes(&mut imported.draft, item.notes.as_deref());
        for field in item.fields {
            if let (Some(name), Some(value)) = (field.name.as_deref(), field.value.as_deref()) {
                add_field(
                    &mut imported.draft,
                    name,
                    value,
                    field.field_type == Some(1),
                );
            }
        }
        if imported.draft.primary_url.is_none() {
            imported.draft.primary_url = imported.draft.uris.first().map(|uri| uri.uri.clone());
        }
        bundle.items.push(imported);
    }
    Ok(bundle)
}

fn add_typed_uri(item: &mut super::types::ImportItem, value: &str, match_type: Option<i64>) {
    let value = value.trim();
    if value.is_empty() || item.draft.uris.iter().any(|uri| uri.uri == value) {
        return;
    }
    item.draft.uris.push(ItemUriInput {
        uri: value.to_owned(),
        match_type: match_type.map_or(UriMatchType::Domain, uri_match_from_code),
    });
}
