use std::collections::HashMap;

use url::Url;

use crate::models::{
    CustomFieldInput, ItemType, ItemUriInput, SecretString, Selectors, UriMatchType, VaultItemDraft,
};

use super::types::{ImportFolder, ImportItem};

pub(crate) fn new_item(
    external_id: &str,
    item_type: ItemType,
    title: &str,
    primary_url: Option<String>,
    username: Option<String>,
    folder_ids: Vec<String>,
) -> ImportItem {
    ImportItem {
        external_id: external_id.to_owned(),
        draft: VaultItemDraft {
            item_type,
            title: if title.trim().is_empty() {
                "Imported entry".to_owned()
            } else {
                title.trim().to_owned()
            },
            primary_url,
            username,
            password: None,
            totp_secret: None,
            notes: None,
            selectors: Selectors::default(),
            created_by_agent: None,
            expires_at: None,
            favorite: false,
            folder_ids,
            uris: Vec::new(),
            custom_fields: Vec::new(),
        },
    }
}

pub(crate) fn generated_external_id(prefix: &str, line: Option<usize>, index: usize) -> String {
    match line {
        Some(line) => format!("{prefix}:{line}:{index}"),
        None => format!("{prefix}:{index}"),
    }
}

pub(crate) fn secret(value: &str) -> Option<SecretString> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(SecretString::new(value.to_owned()))
    }
}

pub(crate) fn add_uri(draft: &mut VaultItemDraft, value: &str) {
    let value = value.trim();
    if value.is_empty() || draft.uris.iter().any(|uri| uri.uri == value) {
        return;
    }
    draft.uris.push(ItemUriInput {
        uri: value.to_owned(),
        match_type: UriMatchType::Domain,
    });
}

pub(crate) fn add_field(draft: &mut VaultItemDraft, name: &str, value: &str, is_secret: bool) {
    let name = name.trim();
    let value = value.trim();
    if name.is_empty() || value.is_empty() {
        return;
    }
    if value.len() > 16_384 {
        return;
    }
    let sort_order = i64::try_from(draft.custom_fields.len()).unwrap_or(i64::MAX);
    draft.custom_fields.push(CustomFieldInput {
        field_name: name.to_owned(),
        value: SecretString::new(value.to_owned()),
        is_secret,
        sort_order,
    });
}

pub(crate) fn set_notes(draft: &mut VaultItemDraft, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        draft.notes = Some(SecretString::new(value.to_owned()));
    }
}

pub(crate) fn normalize_totp(value: &str) -> Option<SecretString> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if value.starts_with("otpauth://") {
        let url = Url::parse(value).ok()?;
        return url
            .query_pairs()
            .find(|(key, _)| key.eq_ignore_ascii_case("secret"))
            .and_then(|(_, secret_value)| secret(secret_value.as_ref()));
    }
    if value.starts_with("http://") || value.starts_with("https://") {
        return None;
    }
    secret(value)
}

pub(crate) fn title_from_url(value: &str) -> Option<String> {
    Url::parse(value)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
}

pub(crate) fn add_folder_path(
    folders: &mut Vec<ImportFolder>,
    known: &mut HashMap<String, String>,
    path: &str,
) -> Option<String> {
    let mut normalized_path = Vec::new();
    let mut parent = None;
    for segment in path.split('/').filter(|segment| !segment.trim().is_empty()) {
        normalized_path.push(segment.trim());
        let key = normalized_path.join("/");
        let external_id = known
            .entry(key.clone())
            .or_insert_with(|| format!("path:{key}"))
            .clone();
        if !folders
            .iter()
            .any(|folder| folder.external_id == external_id)
        {
            folders.push(ImportFolder {
                external_id: external_id.clone(),
                name: segment.trim().to_owned(),
                parent_external_id: parent.clone(),
            });
        }
        parent = Some(external_id);
    }
    parent
}

pub(crate) fn uri_match_from_code(value: i64) -> UriMatchType {
    match value {
        1 => UriMatchType::Host,
        2 => UriMatchType::StartsWith,
        3 => UriMatchType::Exact,
        4 => UriMatchType::Regex,
        _ => UriMatchType::Domain,
    }
}
