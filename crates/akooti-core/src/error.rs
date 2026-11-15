use std::io;

/// Errors returned by the vault core.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum VaultError {
    /// The vault is locked and the operation requires an unlocked connection.
    #[error("vault is locked")]
    Locked,
    /// The supplied password does not unlock this vault.
    #[error("invalid credentials")]
    InvalidCredentials,
    /// The vault header or encrypted database is malformed.
    #[error("vault data is corrupt")]
    CorruptVault,
    /// A system folder cannot be renamed, moved, or deleted.
    #[error("system folder is protected")]
    SystemFolderProtected,
    /// The requested record does not exist or is hidden by a filter.
    #[error("record not found")]
    NotFound,
    /// The request violates a domain invariant.
    #[error("invalid input: {0}")]
    InvalidInput(&'static str),
    /// Filesystem access failed.
    #[error("vault filesystem operation failed")]
    Io(#[source] io::Error),
    /// SQLite or SQLCipher rejected an operation.
    #[error("vault database operation failed")]
    Database(#[source] rusqlite::Error),
    /// Authenticated encryption failed.
    #[error("vault encryption operation failed")]
    Crypto,
}

impl From<io::Error> for VaultError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<rusqlite::Error> for VaultError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}
