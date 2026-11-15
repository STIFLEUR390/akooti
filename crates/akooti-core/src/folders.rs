use rusqlite::{Connection, OptionalExtension, Row, params};
use uuid::Uuid;

use crate::error::VaultError;
use crate::models::{Folder, FolderDraft};

pub(crate) fn list(
    connection: &Connection,
    include_deleted: bool,
) -> Result<Vec<Folder>, VaultError> {
    let sql = if include_deleted {
        "SELECT id, parent_id, name, slug, icon, color, is_system,
                require_biometric_for_mcp, created_at, updated_at, deleted_at
         FROM folders ORDER BY is_system DESC, name COLLATE NOCASE"
    } else {
        "SELECT id, parent_id, name, slug, icon, color, is_system,
                require_biometric_for_mcp, created_at, updated_at, deleted_at
         FROM folders WHERE deleted_at IS NULL
         ORDER BY is_system DESC, name COLLATE NOCASE"
    };
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map([], folder_from_row)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(VaultError::from)
}

pub(crate) fn get(connection: &Connection, id: &str) -> Result<Folder, VaultError> {
    connection
        .query_row(
            "SELECT id, parent_id, name, slug, icon, color, is_system,
                    require_biometric_for_mcp, created_at, updated_at, deleted_at
             FROM folders WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            folder_from_row,
        )
        .optional()?
        .ok_or(VaultError::NotFound)
}

pub(crate) fn create(
    connection: &Connection,
    draft: FolderDraft,
    now: i64,
) -> Result<Folder, VaultError> {
    validate_name(&draft.name)?;
    let id = Uuid::new_v4().to_string();
    connection.execute(
        "INSERT INTO folders (
           id, parent_id, name, slug, icon, color, is_system,
           require_biometric_for_mcp, created_at, updated_at
         ) VALUES (?1, ?2, ?3, NULL, ?4, ?5, 0, ?6, ?7, ?7)",
        params![
            id,
            draft.parent_id,
            draft.name,
            draft.icon,
            draft.color,
            draft.require_biometric_for_mcp,
            now,
        ],
    )?;
    get(connection, &id)
}

pub(crate) fn update(
    connection: &Connection,
    id: &str,
    draft: FolderDraft,
    now: i64,
) -> Result<Folder, VaultError> {
    validate_name(&draft.name)?;
    let existing = get(connection, id)?;
    if existing.is_system {
        return Err(VaultError::SystemFolderProtected);
    }
    connection.execute(
        "UPDATE folders SET parent_id = ?2, name = ?3, icon = ?4, color = ?5,
                            require_biometric_for_mcp = ?6, updated_at = ?7
         WHERE id = ?1 AND deleted_at IS NULL",
        params![
            id,
            draft.parent_id,
            draft.name,
            draft.icon,
            draft.color,
            draft.require_biometric_for_mcp,
            now,
        ],
    )?;
    get(connection, id)
}

pub(crate) fn rename(
    connection: &Connection,
    id: &str,
    name: &str,
    now: i64,
) -> Result<Folder, VaultError> {
    validate_name(name)?;
    let existing = get(connection, id)?;
    if existing.is_system {
        return Err(VaultError::SystemFolderProtected);
    }
    connection.execute(
        "UPDATE folders SET name = ?2, updated_at = ?3 WHERE id = ?1 AND deleted_at IS NULL",
        params![id, name, now],
    )?;
    get(connection, id)
}

pub(crate) fn soft_delete(connection: &Connection, id: &str, now: i64) -> Result<(), VaultError> {
    let existing = get(connection, id)?;
    if existing.is_system {
        return Err(VaultError::SystemFolderProtected);
    }
    connection.execute(
        "UPDATE folders SET deleted_at = ?2, updated_at = ?2
         WHERE id = ?1 AND deleted_at IS NULL",
        params![id, now],
    )?;
    Ok(())
}

fn folder_from_row(row: &Row<'_>) -> rusqlite::Result<Folder> {
    Ok(Folder {
        id: row.get(0)?,
        parent_id: row.get(1)?,
        name: row.get(2)?,
        slug: row.get(3)?,
        icon: row.get(4)?,
        color: row.get(5)?,
        is_system: row.get(6)?,
        require_biometric_for_mcp: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        deleted_at: row.get(10)?,
    })
}

fn validate_name(name: &str) -> Result<(), VaultError> {
    if name.trim().is_empty() {
        return Err(VaultError::InvalidInput("folder name is empty"));
    }
    Ok(())
}
