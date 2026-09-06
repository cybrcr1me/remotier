use rusqlite::params;
use serde::Deserialize;
use tauri::State;

use crate::db::models::Group;
use crate::db::{new_id, now_ms, query_all, query_one};
use crate::error::{Error, Result};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupInput {
    pub name: String,
    pub parent_id: Option<String>,
    pub sort: Option<i64>,
    pub default_port: Option<i64>,
    pub default_identity_id: Option<String>,
    pub default_jump_host_id: Option<String>,
}

#[tauri::command(async)]
pub fn list_groups(state: State<'_, AppState>) -> Result<Vec<Group>> {
    let groups = state.db.read(|conn| {
        let sql = format!("SELECT {} FROM groups ORDER BY sort, name", Group::COLUMNS);
        query_all(conn, &sql, [], Group::from_row)
    })?;
    log::debug!("list_groups -> {} rows", groups.len());
    Ok(groups)
}

#[tauri::command(async)]
pub fn create_group(state: State<'_, AppState>, input: GroupInput) -> Result<Group> {
    if input.name.trim().is_empty() {
        return Err(Error::Invalid("group name must not be empty".into()));
    }
    let id = new_id();
    let now = now_ms();

    state.db.write(|tx| {
        tx.execute(
            "INSERT INTO groups (id, parent_id, name, sort, default_port, default_identity_id,
                                 default_jump_host_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![
                id,
                input.parent_id,
                input.name.trim(),
                input.sort.unwrap_or(0),
                input.default_port,
                input.default_identity_id,
                input.default_jump_host_id,
                now,
            ],
        )?;
        Ok(())
    })?;

    get_group(state, id)
}

#[tauri::command(async)]
pub fn update_group(state: State<'_, AppState>, id: String, input: GroupInput) -> Result<Group> {
    if input.name.trim().is_empty() {
        return Err(Error::Invalid("group name must not be empty".into()));
    }
    if input.parent_id.as_deref() == Some(id.as_str()) {
        return Err(Error::Invalid("a group cannot be its own parent".into()));
    }

    let changed = state.db.write(|tx| {
        Ok(tx.execute(
            "UPDATE groups SET parent_id = ?2, name = ?3, sort = ?4, default_port = ?5,
                    default_identity_id = ?6, default_jump_host_id = ?7, updated_at = ?8
             WHERE id = ?1",
            params![
                id,
                input.parent_id,
                input.name.trim(),
                input.sort.unwrap_or(0),
                input.default_port,
                input.default_identity_id,
                input.default_jump_host_id,
                now_ms(),
            ],
        )?)
    })?;

    if changed == 0 {
        return Err(Error::NotFound("group", id));
    }
    get_group(state, id)
}

/// Deletes the group and, by `ON DELETE CASCADE`, its subgroups. Hosts are kept and
/// fall back to ungrouped rather than being deleted along with it.
#[tauri::command(async)]
pub fn delete_group(state: State<'_, AppState>, id: String) -> Result<()> {
    let changed = state
        .db
        .write(|tx| Ok(tx.execute("DELETE FROM groups WHERE id = ?1", params![id])?))?;
    if changed == 0 {
        return Err(Error::NotFound("group", id));
    }
    Ok(())
}

fn get_group(state: State<'_, AppState>, id: String) -> Result<Group> {
    state.db.read(|conn| {
        let sql = format!("SELECT {} FROM groups WHERE id = ?1", Group::COLUMNS);
        query_one(conn, &sql, params![id], Group::from_row, "group", &id)
    })
}
