use rusqlite::{Connection, params};

use crate::error::VaultError;

/// Default lifetime for agent-created ephemeral accounts.
pub const DEFAULT_EPHEMERAL_TTL_SECONDS: i64 = 24 * 60 * 60;

pub(crate) fn purge_expired(connection: &Connection, now: i64) -> Result<usize, VaultError> {
    let deleted = connection.execute(
        "DELETE FROM vault_items
         WHERE expires_at IS NOT NULL AND expires_at <= ?1",
        params![now],
    )?;
    if deleted > 0 {
        tracing::info!(deleted, "purged expired vault items");
    }
    Ok(deleted)
}
