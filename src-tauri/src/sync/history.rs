//! What the last few cycles did, so the sync panel can show it.
//!
//! `sync_log` is local-only: no trigger, never collected. A synced history would have
//! every machine replay every other machine's activity as its own, and the point of the
//! list is to answer "what did *this* device just do".
//!
//! The label of a record is written into the row rather than joined out at read time.
//! Half the interesting entries are deletions, and after one there is nothing left to
//! join to - so a deleted host would read as a bare uuid, which is exactly the entry a
//! user most wants to understand.

use rusqlite::{Connection, OptionalExtension, Transaction};
use serde::Serialize;

use crate::db::{now_ms, query_all};
use crate::error::Result;

/// How many rows are kept. The panel shows 30; keeping more means one busy cycle does
/// not wipe out the run before it, and the table stays trivially small either way.
const KEEP: i64 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// This machine sent it.
    Push,
    /// This machine received it.
    Pull,
}

impl Direction {
    fn as_str(self) -> &'static str {
        match self {
            Direction::Push => "push",
            Direction::Pull => "pull",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Written,
    Deleted,
}

impl Action {
    fn as_str(self) -> &'static str {
        match self {
            Action::Written => "written",
            Action::Deleted => "deleted",
        }
    }
}

/// One record, as it went past.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub kind: &'static str,
    pub id: String,
    pub action: Action,
    /// The record's name at the time. `None` for a tombstone this machine pushed: the
    /// row was already gone before the cycle started.
    pub label: Option<String>,
}

/// A row as the UI reads it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub at: i64,
    pub direction: String,
    pub action: String,
    pub kind: String,
    pub record_id: String,
    pub label: Option<String>,
}

/// Append a cycle's worth of entries, then trim.
pub fn record(tx: &Transaction, direction: Direction, entries: &[Entry]) -> Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    let at = now_ms();
    for entry in entries {
        tx.execute(
            "INSERT INTO sync_log (at, direction, action, kind, record_id, label)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                at,
                direction.as_str(),
                entry.action.as_str(),
                entry.kind,
                entry.id,
                entry.label
            ],
        )?;
    }
    // Trimmed on write rather than on read: a table that only ever grows would carry
    // every record name this account has ever held, for no one to look at.
    tx.execute(
        "DELETE FROM sync_log WHERE id <= (
             SELECT max(id) FROM sync_log
         ) - ?1",
        [KEEP],
    )?;
    Ok(())
}

/// The newest entries first.
pub fn recent(conn: &Connection, limit: usize) -> Result<Vec<HistoryEntry>> {
    query_all(
        conn,
        "SELECT at, direction, action, kind, record_id, label
           FROM sync_log ORDER BY id DESC LIMIT ?1",
        [limit as i64],
        |row| {
            Ok(HistoryEntry {
                at: row.get(0)?,
                direction: row.get(1)?,
                action: row.get(2)?,
                kind: row.get(3)?,
                record_id: row.get(4)?,
                label: row.get(5)?,
            })
        },
    )
}

/// Dropped on sign-out, like the other devices' layouts: it is an account's activity,
/// and it names records a colleague shared rather than only this user's own.
pub fn clear(tx: &Transaction) -> Result<()> {
    tx.execute("DELETE FROM sync_log", [])?;
    Ok(())
}

/// What to call a record that is still here.
///
/// A setting is named by its key, which *is* its id - so the caller gets something
/// readable rather than a lookup that would return the value.
pub fn label_of(conn: &Connection, kind: &str, id: &str) -> Result<Option<String>> {
    let sql = match kind {
        "host" => "SELECT label FROM hosts WHERE id = ?1",
        "group" => "SELECT name FROM groups WHERE id = ?1",
        "identity" => "SELECT label FROM identities WHERE id = ?1",
        "var_def" => "SELECT name FROM var_defs WHERE id = ?1",
        "workspace" => "SELECT name FROM workspaces WHERE id = ?1",
        "setting" => return Ok(Some(id.to_string())),
        "device_layout" => "SELECT device_name FROM device_layouts WHERE device_id = ?1",
        _ => return Ok(None),
    };
    Ok(conn.query_row(sql, [id], |row| row.get(0)).optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        migrations::apply(&mut conn).unwrap();
        conn
    }

    fn entry(id: &str, action: Action) -> Entry {
        Entry {
            kind: "host",
            id: id.into(),
            action,
            label: Some(format!("host {id}")),
        }
    }

    #[test]
    fn newest_first() {
        let mut conn = db();
        let tx = conn.transaction().unwrap();
        record(&tx, Direction::Pull, &[entry("a", Action::Written)]).unwrap();
        record(&tx, Direction::Push, &[entry("b", Action::Deleted)]).unwrap();
        tx.commit().unwrap();

        let rows = recent(&conn, 30).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].record_id, "b");
        assert_eq!(rows[0].direction, "push");
        assert_eq!(rows[0].action, "deleted");
        assert_eq!(rows[1].record_id, "a");
    }

    #[test]
    fn the_table_is_trimmed() {
        let mut conn = db();
        let tx = conn.transaction().unwrap();
        let entries: Vec<Entry> = (0..KEEP + 50)
            .map(|i| entry(&i.to_string(), Action::Written))
            .collect();
        record(&tx, Direction::Pull, &entries).unwrap();
        tx.commit().unwrap();

        let count: i64 = conn
            .query_row("SELECT count(*) FROM sync_log", [], |r| r.get(0))
            .unwrap();
        assert!(count <= KEEP + 1, "kept {count} rows");
        // The newest survived, which is the half that matters.
        assert_eq!(recent(&conn, 1).unwrap()[0].record_id, (KEEP + 49).to_string());
    }

    #[test]
    fn a_setting_is_named_by_its_key() {
        let conn = db();
        assert_eq!(
            label_of(&conn, "setting", "ssh.agentSocket").unwrap(),
            Some("ssh.agentSocket".into())
        );
        // A record this build does not know, or one already deleted, has no name and
        // must not be an error - the entry is still worth showing.
        assert_eq!(label_of(&conn, "host", "gone").unwrap(), None);
        assert_eq!(label_of(&conn, "something_new", "x").unwrap(), None);
    }
}
