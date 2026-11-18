//! Tauri command surface exposed to the React UI.
//!
//! Every command returns [`crate::error::CommandError`] on failure and never
//! logs a secret. Mutating commands run through [`crate::VaultRuntime`], which
//! also refreshes the inactivity timer used by the 15-minute auto-lock.

pub mod backup;
pub mod clipboard;
pub mod export;
pub mod folders;
pub mod generator;
pub mod health;
pub mod import_export;
pub mod items;
pub mod mcp;
pub mod routing;
pub mod sync;
pub mod system;
pub mod vault;
