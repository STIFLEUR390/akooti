use std::net::IpAddr;

use regex::Regex;
use rusqlite::{Connection, OptionalExtension, Row, params};
use url::Url;

use crate::error::VaultError;

/// Pattern interpretation for automatic local routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingPatternType {
    /// Match a hostname suffix such as `.test`.
    Suffix,
    /// Match the complete host.
    ExactHost,
    /// Match an IP address inside a CIDR network.
    Cidr,
    /// Match the complete URL with a regular expression.
    Regex,
}

impl RoutingPatternType {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "suffix" => Some(Self::Suffix),
            "exact_host" => Some(Self::ExactHost),
            "cidr" => Some(Self::Cidr),
            "regex" => Some(Self::Regex),
            _ => None,
        }
    }
}

/// A persisted automatic routing rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingRule {
    /// Rule identifier.
    pub id: String,
    /// Host, suffix, CIDR, or regex pattern.
    pub pattern: String,
    /// Pattern interpretation.
    pub pattern_type: RoutingPatternType,
    /// Destination folder.
    pub target_folder_id: String,
    /// Whether this rule came from the default seed.
    pub is_default: bool,
    /// Whether this rule is active.
    pub enabled: bool,
    /// Creation time in Unix seconds.
    pub created_at: i64,
}

impl RoutingRule {
    /// Tests this rule against an absolute or host-like URL candidate.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed regex or CIDR rules.
    pub fn matches(&self, candidate: &str) -> Result<bool, VaultError> {
        matches(self, candidate)
    }
}

/// New custom routing rule input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRoutingRule {
    /// Host, suffix, CIDR, or regex pattern.
    pub pattern: String,
    /// Pattern interpretation.
    pub pattern_type: RoutingPatternType,
    /// Destination folder.
    pub target_folder_id: String,
    /// Whether the rule is immediately active.
    pub enabled: bool,
}

pub(crate) fn apply_to_item(
    connection: &Connection,
    item_id: &str,
    now: i64,
) -> Result<usize, VaultError> {
    let exists: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM vault_items WHERE id = ?1 AND deleted_at IS NULL",
            [item_id],
            |row| row.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return Err(VaultError::NotFound);
    }
    let mut candidates = Vec::new();
    let primary: Option<String> = connection.query_row(
        "SELECT primary_url FROM vault_items WHERE id = ?1 AND deleted_at IS NULL",
        [item_id],
        |row| row.get(0),
    )?;
    if let Some(primary) = primary {
        candidates.push(primary);
    }
    let mut statement = connection.prepare("SELECT uri FROM item_uris WHERE item_id = ?1")?;
    let uris = statement
        .query_map([item_id], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    candidates.extend(uris);
    apply_candidates(connection, item_id, &candidates, now)
}

pub(crate) fn apply_candidates(
    connection: &Connection,
    item_id: &str,
    candidates: &[String],
    now: i64,
) -> Result<usize, VaultError> {
    let rules = crate::routing::store::list(connection, true)?;
    let mut added = 0;
    for candidate in candidates {
        for rule in &rules {
            if matches(rule, candidate)? {
                added += connection.execute(
                    "INSERT OR IGNORE INTO item_folders (item_id, folder_id, added_at)
                     VALUES (?1, ?2, ?3)",
                    params![item_id, rule.target_folder_id, now],
                )?;
            }
        }
    }
    Ok(added)
}

fn matches(rule: &RoutingRule, candidate: &str) -> Result<bool, VaultError> {
    if !rule.enabled {
        return Ok(false);
    }
    let Ok((url, host)) = parse_candidate(candidate) else {
        return Ok(false);
    };
    match rule.pattern_type {
        RoutingPatternType::Suffix => {
            let suffix = rule.pattern.to_ascii_lowercase();
            Ok(host.ends_with(&suffix))
        }
        RoutingPatternType::ExactHost => Ok(host.eq_ignore_ascii_case(&rule.pattern)),
        RoutingPatternType::Cidr => match host.parse::<IpAddr>() {
            Ok(address) => cidr_contains(&rule.pattern, address),
            Err(_) => Ok(false),
        },
        RoutingPatternType::Regex => Regex::new(&rule.pattern)
            .map(|regex| regex.is_match(url.as_str()))
            .map_err(|_| VaultError::InvalidInput("routing regex is invalid")),
    }
}

pub(crate) fn rule_from_row(row: &Row<'_>) -> rusqlite::Result<RoutingRule> {
    let pattern_type: String = row.get(2)?;
    Ok(RoutingRule {
        id: row.get(0)?,
        pattern: row.get(1)?,
        pattern_type: RoutingPatternType::parse(&pattern_type)
            .ok_or(rusqlite::Error::InvalidQuery)?,
        target_folder_id: row.get(3)?,
        is_default: row.get(4)?,
        enabled: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn parse_candidate(candidate: &str) -> Result<(Url, String), VaultError> {
    let normalized = if candidate.contains("://") {
        candidate.to_owned()
    } else {
        format!("https://{candidate}")
    };
    let url = Url::parse(&normalized)
        .map_err(|_| VaultError::InvalidInput("routing candidate URL is invalid"))?;
    let host = url
        .host_str()
        .ok_or(VaultError::InvalidInput("routing candidate has no host"))?
        .trim_matches(['[', ']'])
        .to_ascii_lowercase();
    Ok((url, host))
}

fn cidr_contains(pattern: &str, address: IpAddr) -> Result<bool, VaultError> {
    let (network, prefix) = pattern
        .split_once('/')
        .ok_or(VaultError::InvalidInput("CIDR routing rule has no prefix"))?;
    let network = network
        .parse::<IpAddr>()
        .map_err(|_| VaultError::InvalidInput("CIDR routing network is invalid"))?;
    let prefix = prefix
        .parse::<u8>()
        .map_err(|_| VaultError::InvalidInput("CIDR prefix is invalid"))?;
    let (network_bits, width) = address_bits(network)?;
    let (address_bits, address_width) = address_bits(address)?;
    if width != address_width || prefix > width {
        return Ok(false);
    }
    if prefix == 0 {
        return Ok(true);
    }
    let shift = width - prefix;
    let mask = u128::MAX << shift;
    Ok(network_bits & mask == address_bits & mask)
}

fn address_bits(address: IpAddr) -> Result<(u128, u8), VaultError> {
    match address {
        IpAddr::V4(address) => Ok((u128::from(u32::from(address)), 32)),
        IpAddr::V6(address) => Ok((u128::from(address), 128)),
    }
}
