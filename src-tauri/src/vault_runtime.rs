use std::sync::{Mutex, PoisonError};
use std::time::SystemTime;

use akooti_core::{VaultError, VaultPaths, VaultSession, VaultStatus};

/// Process-wide vault lifecycle shared by future Tauri commands and the daemon.
pub struct VaultRuntime {
    session: Mutex<VaultSession>,
}

impl VaultRuntime {
    /// Creates a locked runtime below the platform application data directory.
    pub fn new(paths: VaultPaths) -> Self {
        Self {
            session: Mutex::new(VaultSession::new(paths, 15)),
        }
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
