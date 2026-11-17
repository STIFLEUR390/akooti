//! Password and passphrase generation commands.
//!
//! The generated value is returned once to the UI so it can be shown and
//! copied; it is never persisted and never logged.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

use crate::error::CommandError;
use crate::vault_runtime::VaultRuntime;

/// Password generation options coming from the generator modal.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordOptionsInput {
    #[serde(default = "default_length")]
    pub length: usize,
    #[serde(default = "default_true")]
    pub lowercase: bool,
    #[serde(default = "default_true")]
    pub uppercase: bool,
    #[serde(default = "default_true")]
    pub digits: bool,
    #[serde(default = "default_true")]
    pub symbols: bool,
    #[serde(default = "default_true")]
    pub exclude_ambiguous: bool,
}

impl Default for PasswordOptionsInput {
    fn default() -> Self {
        Self {
            length: default_length(),
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: true,
            exclude_ambiguous: true,
        }
    }
}

/// Passphrase generation options coming from the generator modal.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PassphraseOptionsInput {
    #[serde(default = "default_words")]
    pub words: usize,
    #[serde(default = "default_separator")]
    pub separator: String,
    #[serde(default = "default_true")]
    pub capitalize: bool,
}

impl Default for PassphraseOptionsInput {
    fn default() -> Self {
        Self {
            words: default_words(),
            separator: default_separator(),
            capitalize: true,
        }
    }
}

/// Generated secret returned to the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedSecretDto {
    /// Plaintext value to display and copy.
    pub value: String,
    /// Estimated entropy in bits.
    pub entropy_bits: f64,
}

/// Generates a random password.
///
/// # Errors
///
/// Returns `invalid_input` for an out-of-range length or no character class.
#[tauri::command]
pub fn generate_password(
    _runtime: State<'_, Arc<VaultRuntime>>,
    options: Option<PasswordOptionsInput>,
) -> Result<GeneratedSecretDto, CommandError> {
    let options = options.unwrap_or_default();
    let generated = akooti_core::generate_password(&akooti_core::PasswordOptions {
        length: options.length,
        lowercase: options.lowercase,
        uppercase: options.uppercase,
        digits: options.digits,
        symbols: options.symbols,
        exclude_ambiguous: options.exclude_ambiguous,
    })?;
    Ok(GeneratedSecretDto {
        value: generated.value.expose().to_owned(),
        entropy_bits: generated.entropy_bits,
    })
}

/// Generates a Diceware passphrase.
///
/// # Errors
///
/// Returns `invalid_input` for an out-of-range word count.
#[tauri::command]
pub fn generate_passphrase(
    _runtime: State<'_, Arc<VaultRuntime>>,
    options: Option<PassphraseOptionsInput>,
) -> Result<GeneratedSecretDto, CommandError> {
    let options = options.unwrap_or_default();
    let generated = akooti_core::generate_passphrase(&akooti_core::PassphraseOptions {
        words: options.words,
        separator: options.separator,
        capitalize: options.capitalize,
    })?;
    Ok(GeneratedSecretDto {
        value: generated.value.expose().to_owned(),
        entropy_bits: generated.entropy_bits,
    })
}

const fn default_length() -> usize {
    24
}

const fn default_words() -> usize {
    6
}

const fn default_true() -> bool {
    true
}

fn default_separator() -> String {
    "-".to_owned()
}
