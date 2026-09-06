use rusqlite::params;
use serde::Deserialize;
use tauri::State;

use crate::db::models::Host;
use crate::db::{new_id, now_ms, query_all, query_one};
use crate::error::{Error, Result};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostInput {
    pub label: String,
    pub hostname: String,
    pub group_id: Option<String>,
    /// `None` means "inherit", it is not the same as 22.
    pub port: Option<i64>,
    pub identity_id: Option<String>,
    pub jump_host_id: Option<String>,
    pub color: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub sort: Option<i64>,
}

impl HostInput {
    fn validate(&self) -> Result<()> {
        if self.hostname.trim().is_empty() {
            return Err(Error::Invalid("hostname must not be empty".into()));
        }
        if let Some(port) = self.port {
            if !(1..=65535).contains(&port) {
                return Err(Error::Invalid(format!("port {port} is out of range")));
            }
        }
        Ok(())
    }

    /// Falls back to the hostname so a host is never nameless in the sidebar.
    fn label(&self) -> String {
        let label = self.label.trim();
        if label.is_empty() {
            self.hostname.trim().to_string()
        } else {
            label.to_string()
        }
    }
}

#[tauri::command(async)]
pub fn list_hosts(state: State<'_, AppState>) -> Result<Vec<Host>> {
    let hosts = state.db.read(|conn| {
        let sql = format!("SELECT {} FROM hosts ORDER BY sort, label", Host::COLUMNS);
        query_all(conn, &sql, [], Host::from_row)
    })?;
    log::debug!("list_hosts -> {} rows", hosts.len());
    Ok(hosts)
}

#[tauri::command(async)]
pub fn create_host(state: State<'_, AppState>, input: HostInput) -> Result<Host> {
    input.validate()?;
    let id = new_id();
    let now = now_ms();
    let tags = serde_json::to_string(&input.tags)?;

    state.db.write(|tx| {
        tx.execute(
            "INSERT INTO hosts (id, group_id, label, hostname, port, identity_id, jump_host_id,
                                color, tags, sort, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
            params![
                id,
                input.group_id,
                input.label(),
                input.hostname.trim(),
                input.port,
                input.identity_id,
                input.jump_host_id,
                input.color,
                tags,
                input.sort.unwrap_or(0),
                now,
            ],
        )?;
        Ok(())
    })?;

    get_host(state, id)
}

#[tauri::command(async)]
pub fn update_host(state: State<'_, AppState>, id: String, input: HostInput) -> Result<Host> {
    input.validate()?;
    if input.jump_host_id.as_deref() == Some(id.as_str()) {
        return Err(Error::Invalid("a host cannot be its own jump host".into()));
    }
    let tags = serde_json::to_string(&input.tags)?;

    let changed = state.db.write(|tx| {
        Ok(tx.execute(
            "UPDATE hosts SET group_id = ?2, label = ?3, hostname = ?4, port = ?5,
                    identity_id = ?6, jump_host_id = ?7, color = ?8, tags = ?9, sort = ?10,
                    updated_at = ?11
             WHERE id = ?1",
            params![
                id,
                input.group_id,
                input.label(),
                input.hostname.trim(),
                input.port,
                input.identity_id,
                input.jump_host_id,
                input.color,
                tags,
                input.sort.unwrap_or(0),
                now_ms(),
            ],
        )?)
    })?;

    if changed == 0 {
        return Err(Error::NotFound("host", id));
    }
    get_host(state, id)
}

#[tauri::command(async)]
pub fn delete_host(state: State<'_, AppState>, id: String) -> Result<()> {
    let changed = state
        .db
        .write(|tx| Ok(tx.execute("DELETE FROM hosts WHERE id = ?1", params![id])?))?;
    if changed == 0 {
        return Err(Error::NotFound("host", id));
    }
    Ok(())
}

/// Bulk reorder / regroup, used by sidebar drag and drop.
#[tauri::command(async)]
pub fn move_hosts(
    state: State<'_, AppState>,
    ids: Vec<String>,
    group_id: Option<String>,
) -> Result<()> {
    let now = now_ms();
    state.db.write(|tx| {
        for (index, id) in ids.iter().enumerate() {
            tx.execute(
                "UPDATE hosts SET group_id = ?2, sort = ?3, updated_at = ?4 WHERE id = ?1",
                params![id, group_id, index as i64, now],
            )?;
        }
        Ok(())
    })
}

fn get_host(state: State<'_, AppState>, id: String) -> Result<Host> {
    state.db.read(|conn| {
        let sql = format!("SELECT {} FROM hosts WHERE id = ?1", Host::COLUMNS);
        query_one(conn, &sql, params![id], Host::from_row, "host", &id)
    })
}
