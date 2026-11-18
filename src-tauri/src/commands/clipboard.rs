//! Secure clipboard writes.
//!
//! Copying a secret must never leave it on the system clipboard: the value is
//! written, then cleared after [`CLIPBOARD_TTL_SECONDS`] **only if the
//! clipboard still holds exactly that value**. A newer copy — or anything the
//! user pasted elsewhere in the meantime — is never overwritten, so the purge
//! cannot destroy unrelated user data.
//!
//! The value travels from the UI to this command, is written once, and is
//! dropped. It is never logged, and the receipt returned to the UI carries the
//! countdown only.

use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::error::CommandError;

/// Seconds after which a copied secret is cleared from the clipboard.
pub const CLIPBOARD_TTL_SECONDS: u64 = 30;

/// Tracks the most recent copy so a stale purge cannot clear a newer value.
#[derive(Debug, Default)]
pub struct ClipboardGuard {
    generation: Mutex<u64>,
}

impl ClipboardGuard {
    /// Registers a new copy and returns its generation.
    fn next_generation(&self) -> u64 {
        let mut generation = self
            .generation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *generation = generation.wrapping_add(1);
        *generation
    }

    /// True when `generation` is still the latest copy.
    fn is_current(&self, generation: u64) -> bool {
        let current = self
            .generation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *current == generation
    }
}

/// Receipt returned to the UI so it can show the countdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardReceiptDto {
    /// Seconds until the clipboard is cleared.
    pub ttl_seconds: u64,
}

/// Copies a value and schedules its removal.
///
/// # Errors
///
/// Returns `clipboard` when the platform clipboard cannot be written.
#[tauri::command]
pub fn copy_secret(
    app: AppHandle,
    guard: State<'_, ClipboardGuard>,
    value: String,
) -> Result<ClipboardReceiptDto, CommandError> {
    if value.is_empty() {
        return Err(CommandError::new(
            "invalid_input",
            "Il n'y a rien à copier.",
        ));
    }
    app.clipboard()
        .write_text(value.clone())
        .map_err(|_| CommandError::new("clipboard", "Le presse-papiers est indisponible."))?;

    let generation = guard.next_generation();
    let handle = app.clone();
    // A plain thread keeps the purge independent from the webview event loop:
    // locking or closing the window must not cancel the cleanup.
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(CLIPBOARD_TTL_SECONDS));
        let guard = handle.state::<ClipboardGuard>();
        if !guard.is_current(generation) {
            return;
        }
        // Only clear when the clipboard still contains our value.
        if handle
            .clipboard()
            .read_text()
            .is_ok_and(|text| text == value)
        {
            let _ = handle.clipboard().clear();
        }
    });

    Ok(ClipboardReceiptDto {
        ttl_seconds: CLIPBOARD_TTL_SECONDS,
    })
}

#[cfg(test)]
mod tests {
    use super::ClipboardGuard;

    #[test]
    fn only_the_latest_copy_is_eligible_for_purging() {
        let guard = ClipboardGuard::default();
        let first = guard.next_generation();
        assert!(guard.is_current(first));

        let second = guard.next_generation();
        assert_ne!(first, second);
        assert!(!guard.is_current(first));
        assert!(guard.is_current(second));
    }
}

/// Copies non-secret text (setup script, project URL) without a purge timer.
///
/// # Errors
///
/// Returns `clipboard` when the platform clipboard cannot be written.
#[tauri::command]
pub fn copy_text(app: AppHandle, value: String) -> Result<(), CommandError> {
    if value.is_empty() {
        return Err(CommandError::new(
            "invalid_input",
            "Il n'y a rien à copier.",
        ));
    }
    app.clipboard()
        .write_text(value)
        .map_err(|_| CommandError::new("clipboard", "Le presse-papiers est indisponible."))
}
