use rusqlite::params;
use serde::Deserialize;
use tauri::State;

use crate::commands::secrets;
use crate::db::models::{AuthKind, Host};
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
    pub icon: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub sort: Option<i64>,
    /// Credentials set on the host itself, for a server that does not warrant an
    /// identity. `auth_kind` of `None` keeps using the inherited identity.
    pub username: Option<String>,
    pub auth_kind: Option<AuthKind>,
    /// Inbound only. `None` keeps any stored password, `Some("")` clears it.
    pub password: Option<String>,
    pub key_id: Option<String>,
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
        let password_ref = match input.password.as_deref().filter(|p| !p.is_empty()) {
            Some(password) => Some(secrets::put(tx, state.vault()?, None, password)?),
            None => None,
        };

        tx.execute(
            "INSERT INTO hosts (id, group_id, label, hostname, port, identity_id, jump_host_id,
                                color, tags, sort, username, auth_kind, password_ref, key_id,
                                icon, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?16)",
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
                input.username.as_deref().map(str::trim).filter(|u| !u.is_empty()),
                input.auth_kind.map(AuthKind::as_str),
                password_ref,
                input.key_id,
                input.icon,
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
        let existing: Option<String> = tx
            .query_row(
                "SELECT password_ref FROM hosts WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .map_err(|_| Error::NotFound("host", id.clone()))?;

        let password_ref = match input.password.as_deref() {
            // Absent: keep whatever is stored.
            None => existing,
            // Empty string: explicit clear.
            Some("") => {
                secrets::delete(tx, existing.as_deref())?;
                None
            }
            Some(password) => Some(secrets::put(
                tx,
                state.vault()?,
                existing.as_deref(),
                password,
            )?),
        };

        Ok(tx.execute(
            "UPDATE hosts SET group_id = ?2, label = ?3, hostname = ?4, port = ?5,
                    identity_id = ?6, jump_host_id = ?7, color = ?8, tags = ?9, sort = ?10,
                    username = ?11, auth_kind = ?12, password_ref = ?13, key_id = ?14,
                    icon = ?15, updated_at = ?16
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
                input.username.as_deref().map(str::trim).filter(|u| !u.is_empty()),
                input.auth_kind.map(AuthKind::as_str),
                password_ref,
                input.key_id,
                input.icon,
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
    let changed = state.db.write(|tx| {
        // Take the sealed password with it rather than orphaning it in the vault.
        let password_ref: Option<String> = tx
            .query_row(
                "SELECT password_ref FROM hosts WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .ok()
            .flatten();
        secrets::delete(tx, password_ref.as_deref())?;
        Ok(tx.execute("DELETE FROM hosts WHERE id = ?1", params![id])?)
    })?;

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
