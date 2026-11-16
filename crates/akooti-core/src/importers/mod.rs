mod archive_support;
mod bitwarden;
mod bitwarden_crypto;
mod browser_csv;
mod buttercup;
mod crypto_support;
mod csv_support;
mod dedup;
mod error;
mod keepass;
mod onepassword;
mod onepassword_types;
mod pipeline;
mod protonpass;
mod support;
mod types;

use crate::models::SecretString;

pub use error::ImportError;
pub use types::{
    DuplicateMatch, DuplicatePolicy, DuplicateReason, ImportBundle, ImportCleanup, ImportFolder,
    ImportFormat, ImportIssue, ImportIssueKind, ImportItem, ImportOptions, ImportPreview,
    ImportReport, SecureDeleteReport,
};

pub(crate) fn parse(
    format: ImportFormat,
    bytes: &[u8],
    password: Option<&SecretString>,
) -> Result<ImportBundle, ImportError> {
    let bundle = match format {
        ImportFormat::Buttercup => buttercup::parse_bcup(bytes, password)?,
        ImportFormat::ButtercupCsv => buttercup::parse_csv(bytes)?,
        ImportFormat::BitwardenJson => bitwarden::parse(bytes, password)?,
        ImportFormat::ProtonPass => {
            if bytes.starts_with(b"PK")
                || std::str::from_utf8(bytes)
                    .ok()
                    .is_some_and(|text| text.trim_start().starts_with('{'))
            {
                protonpass::parse(bytes)?
            } else {
                protonpass::parse_csv(bytes)?
            }
        }
        ImportFormat::KeePass => keepass::parse_kdbx(bytes, password)?,
        ImportFormat::KeePassXml => keepass::parse_xml(bytes)?,
        ImportFormat::KeePassCsv => keepass::parse_csv(bytes)?,
        ImportFormat::OnePasswordPux => onepassword::parse_pux(bytes)?,
        ImportFormat::OnePasswordCsv => onepassword::parse_csv(bytes)?,
        ImportFormat::BrowserCsv => browser_csv::parse(bytes)?,
    };
    ensure_non_empty(bundle)
}

fn ensure_non_empty(bundle: ImportBundle) -> Result<ImportBundle, ImportError> {
    if bundle.items.is_empty() {
        if bundle.issues.is_empty() {
            Err(ImportError::Empty)
        } else {
            Ok(bundle)
        }
    } else {
        Ok(bundle)
    }
}

pub(crate) fn detect_format(path: Option<&std::path::Path>, bytes: &[u8]) -> ImportFormat {
    if bytes.starts_with(b"b~>buttercup/b") {
        return ImportFormat::Buttercup;
    }
    if let Some(path) = path
        && let Some(extension) = path.extension().and_then(|value| value.to_str())
    {
        match extension.to_ascii_lowercase().as_str() {
            "bcup" => return ImportFormat::Buttercup,
            "kdbx" | "kdb" => return ImportFormat::KeePass,
            "xml" => return ImportFormat::KeePassXml,
            "1pux" => return ImportFormat::OnePasswordPux,
            "zip" => return ImportFormat::ProtonPass,
            _ => {}
        }
    }
    let text = csv_support::decode_text(bytes);
    let trimmed = text.trim_start();
    if trimmed.starts_with('{')
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed)
    {
        if value.get("passwordProtected").is_some()
            || value.get("encKeyValidation_DO_NOT_EDIT").is_some()
        {
            return ImportFormat::BitwardenJson;
        }
        if value.get("vaults").is_some() && value.get("version").is_some() {
            return ImportFormat::ProtonPass;
        }
        if value.get("items").is_some() {
            return ImportFormat::BitwardenJson;
        }
    }
    let header = trimmed
        .lines()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if header.contains("!group_id") || header.contains("!group_name") {
        ImportFormat::ButtercupCsv
    } else if header.contains("totp") && header.contains("title") {
        ImportFormat::KeePassCsv
    } else if header.contains("uuid") && header.contains("title") {
        ImportFormat::OnePasswordCsv
    } else {
        ImportFormat::BrowserCsv
    }
}
