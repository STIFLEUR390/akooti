//! Optional biometric confirmation.
//!
//! Platform support of `tauri-plugin-biometric`, taken from the official
//! plugin documentation (verified through Context7):
//!
//! | Platform | Supported |
//! | -------- | --------- |
//! | macOS    | yes       |
//! | Android  | yes       |
//! | iOS      | yes       |
//! | Linux    | no        |
//! | Windows  | no        |
//!
//! The plugin is therefore not wired yet: it needs an Apple entitlement, a
//! signed bundle and a JS guest binding, none of which phase 4 ships. Linux
//! would need `fprintd` over D-Bus and Windows would need Windows Hello through
//! the WinRT `UserConsentVerifier` API (see ADR 0004).
//!
//! Until then [`status`] reports an unavailable provider and the UI hides the
//! biometric control instead of pretending to offer it.

use serde::Serialize;

/// Which OS provider backs the check, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BiometricProvider {
    /// Apple Touch ID / Local Authentication.
    TouchId,
    /// Windows Hello.
    WindowsHello,
    /// Linux `fprintd`.
    Fprintd,
}

/// Availability reported to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BiometricsStatus {
    /// Whether the UI may offer a biometric control.
    pub available: bool,
    /// Backing provider.
    pub provider: BiometricProvider,
    /// Stable, non-revealing explanation shown as help text.
    pub reason: &'static str,
}

impl BiometricsStatus {
    /// Builds an unavailable status with a platform specific explanation.
    #[must_use]
    pub const fn unavailable(provider: BiometricProvider, reason: &'static str) -> Self {
        Self {
            available: false,
            provider,
            reason,
        }
    }
}

/// Returns the biometric capability of the current platform.
#[must_use]
pub fn status() -> BiometricsStatus {
    if cfg!(target_os = "macos") {
        BiometricsStatus::unavailable(
            BiometricProvider::TouchId,
            "Touch ID sera proposé une fois l'application signée (phase 9).",
        )
    } else if cfg!(windows) {
        BiometricsStatus::unavailable(
            BiometricProvider::WindowsHello,
            "Windows Hello sera branché sur UserConsentVerifier (voir ADR 0004).",
        )
    } else {
        BiometricsStatus::unavailable(
            BiometricProvider::Fprintd,
            "Aucun lecteur d'empreintes branché : utilisez le mot de passe maître (voir ADR 0004).",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{BiometricProvider, status};

    #[test]
    fn no_platform_claims_a_working_provider_yet() {
        let reported = status();
        assert!(!reported.available);
        assert!(!reported.reason.is_empty());
    }

    #[test]
    fn the_planned_provider_matches_the_platform() {
        let expected = if cfg!(target_os = "macos") {
            BiometricProvider::TouchId
        } else if cfg!(windows) {
            BiometricProvider::WindowsHello
        } else {
            BiometricProvider::Fprintd
        };
        assert_eq!(status().provider, expected);
    }
}
