//! `Dev Local` routing rule commands.

use tauri::State;

use crate::dto::{NewRoutingRuleInput, RoutingRuleDto};
use crate::error::CommandError;
use crate::vault_runtime::VaultRuntime;

/// Lists every routing rule, enabled or not.
#[tauri::command]
pub fn list_routing_rules(
    runtime: State<'_, VaultRuntime>,
) -> Result<Vec<RoutingRuleDto>, CommandError> {
    runtime.touch();
    let rules = runtime.with_vault(|vault| vault.routing_rules())?;
    Ok(rules.into_iter().map(Into::into).collect())
}

/// Adds a custom routing rule.
#[tauri::command]
pub fn add_routing_rule(
    runtime: State<'_, VaultRuntime>,
    rule: NewRoutingRuleInput,
) -> Result<RoutingRuleDto, CommandError> {
    let rule = rule
        .into_core()
        .map_err(|message| CommandError::new("invalid_input", message))?;
    runtime.touch();
    let created = runtime.with_vault_mut(|vault| vault.add_routing_rule(rule))?;
    Ok(created.into())
}

/// Enables or disables one routing rule.
#[tauri::command]
pub fn set_routing_rule_enabled(
    runtime: State<'_, VaultRuntime>,
    id: String,
    enabled: bool,
) -> Result<RoutingRuleDto, CommandError> {
    runtime.touch();
    let rule = runtime.with_vault_mut(|vault| vault.set_routing_rule_enabled(&id, enabled))?;
    Ok(rule.into())
}

/// Applies every enabled rule to every live item and returns the number of
/// added folder memberships.
#[tauri::command]
pub fn apply_routing_rules(runtime: State<'_, VaultRuntime>) -> Result<usize, CommandError> {
    runtime.touch();
    let added = runtime.with_vault_mut(akooti_core::Vault::apply_routing_rules)?;
    Ok(added)
}
