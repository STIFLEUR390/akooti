//! Import wizard commands.
//!
//! Two steps, matching the UI flow:
//!
//! 1. [`preview_import_file`] parses the source **without writing** anything
//!    and reports duplicate candidates;
//! 2. [`import_file`] applies the bundle in one SQLCipher transaction, then
//!    optionally securely deletes the plaintext source.
//!
//! The plaintext file never leaves the backend: the UI only receives counts and
//! non-revealing issue kinds.

use std::path::PathBuf;
use std::sync::Arc;

use akooti_core::{ImportFormat, SecretString};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::dto::{ImportPreviewDto, ImportReportDto, import_options};
use crate::error::CommandError;
use crate::vault_runtime::VaultRuntime;

/// Options for the import wizard.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOptionsDto {
    /// Route matching local entries to `Dev Local`. Checked by default in the UI.
    #[serde(default = "default_true")]
    pub route_dev_local: bool,
    /// Import entries flagged as duplicates instead of skipping them.
    #[serde(default)]
    pub import_duplicates: bool,
    /// Overwrite and remove the source file after a committed import.
    #[serde(default)]
    pub secure_delete_source: bool,
    /// Password protecting an encrypted source, when required.
    #[serde(default)]
    pub password: Option<String>,
}

impl Default for ImportOptionsDto {
    fn default() -> Self {
        Self {
            route_dev_local: true,
            import_duplicates: false,
            secure_delete_source: false,
            password: None,
        }
    }
}

/// One entry of the wizard's format picker.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFormatDto {
    /// Stable identifier sent back to the backend.
    pub id: &'static str,
    /// Human label shown in the picker.
    pub label: &'static str,
}

/// A format the wizard can force, or auto-detection when `id` is `auto`.
struct FormatChoice {
    id: &'static str,
    label: &'static str,
    format: Option<ImportFormat>,
}

const FORMAT_CHOICES: &[FormatChoice] = &[
    FormatChoice {
        id: "auto",
        label: "Détection automatique",
        format: None,
    },
    FormatChoice {
        id: "bitwarden",
        label: "Bitwarden / Vaultwarden (.json)",
        format: Some(ImportFormat::BitwardenJson),
    },
    FormatChoice {
        id: "protonpass",
        label: "Proton Pass (.json, .zip)",
        format: Some(ImportFormat::ProtonPass),
    },
    FormatChoice {
        id: "keepass",
        label: "KeePass / KeePassXC (.kdbx)",
        format: Some(ImportFormat::KeePass),
    },
    FormatChoice {
        id: "keepass_xml",
        label: "KeePass XML (.xml)",
        format: Some(ImportFormat::KeePassXml),
    },
    FormatChoice {
        id: "keepass_csv",
        label: "KeePass CSV (.csv)",
        format: Some(ImportFormat::KeePassCsv),
    },
    FormatChoice {
        id: "onepassword",
        label: "1Password (.1pux)",
        format: Some(ImportFormat::OnePasswordPux),
    },
    FormatChoice {
        id: "onepassword_csv",
        label: "1Password CSV (.csv)",
        format: Some(ImportFormat::OnePasswordCsv),
    },
    FormatChoice {
        id: "buttercup",
        label: "Buttercup (.bcup)",
        format: Some(ImportFormat::Buttercup),
    },
    FormatChoice {
        id: "buttercup_csv",
        label: "Buttercup CSV (.csv)",
        format: Some(ImportFormat::ButtercupCsv),
    },
    FormatChoice {
        id: "browser_csv",
        label: "Navigateur (.csv)",
        format: Some(ImportFormat::BrowserCsv),
    },
];

/// Lists the supported import formats for the wizard's format picker.
#[tauri::command]
#[must_use]
pub fn list_import_formats() -> Vec<ImportFormatDto> {
    FORMAT_CHOICES
        .iter()
        .map(|choice| ImportFormatDto {
            id: choice.id,
            label: choice.label,
        })
        .collect()
}

/// Opens the native file picker and returns the chosen absolute path.
///
/// The dialog runs on a worker thread (`command(async)`) because the blocking
/// picker must not run on the webview's main thread. The path is returned to
/// the UI as a string; the file itself is only ever read by the backend.
///
/// # Errors
///
/// Returns `dialog` when the platform picker cannot be shown.
#[tauri::command(async)]
pub fn pick_import_file(app: tauri::AppHandle) -> Result<Option<String>, CommandError> {
    use tauri_plugin_dialog::DialogExt;

    let picked = app
        .dialog()
        .file()
        .set_title("Choisir un fichier à importer")
        .add_filter(
            "Coffres et exports",
            &["json", "zip", "kdbx", "xml", "csv", "1pux", "bcup"],
        )
        .add_filter("Tous les fichiers", &["*"])
        .blocking_pick_file();
    match picked {
        None => Ok(None),
        Some(path) => {
            let path = path
                .into_path()
                .map_err(|_| CommandError::new("dialog", "Ce fichier n'est pas accessible."))?;
            Ok(Some(path.to_string_lossy().into_owned()))
        }
    }
}

/// Parses a source file without writing anything.
///
/// # Errors
///
/// Returns a typed import error when the file cannot be read or parsed, or when
/// it needs a password that was not supplied.
#[tauri::command]
pub fn preview_import_file(
    runtime: State<'_, Arc<VaultRuntime>>,
    path: String,
    format: Option<String>,
    password: Option<String>,
) -> Result<ImportPreviewDto, CommandError> {
    runtime.touch();
    let path = PathBuf::from(path);
    let format = parse_format(format.as_deref())?;
    let password = password.map(SecretString::new);
    let preview = runtime.run(|vault| {
        let preview = vault.preview_import_file(&path, format, password.as_ref())?;
        Ok(preview.into())
    })?;
    Ok(preview)
}

/// Imports a source file.
///
/// # Errors
///
/// Returns a typed import error when the file cannot be read or the transaction
/// fails. The transaction rolls back every folder and item on failure.
#[tauri::command]
pub fn import_file(
    runtime: State<'_, Arc<VaultRuntime>>,
    path: String,
    format: Option<String>,
    options: Option<ImportOptionsDto>,
) -> Result<ImportReportDto, CommandError> {
    let options = options.unwrap_or_default();
    let core_options = import_options(
        options.route_dev_local,
        options.import_duplicates,
        options.secure_delete_source,
    );
    let path = PathBuf::from(path);
    let format = parse_format(format.as_deref())?;
    let password = options.password.map(SecretString::new);
    runtime.touch();
    let report = runtime.run_mut(|vault| {
        let report = vault.import_file(&path, format, password.as_ref(), core_options)?;
        Ok(report.into())
    })?;
    Ok(report)
}

fn parse_format(value: Option<&str>) -> Result<Option<ImportFormat>, CommandError> {
    let id = value.unwrap_or("auto");
    FORMAT_CHOICES
        .iter()
        .find(|choice| choice.id == id)
        .map(|choice| choice.format)
        .ok_or_else(|| {
            CommandError::new("invalid_input", format!("format d'import inconnu : {id}"))
        })
}

const fn default_true() -> bool {
    true
}
