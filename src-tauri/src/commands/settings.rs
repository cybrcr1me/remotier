use std::collections::HashMap;

use rusqlite::params;
use tauri::State;

use crate::db::query_all;
use crate::error::Result;
use crate::state::AppState;

#[tauri::command(async)]
pub fn get_settings(state: State<'_, AppState>) -> Result<HashMap<String, String>> {
    state.db.read(|conn| {
        let pairs = query_all(conn, "SELECT key, value FROM settings", [], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        Ok(pairs.into_iter().collect())
    })
}

#[tauri::command(async)]
pub fn set_setting(state: State<'_, AppState>, key: String, value: String) -> Result<()> {
    state.db.write(|tx| {
        tx.execute(
            // `updated_at` is the LWW clock a settings row is merged on. A write that
            // does not stamp it leaves the row at 0 forever, so it would lose to every
            // other device no matter how recently it was changed here.
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value,
                                            updated_at = excluded.updated_at",
            params![key, value, crate::db::now_ms()],
        )?;
        Ok(())
    })
}

/// Lets the UI tell the user why saving a password would fail, before they try.
#[tauri::command(async)]
pub fn vault_status(state: State<'_, AppState>) -> Result<crate::state::VaultStatus> {
    Ok(state.vault_status())
}

/// The tabs and layout to restore next launch.
///
/// Stored as opaque JSON: the shape is the frontend's `LayoutNode` tree, and the backend
/// has no reason to understand it.
#[tauri::command(async)]
pub fn get_session_state(state: State<'_, AppState>) -> Result<Option<String>> {
    let payload = state.db.read(|conn| {
        Ok(conn
            .query_row("SELECT payload FROM session_state WHERE id = 1", [], |row| {
                row.get::<_, String>(0)
            })
            .ok())
    })?;
    log::debug!(
        "get_session_state -> {}",
        payload.as_ref().map_or("nothing saved".to_string(), |p| format!("{} bytes", p.len()))
    );
    Ok(payload)
}

#[tauri::command(async)]
pub fn set_session_state(state: State<'_, AppState>, payload: String) -> Result<()> {
    log::debug!("set_session_state <- {} bytes", payload.len());
    state.db.write(|tx| {
        tx.execute(
            "INSERT INTO session_state (id, payload, updated_at) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET payload = excluded.payload,
                                           updated_at = excluded.updated_at",
            params![payload, crate::db::now_ms()],
        )?;
        Ok(())
    })
}
