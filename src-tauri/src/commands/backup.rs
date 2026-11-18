//! Backup target commands: configuration, test, manual run and history.
//!
//! Provider credentials are write-only from the UI's point of view: they are
//! encrypted by the core and never returned. Editing a target without retyping
//! them keeps the stored configuration.

use std::sync::Arc;

use akooti_core::{
    BackupConfig, BackupRun, BackupTarget, BackupTargetDraft, MIN_INTERVAL_MINUTES, MIN_RETENTION,
};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::backup;
use crate::error::CommandError;
use crate::vault_runtime::VaultRuntime;

/// Target as displayed; never carries a credential.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupTargetDto {
    /// Target identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// `local`, `s3` or `gdrive`.
    pub provider: &'static str,
    /// Non-secret summary (`path`, `endpoint · bucket`, ...).
    pub summary: String,
    /// Minutes between runs.
    pub interval_minutes: u32,
    /// Snapshots kept.
    pub retention_count: u32,
    /// Whether the scheduler considers it.
    pub enabled: bool,
    /// Last run time.
    pub last_run_at: Option<i64>,
    /// Last run status.
    pub last_status: Option<String>,
}

impl From<BackupTarget> for BackupTargetDto {
    fn from(target: BackupTarget) -> Self {
        Self {
            id: target.id,
            name: target.name,
            provider: target.provider.as_str(),
            summary: target.summary,
            interval_minutes: target.interval_minutes,
            retention_count: target.retention_count,
            enabled: target.enabled,
            last_run_at: target.last_run_at,
            last_status: target.last_status,
        }
    }
}

/// One history entry.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRunDto {
    /// Run identifier.
    pub id: String,
    /// Target it ran for.
    pub target_id: String,
    /// Start time.
    pub started_at: i64,
    /// `success` or `error`.
    pub status: String,
    /// Short non-secret diagnostic.
    pub detail: Option<String>,
    /// Snapshot size.
    pub size_bytes: Option<i64>,
    /// Snapshot name.
    pub snapshot_name: Option<String>,
}

impl From<BackupRun> for BackupRunDto {
    fn from(run: BackupRun) -> Self {
        Self {
            id: run.id,
            target_id: run.target_id,
            started_at: run.started_at,
            status: run.status,
            detail: run.detail,
            size_bytes: run.size_bytes,
            snapshot_name: run.snapshot_name,
        }
    }
}

/// Flat form payload; the provider decides which fields matter.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupTargetInput {
    /// Display name.
    pub name: String,
    /// `local`, `s3` or `gdrive`.
    pub provider: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub bucket: Option<String>,
    #[serde(default)]
    pub access_key_id: Option<String>,
    #[serde(default)]
    pub secret_access_key: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub folder_id: Option<String>,
    #[serde(default = "default_interval")]
    pub interval_minutes: u32,
    #[serde(default = "default_retention")]
    pub retention_count: u32,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_interval() -> u32 {
    MIN_INTERVAL_MINUTES
}

fn default_retention() -> u32 {
    24
}

