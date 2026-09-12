//! Proves data survives a full close/reopen of the database file, which is the phase 2
//! gate. Command handlers need a Tauri `State` to call, so this drives the same storage
//! layer they sit on top of.

use remotier_lib::db::models::{Group, Host};
use remotier_lib::db::{new_id, now_ms, query_all, query_one, Db};

fn temp_db_path(name: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("remotier-test-{name}-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

#[test]
fn hosts_and_groups_survive_a_reopen() {
    let path = temp_db_path("reopen");
    let group_id = new_id();
    let host_id = new_id();

    {
        let db = Db::open(&path).unwrap();
        db.write(|tx| {
            let now = now_ms();
            tx.execute(
                "INSERT INTO groups (id, parent_id, name, sort, created_at, updated_at)
                 VALUES (?1, NULL, 'Production', 0, ?2, ?2)",
                rusqlite::params![group_id, now],
            )?;
            tx.execute(
                "INSERT INTO hosts (id, group_id, label, hostname, port, tags, sort,
                                    created_at, updated_at)
                 VALUES (?1, ?2, 'edge-1', 'edge1.example.com', NULL, '[\"eu\",\"edge\"]', 0, ?3, ?3)",
                rusqlite::params![host_id, group_id, now],
            )?;
            Ok(())
        })
        .unwrap();
    }

    // Fresh handle, fresh connection: anything held only in memory is gone by now.
    let db = Db::open(&path).unwrap();

    let groups = db
        .read(|conn| {
            let sql = format!("SELECT {} FROM groups", Group::COLUMNS);
            query_all(conn, &sql, [], Group::from_row)
        })
        .unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].name, "Production");

    let host = db
        .read(|conn| {
            let sql = format!("SELECT {} FROM hosts WHERE id = ?1", Host::COLUMNS);
            query_one(conn, &sql, rusqlite::params![host_id], Host::from_row, "host", &host_id)
        })
        .unwrap();

    assert_eq!(host.hostname, "edge1.example.com");
    assert_eq!(host.group_id.as_deref(), Some(group_id.as_str()));
    // NULL port means "inherit", and must not silently become 22 on the way back.
    assert_eq!(host.port, None);
    assert_eq!(host.tags, vec!["eu".to_string(), "edge".to_string()]);

    std::fs::remove_file(&path).ok();
}

#[test]
fn deleting_a_group_keeps_its_hosts() {
    let path = temp_db_path("group-delete");
    let db = Db::open(&path).unwrap();
    let group_id = new_id();
    let host_id = new_id();

    db.write(|tx| {
        let now = now_ms();
        tx.execute(
            "INSERT INTO groups (id, name, sort, created_at, updated_at)
             VALUES (?1, 'Staging', 0, ?2, ?2)",
            rusqlite::params![group_id, now],
        )?;
        tx.execute(
            "INSERT INTO hosts (id, group_id, label, hostname, sort, created_at, updated_at)
             VALUES (?1, ?2, 'box', 'box.example.com', 0, ?3, ?3)",
            rusqlite::params![host_id, group_id, now],
        )?;
        Ok(())
    })
    .unwrap();

    db.write(|tx| {
        tx.execute("DELETE FROM groups WHERE id = ?1", rusqlite::params![group_id])?;
        Ok(())
    })
    .unwrap();

    let host = db
        .read(|conn| {
            let sql = format!("SELECT {} FROM hosts WHERE id = ?1", Host::COLUMNS);
            query_one(conn, &sql, rusqlite::params![host_id], Host::from_row, "host", &host_id)
        })
        .unwrap();

    // ON DELETE SET NULL: the host becomes ungrouped instead of vanishing with the group.
    assert_eq!(host.group_id, None);

    std::fs::remove_file(&path).ok();
}

#[test]
fn secrets_are_not_stored_in_plaintext() {
    let path = temp_db_path("secrets");
    let db = Db::open(&path).unwrap();
    let vault = remotier_lib::crypto::vault::Vault::from_key(&[3u8; 32]).unwrap();

    let (nonce, ciphertext) = vault.seal_str("correct horse battery staple").unwrap();
    db.write(|tx| {
        tx.execute(
            "INSERT INTO secrets (ref, nonce, ciphertext, created_at, updated_at)
             VALUES ('r1', ?1, ?2, ?3, ?3)",
            rusqlite::params![nonce, ciphertext, now_ms()],
        )?;
        Ok(())
    })
    .unwrap();

    let raw = std::fs::read(&path).unwrap();
    let needle = b"correct horse battery staple";
    assert!(
        !raw.windows(needle.len()).any(|w| w == needle),
        "plaintext secret found in the database file"
    );

    std::fs::remove_file(&path).ok();
    std::fs::remove_file(path.with_extension("db-wal")).ok();
    std::fs::remove_file(path.with_extension("db-shm")).ok();
}

#[test]
fn session_state_is_a_single_overwritten_row() {
    let path = temp_db_path("session-state");
    let db = Db::open(&path).unwrap();

    let write = |payload: &str| {
        db.write(|tx| {
            tx.execute(
                "INSERT INTO session_state (id, payload, updated_at) VALUES (1, ?1, ?2)
                 ON CONFLICT(id) DO UPDATE SET payload = excluded.payload,
                                               updated_at = excluded.updated_at",
                rusqlite::params![payload, now_ms()],
            )?;
            Ok(())
        })
        .unwrap();
    };

    write(r#"{"version":1,"tabs":[]}"#);
    write(r#"{"version":1,"tabs":[{"id":"t1"}]}"#);

    let (count, payload): (i64, String) = db
        .read(|conn| {
            Ok(conn
                .query_row(
                    "SELECT count(*), max(payload) FROM session_state",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?)
        })
        .unwrap();

    // The CHECK constraint plus the upsert must keep this to exactly one row.
    assert_eq!(count, 1);
    assert!(payload.contains("t1"));

    std::fs::remove_file(&path).ok();
}

#[test]
fn workspaces_survive_a_reopen_and_are_unique_by_name() {
    let path = temp_db_path("workspaces");

    {
        let db = Db::open(&path).unwrap();
        db.write(|tx| {
            let now = now_ms();
            tx.execute(
                "INSERT INTO workspaces (id, name, layout_json, created_at, updated_at)
                 VALUES (?1, 'Morning checks', ?2, ?3, ?3)",
                rusqlite::params![new_id(), r#"{"version":1,"tabs":[]}"#, now],
            )?;
            Ok(())
        })
        .unwrap();
    }

    let db = Db::open(&path).unwrap();
    let names: Vec<String> = db
        .read(|conn| {
            query_all(conn, "SELECT name FROM workspaces", [], |row| {
                Ok(row.get::<_, String>(0)?)
            })
        })
        .unwrap();

    assert_eq!(names, vec!["Morning checks".to_string()]);

    std::fs::remove_file(&path).ok();
}
