use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

use rusqlite::Connection;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::Vault;
use crate::folders;
use crate::items::{self, InsertOptions};
use crate::models::{ItemFilter, SecretString};

use super::dedup::duplicate_for;
use super::error::ImportError;
use super::types::{
    DuplicatePolicy, ImportBundle, ImportCleanup, ImportFormat, ImportIssue, ImportIssueKind,
    ImportOptions, ImportPreview, ImportReport, SecureDeleteReport,
};
use super::{detect_format, parse};

impl Vault {
    /// Computes duplicate matches for a normalized import bundle.
    ///
    /// # Errors
    ///
    /// Returns an error when existing item metadata cannot be read.
    pub fn preview_import(&self, bundle: &ImportBundle) -> Result<ImportPreview, ImportError> {
        let existing = self.list_items(&ItemFilter::default())?;
        let mut duplicates = Vec::new();
        for imported in &bundle.items {
            if let Some(found) =
                duplicate_for(&imported.draft, imported.external_id.as_str(), &existing)
            {
                duplicates.push(found);
            }
        }
        Ok(ImportPreview {
            item_count: bundle.items.len(),
            folder_count: bundle.folders.len(),
            duplicates,
        })
    }

    /// Applies a normalized bundle in one SQLCipher transaction.
    ///
    /// # Errors
    ///
    /// Returns an error and rolls back every folder and item when any insert or
    /// validation fails. Duplicate choices are applied before the transaction.
    pub fn import_bundle(
        &mut self,
        bundle: ImportBundle,
        options: ImportOptions,
    ) -> Result<ImportReport, ImportError> {
        let preview = self.preview_import(&bundle)?;
        let skipped = match options.duplicate_policy {
            DuplicatePolicy::Skip => preview
                .duplicates
                .iter()
                .map(|item| item.imported_external_id.as_str())
                .collect::<HashSet<_>>(),
            DuplicatePolicy::ImportAnyway => HashSet::new(),
        };
        let now = crate::vault::unix_seconds(std::time::SystemTime::now())?;
        let master_key = Zeroizing::new(*self.master_key());
        let transaction = self.connection_mut().transaction()?;
        let folder_map = insert_folders(&transaction, &bundle)?;
        let (imported, routed) = insert_items(
            &transaction,
            &master_key,
            bundle.items,
            &folder_map,
            &skipped,
            options.route_dev_local,
            now,
        )?;
        transaction.commit()?;
        Ok(ImportReport {
            imported,
            skipped: skipped.len(),
            routed,
            issues: bundle.issues,
            cleanup: ImportCleanup::NotRequested,
        })
    }

    /// Parses and imports an in-memory source.
    ///
    /// # Errors
    ///
    /// Returns a parse or transaction error when the source cannot be imported.
    pub fn import_bytes(
        &mut self,
        format: ImportFormat,
        bytes: &[u8],
        password: Option<&SecretString>,
        options: ImportOptions,
    ) -> Result<ImportReport, ImportError> {
        let bundle = parse(format, bytes, password)?;
        self.import_bundle(bundle, options)
    }

    /// Imports a source file and optionally securely removes it after commit.
    ///
    /// # Errors
    ///
    /// Returns an error before commit when the file cannot be read, parsed, or
    /// persisted. Cleanup failures after commit are reported in [`ImportReport`].
    pub fn import_file(
        &mut self,
        path: &Path,
        format: Option<ImportFormat>,
        password: Option<&SecretString>,
        options: ImportOptions,
    ) -> Result<ImportReport, ImportError> {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(ImportError::InvalidSource);
        }
        let bytes = Zeroizing::new(fs::read(path)?);
        let selected = format.unwrap_or_else(|| detect_format(Some(path), bytes.as_slice()));
        let bundle = parse(selected, bytes.as_slice(), password)?;
        let mut report = self.import_bundle(bundle, options)?;
        if options.secure_delete_source {
            match secure_delete(path) {
                Ok(result) => report.cleanup = ImportCleanup::Completed(result),
                Err(_) => {
                    let issue = ImportIssue {
                        line: None,
                        kind: ImportIssueKind::SecureDeleteFailed,
                    };
                    report.issues.push(issue.clone());
                    report.cleanup = ImportCleanup::Failed(issue);
                }
            }
        }
        Ok(report)
    }
}

