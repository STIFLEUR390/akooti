use crate::models::{ItemType, PERSONAL_FOLDER_ID};

use super::csv_support::parse_csv;
use super::error::ImportError;
use super::support::{add_uri, generated_external_id, new_item, secret, set_notes, title_from_url};
use super::types::{ImportBundle, ImportFormat, ImportIssue, ImportIssueKind};

pub(crate) fn parse(bytes: &[u8]) -> Result<ImportBundle, ImportError> {
    let document = parse_csv(bytes)?;
    let mut bundle = ImportBundle {
        format: ImportFormat::BrowserCsv,
        folders: Vec::new(),
        items: Vec::new(),
        issues: document.issues,
    };
    for (index, row) in document.rows.into_iter().enumerate() {
        let url = row.value(&["url", "uri", "login_uri", "hostname"]);
        if url == Some("chrome://FirefoxAccounts") {
            continue;
        }
        let username = row.value(&["username", "login_username"]);
        let password = row.value(&["password", "login_password"]);
        if url.is_none() && username.is_none() && password.is_none() {
            bundle.issues.push(ImportIssue {
                line: row.line,
                kind: ImportIssueKind::InvalidRow,
            });
            continue;
        }
        let title = row
            .value(&["name", "title"])
            .map(str::to_owned)
            .or_else(|| url.and_then(title_from_url))
            .unwrap_or_else(|| "Imported login".to_owned());
        let external_id = generated_external_id("browser", row.line, index);
        let mut item = new_item(
            &external_id,
            ItemType::Login,
            &title,
            url.map(str::to_owned),
            username.map(str::to_owned),
            vec![PERSONAL_FOLDER_ID.to_owned()],
        );
        item.draft.password = password.and_then(secret);
        if let Some(url) = url {
            add_uri(&mut item.draft, url);
        }
        set_notes(&mut item.draft, row.value(&["note", "notes", "comment"]));
        bundle.items.push(item);
    }
    Ok(bundle)
}
