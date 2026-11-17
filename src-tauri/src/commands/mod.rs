//! Tauri command surface exposed to the React UI.
//!
//! Every command returns [`crate::error::CommandError`] on failure and never
//! logs a secret. Mutating commands run through [`crate::VaultRuntime`], which
//! also refreshes the inactivity timer used by the 15-minute auto-lock.

pub mod clipboard;
pub mod folders;
pub mod generator;
pub mod import_export;
pub mod items;
pub mod mcp;
pub mod routing;
pub mod system;
pub mod vault;
