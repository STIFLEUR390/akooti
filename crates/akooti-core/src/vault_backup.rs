use crate::backup::{BackupConfig, BackupRun, BackupTarget, BackupTargetDraft, NewBackupRun};
use crate::error::VaultError;
use crate::vault::Vault;

impl Vault {
    /// Lists configured backup targets, without provider credentials.
    ///
    /// # Errors
    ///
    /// Returns a database error or a decryption error for a corrupted target.
    pub fn backup_targets(&self) -> Result<Vec<BackupTarget>, VaultError> {
        crate::backup::list_targets(self.connection(), self.master_key())
    }

    /// Fetches one target, without provider credentials.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::NotFound`] for an unknown target.
    pub fn backup_target(&self, id: &str) -> Result<BackupTarget, VaultError> {
        crate::backup::get(self.connection(), self.master_key(), id)
    }

    /// Creates a target and encrypts its configuration.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::InvalidInput`] when the configuration is incomplete.
    pub fn create_backup_target(
        &mut self,
        draft: BackupTargetDraft,
    ) -> Result<BackupTarget, VaultError> {
        crate::backup::create(
            self.connection(),
            self.master_key(),
            &draft,
            crate::vault::unix_seconds(std::time::SystemTime::now())?,
        )
    }

    /// Replaces a target's configuration and scheduling.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::NotFound`] for an unknown target.
    pub fn update_backup_target(
        &mut self,
        id: &str,
        draft: BackupTargetDraft,
    ) -> Result<BackupTarget, VaultError> {
        crate::backup::update(self.connection(), self.master_key(), id, &draft)
    }

    /// Deletes a target and its run history.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::NotFound`] for an unknown target.
    pub fn delete_backup_target(&mut self, id: &str) -> Result<(), VaultError> {
        crate::backup::delete(self.connection(), id)
    }

    /// Enables or disables one target.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::NotFound`] for an unknown target.
    pub fn set_backup_target_enabled(
        &mut self,
        id: &str,
        enabled: bool,
    ) -> Result<BackupTarget, VaultError> {
        crate::backup::set_enabled(self.connection(), self.master_key(), id, enabled)
    }

    /// Returns the decrypted provider configuration, for the shell only.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::NotFound`] for an unknown target.
    pub fn backup_config(&self, id: &str) -> Result<BackupConfig, VaultError> {
        crate::backup::read_config(self.connection(), self.master_key(), id)
    }

    /// Targets whose next run is due.
    ///
    /// # Errors
    ///
    /// Returns a database error when the targets cannot be listed.
    pub fn due_backup_targets(&self, now: i64) -> Result<Vec<BackupTarget>, VaultError> {
        crate::backup::due_targets(self.connection(), self.master_key(), now)
    }

    /// Records one backup attempt and refreshes the target's last status.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::NotFound`] for an unknown target.
    pub fn record_backup_run(
        &mut self,
        target_id: &str,
        run: NewBackupRun,
        now: i64,
    ) -> Result<BackupRun, VaultError> {
        crate::backup::record_run(self.connection(), target_id, &run, now)
    }

    /// Lists the run history, newest first.
    ///
    /// # Errors
    ///
    /// Returns a database error when the history cannot be read.
    pub fn backup_runs(
        &self,
        target_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<BackupRun>, VaultError> {
        crate::backup::list_runs(self.connection(), target_id, limit)
    }
}
