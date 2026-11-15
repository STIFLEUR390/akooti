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
mod header;
mod items;
mod models;
mod session;
mod vault;

pub use crypto::KdfParams;
pub use error::VaultError;
pub use header::{VaultHeader, VaultPaths};
pub use models::{
    AGENTS_MCP_FOLDER_ID, CustomField, CustomFieldInput, DEV_LOCAL_FOLDER_ID, Folder, FolderDraft,
    ItemFilter, ItemType, ItemUri, ItemUriInput, PERSONAL_FOLDER_ID, RevealedSecret, SecretField,
    SecretString, Selectors, UriMatchType, VaultItem, VaultItemDraft, VaultStatus,
};
pub use session::VaultSession;
pub use vault::Vault;

/// Current version of the core crate, as declared in `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
