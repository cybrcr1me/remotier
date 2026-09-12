//! Saved tab layouts.
//!
//! A workspace is a named snapshot of the tab tree, stored as opaque JSON. It is the same
//! shape the session resume uses, so applying one is just replacing the live tabs.

use rusqlite::params;
use tauri::State;

use crate::db::{new_id, now_ms, query_all, query_one};
use crate::error::{Error, Result};
use crate::state::AppState;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub layout_json: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Workspace {
    const COLUMNS: &'static str = "id, name, layout_json, created_at, updated_at";

    fn from_row(row: &rusqlite::Row) -> Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            layout_json: row.get(2)?,
            created_at: row.get(3)?,
            updated_at: row.get(4)?,
        })
    }
}

#[tauri::command(async)]
pub fn list_workspaces(state: State<'_, AppState>) -> Result<Vec<Workspace>> {
    state.db.read(|conn| {
        let sql = format!("SELECT {} FROM workspaces ORDER BY name", Workspace::COLUMNS);
        query_all(conn, &sql, [], Workspace::from_row)
    })
}

/// Create a workspace, or overwrite the one with the same name.
#[tauri::command(async)]
pub fn save_workspace(
    state: State<'_, AppState>,
    name: String,
    layout_json: String,
) -> Result<Workspace> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(Error::Invalid("a workspace needs a name".into()));
    }

    let now = now_ms();
    state.db.write(|tx| {
        let existing: Option<String> = tx
            .query_row(
                "SELECT id FROM workspaces WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .ok();

        match existing {
            Some(id) => tx.execute(
                "UPDATE workspaces SET layout_json = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, layout_json, now],
            )?,
            None => tx.execute(
                "INSERT INTO workspaces (id, name, layout_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                params![new_id(), name, layout_json, now],
            )?,
        };
        Ok(())
    })?;

    state.db.read(|conn| {
        let sql = format!("SELECT {} FROM workspaces WHERE name = ?1", Workspace::COLUMNS);
        query_one(conn, &sql, params![name], Workspace::from_row, "workspace", &name)
    })
}

#[tauri::command(async)]
pub fn delete_workspace(state: State<'_, AppState>, id: String) -> Result<()> {
    let changed = state
        .db
        .write(|tx| Ok(tx.execute("DELETE FROM workspaces WHERE id = ?1", params![id])?))?;
    if changed == 0 {
        return Err(Error::NotFound("workspace", id));
    }
    Ok(())
}
