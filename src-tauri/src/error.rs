//! Error envelope shared by every Tauri command.
//!
//! Two rules (see `docs/regles.md` §1.1):
//!
//! * the message never contains a plaintext secret, a token, or a bound
//!   value — only the static description of the error variant;
//! * the `code` is stable so the UI can branch without parsing prose.

use akooti_core::{ImportError, VaultError};
use serde::Serialize;

/// Serializable command error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    /// Stable machine-readable code.
    pub code: String,
    /// Human-readable message, safe to display.
    pub message: String,
}

impl CommandError {
    /// Builds an error from a code and message.
    #[must_use]
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
        }
    }

    /// Internal failure that must not leak implementation details.
    #[must_use]
    pub fn internal() -> Self {
        Self::new("internal", "Une erreur interne est survenue.")
    }
}

/// Stable code for a vault error, without copying any bound value.
const fn vault_code(error: &VaultError) -> &'static str {
    match error {
        VaultError::Locked => "locked",
        VaultError::InvalidCredentials => "invalid_credentials",
        VaultError::CorruptVault => "corrupt_vault",
        VaultError::SystemFolderProtected => "system_folder_protected",
        VaultError::NotFound => "not_found",
        VaultError::Forbidden => "forbidden",
        VaultError::InvalidInput(_) => "invalid_input",
        VaultError::Io(_) => "io",
        VaultError::Database(_) => "database",
        VaultError::Crypto => "crypto",
        _ => "vault",
    }
}

impl From<VaultError> for CommandError {
    fn from(error: VaultError) -> Self {
        Self::new(vault_code(&error), error.to_string())
    }
}

impl From<ImportError> for CommandError {
    fn from(error: ImportError) -> Self {
        let code = match &error {
            ImportError::UnsupportedFormat => "unsupported_format",
            ImportError::PasswordRequired => "password_required",
            ImportError::InvalidPassword => "invalid_password",
            ImportError::UnsupportedEncryption => "unsupported_encryption",
            ImportError::CorruptSource => "corrupt_source",
            ImportError::InvalidFolderGraph => "invalid_folder_graph",
            ImportError::InvalidSource => "invalid_source",
            ImportError::Empty => "empty_source",
            ImportError::Io(_) => "io",
            ImportError::Zip(_) => "archive",
            ImportError::Json(_) => "json",
            ImportError::Csv(_) => "csv",
            ImportError::Xml(_) => "xml",
            ImportError::Vault(inner) => vault_code(inner),
            _ => "import",
        };
        Self::new(code, error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::CommandError;
    use akooti_core::{ImportError, VaultError};

    #[test]
    fn vault_errors_map_to_stable_codes() {
        assert_eq!(CommandError::from(VaultError::Locked).code, "locked");
        assert_eq!(
            CommandError::from(VaultError::InvalidCredentials).code,
            "invalid_credentials"
        );
        assert_eq!(CommandError::from(VaultError::NotFound).code, "not_found");
        assert_eq!(
            CommandError::from(VaultError::SystemFolderProtected).code,
            "system_folder_protected"
        );
    }

    #[test]
    fn import_errors_map_to_stable_codes() {
        assert_eq!(
            CommandError::from(ImportError::PasswordRequired).code,
            "password_required"
        );
        assert_eq!(
            CommandError::from(ImportError::InvalidPassword).code,
            "invalid_password"
        );
        assert_eq!(
            CommandError::from(ImportError::Vault(VaultError::Locked)).code,
            "locked"
        );
    }

    #[test]
    fn generic_errors_never_echo_internal_details() {
        assert_eq!(CommandError::internal().code, "internal");
        assert!(!CommandError::internal().message.is_empty());
    }
}
