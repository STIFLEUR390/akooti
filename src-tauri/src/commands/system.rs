//! Process-level commands: lock, theme-agnostic system information.

use serde::Serialize;
use tauri::State;

use crate::biometrics::{self, BiometricsStatus};
use crate::vault_runtime::VaultRuntime;

/// Locks the vault immediately and drops the master key from RAM.
#[tauri::command]
pub fn lock_vault(runtime: State<'_, VaultRuntime>) {
    runtime.lock();
}

/// Reports whether a biometric unlock control may be offered.
#[tauri::command]
pub fn biometrics_status() -> BiometricsStatus {
    biometrics::status()
}

/// Build information displayed in the footer.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildInfoDto {
    /// Core crate version.
    pub core_version: &'static str,
    /// Application version from `tauri.conf.json`.
    pub app_version: &'static str,
    /// Target triple family, e.g. `linux`.
    pub platform: &'static str,
}

/// Returns build information.
#[tauri::command]
pub fn build_info() -> BuildInfoDto {
    BuildInfoDto {
        core_version: akooti_core::VERSION,
        app_version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
    }
}
