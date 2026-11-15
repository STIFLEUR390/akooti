//! Akooti core.
//!
//! Pure business logic of the vault: cryptography, SQLCipher persistence,
//! `Dev Local` routing, audit and importers.
//!
//! This crate must never depend on Tauri, WebView or React so it stays
//! unit-testable and portable (see `docs/regles.md` §2.1).

#![forbid(unsafe_code)]

/// Current version of the core crate, as declared in `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
