use crate::error::VaultError;
use crate::sync::{self, SyncEnvelope, SyncRecord, SyncSession, SyncSettings, SyncSummary};
use crate::vault::Vault;

impl Vault {
    /// Local sync configuration, including the last cursor and status.
    ///
    /// # Errors
    ///
    /// Returns a database error when the configuration cannot be read.
    pub fn sync_settings(&self) -> Result<SyncSettings, VaultError> {
        sync::settings(self.connection())
    }

    /// Stores the sync endpoint and whether the scheduler may use it.
    ///
    /// # Errors
    ///
    /// Returns a database error when the configuration cannot be written.
    pub fn configure_sync(
        &mut self,
        enabled: bool,
        supabase_url: Option<&str>,
        anon_key: Option<&str>,
        user_id: Option<&str>,
    ) -> Result<SyncSettings, VaultError> {
        sync::configure(self.connection(), enabled, supabase_url, anon_key, user_id)?;
        self.sync_settings()
    }

    /// Returns the encrypted Supabase session, if the user signed in.
    ///
    /// # Errors
    ///
    /// Returns a decryption error for a corrupted session row.
    pub fn sync_session(&self) -> Result<Option<SyncSession>, VaultError> {
        sync::session(self.connection(), self.master_key())
    }

    /// Stores the Supabase tokens, encrypted with the master key.
    ///
    /// # Errors
    ///
    /// Returns a crypto or database error when the session cannot be written.
    pub fn save_sync_session(&mut self, session: &SyncSession) -> Result<(), VaultError> {
        sync::save_session(self.connection(), self.master_key(), session)
    }

    /// Records the outcome of a sync run and advances the delta cursor.
    ///
    /// # Errors
    ///
    /// Returns a database error when the status cannot be written.
    pub fn mark_sync(
        &mut self,
        cursor: Option<&str>,
        status: &str,
        error: Option<&str>,
        now: i64,
    ) -> Result<(), VaultError> {
        sync::mark(self.connection(), cursor, status, error, now)
    }

    /// Public cryptographic envelope pushed once per Supabase user.
    ///
    /// # Errors
    ///
    /// Returns a database error when the vault metadata cannot be read.
    pub fn sync_envelope(&self) -> Result<SyncEnvelope, VaultError> {
        sync::envelope(self.connection())
    }

    /// Serializes and encrypts every record to push.
    ///
    /// # Errors
    ///
    /// Returns a crypto error when a secret cannot be re-encrypted.
    pub fn sync_records(&self) -> Result<Vec<SyncRecord>, VaultError> {
        sync::records(self.connection(), self.master_key())
    }

    /// Applies remote records with last-writer-wins semantics.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::InvalidInput`] for a record that fails to decrypt,
    /// and a database error when the transaction cannot be committed.
    pub fn apply_sync_records(
        &mut self,
        records: &[SyncRecord],
    ) -> Result<SyncSummary, VaultError> {
        let master_key = *self.master_key();
        sync::apply_records(self.connection_mut(), &master_key, records)
    }
}
