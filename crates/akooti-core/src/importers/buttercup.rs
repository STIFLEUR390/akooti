use std::collections::{BTreeMap, HashMap};

use serde::Deserialize;

use crate::models::{ItemType, SecretString};

use super::crypto_support::{
    aes_256_cbc_decrypt, decode_base64, decode_hex, derive_pbkdf2_sha256, gunzip_base64,
    hmac_sha256_matches,
};
use super::csv_support::parse_csv as parse_csv_document;
use super::error::ImportError;
use super::support::{
    add_field, add_folder_path, add_uri, generated_external_id, new_item, normalize_totp, secret,
    set_notes,
};
use super::types::{ImportBundle, ImportFormat, ImportIssue, ImportIssueKind};

const SIGNATURE: &str = "b~>buttercup/b";

#[derive(Deserialize)]
struct Vault {
    #[serde(default)]
    g: Vec<Group>,
    #[serde(default)]
    e: Vec<Entry>,
}

#[derive(Deserialize)]
struct Group {
    id: String,
    #[serde(default)]
    t: String,
    #[serde(default)]
    g: String,
}

#[derive(Deserialize)]
struct Entry {
    id: String,
    #[serde(default)]
    g: String,
    #[serde(default)]
    a: BTreeMap<String, Value>,
    #[serde(default)]
    p: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
struct Value {
    #[serde(default)]
    value: String,
    deleted: Option<i64>,
}

pub(crate) fn parse_bcup(
    bytes: &[u8],
    password: Option<&SecretString>,
) -> Result<ImportBundle, ImportError> {
    let text = std::str::from_utf8(bytes).map_err(|_| ImportError::CorruptSource)?;
    let encrypted = text
        .trim()
        .strip_prefix(SIGNATURE)
        .ok_or(ImportError::UnsupportedFormat)?;
    let password = password.ok_or(ImportError::PasswordRequired)?;
    let json = decrypt(encrypted, password.expose())?;
    let vault: Vault = serde_json::from_slice(json.as_slice())?;
    Ok(convert(vault))
}

fn decrypt(encrypted: &str, password: &str) -> Result<zeroize::Zeroizing<Vec<u8>>, ImportError> {
    let parts = encrypted.split('$').collect::<Vec<_>>();
    if parts.len() < 4 || parts.len() > 6 {
        return Err(ImportError::CorruptSource);
    }
    let content = parts[0];
    let iv_text = parts[1];
    let salt = parts[2];
    let auth_text = parts[3];
    let rounds = parts
        .get(4)
        .filter(|value| !value.is_empty())
        .map_or(Ok(1000), |value| {
            value.parse::<u32>().map_err(|_| ImportError::CorruptSource)
        })?;
    if parts.get(5).is_some_and(|method| *method != "cbc") {
        return Err(ImportError::UnsupportedEncryption);
    }
    let derived = derive_pbkdf2_sha256(password, salt.as_bytes(), rounds, 64)?;
    let (key, mac_key) = derived.split_at(32);
    let expected = decode_hex(auth_text)?;
    if !hmac_sha256_matches(
        mac_key,
        &[content.as_bytes(), iv_text.as_bytes(), salt.as_bytes()],
        &expected,
    )? {
        return Err(ImportError::InvalidPassword);
    }
    let iv = decode_hex(iv_text)?;
    let ciphertext = decode_base64(content)?;
    let inflated_base64 = aes_256_cbc_decrypt(key, &iv, ciphertext.as_slice())?;
    let inflated_base64 =
        std::str::from_utf8(inflated_base64.as_slice()).map_err(|_| ImportError::CorruptSource)?;
    gunzip_base64(inflated_base64)
}

fn convert(vault: Vault) -> ImportBundle {
    let mut bundle = ImportBundle {
        format: ImportFormat::Buttercup,
        folders: Vec::new(),
        items: Vec::new(),
        issues: Vec::new(),
    };
    for group in &vault.g {
        bundle.folders.push(super::types::ImportFolder {
            external_id: group.id.clone(),
            name: if group.t.trim().is_empty() {
                "Imported group".to_owned()
            } else {
                group.t.clone()
            },
            parent_external_id: (!group.g.is_empty() && group.g != "0").then(|| group.g.clone()),
        });
    }
    for entry in vault.e {
        let group_id = (!entry.g.is_empty() && entry.g != "0").then(|| entry.g.clone());
        let folder_ids = group_id.into_iter().collect();
        let mut item = new_item(
            &entry.id,
            ItemType::Login,
            property(&entry.p, &["title"]).unwrap_or("Imported entry"),
            property(&entry.p, &["url", "URL"]).map(str::to_owned),
            property(&entry.p, &["username"]).map(str::to_owned),
            folder_ids,
        );
        item.draft.password = property(&entry.p, &["password"]).and_then(secret);
        if let Some(url) = item.draft.primary_url.clone() {
            add_uri(&mut item.draft, &url);
        }
        if let Some(totp) = property(&entry.p, &["otp", "totp"]) {
            item.draft.totp_secret = normalize_totp(totp);
            if item.draft.totp_secret.is_none() {
                bundle.issues.push(ImportIssue {
                    line: None,
                    kind: ImportIssueKind::InvalidTotp,
                });
            }
        }
        set_notes(&mut item.draft, property(&entry.p, &["notes"]));
        for (name, value) in entry.a {
            if value.deleted.is_none() {
                add_field(&mut item.draft, &name, &value.value, true);
            }
        }
        bundle.items.push(item);
    }
    bundle
}

fn property<'a>(properties: &'a BTreeMap<String, Value>, names: &[&str]) -> Option<&'a str> {
    properties
        .iter()
        .find(|(key, value)| {
            value.deleted.is_none() && names.iter().any(|name| key.eq_ignore_ascii_case(name))
        })
        .map(|(_, value)| value.value.as_str())
        .filter(|value| !value.trim().is_empty())
}

