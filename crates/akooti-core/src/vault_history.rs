use crate::error::VaultError;
use crate::items;
use crate::vault::Vault;

impl Vault {
    /// Returns the number of encrypted historical passwords for one item.
    ///
    /// # Errors
    ///
    /// Returns a database error when the count cannot be read.
    pub fn password_history_count(&self, item_id: &str) -> Result<usize, VaultError> {
        items::password_history_count(self.connection(), item_id)
    }
}
