use crate::error::VaultError;
use crate::items;
use crate::vault::Vault;

impl Vault {
    /// Permanently removes items whose TTL has elapsed.
    ///
    /// # Errors
    ///
    /// Returns a database error when expired items cannot be deleted.
    pub fn purge_expired(&mut self, now: i64) -> Result<usize, VaultError> {
        items::purge_expired(self.connection(), now)
    }
}
