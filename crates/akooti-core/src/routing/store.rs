use rusqlite::{Connection, params};
use uuid::Uuid;

use crate::error::VaultError;
use crate::routing::rules::{
    NewRoutingRule, RoutingPatternType, RoutingRule, apply_candidates, rule_from_row,
};

pub(crate) fn list(
    connection: &Connection,
    enabled_only: bool,
) -> Result<Vec<RoutingRule>, VaultError> {
    let sql = if enabled_only {
        "SELECT id, pattern, pattern_type, target_folder_id, is_default, enabled, created_at
         FROM local_routing_rules WHERE enabled = 1 ORDER BY is_default DESC, created_at, id"
    } else {
        "SELECT id, pattern, pattern_type, target_folder_id, is_default, enabled, created_at
         FROM local_routing_rules ORDER BY is_default DESC, created_at, id"
    };
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map([], rule_from_row)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(VaultError::from)
}

pub(crate) fn set_enabled(
    connection: &Connection,
    id: &str,
    enabled: bool,
) -> Result<RoutingRule, VaultError> {
    let changed = connection.execute(
        "UPDATE local_routing_rules SET enabled = ?2 WHERE id = ?1",
        params![id, enabled],
    )?;
    if changed == 0 {
        return Err(VaultError::NotFound);
    }
    list(connection, false)?
        .into_iter()
        .find(|rule| rule.id == id)
        .ok_or(VaultError::CorruptVault)
}

pub(crate) fn apply_retroactively(connection: &Connection, now: i64) -> Result<usize, VaultError> {
    let mut statement = connection
        .prepare("SELECT id, primary_url FROM vault_items WHERE deleted_at IS NULL ORDER BY id")?;
    let items = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut added = 0;
    let mut uri_statement = connection.prepare("SELECT uri FROM item_uris WHERE item_id = ?1")?;
    for (item_id, primary_url) in items {
        let mut candidates = primary_url.into_iter().collect::<Vec<_>>();
        candidates.extend(
            uri_statement
                .query_map([&item_id], |row| row.get(0))?
                .collect::<Result<Vec<String>, _>>()?,
        );
        added += apply_candidates(connection, &item_id, &candidates, now)?;
    }
    Ok(added)
}

pub(crate) fn insert(
    connection: &Connection,
    input: NewRoutingRule,
    now: i64,
) -> Result<RoutingRule, VaultError> {
    validate(&input)?;
    crate::folders::get(connection, &input.target_folder_id)?;
    let id = Uuid::new_v4().to_string();
    connection.execute(
        "INSERT INTO local_routing_rules (
           id, pattern, pattern_type, target_folder_id, is_default, enabled, created_at
         ) VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)",
        params![
            id,
            input.pattern,
            pattern_type_sql(input.pattern_type),
            input.target_folder_id,
            input.enabled,
            now,
        ],
    )?;
    list(connection, false)?
        .into_iter()
        .find(|rule| rule.id == id)
        .ok_or(VaultError::CorruptVault)
}

fn validate(input: &NewRoutingRule) -> Result<(), VaultError> {
    if input.pattern.is_empty() || input.pattern.chars().count() > 1_024 {
        return Err(VaultError::InvalidInput("routing pattern is invalid"));
    }
    match input.pattern_type {
        RoutingPatternType::Regex => regex::Regex::new(&input.pattern)
            .map(|_| ())
            .map_err(|_| VaultError::InvalidInput("routing regex is invalid")),
        RoutingPatternType::Cidr => {
            let (network, prefix) = input
                .pattern
                .split_once('/')
                .ok_or(VaultError::InvalidInput("CIDR routing rule has no prefix"))?;
            let network = network
                .parse::<std::net::IpAddr>()
                .map_err(|_| VaultError::InvalidInput("CIDR routing network is invalid"))?;
            let prefix = prefix
                .parse::<u8>()
                .map_err(|_| VaultError::InvalidInput("CIDR prefix is invalid"))?;
            let width = match network {
                std::net::IpAddr::V4(_) => 32,
                std::net::IpAddr::V6(_) => 128,
            };
            if prefix > width {
                return Err(VaultError::InvalidInput("CIDR prefix is invalid"));
            }
            Ok(())
        }
        RoutingPatternType::Suffix | RoutingPatternType::ExactHost => Ok(()),
    }
}

const fn pattern_type_sql(pattern_type: RoutingPatternType) -> &'static str {
    match pattern_type {
        RoutingPatternType::Suffix => "suffix",
        RoutingPatternType::ExactHost => "exact_host",
        RoutingPatternType::Cidr => "cidr",
        RoutingPatternType::Regex => "regex",
    }
}
