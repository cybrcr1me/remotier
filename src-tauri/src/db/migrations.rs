//! Schema migrations, applied by stepping `PRAGMA user_version`.
//!
//! Append-only: never edit a migration that has shipped, add a new one.

use rusqlite::Connection;

use crate::error::Result;

const MIGRATIONS: &[&str] = &[
    // 1 - initial schema
    r#"
CREATE TABLE settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL
) STRICT;

-- Every secret in the app lives here, sealed with the keychain-backed DEK.
CREATE TABLE secrets (
    ref        TEXT PRIMARY KEY,
    nonce      BLOB NOT NULL,
    ciphertext BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE keys (
    id              TEXT PRIMARY KEY,
    label           TEXT NOT NULL,
    algorithm       TEXT NOT NULL,
    -- managed: private key sealed in `secrets`
    -- system_path: left on disk at `path`, never copied
    -- agent: held by the ssh-agent, we only know the public half
    source          TEXT NOT NULL CHECK (source IN ('managed', 'system_path', 'agent')),
    public_key      TEXT NOT NULL,
    fingerprint     TEXT NOT NULL,
    private_key_ref TEXT REFERENCES secrets(ref) ON DELETE SET NULL,
    path            TEXT,
    passphrase_ref  TEXT REFERENCES secrets(ref) ON DELETE SET NULL,
    comment         TEXT,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
) STRICT;

CREATE TABLE identities (
    id           TEXT PRIMARY KEY,
    label        TEXT NOT NULL,
    -- may contain {{placeholders}}, resolved at connect time
    username     TEXT NOT NULL,
    auth_kind    TEXT NOT NULL CHECK (auth_kind IN ('password', 'key', 'agent', 'interactive')),
    password_ref TEXT REFERENCES secrets(ref) ON DELETE SET NULL,
    key_id       TEXT REFERENCES keys(id) ON DELETE SET NULL,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
) STRICT;

-- Nested tree. NULL columns on a host mean "inherit from here".
CREATE TABLE groups (
    id                   TEXT PRIMARY KEY,
    parent_id            TEXT REFERENCES groups(id) ON DELETE CASCADE,
    name                 TEXT NOT NULL,
    sort                 INTEGER NOT NULL DEFAULT 0,
    default_port         INTEGER,
    default_identity_id  TEXT REFERENCES identities(id) ON DELETE SET NULL,
    default_jump_host_id TEXT,
    created_at           INTEGER NOT NULL,
    updated_at           INTEGER NOT NULL
) STRICT;

CREATE TABLE hosts (
    id           TEXT PRIMARY KEY,
    group_id     TEXT REFERENCES groups(id) ON DELETE SET NULL,
    label        TEXT NOT NULL,
    hostname     TEXT NOT NULL,
    port         INTEGER,
    identity_id  TEXT REFERENCES identities(id) ON DELETE SET NULL,
    jump_host_id TEXT REFERENCES hosts(id) ON DELETE SET NULL,
    color        TEXT,
    tags         TEXT NOT NULL DEFAULT '[]',
    sort         INTEGER NOT NULL DEFAULT 0,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
) STRICT;

-- Placeholder declarations. Shared/syncable: they describe what a group or host needs.
CREATE TABLE var_defs (
    id            TEXT PRIMARY KEY,
    scope         TEXT NOT NULL CHECK (scope IN ('group', 'host')),
    scope_id      TEXT NOT NULL,
    name          TEXT NOT NULL,
    label         TEXT,
    default_value TEXT,
    required      INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,
    UNIQUE (scope, scope_id, name)
) STRICT;

-- The per-user half of the placeholder feature. LOCAL ONLY - this table must never be
-- included in a sync payload, that is the whole point of splitting it from var_defs.
CREATE TABLE var_values (
    scope      TEXT NOT NULL CHECK (scope IN ('group', 'host')),
    scope_id   TEXT NOT NULL,
    name       TEXT NOT NULL,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (scope, scope_id, name)
) STRICT;

CREATE TABLE workspaces (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    layout_json TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
) STRICT;

-- Single row holding the tabs/layout to restore on next launch.
CREATE TABLE session_state (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    payload    TEXT NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE INDEX idx_groups_parent ON groups(parent_id);
CREATE INDEX idx_hosts_group ON hosts(group_id);
CREATE INDEX idx_hosts_identity ON hosts(identity_id);
CREATE INDEX idx_var_defs_scope ON var_defs(scope, scope_id);
"#,
    // 2 - credentials set directly on a host, for one-off servers that do not warrant
    // an identity of their own. NULL auth_kind means "use the inherited identity".
    r#"
ALTER TABLE hosts ADD COLUMN username TEXT;
ALTER TABLE hosts ADD COLUMN auth_kind TEXT
    CHECK (auth_kind IS NULL OR auth_kind IN ('password', 'key', 'agent', 'interactive'));
ALTER TABLE hosts ADD COLUMN password_ref TEXT REFERENCES secrets(ref) ON DELETE SET NULL;
ALTER TABLE hosts ADD COLUMN key_id TEXT REFERENCES keys(id) ON DELETE SET NULL;
"#,
    // 3 - a small amount of visual identity, so a wall of hosts is scannable. Both are
    // opaque keys resolved by the frontend, not colour values or asset paths.
    r#"
ALTER TABLE hosts ADD COLUMN icon TEXT;
ALTER TABLE groups ADD COLUMN icon TEXT;
ALTER TABLE groups ADD COLUMN color TEXT;
"#,
];

pub fn apply(conn: &mut Connection) -> Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let target = MIGRATIONS.len() as i64;

    for version in current..target {
        let tx = conn.transaction()?;
        tx.execute_batch(MIGRATIONS[version as usize])?;
        // PRAGMA does not accept bound parameters.
        tx.pragma_update(None, "user_version", version + 1)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_from_empty() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
    }

    #[test]
    fn is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();
        apply(&mut conn).unwrap();
        let tables: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'hosts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 1);
    }

    #[test]
    fn host_credentials_columns_are_added() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();

        let columns: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('hosts')")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();

        for column in ["username", "auth_kind", "password_ref", "key_id"] {
            assert!(columns.contains(&column.to_string()), "missing hosts.{column}");
        }
    }

    #[test]
    fn appearance_columns_are_added() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();

        let columns = |table: &str| -> Vec<String> {
            conn.prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
                .unwrap()
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap()
        };

        assert!(columns("hosts").contains(&"icon".to_string()));
        assert!(columns("groups").contains(&"icon".to_string()));
        assert!(columns("groups").contains(&"color".to_string()));
    }

    #[test]
    fn upgrades_an_existing_database_in_place() {
        let mut conn = Connection::open_in_memory().unwrap();
        // Stop at the first migration, as an older install would be.
        let tx = conn.transaction().unwrap();
        tx.execute_batch(MIGRATIONS[0]).unwrap();
        tx.pragma_update(None, "user_version", 1).unwrap();
        tx.commit().unwrap();

        conn.execute(
            "INSERT INTO hosts (id, label, hostname, tags, sort, created_at, updated_at)
             VALUES ('h1', 'old', 'old.example.com', '[]', 0, 0, 0)",
            [],
        )
        .unwrap();

        apply(&mut conn).unwrap();

        // The existing row survives the upgrade and gains the new columns as NULL.
        let username: Option<String> = conn
            .query_row("SELECT username FROM hosts WHERE id = 'h1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(username, None);
    }

    #[test]
    fn expected_tables_exist() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();
        for table in [
            "settings", "secrets", "keys", "identities", "groups", "hosts", "var_defs",
            "var_values", "workspaces", "session_state",
        ] {
            let found: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(found, 1, "missing table {table}");
        }
    }
}
