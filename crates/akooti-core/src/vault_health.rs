use std::collections::HashSet;

use crate::error::VaultError;
use crate::health::{self, BreachTarget, HealthReport};
use crate::vault::Vault;

impl Vault {
    /// Scores every stored password locally (zxcvbn, reuse, age) and persists
    /// the strength score and SHA-1 prefix used by the breach check.
    ///
    /// `on_progress(done, total)` is called after each item so the caller can
    /// report progress without holding a copy of the report.
    ///
    /// # Errors
    ///
    /// Returns a database or crypto error when an item cannot be decrypted or
    /// its score cannot be persisted.
    pub fn audit_health(
        &self,
        now: i64,
        on_progress: impl FnMut(usize, usize),
    ) -> Result<HealthReport, VaultError> {
        let mut on_progress = on_progress;
        health::audit(self.connection(), self.master_key(), now, &mut on_progress)
    }

    /// Distinct SHA-1 prefixes to check against HIBP, with their item counts.
    ///
    /// # Errors
    ///
    /// Returns a database error when the prefixes cannot be read.
    pub fn breach_targets(&self) -> Result<Vec<BreachTarget>, VaultError> {
        health::breach_targets(self.connection())
    }

    /// Applies one HIBP range response: only the suffixes are compared, locally.
    ///
    /// # Errors
    ///
    /// Returns a database or crypto error when an item cannot be decrypted or
    /// its flag cannot be persisted.
    pub fn apply_breach_range(
        &self,
        prefix: &str,
        suffixes: &HashSet<String>,
    ) -> Result<usize, VaultError> {
        health::apply_breach_range(self.connection(), self.master_key(), prefix, suffixes)
    }
}
