//! Hourly backup scheduler.
//!
//! One thread, one run at a time: the interval comes from each target and a
//! target that never ran is due immediately. While the vault is locked nothing
//! happens — the thread simply finds no due target.

use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use crate::backup::{BACKUP_EVENT, due, run_target};
use crate::sync::time::now_seconds;
use crate::vault_runtime::VaultRuntime;

const TICK: Duration = Duration::from_secs(60);

pub fn spawn(runtime: Arc<VaultRuntime>, app: AppHandle) {
    std::thread::spawn(move || {
        loop {
            for target in due(&runtime, now_seconds()) {
                if crate::backup::is_running() {
                    break;
                }
                let outcome = run_target(&runtime, &target.id);
                let _ = app.emit(
                    BACKUP_EVENT,
                    serde_json::json!({
                        "targetId": target.id,
                        "name": target.name,
                        "status": if outcome.is_ok() { "success" } else { "error" },
                    }),
                );
            }
            std::thread::sleep(TICK);
        }
    });
}