fn default_true() -> bool {
    true
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

impl BackupTargetInput {
    fn config(&self) -> Result<BackupConfig, CommandError> {
        let missing = |field: &str| {
            CommandError::new("invalid_input", format!("Le champ « {field} » est requis."))
        };
        match self.provider.as_str() {
            "local" => Ok(BackupConfig::Local {
                path: self
                    .path
                    .as_deref()
                    .and_then(|path| non_empty(Some(path)))
                    .ok_or_else(|| missing("dossier"))?,
            }),
            "s3" => Ok(BackupConfig::S3 {
                endpoint: non_empty(self.endpoint.as_deref()).ok_or_else(|| missing("endpoint"))?,
                region: non_empty(self.region.as_deref()).ok_or_else(|| missing("region"))?,
                bucket: non_empty(self.bucket.as_deref()).ok_or_else(|| missing("bucket"))?,
                access_key_id: non_empty(self.access_key_id.as_deref())
                    .ok_or_else(|| missing("clé d'accès"))?,
                secret_access_key: non_empty(self.secret_access_key.as_deref())
                    .ok_or_else(|| missing("clé secrète"))?,
                prefix: non_empty(self.prefix.as_deref()),
            }),
            "gdrive" => Ok(BackupConfig::GDrive {
                client_id: non_empty(self.client_id.as_deref())
                    .ok_or_else(|| missing("client id"))?,
                client_secret: non_empty(self.client_secret.as_deref())
                    .ok_or_else(|| missing("client secret"))?,
                refresh_token: non_empty(self.refresh_token.as_deref())
                    .ok_or_else(|| missing("refresh token"))?,
                folder_id: non_empty(self.folder_id.as_deref()),
            }),
            other => Err(CommandError::new(
                "invalid_input",
                format!("Fournisseur inconnu : {other}."),
            )),
        }
    }

    fn into_draft(self) -> Result<BackupTargetDraft, CommandError> {
        let config = self.config()?;
        Ok(BackupTargetDraft {
            name: self.name,
            config,
            interval_minutes: self.interval_minutes.max(MIN_INTERVAL_MINUTES),
            retention_count: self.retention_count.max(MIN_RETENTION),
            enabled: self.enabled,
        })
    }
}

/// Lists configured targets.
///
/// # Errors
///
/// Returns `locked` when the vault is locked.
#[tauri::command]
pub fn list_backup_targets(
    runtime: State<'_, Arc<VaultRuntime>>,
) -> Result<Vec<BackupTargetDto>, CommandError> {
    runtime.touch();
    let targets = runtime.with_vault(|vault| vault.backup_targets())?;
    Ok(targets.into_iter().map(Into::into).collect())
}

/// Creates a target.
///
/// # Errors
///
/// Returns `invalid_input` when a required provider field is missing.
#[tauri::command]
pub fn create_backup_target(
    runtime: State<'_, Arc<VaultRuntime>>,
    input: BackupTargetInput,
) -> Result<BackupTargetDto, CommandError> {
    runtime.touch();
    let draft = input.into_draft()?;
    let target = runtime.with_vault_mut(|vault| vault.create_backup_target(draft))?;
    Ok(target.into())
}

/// Updates a target; omitted credentials keep the stored configuration.
///
/// # Errors
///
/// Returns `not_found` for an unknown target, `invalid_input` otherwise.
#[tauri::command]
pub fn update_backup_target(
    runtime: State<'_, Arc<VaultRuntime>>,
    id: String,
    input: BackupTargetInput,
) -> Result<BackupTargetDto, CommandError> {
    runtime.touch();
    let existing = runtime.with_vault(|vault| vault.backup_config(&id))?;
    let same_provider = existing.provider().as_str() == input.provider;
    let config = if same_provider && credentials_missing(&input) {
        existing
    } else {
        input.config()?
    };
    let draft = BackupTargetDraft {
        name: input.name,
        config,
        interval_minutes: input.interval_minutes.max(MIN_INTERVAL_MINUTES),
        retention_count: input.retention_count.max(MIN_RETENTION),
        enabled: input.enabled,
    };
    let target = runtime.with_vault_mut(|vault| vault.update_backup_target(&id, draft))?;
    Ok(target.into())
}

fn credentials_missing(input: &BackupTargetInput) -> bool {
    match input.provider.as_str() {
        "local" => input
            .path
            .as_deref()
            .and_then(|value| non_empty(Some(value)))
            .is_none(),
        "s3" => {
            input
                .secret_access_key
                .as_deref()
                .and_then(|v| non_empty(Some(v)))
                .is_none()
                || input
                    .access_key_id
                    .as_deref()
                    .and_then(|v| non_empty(Some(v)))
                    .is_none()
        }
        "gdrive" => input
            .refresh_token
            .as_deref()
            .and_then(|v| non_empty(Some(v)))
            .is_none(),
        _ => false,
    }
}

/// Deletes a target and its history.
///
/// # Errors
///
/// Returns `not_found` for an unknown target.
#[tauri::command]
pub fn delete_backup_target(
    runtime: State<'_, Arc<VaultRuntime>>,
    id: String,
) -> Result<(), CommandError> {
    runtime.touch();
    runtime.with_vault_mut(|vault| vault.delete_backup_target(&id))?;
    Ok(())
}

/// Enables or disables one target.
///
/// # Errors
///
/// Returns `not_found` for an unknown target.
#[tauri::command]
pub fn set_backup_target_enabled(
    runtime: State<'_, Arc<VaultRuntime>>,
    id: String,
    enabled: bool,
) -> Result<BackupTargetDto, CommandError> {
    runtime.touch();
    let target = runtime.with_vault_mut(|vault| vault.set_backup_target_enabled(&id, enabled))?;
    Ok(target.into())
}

/// Tests a configuration without saving it.
///
/// # Errors
///
/// Returns `backup` with a short diagnostic when the provider refuses.
#[tauri::command(async)]
pub fn test_backup_target(input: BackupTargetInput) -> Result<String, CommandError> {
    let config = input.config()?;
    backup::test_config(&config)
}

/// Runs one target now.
///
/// # Errors
///
/// Returns `backup` with the provider diagnostic, `backup_busy` when a run is
/// already in flight, or `locked` when the vault is locked.
#[tauri::command(async)]
pub fn run_backup_now(
    runtime: State<'_, Arc<VaultRuntime>>,
    id: String,
) -> Result<BackupRunDto, CommandError> {
    runtime.touch();
    let run = backup::run_target(runtime.inner(), &id)?;
    Ok(run.into())
}

/// Lists the run history, newest first.
///
/// # Errors
///
/// Returns `locked` when the vault is locked.
#[tauri::command]
pub fn list_backup_runs(
    runtime: State<'_, Arc<VaultRuntime>>,
    target_id: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<BackupRunDto>, CommandError> {
    runtime.touch();
    let runs =
        runtime.with_vault(|vault| vault.backup_runs(target_id.as_deref(), limit.unwrap_or(50)))?;
    Ok(runs.into_iter().map(Into::into).collect())
}

/// Opens the native folder picker for a local target.
///
/// # Errors
///
/// Returns `dialog` when the picker cannot be shown.
#[tauri::command(async)]
pub fn pick_backup_folder(app: tauri::AppHandle) -> Result<Option<String>, CommandError> {
    use tauri_plugin_dialog::DialogExt;

    let picked = app
        .dialog()
        .file()
        .set_title("Dossier de sauvegarde")
        .blocking_pick_folder();
    match picked {
        None => Ok(None),
        Some(path) => path
            .into_path()
            .map(|path| Some(path.to_string_lossy().into_owned()))
            .map_err(|_| CommandError::new("dialog", "Ce dossier n'est pas accessible.")),
    }
}
