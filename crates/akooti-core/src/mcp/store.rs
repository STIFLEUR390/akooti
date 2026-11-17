//! SQLite persistence for MCP tokens.

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::VaultError;

use super::token::{McpScope, McpToken};

pub(crate) struct StoredToken {
    pub(crate) token: McpToken,
    pub(crate) token_hash: String,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn insert(
    connection: &Connection,
    id: &str,
    name: &str,
    token_hash: &str,
    token_prefix: &str,
    scopes: &[McpScope],
    folder_ids: &[String],
    default_create_folder_id: Option<&str>,
    expires_at: Option<i64>,
    now: i64,
) -> Result<(), VaultError> {
    let scopes_json = serde_json::to_string(&scopes.iter().map(|s| s.as_str()).collect::<Vec<_>>())
        .map_err(|_| VaultError::CorruptVault)?;
    connection.execute(
        "INSERT INTO mcp_tokens (
           id, name, token_hash, token_prefix, scopes_json,
           default_create_folder_id, expires_at, revoked_at, last_used_at, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL, ?8)",
        params![
            id,
            name,
            token_hash,
            token_prefix,
            scopes_json,
            default_create_folder_id,
            expires_at,
            now,
        ],
    )?;
    for folder_id in folder_ids {
        connection.execute(
            "INSERT OR IGNORE INTO mcp_token_folders (token_id, folder_id) VALUES (?1, ?2)",
            params![id, folder_id],
        )?;
    }
    Ok(())
}

pub(crate) fn list(connection: &Connection) -> Result<Vec<McpToken>, VaultError> {
    let mut statement = connection.prepare(
        "SELECT id, name, token_prefix, scopes_json, default_create_folder_id,
                expires_at, revoked_at, last_used_at, created_at
         FROM mcp_tokens
         ORDER BY created_at DESC, id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<i64>>(5)?,
            row.get::<_, Option<i64>>(6)?,
            row.get::<_, Option<i64>>(7)?,
            row.get::<_, i64>(8)?,
        ))
    })?;
    let mut tokens = Vec::new();
    for row in rows {
        let (
            id,
            name,
            token_prefix,
            scopes_json,
            default_create_folder_id,
            expires_at,
            revoked_at,
            last_used_at,
            created_at,
        ) = row?;
        tokens.push(McpToken {
            folder_ids: folders(connection, &id)?,
            scopes: parse_scopes(&scopes_json),
            id,
            name,
            token_prefix,
            default_create_folder_id,
            expires_at,
            revoked_at,
            last_used_at,
            created_at,
        });
    }
    Ok(tokens)
}

/// Raw token columns, in `SELECT` order.
type TokenRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    i64,
);

pub(crate) fn by_hash(
    connection: &Connection,
    token_hash: &str,
) -> Result<Option<StoredToken>, VaultError> {
    let found: Option<TokenRow> = connection
        .query_row(
            "SELECT id, name, token_prefix, scopes_json, default_create_folder_id,
                    expires_at, revoked_at, last_used_at, created_at
             FROM mcp_tokens WHERE token_hash = ?1",
            [token_hash],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                ))
            },
        )
        .optional()?;
    let Some((
        id,
        name,
        token_prefix,
        scopes_json,
        default_create_folder_id,
        expires_at,
        revoked_at,
        last_used_at,
        created_at,
    )) = found
    else {
        return Ok(None);
    };
    Ok(Some(StoredToken {
        token: McpToken {
            folder_ids: folders(connection, &id)?,
            scopes: parse_scopes(&scopes_json),
            default_create_folder_id,
            expires_at,
            revoked_at,
            last_used_at,
            created_at,
            id,
            name,
            token_prefix,
        },
        token_hash: token_hash.to_owned(),
    }))
}

pub(crate) fn set_revoked(
    connection: &Connection,
    id: &str,
    now: i64,
) -> Result<McpToken, VaultError> {
    let changed = connection.execute(
        "UPDATE mcp_tokens SET revoked_at = ?2 WHERE id = ?1 AND revoked_at IS NULL",
        params![id, now],
    )?;
    if changed == 0 {
        return Err(VaultError::NotFound);
    }
    list(connection)?
        .into_iter()
        .find(|token| token.id == id)
        .ok_or(VaultError::CorruptVault)
}

pub(crate) fn touch(connection: &Connection, id: &str, now: i64) -> Result<(), VaultError> {
    connection.execute(
        "UPDATE mcp_tokens SET last_used_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    Ok(())
}

pub(crate) fn delete(connection: &Connection, id: &str) -> Result<(), VaultError> {
    let changed = connection.execute("DELETE FROM mcp_tokens WHERE id = ?1", [id])?;
    if changed == 0 {
        return Err(VaultError::NotFound);
    }
    Ok(())
}

fn folders(connection: &Connection, token_id: &str) -> Result<Vec<String>, VaultError> {
    let mut statement =
        connection.prepare("SELECT folder_id FROM mcp_token_folders WHERE token_id = ?1")?;
    let rows = statement.query_map([token_id], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn parse_scopes(value: &str) -> Vec<McpScope> {
    serde_json::from_str::<Vec<String>>(value)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|name| McpScope::parse(&name))
        .collect()
}
