use crate::error::VaultError;
use crate::routing::{self, NewRoutingRule, RoutingRule};
use crate::vault::{Vault, unix_seconds};
use std::time::SystemTime;

impl Vault {
    /// Returns all automatic routing rules.
    ///
    /// # Errors
    ///
    /// Returns a database error when rules cannot be loaded.
    pub fn routing_rules(&self) -> Result<Vec<RoutingRule>, VaultError> {
        routing::list(self.connection(), false)
    }

    /// Adds a custom automatic routing rule.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid patterns, missing folders, or database
    /// failures.
    pub fn add_routing_rule(&mut self, input: NewRoutingRule) -> Result<RoutingRule, VaultError> {
        routing::insert(self.connection(), input, unix_seconds(SystemTime::now())?)
    }

    /// Enables or disables one routing rule.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::NotFound`] for an unknown rule.
    pub fn set_routing_rule_enabled(
        &mut self,
        id: &str,
        enabled: bool,
    ) -> Result<RoutingRule, VaultError> {
        routing::set_enabled(self.connection(), id, enabled)
    }

    /// Adds matching local-routing folders to one item without removing folders.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed rules or database failures.
    pub fn route_item(&mut self, item_id: &str) -> Result<usize, VaultError> {
        routing::apply_to_item(self.connection(), item_id, unix_seconds(SystemTime::now())?)
    }

    /// Applies enabled routing rules to every live item.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed rules or database failures.
    pub fn apply_routing_rules(&mut self) -> Result<usize, VaultError> {
        routing::apply_retroactively(self.connection(), unix_seconds(SystemTime::now())?)
    }
}
