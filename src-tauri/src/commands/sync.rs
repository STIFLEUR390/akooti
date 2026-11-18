//! Sync commands: endpoint configuration, sign-in, manual runs and Realtime.
//!
//! Secrets never cross this boundary: the UI sends the Supabase password once,
//! the command stores only the resulting tokens (encrypted by the core), and
//! every later call reads them from the vault.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use akooti_core::{SyncSession, SyncSettings};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::error::CommandError;
use crate::sync::client::SupabaseClient;
use crate::sync::realtime::{self, RealtimeConfig};
use crate::sync::time::now_seconds;
use crate::vault_runtime::VaultRuntime;

/// Event carrying the outcome of one run.
pub const STATUS_EVENT: &str = "sync:status";

/// Realtime and run state owned by the desktop app.
#[derive(Debug, Default)]
pub struct SyncState {
    stop: Arc<AtomicBool>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl SyncState {
    fn realtime_active(&self) -> bool {
        self.handle
            .lock()
            .map(|slot| slot.is_some())
            .unwrap_or(false)
    }

    fn stop_realtime(&self) {
        self.stop.store(true, Ordering::SeqCst);
        let handle = self.handle.lock().ok().and_then(|mut slot| slot.take());
        if let Some(handle) = handle {
            let _ = handle.join();
        }
        self.stop.store(false, Ordering::SeqCst);
    }

    /// Starts (or restarts) the Realtime subscription when sync is usable.
    fn ensure_realtime(&self, app: &AppHandle, runtime: &Arc<VaultRuntime>) {
        self.stop_realtime();
        let Ok(settings) = runtime.with_vault(|vault| vault.sync_settings()) else {
            return;
        };
        let Ok(Some(session)) = runtime.with_vault(|vault| vault.sync_session()) else {
            return;
        };
        if !settings.enabled {
            return;
        }
        let (Some(base_url), Some(anon_key), Some(user_id)) = (
            settings.supabase_url.clone(),
            settings.anon_key.clone(),
            settings.user_id.clone(),
        ) else {
            return;
        };
        let config = RealtimeConfig {
            base_url,
            anon_key,
            access_token: session.access_token,
            user_id,
        };
        let stop = Arc::clone(&self.stop);
        let app = app.clone();
        let runtime = Arc::clone(runtime);
        let state = Arc::new(SyncRunContext {
            running: AtomicBool::new(false),
        });
        let handle = std::thread::spawn(move || {
            realtime::run(&config, &stop, &|| {
                run_background(&app, &runtime, &state, false);
            });
        });
        if let Ok(mut slot) = self.handle.lock() {
            *slot = Some(handle);
        }
    }
}

/// Per-run guard, kept out of `SyncState` so the Realtime thread owns it.
#[derive(Debug)]
struct SyncRunContext {
    running: AtomicBool,
}

/// Sync configuration and its last outcome, as displayed by the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatusDto {
    /// Whether the scheduler and Realtime may run.
    pub enabled: bool,
    /// A session exists (tokens stored).
    pub connected: bool,
    /// Project URL.
    pub supabase_url: Option<String>,
    /// Supabase user identifier.
    pub user_id: Option<String>,
    /// Delta cursor.
    pub cursor: Option<String>,
    /// Last run time, Unix seconds.
    pub last_sync_at: Option<i64>,
    /// `success`, `error`, `locked`, ...
    pub last_status: Option<String>,
    /// Short non-secret diagnostic.
    pub last_error: Option<String>,
}

impl From<SyncSettings> for SyncStatusDto {
    fn from(settings: SyncSettings) -> Self {
        Self {
            enabled: settings.enabled,
            connected: false,
            supabase_url: settings.supabase_url,
            user_id: settings.user_id,
            cursor: settings.cursor,
            last_sync_at: settings.last_sync_at,
            last_status: settings.last_status,
            last_error: settings.last_error,
        }
    }
}

/// Sign-in payload.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSignInInput {
    /// Project URL.
    pub supabase_url: String,
    /// Public anon key.
    pub anon_key: String,
    /// Supabase account e-mail.
    pub email: String,
    /// Supabase account password, used once and never stored.
    pub password: String,
}

