//! Akooti core.
//!
//! Pure business logic of the vault: cryptography, SQLCipher persistence,
//! `Dev Local` routing, audit and importers.
//!
//! This crate must never depend on Tauri, WebView or React so it stays
//! unit-testable and portable (see `docs/regles.md` §2.1).

#![forbid(unsafe_code)]

mod crypto;
mod db;
mod error;
mod folders;
mod generator;
mod header;
mod importers;
mod items;
mod models;
mod routing;
mod session;
mod totp;
mod validation;
mod vault;
mod vault_history;
mod vault_items;
mod vault_maintenance;
mod vault_routing;
mod vault_totp;

pub use crypto::KdfParams;
pub use error::VaultError;
pub use generator::{
    GeneratedSecret, PassphraseOptions, PasswordOptions, generate_passphrase, generate_password,
};
pub use header::{VaultHeader, VaultPaths};
pub use importers::{
    DuplicateMatch, DuplicatePolicy, DuplicateReason, ImportBundle, ImportCleanup, ImportError,
    ImportFolder, ImportFormat, ImportIssue, ImportIssueKind, ImportItem, ImportOptions,
    ImportPreview, ImportReport, SecureDeleteReport,
};
pub use models::{
    AGENTS_MCP_FOLDER_ID, CustomField, CustomFieldInput, DEV_LOCAL_FOLDER_ID, Folder, FolderDraft,
    ItemFilter, ItemMetadataDraft, ItemType, ItemUri, ItemUriInput, PERSONAL_FOLDER_ID,
    RevealedSecret, SecretField, SecretString, Selectors, UriMatchType, VaultItem, VaultItemDraft,
    VaultStatus,
};
pub use routing::{NewRoutingRule, RoutingPatternType, RoutingRule};
pub use session::VaultSession;
pub use totp::{TotpAlgorithm, generate as generate_totp_code, verify as verify_totp_code};
pub use vault::Vault;

/// Current version of the core crate, as declared in `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
