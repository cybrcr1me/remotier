pub mod migrations;
pub mod models;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, Params, Row};

use crate::error::{Error, Result};

/// The application database.
///
/// Commands that touch it are declared `#[tauri::command(async)]` so SQLite never runs
/// on the main thread. Nothing awaits while the guard is held.
pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut conn = Connection::open(path)?;
        Self::configure(&conn)?;
        migrations::apply(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// A configured, migrated database in memory.
    ///
    /// Public rather than `#[cfg(test)]` because the integration tests in `tests/` are a
    /// separate crate and cannot see a test-only item. It goes through `configure` and
    /// `apply` like any other database, so a test is never running against a schema or a
    /// set of pragmas the app does not use.
    pub fn open_in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        Self::configure(&conn)?;
        migrations::apply(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn configure(conn: &Connection) -> Result<()> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        // Without this, a delete performed by an ON DELETE CASCADE fires no delete
        // trigger, so deleting a group would wipe its whole subtree with no sync
        // tombstone for any of it - the rows would simply come back from the next pull.
        conn.pragma_update(None, "recursive_triggers", true)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(())
    }

    pub fn read<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        f(&conn)
    }

    /// Run `f` inside a transaction, committing when it returns `Ok`.
    pub fn write<T>(&self, f: impl FnOnce(&rusqlite::Transaction) -> Result<T>) -> Result<T> {
        let mut conn = self.conn.lock().expect("db mutex poisoned");
        let tx = conn.transaction()?;
        let out = f(&tx)?;
        tx.commit()?;
        Ok(out)
    }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn query_all<T, P: Params>(
    conn: &Connection,
    sql: &str,
    params: P,
    map: fn(&Row) -> Result<T>,
) -> Result<Vec<T>> {
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query(params)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(map(row)?);
    }
    Ok(out)
}

pub fn query_one<T, P: Params>(
    conn: &Connection,
    sql: &str,
    params: P,
    map: fn(&Row) -> Result<T>,
    what: &'static str,
    id: &str,
) -> Result<T> {
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query(params)?;
    match rows.next()? {
        Some(row) => map(row),
        None => Err(Error::NotFound(what, id.to_string())),
    }
}