/// Result of one run.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRunDto {
    /// Records pushed.
    pub pushed: usize,
    /// Records received.
    pub pulled: usize,
    /// Records applied locally.
    pub applied: usize,
    /// Records skipped (local copy newer or equal).
    pub skipped: usize,
    /// Run time, Unix seconds.
    pub at: i64,
    /// Whether the pull restarted from scratch.
    pub full: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusEvent {
    status: &'static str,
    pushed: usize,
    pulled: usize,
    applied: usize,
    skipped: usize,
    detail: Option<String>,
    at: i64,
}

impl From<crate::sync::SyncError> for CommandError {
    fn from(error: crate::sync::SyncError) -> Self {
        Self::new(error.code, error.detail)
    }
}

/// SQL setup script, embedded at build time so packaged users can copy it.
pub const SETUP_SQL: &str = include_str!("../../../supabase/setup.sql");

/// Returns the Supabase setup script shipped with this build.
#[tauri::command]
#[must_use]
pub fn supabase_setup_sql() -> String {
    SETUP_SQL.to_owned()
}

/// Writes the setup script to a file chosen by the user.
///
/// # Errors
///
/// Returns `dialog` when the picker fails, or `io` when the file cannot be
/// written.
#[tauri::command(async)]
pub fn save_supabase_setup(app: AppHandle) -> Result<Option<String>, CommandError> {
    use tauri_plugin_dialog::DialogExt;

    let picked = app
        .dialog()
        .file()
        .set_title("Enregistrer le script Supabase")
        .set_file_name("akooti-supabase-setup.sql")
        .add_filter("SQL", &["sql"])
        .blocking_save_file();
    let Some(path) = picked else {
        return Ok(None);
    };
    let path = path
        .into_path()
        .map_err(|_| CommandError::new("dialog", "Ce fichier n'est pas accessible."))?;
    std::fs::write(&path, SETUP_SQL)
        .map_err(|_| CommandError::new("io", "Le script n'a pas pu être écrit."))?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// Returns the current configuration, and starts Realtime when possible.
///
/// # Errors
///
/// Returns `locked` when the vault is locked.
#[tauri::command]
pub fn sync_status(
    app: AppHandle,
    state: State<'_, Arc<SyncState>>,
    runtime: State<'_, Arc<VaultRuntime>>,
) -> Result<SyncStatusDto, CommandError> {
    runtime.touch();
    let settings = runtime.with_vault(|vault| vault.sync_settings())?;
    let has_session = runtime
        .with_vault(|vault| vault.sync_session())
        .map(|session| session.is_some())
        .unwrap_or(false);
    let mut status = SyncStatusDto::from(settings);
    status.connected = has_session;
    if has_session && status.enabled && !state.realtime_active() {
        state.ensure_realtime(&app, runtime.inner());
    }
    Ok(status)
}

/// Signs in to the user's Supabase project and stores the encrypted tokens.
///
/// # Errors
///
/// Returns `unauthorized` for rejected credentials, `network` when the project
/// is unreachable, or `locked` when the vault is locked.
#[tauri::command(async)]
pub fn sync_sign_in(
    app: AppHandle,
    state: State<'_, Arc<SyncState>>,
    runtime: State<'_, Arc<VaultRuntime>>,
    input: SyncSignInInput,
) -> Result<SyncStatusDto, CommandError> {
    runtime.touch();
    let sign_in = SupabaseClient::sign_in(
        &input.supabase_url,
        &input.anon_key,
        &input.email,
        &input.password,
    )?;
    let status = runtime.with_vault_mut(|vault| {
        vault.configure_sync(
            true,
            Some(&input.supabase_url),
            Some(&input.anon_key),
            Some(&sign_in.user_id),
        )?;
        vault.save_sync_session(&sign_in.session)?;
        vault.sync_settings()
    })?;
    state.ensure_realtime(&app, runtime.inner());
    let mut dto = SyncStatusDto::from(status);
    dto.connected = true;
    Ok(dto)
}

/// Stores an endpoint without signing in, or pauses synchronisation.
///
/// # Errors
///
/// Returns `locked` when the vault is locked.
#[tauri::command]
pub fn sync_configure(
    state: State<'_, Arc<SyncState>>,
    runtime: State<'_, Arc<VaultRuntime>>,
    enabled: bool,
    supabase_url: Option<String>,
    anon_key: Option<String>,
) -> Result<SyncStatusDto, CommandError> {
    runtime.touch();
    let previous = runtime.with_vault(|vault| vault.sync_settings()).ok();
    let user_id = match (&previous, &supabase_url) {
        (Some(settings), Some(url)) if settings.supabase_url.as_deref() != Some(url.as_str()) => {
            None
        }
        (Some(settings), _) => settings.user_id.clone(),
        _ => None,
    };
    let settings = runtime.with_vault_mut(|vault| {
        vault.configure_sync(
            enabled,
            supabase_url.as_deref(),
            anon_key.as_deref(),
            user_id.as_deref(),
        )
    })?;
    if !enabled {
        state.stop_realtime();
    }
    let has_session = runtime
        .with_vault(|vault| vault.sync_session())
        .map(|session| session.is_some())
        .unwrap_or(false);
    let mut dto = SyncStatusDto::from(settings);
    dto.connected = has_session;
    Ok(dto)
}

/// Forgets the session and stops Realtime.
///
/// # Errors
///
/// Returns `locked` when the vault is locked.
#[tauri::command]
pub fn sync_disconnect(
    state: State<'_, Arc<SyncState>>,
    runtime: State<'_, Arc<VaultRuntime>>,
) -> Result<SyncStatusDto, CommandError> {
    runtime.touch();
    state.stop_realtime();
    runtime.with_vault_mut(|vault| {
        let empty = SyncSession {
            access_token: String::new(),
            refresh_token: String::new(),
            expires_at: 0,
        };
        vault.save_sync_session(&empty)?;
        vault.configure_sync(false, None, None, None)
    })?;
    Ok(SyncStatusDto {
        enabled: false,
        connected: false,
        supabase_url: None,
        user_id: None,
        cursor: None,
        last_sync_at: None,
        last_status: None,
        last_error: None,
    })
}

/// Runs one push + pull cycle now.
///
/// # Errors
///
/// Returns `sync_disabled`, `unauthorized`, `network`, `sync_busy` or `locked`.
#[tauri::command(async)]
pub fn sync_now(
    app: AppHandle,
    runtime: State<'_, Arc<VaultRuntime>>,
) -> Result<SyncRunDto, CommandError> {
    runtime.touch();
    run_once(&app, runtime.inner(), false)
}

/// Pulls every record again (after a missed delta) then applies it.
///
/// # Errors
///
/// Returns the same codes as [`sync_now`].
#[tauri::command(async)]
pub fn sync_resync(
    app: AppHandle,
    runtime: State<'_, Arc<VaultRuntime>>,
) -> Result<SyncRunDto, CommandError> {
    runtime.touch();
    run_once(&app, runtime.inner(), true)
}

fn run_once(
    app: &AppHandle,
    runtime: &Arc<VaultRuntime>,
    full: bool,
) -> Result<SyncRunDto, CommandError> {
    let run = perform_sync(runtime, full, true)?;
    let _ = app.emit(
        STATUS_EVENT,
        StatusEvent {
            status: "success",
            pushed: run.pushed,
            pulled: run.pulled,
            applied: run.applied,
            skipped: run.skipped,
            detail: None,
            at: run.at,
        },
    );
    Ok(run)
}

fn run_background(
    app: &AppHandle,
    runtime: &Arc<VaultRuntime>,
    context: &SyncRunContext,
    full: bool,
) {
    if context.running.swap(true, Ordering::SeqCst) {
        return;
    }
    let result = perform_sync(runtime, full, false);
    context.running.store(false, Ordering::SeqCst);
    let event = match result {
        Ok(run) => StatusEvent {
            status: "success",
            pushed: run.pushed,
            pulled: run.pulled,
            applied: run.applied,
            skipped: run.skipped,
            detail: None,
            at: run.at,
        },
        Err(error) if error.code == "locked" => return,
        Err(error) => StatusEvent {
            status: "error",
            pushed: 0,
            pulled: 0,
            applied: 0,
            skipped: 0,
            detail: Some(error.message),
            at: now_seconds(),
        },
    };
    let _ = app.emit(STATUS_EVENT, event);
}

fn perform_sync(
    runtime: &Arc<VaultRuntime>,
    full: bool,
    report_errors: bool,
) -> Result<SyncRunDto, CommandError> {
    let settings = runtime.with_vault(|vault| vault.sync_settings())?;
    if !settings.enabled {
        return Err(CommandError::new(
            "sync_disabled",
            "La synchronisation est désactivée.",
        ));
    }
    let (Some(base_url), Some(anon_key), Some(user_id)) = (
        settings.supabase_url.clone(),
        settings.anon_key.clone(),
        settings.user_id.clone(),
    ) else {
        return Err(CommandError::new(
            "sync_disabled",
            "Aucun projet Supabase n'est configuré.",
        ));
    };
    let mut session = ensure_session(runtime, &base_url, &anon_key, false)?;
    let envelope = runtime.with_vault(|vault| vault.sync_envelope())?;
    let records = runtime.with_vault(|vault| vault.sync_records())?;
    let cursor = if full { None } else { settings.cursor.clone() };

    let mut client = SupabaseClient::new(&base_url, &anon_key, &session.access_token);
    let mut outcome = execute(
        &client,
        runtime,
        &user_id,
        &envelope,
        &records,
        cursor.as_deref(),
        full,
    );
    if matches!(&outcome, Err(error) if error.code == "unauthorized") {
        // Refresh once, then retry: a stale access token is not a lost run.
        session = ensure_session(runtime, &base_url, &anon_key, true)?;
        client = SupabaseClient::new(&base_url, &anon_key, &session.access_token);
        outcome = execute(
            &client,
            runtime,
            &user_id,
            &envelope,
            &records,
            cursor.as_deref(),
            full,
        );
    }

    match outcome {
        Ok((run, next_cursor)) => {
            let _ = runtime.with_vault_mut(|vault| {
                vault.mark_sync(next_cursor.as_deref(), "success", None, run.at)
            });
            Ok(run)
        }
        Err(error) => {
            if report_errors {
                let _ = runtime.with_vault_mut(|vault| {
                    vault.mark_sync(None, "error", Some(&error.detail), now_seconds())
                });
            }
            Err(error.into())
        }
    }
}

fn execute(
    client: &SupabaseClient,
    runtime: &Arc<VaultRuntime>,
    user_id: &str,
    envelope: &akooti_core::SyncEnvelope,
    records: &[akooti_core::SyncRecord],
    cursor: Option<&str>,
    full: bool,
) -> Result<(SyncRunDto, Option<String>), crate::sync::SyncError> {
    client.upsert_envelope(user_id, envelope)?;
    let pushed = client.push(user_id, records)?;
    let mut cursor = cursor.map(str::to_owned);
    let mut pulled = 0;
    let mut applied = 0;
    let mut skipped = 0;
    loop {
        let batch = client.pull(cursor.as_deref())?;
        if batch.records.is_empty() {
            break;
        }
        let summary = runtime
            .with_vault_mut(|vault| vault.apply_sync_records(&batch.records))
            .map_err(|_| crate::sync::SyncError::server())?;
        pulled += summary.pulled;
        applied += summary.applied;
        skipped += summary.skipped;
        cursor = batch.cursor.clone();
        if batch.records.len() < 500 {
            break;
        }
    }
    Ok((
        SyncRunDto {
            pushed,
            pulled,
            applied,
            skipped,
            at: now_seconds(),
            full,
        },
        cursor,
    ))
}

fn ensure_session(
    runtime: &Arc<VaultRuntime>,
    base_url: &str,
    anon_key: &str,
    force_refresh: bool,
) -> Result<SyncSession, CommandError> {
    let session = runtime
        .with_vault(|vault| vault.sync_session())?
        .filter(|session| !session.refresh_token.is_empty())
        .ok_or_else(|| {
            CommandError::new(
                "unauthorized",
                "Connectez-vous à Supabase pour synchroniser.",
            )
        })?;
    if !force_refresh && session.expires_at > now_seconds() + 60 {
        return Ok(session);
    }
    let refreshed = SupabaseClient::refresh(base_url, anon_key, &session.refresh_token)?;
    runtime.with_vault_mut(|vault| vault.save_sync_session(&refreshed))?;
    Ok(refreshed)
}
