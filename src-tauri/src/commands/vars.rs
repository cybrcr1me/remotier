//! Placeholder declarations and the local answers to them.
//!
//! `var_defs` is shared configuration: a group says "hosts under me need a `wg_user`".
//! `var_values` is this machine's answer and is deliberately kept in its own table so a
//! future sync can ship defs without ever shipping anybody's personal values.

use rusqlite::params;
use serde::Deserialize;
use tauri::State;

use crate::db::models::{VarDef, VarScope, VarValue};
use crate::db::{new_id, now_ms, query_all};
use crate::error::{Error, Result};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VarDefInput {
    pub scope: VarScope,
    pub scope_id: String,
    pub name: String,
    pub label: Option<String>,
    pub default_value: Option<String>,
    #[serde(default)]
    pub required: bool,
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[tauri::command(async)]
pub fn list_var_defs(state: State<'_, AppState>) -> Result<Vec<VarDef>> {
    state.db.read(|conn| {
        let sql = format!("SELECT {} FROM var_defs ORDER BY name", VarDef::COLUMNS);
        query_all(conn, &sql, [], VarDef::from_row)
    })
}

#[tauri::command(async)]
pub fn upsert_var_def(state: State<'_, AppState>, input: VarDefInput) -> Result<VarDef> {
    let name = input.name.trim();
    if !valid_name(name) {
        return Err(Error::Invalid(format!(
            "'{name}' is not a valid variable name (use letters, digits, _ or -)"
        )));
    }
    let now = now_ms();

    state.db.write(|tx| {
        tx.execute(
            "INSERT INTO var_defs (id, scope, scope_id, name, label, default_value, required,
                                   created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
             ON CONFLICT(scope, scope_id, name) DO UPDATE SET
                 label = excluded.label,
                 default_value = excluded.default_value,
                 required = excluded.required,
                 updated_at = excluded.updated_at",
            params![
                new_id(),
                input.scope.as_str(),
                input.scope_id,
                name,
                input.label,
                input.default_value,
                i64::from(input.required),
                now,
            ],
        )?;
        Ok(())
    })?;

    state.db.read(|conn| {
        let sql = format!(
            "SELECT {} FROM var_defs WHERE scope = ?1 AND scope_id = ?2 AND name = ?3",
            VarDef::COLUMNS
        );
        crate::db::query_one(
            conn,
            &sql,
            params![input.scope.as_str(), input.scope_id, name],
            VarDef::from_row,
            "variable",
            name,
        )
    })
}

#[tauri::command(async)]
pub fn delete_var_def(state: State<'_, AppState>, id: String) -> Result<()> {
    let changed = state
        .db
        .write(|tx| Ok(tx.execute("DELETE FROM var_defs WHERE id = ?1", params![id])?))?;
    if changed == 0 {
        return Err(Error::NotFound("variable", id));
    }
    Ok(())
}

#[tauri::command(async)]
pub fn list_var_values(state: State<'_, AppState>) -> Result<Vec<VarValue>> {
    state.db.read(|conn| {
        query_all(
            conn,
            "SELECT scope, scope_id, name, value FROM var_values",
            [],
            |row| {
                let scope: String = row.get(0)?;
                Ok(VarValue {
                    scope: VarScope::from_db(&scope),
                    scope_id: row.get(1)?,
                    name: row.get(2)?,
                    value: row.get(3)?,
                })
            },
        )
    })
}

#[tauri::command(async)]
pub fn set_var_value(
    state: State<'_, AppState>,
    scope: VarScope,
    scope_id: String,
    name: String,
    value: String,
) -> Result<()> {
    state.db.write(|tx| {
        tx.execute(
            "INSERT INTO var_values (scope, scope_id, name, value, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(scope, scope_id, name) DO UPDATE SET
                 value = excluded.value,
                 updated_at = excluded.updated_at",
            params![scope.as_str(), scope_id, name.trim(), value, now_ms()],
        )?;
        Ok(())
    })
}

#[tauri::command(async)]
pub fn clear_var_value(
    state: State<'_, AppState>,
    scope: VarScope,
    scope_id: String,
    name: String,
) -> Result<()> {
    state.db.write(|tx| {
        tx.execute(
            "DELETE FROM var_values WHERE scope = ?1 AND scope_id = ?2 AND name = ?3",
            params![scope.as_str(), scope_id, name],
        )?;
        Ok(())
    })
}
