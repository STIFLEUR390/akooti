use crate::error::VaultError;
use crate::items;
use crate::models::{CustomFieldInput, ItemMetadataDraft, SecretField, SecretString, VaultItem};
use crate::vault::{Vault, unix_seconds};
use std::time::SystemTime;
use zeroize::Zeroizing;

impl Vault {
    /// Applies a metadata-only edit, leaving every stored secret untouched.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid selectors or URIs, unknown folders, or an
    /// unknown item.
    pub fn update_item_metadata(
        &mut self,
        id: &str,
        draft: ItemMetadataDraft,
    ) -> Result<VaultItem, VaultError> {
        items::update_metadata(
            self.connection(),
            id,
            &draft,
            unix_seconds(SystemTime::now())?,
        )?;
        self.get_item(id)
    }

    /// Writes or clears one encrypted field without revealing the previous one.
    ///
    /// # Errors
    ///
    /// Returns an error for unknown items, custom field targets, or encryption
    /// failures.
    pub fn set_item_secret(
        &mut self,
        id: &str,
        field: SecretField,
        value: Option<SecretString>,
    ) -> Result<(), VaultError> {
        let master_key = Zeroizing::new(*self.master_key());
        items::set_secret(
            self.connection_mut(),
            &master_key,
            id,
            field,
            value.as_ref(),
            unix_seconds(SystemTime::now())?,
        )
    }

    /// Adds one custom field and returns its identifier.
    ///
    /// # Errors
    ///
    /// Returns an error for unknown items or invalid field definitions.
    pub fn add_item_custom_field(
        &mut self,
        item_id: &str,
        field: CustomFieldInput,
    ) -> Result<String, VaultError> {
        items::add_custom_field(self.connection(), self.master_key(), item_id, &field)
    }

    /// Removes one custom field from an item.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::NotFound`] when the field does not belong to the item.
    pub fn remove_item_custom_field(
        &mut self,
        item_id: &str,
        field_id: &str,
    ) -> Result<(), VaultError> {
        items::remove_custom_field(self.connection(), item_id, field_id)
    }
}
