//! Vault health audit: local strength scoring, reuse, age and breach checks.
//!
//! Everything that scores a password runs offline. The HIBP lookup is split in
//! two on purpose: [`breach_targets`] exposes only the 5-character SHA-1
//! prefixes (all the remote service ever sees), then [`apply_breach_range`]
//! compares the suffixes locally. No network code lives here (rule SEC-5): the
//! desktop shell fetches the range, this module decides what it means.

use std::collections::{HashMap, HashSet};

use hmac::{Hmac, Mac};
use rusqlite::{Connection, params};
use sha1::{Digest, Sha1};
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::crypto::KEY_LEN;
use crate::error::VaultError;
use crate::models::SecretField;

/// Number of leading SHA-1 characters sent to HIBP (k-anonymity).
pub const SHA1_PREFIX_LEN: usize = 5;
/// SHA-1 digest length, in hexadecimal characters.
const SHA1_HEX_LEN: usize = 40;
const SECONDS_PER_DAY: i64 = 86_400;

/// One audited item. Derived secret material never appears here.
#[derive(Debug, Clone)]
pub struct HealthFinding {
    /// Item identifier.
    pub item_id: String,
    /// Display title, for the dashboard.
    pub title: String,
    /// Primary URL, when set.
    pub primary_url: Option<String>,
    /// Username, when set.
    pub username: Option<String>,
    /// zxcvbn score, 0 (weak) to 4 (strong).
    pub score: u8,
    /// Another live item stores the same password.
    pub reused: bool,
    /// The password appears in a known breach (last HIBP check).
    pub compromised: bool,
    /// Days since the password was last changed.
    pub age_days: Option<i64>,
}

/// Outcome of a full local audit.
#[derive(Debug, Clone)]
pub struct HealthReport {
    /// One entry per item holding a password, ordered by identifier.
    pub findings: Vec<HealthFinding>,
    /// Live items without a password (notes, key sets, ...), left unscored.
    pub without_password: usize,
    /// Mean score over [`Self::findings`], absent when nothing was scored.
    pub average_score: Option<f64>,
}

/// A distinct SHA-1 prefix to check against HIBP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BreachTarget {
    /// First 5 hexadecimal characters of the SHA-1 digest.
    pub prefix: String,
    /// Items sharing that prefix.
    pub items: usize,
}

struct HealthRow {
    id: String,
    title: String,
    primary_url: Option<String>,
    username: Option<String>,
    password_updated_at: Option<i64>,
    compromised: bool,
}

