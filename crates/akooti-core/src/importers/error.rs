use std::io;

use crate::error::VaultError;

/// Errors returned while parsing or applying an import.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ImportError {
    /// The source format is not supported or cannot be detected.
    #[error("unsupported import format")]
    UnsupportedFormat,
    /// An encrypted source requires a password.
    #[error("import password is required")]
    PasswordRequired,
    /// The supplied password did not authenticate the source.
    #[error("invalid import password")]
    InvalidPassword,
    /// Encrypted parameters are outside supported safety bounds.
    #[error("unsupported import encryption parameters")]
    UnsupportedEncryption,
    /// The encrypted source is structurally corrupt.
    #[error("import source is corrupt")]
    CorruptSource,
    /// The source folder graph cannot be resolved.
    #[error("import folder graph is invalid")]
    InvalidFolderGraph,
    /// The source path cannot be safely overwritten.
    #[error("import source path is invalid")]
    InvalidSource,
    /// The source has no importable content.
    #[error("import source is empty")]
    Empty,
    /// Filesystem access failed.
    #[error("import filesystem operation failed")]
    Io(#[source] io::Error),
    /// A ZIP archive could not be read.
    #[error("import archive operation failed")]
    Zip(#[source] zip::result::ZipError),
    /// JSON parsing failed.
    #[error("import JSON parsing failed")]
    Json(#[source] serde_json::Error),
    /// CSV parsing failed.
    #[error("import CSV parsing failed")]
    Csv(#[source] csv::Error),
    /// XML parsing failed.
    #[error("import XML parsing failed")]
    Xml(#[source] quick_xml::DeError),
    /// Vault validation or persistence failed.
    #[error(transparent)]
    Vault(#[from] VaultError),
}

impl From<io::Error> for ImportError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<zip::result::ZipError> for ImportError {
    fn from(error: zip::result::ZipError) -> Self {
        Self::Zip(error)
    }
}

impl From<serde_json::Error> for ImportError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<csv::Error> for ImportError {
    fn from(error: csv::Error) -> Self {
        Self::Csv(error)
    }
}

impl From<quick_xml::DeError> for ImportError {
    fn from(error: quick_xml::DeError) -> Self {
        Self::Xml(error)
    }
}

impl From<rusqlite::Error> for ImportError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Vault(VaultError::Database(error))
    }
}
