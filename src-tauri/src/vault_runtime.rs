use std::sync::{Mutex, PoisonError};
use std::time::SystemTime;

use akooti_core::{NewAuditEntry, Vault, VaultError, VaultPaths, VaultSession, VaultStatus};

use crate::error::CommandError;

/// Process-wide vault lifecycle shared by Tauri commands and the daemon.
#[derive(Debug)]
pub struct VaultRuntime {
    paths: VaultPaths,
    session: Mutex<VaultSession>,
}

impl VaultRuntime {
    /// Creates a locked runtime below the platform application data directory.
    pub fn new(paths: VaultPaths) -> Self {
        Self {
            session: Mutex::new(VaultSession::new(paths.clone(), 15)),
            paths,
        }
    }

    /// Reports whether a vault file already exists on disk.
    pub fn is_initialized(&self) -> bool {
        self.paths.header.is_file() && self.paths.database.is_file()
    }

    /// Runs a read-only vault operation under the runtime lock.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Locked`] when no session is open, and propagates
    /// the operation error otherwise.
    pub fn with_vault<T>(
        &self,
        operation: impl FnOnce(&Vault) -> Result<T, VaultError>,
    ) -> Result<T, VaultError> {
        let session = self.session();
        operation(session.vault()?)
    }

    /// Runs a mutating vault operation under the runtime lock.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Locked`] when no session is open, and propagates
    /// the operation error otherwise.
    pub fn with_vault_mut<T>(
        &self,
        operation: impl FnOnce(&mut Vault) -> Result<T, VaultError>,
    ) -> Result<T, VaultError> {
        let mut session = self.session();
        operation(session.vault_mut()?)
    }

    /// Runs a read-only operation whose own errors are command errors.
    ///
    /// Used by the import wizard, where a parse failure must keep its own
    /// stable code instead of collapsing into a vault error.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError::locked`] when no session is open.
    pub fn run<T>(
        &self,
        operation: impl FnOnce(&Vault) -> Result<T, CommandError>,
    ) -> Result<T, CommandError> {
        let session = self.session();
        let vault = session.vault().map_err(CommandError::from)?;
        operation(vault)
    }

    /// Runs a mutating operation whose own errors are command errors.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError::locked`] when no session is open.
    pub fn run_mut<T>(
        &self,
        operation: impl FnOnce(&mut Vault) -> Result<T, CommandError>,
    ) -> Result<T, CommandError> {
        let mut session = self.session();
        let vault = session.vault_mut().map_err(CommandError::from)?;
        operation(vault)
    }

    /// Appends one audit row to the unlocked vault.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Locked`] when no session is open.
    pub fn record_audit(&self, entry: NewAuditEntry) -> Result<(), VaultError> {
        self.with_vault(|vault| vault.record_audit(entry))
    }

    /// Creates the initial vault.
    ///
    /// # Errors
    ///
    /// Returns a vault error when files or cryptographic metadata cannot be created.
    pub fn create(&self, password: &str) -> Result<(), VaultError> {
        self.session().create(password)
    }

    /// Unlocks the existing vault.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::InvalidCredentials`] for a wrong password.
    pub fn unlock(&self, password: &str) -> Result<(), VaultError> {
        self.session().unlock(password)
    }

    /// Drops the connection and master key.
    pub fn lock(&self) {
        self.session().lock();
    }

    /// Returns the current vault state.
    pub fn status(&self) -> VaultStatus {
        self.session().status()
    }

    /// Records activity from a trusted local UI or daemon adapter.
    pub fn touch(&self) {
        let mut session = self.session();
        if !session.enforce_auto_lock(SystemTime::now())
            && session.status() == VaultStatus::Unlocked
        {
            session.touch();
        }
    }

    /// Locks when the inactivity deadline has elapsed.
    pub fn enforce_auto_lock(&self) {
        self.session().enforce_auto_lock(SystemTime::now());
    }

    /// Runs hourly vault maintenance and reports purge failures.
    pub fn run_maintenance(&self) {
        if let Err(error) = self.session().run_maintenance(SystemTime::now()) {
            tracing::error!(%error, "vault maintenance failed");
        }
    }

    /// Applies the conservative resume policy: lock on explicit resume, then
    /// enforce the inactivity deadline when focus returns.
    pub fn resume(&self) {
        self.lock();
    }

    /// Handles a focus-gain event without defeating an expired timeout.
    pub fn focus_gained(&self) {
        let mut session = self.session();
        if !session.enforce_auto_lock(SystemTime::now())
            && session.status() == VaultStatus::Unlocked
        {
            session.touch();
        }
    }

    fn session(&self) -> std::sync::MutexGuard<'_, VaultSession> {
        self.session.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
