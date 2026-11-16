use crate::error::VaultError;
use crate::items;
use crate::models::{SecretField, SecretString};
use crate::totp::{self, TotpAlgorithm};
use crate::vault::Vault;

impl Vault {
    /// Generates a TOTP code from the item's encrypted TOTP secret.
    ///
    /// # Errors
    ///
    /// Returns an error when the item has no TOTP secret or the secret is
    /// malformed.
    pub fn generate_totp(
        &self,
        item_id: &str,
        algorithm: TotpAlgorithm,
        timestamp: u64,
    ) -> Result<SecretString, VaultError> {
        let secret = items::reveal(
            self.connection(),
            self.master_key(),
            item_id,
            SecretField::Totp,
            None,
        )?;
        totp::generate(&secret.value, algorithm, timestamp)
    }

    /// Verifies a TOTP code against the item's encrypted TOTP secret.
    ///
    /// # Errors
    ///
    /// Returns an error when the item has no TOTP secret or the secret is
    /// malformed.
    pub fn verify_totp(
        &self,
        item_id: &str,
        algorithm: TotpAlgorithm,
        token: &str,
        timestamp: u64,
    ) -> Result<bool, VaultError> {
        let secret = items::reveal(
            self.connection(),
            self.master_key(),
            item_id,
            SecretField::Totp,
            None,
        )?;
        totp::verify(&secret.value, algorithm, token, timestamp)
    }
}
