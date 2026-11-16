use strsim::normalized_levenshtein;
use url::Url;

use crate::models::{VaultItem, VaultItemDraft};

use super::types::{DuplicateMatch, DuplicateReason};

pub(crate) fn duplicate_for(
    imported: &VaultItemDraft,
    external_id: &str,
    existing: &[VaultItem],
) -> Option<DuplicateMatch> {
    let mut best: Option<DuplicateMatch> = None;
    for item in existing {
        if same_url_and_username(imported, item) {
            return Some(DuplicateMatch {
                imported_external_id: external_id.to_owned(),
                existing_item_id: item.id.clone(),
                reason: DuplicateReason::UrlAndUsername,
                score: 1.0,
            });
        }
        let score = title_similarity(&imported.title, &item.title);
        if score >= 0.9 && best.as_ref().is_none_or(|current| score > current.score) {
            best = Some(DuplicateMatch {
                imported_external_id: external_id.to_owned(),
                existing_item_id: item.id.clone(),
                reason: DuplicateReason::FuzzyTitle,
                score,
            });
        }
    }
    best
}

fn same_url_and_username(imported: &VaultItemDraft, existing: &VaultItem) -> bool {
    let imported_url = imported.primary_url.as_deref().and_then(normalize_url);
    let existing_url = existing.primary_url.as_deref().and_then(normalize_url);
    imported_url.is_some()
        && imported_url == existing_url
        && normalize_username(imported.username.as_deref())
            == normalize_username(existing.username.as_deref())
}

fn normalize_url(value: &str) -> Option<String> {
    let mut url = Url::parse(value).ok()?;
    url.set_fragment(None);
    let host = url.host_str()?.to_ascii_lowercase();
    let path = url.path().trim_end_matches('/');
    Some(format!("{host}{path}"))
}

fn normalize_username(value: Option<&str>) -> String {
    value.unwrap_or_default().trim().to_lowercase()
}

fn title_similarity(left: &str, right: &str) -> f64 {
    let left = normalize_title(left);
    let right = normalize_title(right);
    if left.len() < 4 || right.len() < 4 {
        return 0.0;
    }
    normalized_levenshtein(&left, &right)
}

fn normalize_title(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