pub(crate) fn parse_csv(bytes: &[u8]) -> Result<ImportBundle, ImportError> {
    let document = parse_csv_document(bytes)?;
    let mut folders = Vec::new();
    let mut known_folders = HashMap::new();
    let mut bundle = ImportBundle {
        format: ImportFormat::ButtercupCsv,
        folders: Vec::new(),
        items: Vec::new(),
        issues: document.issues,
    };
    for (index, row) in document.rows.into_iter().enumerate() {
        let group = row.value(&["!group_name", "group"]).unwrap_or_default();
        let folder_id = add_folder_path(&mut folders, &mut known_folders, group);
        let title = row.value(&["title", "name"]).unwrap_or_default();
        if title.is_empty() {
            bundle.issues.push(ImportIssue {
                line: row.line,
                kind: ImportIssueKind::MissingTitle,
            });
            continue;
        }
        let external_id = row
            .value(&["id"])
            .map(str::to_owned)
            .unwrap_or_else(|| generated_external_id("buttercup", row.line, index));
        let mut item = new_item(
            &external_id,
            ItemType::Login,
            title,
            row.value(&["url"]).map(str::to_owned),
            row.value(&["username"]).map(str::to_owned),
            folder_id.into_iter().collect(),
        );
        item.draft.password = row.value(&["password"]).and_then(secret);
        if let Some(url) = row.value(&["url"]) {
            add_uri(&mut item.draft, url);
        }
        if let Some(totp) = row.value(&["otp", "totp"]) {
            item.draft.totp_secret = normalize_totp(totp);
        }
        set_notes(&mut item.draft, row.value(&["note", "notes"]));
        for (name, value) in row.entries() {
            if !is_official(name) {
                add_field(&mut item.draft, name, value, true);
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
        "!group_id"
            | "!group_name"
            | "!type"
            | "title"
            | "username"
            | "password"
            | "url"
            | "note"
            | "notes"
            | "id"
            | "group"
            | "otp"
            | "totp"
    )
}