pub(crate) fn audit(
    connection: &Connection,
    master_key: &[u8; KEY_LEN],
    now: i64,
    on_progress: &mut dyn FnMut(usize, usize),
) -> Result<HealthReport, VaultError> {
    let total: i64 = connection.query_row(
        "SELECT COUNT(*) FROM vault_items WHERE deleted_at IS NULL",
        [],
        |row| row.get(0),
    )?;
    let rows = {
        let mut statement = connection.prepare(
            "SELECT id, title, primary_url, username, password_updated_at, is_compromised
             FROM vault_items
             WHERE deleted_at IS NULL AND encrypted_password IS NOT NULL
             ORDER BY id",
        )?;
        statement
            .query_map([], |row| {
                Ok(HealthRow {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    primary_url: row.get(2)?,
                    username: row.get(3)?,
                    password_updated_at: row.get(4)?,
                    compromised: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?
    };

    let total_scanned = rows.len();
    let mut findings = Vec::with_capacity(total_scanned);
    let mut reuse_keys: Vec<[u8; 32]> = Vec::with_capacity(total_scanned);
    let mut reuse_counts: HashMap<[u8; 32], usize> = HashMap::new();

    for (index, row) in rows.iter().enumerate() {
        let revealed =
            crate::items::reveal(connection, master_key, &row.id, SecretField::Password, None)?;
        let password = revealed.value.expose();
        let digest = sha1_hex(password);
        let prefix: String = digest.chars().take(SHA1_PREFIX_LEN).collect();
        let score = u8::from(zxcvbn::zxcvbn(password, &user_inputs(row)).score());

        connection.execute(
            "UPDATE vault_items SET zxcvbn_score = ?2, password_sha1_prefix = ?3 WHERE id = ?1",
            params![row.id, score, prefix],
        )?;

        if !password.is_empty() {
            let key = reuse_key(master_key, password)?;
            *reuse_counts.entry(key).or_insert(0) += 1;
            reuse_keys.push(key);
        } else {
            reuse_keys.push([0_u8; 32]);
        }

        findings.push(HealthFinding {
            item_id: row.id.clone(),
            title: row.title.clone(),
            primary_url: row.primary_url.clone(),
            username: row.username.clone(),
            score,
            reused: false,
            compromised: row.compromised,
            age_days: row
                .password_updated_at
                .map(|updated| (now - updated).max(0) / SECONDS_PER_DAY),
        });
        on_progress(index + 1, total_scanned);
    }

    for (finding, key) in findings.iter_mut().zip(&reuse_keys) {
        if reuse_counts.get(key).is_some_and(|count| *count > 1) {
            finding.reused = true;
        }
    }

    let scored: u32 = findings
        .iter()
        .map(|finding| u32::from(finding.score))
        .sum();
    let average_score = if findings.is_empty() {
        None
    } else {
        Some(f64::from(scored) / findings.len() as f64)
    };

    Ok(HealthReport {
        findings,
        without_password: usize::try_from(total.max(0))
            .unwrap_or(0)
            .saturating_sub(total_scanned),
        average_score,
    })
}

pub(crate) fn breach_targets(connection: &Connection) -> Result<Vec<BreachTarget>, VaultError> {
    let mut statement = connection.prepare(
        "SELECT password_sha1_prefix, COUNT(*)
         FROM vault_items
         WHERE deleted_at IS NULL AND encrypted_password IS NOT NULL
           AND password_sha1_prefix IS NOT NULL
         GROUP BY password_sha1_prefix
         ORDER BY password_sha1_prefix",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .map(|(prefix, items)| BreachTarget {
            prefix,
            items: usize::try_from(items.max(0)).unwrap_or(0),
        })
        .collect())
}

/// Marks every item under `prefix` that matches one of `suffixes`, and clears
/// the flag on the others: a checked password is no longer "unknown".
///
/// Returns the number of items now flagged as compromised.
pub(crate) fn apply_breach_range(
    connection: &Connection,
    master_key: &[u8; KEY_LEN],
    prefix: &str,
    suffixes: &HashSet<String>,
) -> Result<usize, VaultError> {
    let ids = {
        let mut statement = connection.prepare(
            "SELECT id FROM vault_items
             WHERE deleted_at IS NULL AND encrypted_password IS NOT NULL
               AND password_sha1_prefix = ?1",
        )?;
        statement
            .query_map([prefix], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
    };

    let mut compromised = 0;
    for id in ids {
        let revealed =
            crate::items::reveal(connection, master_key, &id, SecretField::Password, None)?;
        let digest = sha1_hex(revealed.value.expose());
        if !digest.starts_with(prefix) {
            // The password changed without the prefix being refreshed.
            continue;
        }
        let hit = digest
            .get(SHA1_PREFIX_LEN..SHA1_HEX_LEN)
            .is_some_and(|suffix| suffixes.contains(suffix));
        if hit {
            compromised += 1;
        }
        connection.execute(
            "UPDATE vault_items SET is_compromised = ?2 WHERE id = ?1 AND is_compromised <> ?2",
            params![id, hit],
        )?;
    }
    Ok(compromised)
}

fn sha1_hex(password: &str) -> Zeroizing<String> {
    Zeroizing::new(hex::encode_upper(Sha1::digest(password.as_bytes())))
}

/// Keyed digest used only to spot identical passwords within one audit: the
/// raw SHA-1 (or the plaintext) is never compared or stored.
fn reuse_key(master_key: &[u8; KEY_LEN], password: &str) -> Result<[u8; 32], VaultError> {
    let mut mac = Hmac::<Sha256>::new_from_slice(master_key).map_err(|_| VaultError::Crypto)?;
    mac.update(password.as_bytes());
    Ok(mac.finalize().into_bytes().into())
}

/// Feeds zxcvbn's dictionary check with the fields a human actually reuses.
fn user_inputs(row: &HealthRow) -> Vec<&str> {
    let mut inputs = Vec::with_capacity(2);
    if let Some(username) = row.username.as_deref().filter(|value| value.len() >= 3) {
        inputs.push(username);
    }
    if row.title.len() >= 3 {
        inputs.push(row.title.as_str());
    }
    inputs
}