fn insert_folders(
    connection: &Connection,
    bundle: &ImportBundle,
) -> Result<HashMap<String, String>, ImportError> {
    let now = crate::vault::unix_seconds(std::time::SystemTime::now())?;
    let mut map: HashMap<String, String> = HashMap::new();
    let mut remaining = bundle.folders.clone();
    while !remaining.is_empty() {
        let remaining_count = remaining.len();
        let mut deferred = Vec::new();
        for folder in remaining {
            let parent_id = match folder.parent_external_id.as_deref() {
                None => None,
                Some(parent) => match map.get(parent) {
                    Some(mapped) => Some(mapped.clone()),
                    None if folders::get(connection, parent).is_ok() => Some(parent.to_owned()),
                    None => {
                        deferred.push(folder);
                        continue;
                    }
                },
            };
            let created = folders::create(
                connection,
                crate::models::FolderDraft {
                    parent_id,
                    name: folder.name,
                    icon: "folder".to_owned(),
                    color: None,
                    require_biometric_for_mcp: false,
                },
                now,
            )?;
            map.insert(folder.external_id, created.id);
        }
        if deferred.len() == remaining_count {
            return Err(ImportError::InvalidFolderGraph);
        }
        remaining = deferred;
    }
    Ok(map)
}

fn insert_items(
    connection: &Connection,
    master_key: &[u8; crate::crypto::KEY_LEN],
    items: Vec<super::types::ImportItem>,
    folder_map: &HashMap<String, String>,
    skipped: &HashSet<&str>,
    route_dev_local: bool,
    now: i64,
) -> Result<(usize, usize), ImportError> {
    let mut imported = 0;
    let mut routed = 0;
    for item in items {
        if skipped.contains(item.external_id.as_str()) {
            continue;
        }
        let mut draft = item.draft;
        draft.folder_ids = resolve_folders(connection, &draft.folder_ids, folder_map)?;
        let links = items::insert_draft(
            connection,
            master_key,
            &Uuid::new_v4().to_string(),
            draft,
            now,
            InsertOptions {
                route: route_dev_local,
            },
        )?;
        imported += 1;
        if links > 0 {
            routed += 1;
        }
    }
    Ok((imported, routed))
}

fn resolve_folders(
    connection: &Connection,
    references: &[String],
    mapping: &HashMap<String, String>,
) -> Result<Vec<String>, ImportError> {
    let mut resolved = Vec::new();
    for reference in references {
        let folder_id = match mapping.get(reference) {
            Some(value) => value.clone(),
            None if folders::get(connection, reference).is_ok() => reference.clone(),
            None => return Err(ImportError::InvalidFolderGraph),
        };
        if !resolved.contains(&folder_id) {
            resolved.push(folder_id);
        }
    }
    Ok(resolved)
}

/// Overwrites a regular file with zeros and removes it.
///
/// # Errors
///
/// Returns an error for non-regular paths, failed writes, or failed removal.
pub fn secure_delete(path: &Path) -> Result<SecureDeleteReport, ImportError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ImportError::InvalidSource);
    }
    let size = metadata.len();
    let mut file = OpenOptions::new().write(true).open(path)?;
    file.seek(SeekFrom::Start(0))?;
    const ZERO_CHUNK: usize = 8192;
    let zeros = [0_u8; ZERO_CHUNK];
    let mut remaining = size;
    while remaining > 0 {
        let chunk_limit = u64::try_from(ZERO_CHUNK).map_err(|_| ImportError::InvalidSource)?;
        let chunk =
            usize::try_from(remaining.min(chunk_limit)).map_err(|_| ImportError::InvalidSource)?;
        file.write_all(&zeros[..chunk])?;
        remaining -= u64::try_from(chunk).map_err(|_| ImportError::InvalidSource)?;
    }
    file.sync_all()?;
    file.set_len(0)?;
    drop(file);
    fs::remove_file(path)?;
    Ok(SecureDeleteReport {
        bytes_overwritten: size,
    })
}
