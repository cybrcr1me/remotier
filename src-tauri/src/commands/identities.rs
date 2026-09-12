use rusqlite::params;
use serde::Deserialize;
use tauri::State;

use crate::commands::secrets;
use crate::db::models::{AuthKind, Identity};
use crate::db::{new_id, now_ms, query_all, query_one};
use crate::error::{Error, Result};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityInput {
    pub label: String,
    pub username: String,
    pub auth_kind: AuthKind,
    /// Inbound only. `None` leaves any stored password untouched; `Some("")` clears it.
    pub password: Option<String>,
    pub key_id: Option<String>,
}

#[tauri::command(async)]
pub fn list_identities(state: State<'_, AppState>) -> Result<Vec<Identity>> {
    state.db.read(|conn| {
        let sql = format!(
            "SELECT {} FROM identities ORDER BY label",
            Identity::COLUMNS
        );
        query_all(conn, &sql, [], Identity::from_row)
    })
}

#[tauri::command(async)]
pub fn create_identity(state: State<'_, AppState>, input: IdentityInput) -> Result<Identity> {
    if input.username.trim().is_empty() {
        return Err(Error::Invalid("username must not be empty".into()));
    }
    let id = new_id();
    let now = now_ms();
    let label = if input.label.trim().is_empty() {
        input.username.trim().to_string()
    } else {
        input.label.trim().to_string()
    };

    state.db.write(|tx| {
        let password_ref = match input.password.as_deref() {
            Some(password) if !password.is_empty() => {
                Some(secrets::put(tx, state.vault()?, None, password)?)
            }
            _ => None,
        };
        tx.execute(
            "INSERT INTO identities (id, label, username, auth_kind, password_ref, key_id,
                                     created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            params![
                id,
                label,
                input.username.trim(),
                input.auth_kind.as_str(),
                password_ref,
                input.key_id,
                now,
            ],
        )?;
        Ok(())
    })?;

    get_identity(state, id)
}

#[tauri::command(async)]
pub fn update_identity(
    state: State<'_, AppState>,
    id: String,
    input: IdentityInput,
) -> Result<Identity> {
    if input.username.trim().is_empty() {
        return Err(Error::Invalid("username must not be empty".into()));
    }

    state.db.write(|tx| {
        let existing: Option<String> = tx
            .query_row(
                "SELECT password_ref FROM identities WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .map_err(|_| Error::NotFound("identity", id.clone()))?;

        let password_ref = match input.password.as_deref() {
            // Absent: keep whatever is stored.
            None => existing,
            // Empty string: explicit clear.
            Some("") => {
                secrets::delete(tx, existing.as_deref())?;
                None
            }
            Some(password) => Some(secrets::put(tx, state.vault()?, existing.as_deref(), password)?),
        };

        tx.execute(
            "UPDATE identities SET label = ?2, username = ?3, auth_kind = ?4, password_ref = ?5,
                    key_id = ?6, updated_at = ?7
             WHERE id = ?1",
            params![
                id,
                input.label.trim(),
                input.username.trim(),
                input.auth_kind.as_str(),
                password_ref,
                input.key_id,
                now_ms(),
            ],
        )?;
        Ok(())
    })?;

    get_identity(state, id)
}

#[tauri::command(async)]
pub fn delete_identity(state: State<'_, AppState>, id: String) -> Result<()> {
    state.db.write(|tx| {
        let existing: Option<String> = tx
            .query_row(
                "SELECT password_ref FROM identities WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .map_err(|_| Error::NotFound("identity", id.clone()))?;
        secrets::delete(tx, existing.as_deref())?;
        tx.execute("DELETE FROM identities WHERE id = ?1", params![id])?;
        Ok(())
    })
}

fn get_identity(state: State<'_, AppState>, id: String) -> Result<Identity> {
    state.db.read(|conn| {
        let sql = format!("SELECT {} FROM identities WHERE id = ?1", Identity::COLUMNS);
        query_one(conn, &sql, params![id], Identity::from_row, "identity", &id)
    })
}
