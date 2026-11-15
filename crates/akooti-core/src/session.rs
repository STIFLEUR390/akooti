use std::time::{Duration, SystemTime};

use crate::error::VaultError;
use crate::header::VaultPaths;
use crate::models::VaultStatus;
use crate::vault::Vault;

/// Owns the unlocked vault and enforces inactivity locking.
#[derive(Debug)]
pub struct VaultSession {
    paths: VaultPaths,
    vault: Option<Vault>,
    last_activity: SystemTime,
    auto_lock_minutes: u32,
}

impl VaultSession {
    /// Creates a locked session for the supplied paths.
    pub fn new(paths: VaultPaths, auto_lock_minutes: u32) -> Self {
        Self {
            paths,
            vault: None,
            last_activity: SystemTime::now(),
            auto_lock_minutes,
        }
    }

    /// Creates a new vault and leaves the session unlocked.
    pub fn create(&mut self, password: &str) -> Result<(), VaultError> {
        let vault = Vault::create(&self.paths, password)?;
        self.auto_lock_minutes = vault.auto_lock_minutes();
        self.vault = Some(vault);
        self.touch();
        Ok(())
    }

    /// Unlocks the existing vault and resets inactivity.
    pub fn unlock(&mut self, password: &str) -> Result<(), VaultError> {
        let vault = Vault::open(&self.paths, password)?;
        self.auto_lock_minutes = vault.auto_lock_minutes();
        self.vault = Some(vault);
        self.touch();
        Ok(())
    }

    /// Drops the database connection and zeroizes the master key.
    pub fn lock(&mut self) {
        self.vault = None;
    }

    /// Returns the current lock state.
    pub fn status(&self) -> VaultStatus {
        match self.vault {
            Some(_) => VaultStatus::Unlocked,
            None => VaultStatus::Locked,
        }
    }

    /// Borrows the vault or returns [`VaultError::Locked`].
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Locked`] when no unlocked vault is present.
    pub fn vault(&self) -> Result<&Vault, VaultError> {
        self.vault.as_ref().ok_or(VaultError::Locked)
    }

    /// Mutably borrows the vault or returns [`VaultError::Locked`].
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Locked`] when no unlocked vault is present.
    pub fn vault_mut(&mut self) -> Result<&mut Vault, VaultError> {
        self.vault.as_mut().ok_or(VaultError::Locked)
    }

    /// Records user or daemon activity.
    pub fn touch(&mut self) {
        self.last_activity = SystemTime::now();
    }

    /// Updates the timeout and optionally disables it with `0`.
    ///
    /// # Errors
    ///
    /// Returns a database error when an unlocked vault cannot persist the value.
    pub fn set_auto_lock_minutes(&mut self, minutes: u32) -> Result<(), VaultError> {
        if let Some(vault) = self.vault.as_mut() {
            vault.set_auto_lock_minutes(minutes)?;
        }
        self.auto_lock_minutes = minutes;
        Ok(())
    }

    /// Locks the vault when the injected clock exceeds the configured timeout.
    pub fn enforce_auto_lock(&mut self, now: SystemTime) -> bool {
        if self.vault.is_none() || self.auto_lock_minutes == 0 {
            return false;
        }
        let timeout = Duration::from_secs(u64::from(self.auto_lock_minutes).saturating_mul(60));
        let expired = now
            .duration_since(self.last_activity)
            .is_ok_and(|elapsed| elapsed >= timeout);
        if expired {
            self.lock();
        }
        expired
    }
}
