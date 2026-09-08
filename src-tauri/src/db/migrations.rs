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
    // 4 - account-level placeholders, so a value used by everything does not have to be
    // repeated on every group. The scope check has to be widened, and SQLite cannot alter
    // a CHECK constraint, so both tables are rebuilt. `scope_id` is empty for this scope:
    // there is one account, so there is nothing to point at.
    r#"
CREATE TABLE var_defs_new (
    id            TEXT PRIMARY KEY,
    scope         TEXT NOT NULL CHECK (scope IN ('global', 'group', 'host')),
    scope_id      TEXT NOT NULL,
    name          TEXT NOT NULL,
    label         TEXT,
    default_value TEXT,
    required      INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,
    UNIQUE (scope, scope_id, name)
) STRICT;
INSERT INTO var_defs_new SELECT * FROM var_defs;
DROP TABLE var_defs;
ALTER TABLE var_defs_new RENAME TO var_defs;

CREATE TABLE var_values_new (
    scope      TEXT NOT NULL CHECK (scope IN ('global', 'group', 'host')),
    scope_id   TEXT NOT NULL,
    name       TEXT NOT NULL,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (scope, scope_id, name)
) STRICT;
INSERT INTO var_values_new SELECT * FROM var_values;
DROP TABLE var_values;
ALTER TABLE var_values_new RENAME TO var_values;
"#,
    // 5 - sync bookkeeping. Deliberately outside the domain tables: no existing model or
    // command signature changes shape, and a `dirty` flag that has to be remembered at
    // every call site is a flag that will eventually be forgotten.
    //
    // The AFTER DELETE triggers are the reason this is done in SQL rather than in Rust.
    // Deleting a group cascades through `groups.parent_id` into an arbitrarily deep
    // subtree, and no call site can enumerate what vanished - but the trigger sees every
    // row go and writes a tombstone for each. This needs `PRAGMA recursive_triggers`,
    // which `Db::configure` sets; `cascade_delete_leaves_tombstones` is the test.
    //
    // `var_values` and `session_state` have no triggers, on purpose. They are local-only.
    r#"
CREATE TABLE sync_meta (
    kind       TEXT NOT NULL,
    id         TEXT NOT NULL,
    -- NULL until the server has accepted this record at least once.
    server_seq INTEGER,
    local_dirty INTEGER NOT NULL DEFAULT 1,
    -- Set when the domain row is gone. The sync_meta row outlives it, so the deletion
    -- can be told to other devices instead of the record simply reappearing from them.
    deleted_at INTEGER,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (kind, id)
) STRICT;

CREATE INDEX idx_sync_meta_dirty ON sync_meta(kind, id) WHERE local_dirty = 1;

-- One row, like session_state. Holds which account and instance this machine is signed
-- in to, and how far through the server's change log it has read.
CREATE TABLE sync_state (
    id            INTEGER PRIMARY KEY CHECK (id = 1),
    instance_url  TEXT,
    account_id    TEXT,
    account_email TEXT,
    -- This machine's identity in the LWW tie-break, and the key for its device layout.
    device_id     TEXT NOT NULL,
    -- Server sequence number, never a timestamp: the pull must not depend on clocks.
    cursor        INTEGER NOT NULL DEFAULT 0,
    last_sync_at  INTEGER
) STRICT;

-- Settings had no clock. Sync needs one per key to merge on. Existing rows get 0, which
-- loses to any real edit on either device - the right way round for a default.
ALTER TABLE settings ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;

CREATE TRIGGER sync_hosts_ai AFTER INSERT ON hosts BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, updated_at)
    VALUES ('host', NEW.id, 1, NEW.updated_at)
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1, deleted_at = NULL, updated_at = NEW.updated_at;
END;

CREATE TRIGGER sync_hosts_au AFTER UPDATE ON hosts BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, updated_at)
    VALUES ('host', NEW.id, 1, NEW.updated_at)
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1, deleted_at = NULL, updated_at = NEW.updated_at;
END;

CREATE TRIGGER sync_hosts_ad AFTER DELETE ON hosts BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, deleted_at, updated_at)
    VALUES ('host', OLD.id, 1, CAST((julianday('now') - 2440587.5) * 86400000.0 AS INTEGER), CAST((julianday('now') - 2440587.5) * 86400000.0 AS INTEGER))
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1,
        deleted_at  = excluded.deleted_at,
        updated_at  = excluded.updated_at;
END;

CREATE TRIGGER sync_groups_ai AFTER INSERT ON groups BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, updated_at)
    VALUES ('group', NEW.id, 1, NEW.updated_at)
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1, deleted_at = NULL, updated_at = NEW.updated_at;
END;

CREATE TRIGGER sync_groups_au AFTER UPDATE ON groups BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, updated_at)
    VALUES ('group', NEW.id, 1, NEW.updated_at)
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1, deleted_at = NULL, updated_at = NEW.updated_at;
END;

CREATE TRIGGER sync_groups_ad AFTER DELETE ON groups BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, deleted_at, updated_at)
    VALUES ('group', OLD.id, 1, CAST((julianday('now') - 2440587.5) * 86400000.0 AS INTEGER), CAST((julianday('now') - 2440587.5) * 86400000.0 AS INTEGER))
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1,
        deleted_at  = excluded.deleted_at,
        updated_at  = excluded.updated_at;
END;

CREATE TRIGGER sync_identities_ai AFTER INSERT ON identities BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, updated_at)
    VALUES ('identity', NEW.id, 1, NEW.updated_at)
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1, deleted_at = NULL, updated_at = NEW.updated_at;
END;

CREATE TRIGGER sync_identities_au AFTER UPDATE ON identities BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, updated_at)
    VALUES ('identity', NEW.id, 1, NEW.updated_at)
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1, deleted_at = NULL, updated_at = NEW.updated_at;
END;

CREATE TRIGGER sync_identities_ad AFTER DELETE ON identities BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, deleted_at, updated_at)
    VALUES ('identity', OLD.id, 1, CAST((julianday('now') - 2440587.5) * 86400000.0 AS INTEGER), CAST((julianday('now') - 2440587.5) * 86400000.0 AS INTEGER))
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1,
        deleted_at  = excluded.deleted_at,
        updated_at  = excluded.updated_at;
END;

CREATE TRIGGER sync_var_defs_ai AFTER INSERT ON var_defs BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, updated_at)
    VALUES ('var_def', NEW.id, 1, NEW.updated_at)
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1, deleted_at = NULL, updated_at = NEW.updated_at;
END;

CREATE TRIGGER sync_var_defs_au AFTER UPDATE ON var_defs BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, updated_at)
    VALUES ('var_def', NEW.id, 1, NEW.updated_at)
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1, deleted_at = NULL, updated_at = NEW.updated_at;
END;

CREATE TRIGGER sync_var_defs_ad AFTER DELETE ON var_defs BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, deleted_at, updated_at)
    VALUES ('var_def', OLD.id, 1, CAST((julianday('now') - 2440587.5) * 86400000.0 AS INTEGER), CAST((julianday('now') - 2440587.5) * 86400000.0 AS INTEGER))
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1,
        deleted_at  = excluded.deleted_at,
        updated_at  = excluded.updated_at;
END;

CREATE TRIGGER sync_workspaces_ai AFTER INSERT ON workspaces BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, updated_at)
    VALUES ('workspace', NEW.id, 1, NEW.updated_at)
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1, deleted_at = NULL, updated_at = NEW.updated_at;
END;

CREATE TRIGGER sync_workspaces_au AFTER UPDATE ON workspaces BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, updated_at)
    VALUES ('workspace', NEW.id, 1, NEW.updated_at)
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1, deleted_at = NULL, updated_at = NEW.updated_at;
END;

CREATE TRIGGER sync_workspaces_ad AFTER DELETE ON workspaces BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, deleted_at, updated_at)
    VALUES ('workspace', OLD.id, 1, CAST((julianday('now') - 2440587.5) * 86400000.0 AS INTEGER), CAST((julianday('now') - 2440587.5) * 86400000.0 AS INTEGER))
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1,
        deleted_at  = excluded.deleted_at,
        updated_at  = excluded.updated_at;
END;

CREATE TRIGGER sync_settings_ai AFTER INSERT ON settings BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, updated_at)
    VALUES ('setting', NEW.key, 1, NEW.updated_at)
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1, deleted_at = NULL, updated_at = NEW.updated_at;
END;

CREATE TRIGGER sync_settings_au AFTER UPDATE ON settings BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, updated_at)
    VALUES ('setting', NEW.key, 1, NEW.updated_at)
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1, deleted_at = NULL, updated_at = NEW.updated_at;
END;

CREATE TRIGGER sync_settings_ad AFTER DELETE ON settings BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, deleted_at, updated_at)
    VALUES ('setting', OLD.key, 1, CAST((julianday('now') - 2440587.5) * 86400000.0 AS INTEGER), CAST((julianday('now') - 2440587.5) * 86400000.0 AS INTEGER))
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1,
        deleted_at  = excluded.deleted_at,
        updated_at  = excluded.updated_at;
END;
"#,
    // 6 - group sharing. A shared group has a content key of its own, sealed here under
    // the local DEK like every other secret; the server holds only a copy wrapped to each
    // member's X25519 public key and cannot open either.
    //
    // Records inside a shared group are encrypted under this key instead of the personal
    // one, which is the whole mechanism: a colleague can read the group and nothing else.
    r#"
CREATE TABLE group_keys (
    group_id   TEXT PRIMARY KEY,
    key_ref    TEXT NOT NULL REFERENCES secrets(ref) ON DELETE CASCADE,
    -- The account that shared this group with us, or NULL when it is ours. A member
    -- cannot re-share or rotate, so the two cases behave differently.
    owner_id   TEXT,
    -- Bumped on rotation, for diagnosing a member left on an older key.
    generation INTEGER NOT NULL DEFAULT 1,
    updated_at INTEGER NOT NULL
) STRICT;

-- Which group a record belongs to, remembered independently of the record.
--
-- A tombstone is collected after the domain row is gone, so its group cannot be read from
-- the table any more - and a deletion labelled with the wrong key is filed under the
-- wrong account, where the members of the group never see it. The triggers below are
-- recreated to keep this column up to date; SQLite cannot alter a trigger in place.
ALTER TABLE sync_meta ADD COLUMN group_id TEXT;

UPDATE sync_meta
   SET group_id = (SELECT group_id FROM hosts WHERE hosts.id = sync_meta.id)
 WHERE kind = 'host';
UPDATE sync_meta SET group_id = id WHERE kind = 'group';
UPDATE sync_meta
   SET group_id = (SELECT scope_id FROM var_defs
                    WHERE var_defs.id = sync_meta.id AND var_defs.scope = 'group')
 WHERE kind = 'var_def';

DROP TRIGGER sync_hosts_ai;
DROP TRIGGER sync_hosts_au;
DROP TRIGGER sync_hosts_ad;

CREATE TRIGGER sync_hosts_ai AFTER INSERT ON hosts BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, updated_at, group_id)
    VALUES ('host', NEW.id, 1, NEW.updated_at, NEW.group_id)
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1, deleted_at = NULL, updated_at = NEW.updated_at,
        group_id = excluded.group_id;
END;

CREATE TRIGGER sync_hosts_au AFTER UPDATE ON hosts BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, updated_at, group_id)
    VALUES ('host', NEW.id, 1, NEW.updated_at, NEW.group_id)
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1, deleted_at = NULL, updated_at = NEW.updated_at,
        group_id = excluded.group_id;
END;

CREATE TRIGGER sync_hosts_ad AFTER DELETE ON hosts BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, deleted_at, updated_at, group_id)
    VALUES ('host', OLD.id, 1, CAST((julianday('now') - 2440587.5) * 86400000.0 AS INTEGER), CAST((julianday('now') - 2440587.5) * 86400000.0 AS INTEGER), OLD.group_id)
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1,
        deleted_at  = excluded.deleted_at,
        updated_at  = excluded.updated_at,
        group_id    = excluded.group_id;
END;

DROP TRIGGER sync_groups_ai;
DROP TRIGGER sync_groups_au;
DROP TRIGGER sync_groups_ad;

CREATE TRIGGER sync_groups_ai AFTER INSERT ON groups BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, updated_at, group_id)
    VALUES ('group', NEW.id, 1, NEW.updated_at, NEW.id)
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1, deleted_at = NULL, updated_at = NEW.updated_at,
        group_id = excluded.group_id;
END;

CREATE TRIGGER sync_groups_au AFTER UPDATE ON groups BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, updated_at, group_id)
    VALUES ('group', NEW.id, 1, NEW.updated_at, NEW.id)
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1, deleted_at = NULL, updated_at = NEW.updated_at,
        group_id = excluded.group_id;
END;

CREATE TRIGGER sync_groups_ad AFTER DELETE ON groups BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, deleted_at, updated_at, group_id)
    VALUES ('group', OLD.id, 1, CAST((julianday('now') - 2440587.5) * 86400000.0 AS INTEGER), CAST((julianday('now') - 2440587.5) * 86400000.0 AS INTEGER), OLD.id)
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1,
        deleted_at  = excluded.deleted_at,
        updated_at  = excluded.updated_at,
        group_id    = excluded.group_id;
END;

DROP TRIGGER sync_var_defs_ai;
DROP TRIGGER sync_var_defs_au;
DROP TRIGGER sync_var_defs_ad;

CREATE TRIGGER sync_var_defs_ai AFTER INSERT ON var_defs BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, updated_at, group_id)
    VALUES ('var_def', NEW.id, 1, NEW.updated_at, CASE WHEN NEW.scope = 'group' THEN NEW.scope_id END)
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1, deleted_at = NULL, updated_at = NEW.updated_at,
        group_id = excluded.group_id;
END;

CREATE TRIGGER sync_var_defs_au AFTER UPDATE ON var_defs BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, updated_at, group_id)
    VALUES ('var_def', NEW.id, 1, NEW.updated_at, CASE WHEN NEW.scope = 'group' THEN NEW.scope_id END)
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1, deleted_at = NULL, updated_at = NEW.updated_at,
        group_id = excluded.group_id;
END;

CREATE TRIGGER sync_var_defs_ad AFTER DELETE ON var_defs BEGIN
    INSERT INTO sync_meta (kind, id, local_dirty, deleted_at, updated_at, group_id)
    VALUES ('var_def', OLD.id, 1, CAST((julianday('now') - 2440587.5) * 86400000.0 AS INTEGER), CAST((julianday('now') - 2440587.5) * 86400000.0 AS INTEGER), CASE WHEN OLD.scope = 'group' THEN OLD.scope_id END)
    ON CONFLICT(kind, id) DO UPDATE SET
        local_dirty = 1,
        deleted_at  = excluded.deleted_at,
        updated_at  = excluded.updated_at,
        group_id    = excluded.group_id;
END;
"#,
    // 7 - other machines' tab layouts.
    //
    // A layout is offered, never applied: dropping another device's tabs into this one
    // would replace what the user is looking at. So they are kept here rather than in
    // `session_state`, which is this machine's own and is restored on launch.
    //
    // Local-only and deliberately trigger-free. The row is written by `apply` from a
    // record the server already has; a trigger would push it straight back, and the
    // outgoing copy is built on demand from `session_state` instead.
    r#"
CREATE TABLE device_layouts (
    device_id   TEXT PRIMARY KEY,
    device_name TEXT NOT NULL,
    layout_json TEXT NOT NULL,
    updated_at  INTEGER NOT NULL
) STRICT;

-- What this machine calls itself, so the other devices can name it in their list. Comes
-- from the OS at sign-in; the webview can ask for it and Rust has no dependency that can.
ALTER TABLE sync_state ADD COLUMN device_name TEXT;

-- `session_state` has no trigger - a layout written every 400ms would otherwise become a
-- push every 400ms - so the clock of the last one sent is remembered here instead, and
-- the layout is only pushed when it has actually moved on.
ALTER TABLE sync_state ADD COLUMN layout_pushed_at INTEGER NOT NULL DEFAULT 0;
"#,
    // 8 - the account keys and the session, sealed into `secrets` under the local DEK.
    //
    // Keeping them means sync resumes silently after a restart instead of demanding the
    // account password on every launch. That is the same protection this database already
    // gives SSH private keys and host passwords, so asking for a password to sync while
    // not asking for one to use a stored key would be theatre. No vault, no sync - the
    // same way every other secret-touching feature degrades.
    //
    // These belong with `sync_state` in migration 5 and are not there because that
    // migration had already run - on a developer machine, not in a release, but the
    // effect is the same. Editing it added columns to every *new* database and to no
    // existing one, and the app then failed at startup with "no such column:
    // account_public". Append-only is not a style preference.
    //
    // SQLite cannot add a column with a REFERENCES clause to an existing table, so these
    // carry no foreign key. `secrets` rows are only ever reached through these columns and
    // are deleted explicitly on sign-out, which is what the constraint would have bought.
    r#"
ALTER TABLE sync_state ADD COLUMN account_public TEXT;
ALTER TABLE sync_state ADD COLUMN personal_key_ref TEXT;
ALTER TABLE sync_state ADD COLUMN account_secret_ref TEXT;
ALTER TABLE sync_state ADD COLUMN access_token_ref TEXT;
ALTER TABLE sync_state ADD COLUMN refresh_token_ref TEXT;
"#,
];pub fn apply(conn: &mut Connection) -> Result<()> {
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

    /// A connection configured the way `Db::open` configures one. The tests above build a
    /// bare in-memory database on purpose; the trigger tests cannot, because cascades and
    /// the triggers they fire are both pragma-gated.
    fn configured() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        conn.pragma_update(None, "recursive_triggers", true).unwrap();
        apply(&mut conn).unwrap();
        conn
    }

    fn insert_group(conn: &Connection, id: &str, parent: Option<&str>) {
        conn.execute(
            "INSERT INTO groups (id, parent_id, name, sort, created_at, updated_at)
             VALUES (?1, ?2, ?1, 0, 100, 100)",
            rusqlite::params![id, parent],
        )
        .unwrap();
    }

    fn insert_host(conn: &Connection, id: &str, group: Option<&str>) {
        conn.execute(
            "INSERT INTO hosts (id, group_id, label, hostname, tags, sort, created_at, updated_at)
             VALUES (?1, ?2, ?1, 'example.test', '[]', 0, 100, 100)",
            rusqlite::params![id, group],
        )
        .unwrap();
    }

    fn meta(conn: &Connection, kind: &str, id: &str) -> Option<(i64, Option<i64>)> {
        conn.query_row(
            "SELECT local_dirty, deleted_at FROM sync_meta WHERE kind = ?1 AND id = ?2",
            rusqlite::params![kind, id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok()
    }

    #[test]
    fn settings_survive_the_clock_column() {
        let mut conn = Connection::open_in_memory().unwrap();
        for (version, sql) in MIGRATIONS.iter().enumerate().take(4) {
            conn.execute_batch(sql).unwrap();
            conn.pragma_update(None, "user_version", version as i64 + 1)
                .unwrap();
        }
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('ssh.agentSocket', '/tmp/agent.sock')",
            [],
        )
        .unwrap();

        apply(&mut conn).unwrap();

        let (value, updated_at): (String, i64) = conn
            .query_row(
                "SELECT value, updated_at FROM settings WHERE key = 'ssh.agentSocket'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(value, "/tmp/agent.sock");
        // A setting that predates sync has no clock, so it must lose to any real edit.
        assert_eq!(updated_at, 0);
    }

    #[test]
    fn writes_mark_records_dirty() {
        let conn = configured();
        insert_host(&conn, "h1", None);
        assert_eq!(meta(&conn, "host", "h1"), Some((1, None)));

        conn.execute(
            "UPDATE sync_meta SET local_dirty = 0, server_seq = 7 WHERE kind = 'host'",
            [],
        )
        .unwrap();
        assert_eq!(meta(&conn, "host", "h1"), Some((0, None)));

        conn.execute(
            "UPDATE hosts SET label = 'renamed', updated_at = 200 WHERE id = 'h1'",
            [],
        )
        .unwrap();
        assert_eq!(meta(&conn, "host", "h1"), Some((1, None)));

        let seq: Option<i64> = conn
            .query_row(
                "SELECT server_seq FROM sync_meta WHERE kind = 'host' AND id = 'h1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        // Dirty again but the server still knows it, so the push is an update, not a
        // create.
        assert_eq!(seq, Some(7));
    }

    #[test]
    fn cascade_delete_leaves_tombstones() {
        // Deleting a group cascades through groups.parent_id into the whole subtree and
        // sets hosts.group_id to NULL. No call site can enumerate what went; the triggers
        // can. Without a tombstone for each, the next pull would resurrect the lot.
        let conn = configured();
        insert_group(&conn, "root", None);
        insert_group(&conn, "child", Some("root"));
        insert_group(&conn, "grandchild", Some("child"));
        insert_host(&conn, "h1", Some("grandchild"));

        conn.execute("DELETE FROM sync_meta", []).unwrap();
        conn.execute("DELETE FROM groups WHERE id = 'root'", [])
            .unwrap();

        for id in ["root", "child", "grandchild"] {
            let (dirty, deleted) = meta(&conn, "group", id)
                .unwrap_or_else(|| panic!("no tombstone for the {id} group"));
            assert_eq!(dirty, 1);
            assert!(deleted.is_some(), "{id} has no deleted_at");
        }

        // The host survives - ON DELETE SET NULL - but it changed, so it is dirty and
        // must NOT be tombstoned.
        assert_eq!(meta(&conn, "host", "h1"), Some((1, None)));
    }

    #[test]
    fn a_record_that_comes_back_clears_its_tombstone() {
        let conn = configured();
        insert_host(&conn, "h1", None);
        conn.execute("DELETE FROM hosts WHERE id = 'h1'", []).unwrap();
        assert!(meta(&conn, "host", "h1").unwrap().1.is_some());

        insert_host(&conn, "h1", None);
        assert_eq!(meta(&conn, "host", "h1"), Some((1, None)));
    }

    #[test]
    fn local_only_tables_are_never_tracked() {
        let conn = configured();
        conn.execute(
            "INSERT INTO var_values (scope, scope_id, name, value, updated_at)
             VALUES ('global', '', 'company', 'acme', 100)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session_state (id, payload, updated_at) VALUES (1, '{}', 100)",
            [],
        )
        .unwrap();

        let tracked: i64 = conn
            .query_row("SELECT count(*) FROM sync_meta", [], |r| r.get(0))
            .unwrap();
        assert_eq!(tracked, 0);

        let triggers: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'trigger'
                   AND tbl_name IN ('var_values', 'session_state', 'secrets', 'keys')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        // keys and secrets are excluded too: no secret material syncs.
        assert_eq!(triggers, 0);
    }

    #[test]
    fn a_records_group_is_remembered_for_its_tombstone() {
        // The domain row is gone by the time a deletion is collected, so the group has to
        // have been recorded when the record still existed. A deletion labelled with the
        // wrong key is filed under the wrong account, where the group's members never
        // see it.
        let conn = configured();
        insert_group(&conn, "g1", None);
        insert_host(&conn, "h1", Some("g1"));

        let group: Option<String> = conn
            .query_row(
                "SELECT group_id FROM sync_meta WHERE kind = 'host' AND id = 'h1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(group.as_deref(), Some("g1"));

        conn.execute("DELETE FROM hosts WHERE id = 'h1'", []).unwrap();

        let after: Option<String> = conn
            .query_row(
                "SELECT group_id FROM sync_meta WHERE kind = 'host' AND id = 'h1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(after.as_deref(), Some("g1"), "the group survives the row");
    }

    #[test]
    fn an_existing_database_gains_the_group_column_with_its_values() {
        // Migration 6 backfills, so a database that synced before sharing existed does
        // not have to re-push everything to learn where its records live.
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        conn.pragma_update(None, "recursive_triggers", true).unwrap();
        for (version, sql) in MIGRATIONS.iter().enumerate().take(5) {
            conn.execute_batch(sql).unwrap();
            conn.pragma_update(None, "user_version", version as i64 + 1)
                .unwrap();
        }
        insert_group(&conn, "g1", None);
        insert_host(&conn, "h1", Some("g1"));

        apply(&mut conn).unwrap();

        let group: Option<String> = conn
            .query_row(
                "SELECT group_id FROM sync_meta WHERE kind = 'host' AND id = 'h1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(group.as_deref(), Some("g1"));
    }

    /// The column list of every table, as a comparable string.
    fn schema(conn: &Connection) -> Vec<(String, String)> {
        let mut tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .filter_map(|row| row.ok())
            .collect();
        tables.sort();

        tables
            .into_iter()
            .map(|table| {
                let mut stmt = conn
                    .prepare(&format!("PRAGMA table_info({table})"))
                    .unwrap();
                let mut columns: Vec<String> = stmt
                    .query_map([], |r| r.get::<_, String>(1))
                    .unwrap()
                    .filter_map(|row| row.ok())
                    .collect();
                columns.sort();
                (table, columns.join(","))
            })
            .collect()
    }

    #[test]
    fn an_upgraded_database_matches_a_fresh_one() {
        // Editing a migration that has already run adds its columns to every new database
        // and to no existing one, and the two silently diverge - which is how the app came
        // to fail at startup with "no such column: account_public" on a developer machine
        // whose database was already at the current version.
        //
        // This walks a database up one migration at a time, as a real upgrade does, and
        // compares it against one created in a single pass.
        let mut stepped = Connection::open_in_memory().unwrap();
        stepped.pragma_update(None, "foreign_keys", true).unwrap();
        stepped
            .pragma_update(None, "recursive_triggers", true)
            .unwrap();
        for (version, sql) in MIGRATIONS.iter().enumerate() {
            let tx = stepped.transaction().unwrap();
            tx.execute_batch(sql).unwrap();
            tx.pragma_update(None, "user_version", version as i64 + 1)
                .unwrap();
            tx.commit().unwrap();
        }

        let fresh = configured();

        assert_eq!(
            schema(&stepped),
            schema(&fresh),
            "a database upgraded step by step has drifted from a fresh one - a migration \
             that already ran was edited instead of a new one being added"
        );
    }

    #[test]
    fn a_database_stopped_at_any_version_upgrades_to_the_current_schema() {
        // Every version is a version somebody's database is actually sitting at.
        let fresh = schema(&configured());

        for stop in 1..=MIGRATIONS.len() {
            let mut conn = Connection::open_in_memory().unwrap();
            conn.pragma_update(None, "foreign_keys", true).unwrap();
            conn.pragma_update(None, "recursive_triggers", true).unwrap();

            for (version, sql) in MIGRATIONS.iter().enumerate().take(stop) {
                conn.execute_batch(sql).unwrap();
                conn.pragma_update(None, "user_version", version as i64 + 1)
                    .unwrap();
            }

            apply(&mut conn).unwrap();
            assert_eq!(
                schema(&conn),
                fresh,
                "a database left at version {stop} does not reach the current schema"
            );
        }
    }

    /// Fingerprints of every migration that has been applied to a real database.
    ///
    /// This is the guard the two tests above cannot be: they walk `MIGRATIONS` as it
    /// stands, so an edit to an already-applied migration looks consistent to them and is
    /// invisible. Only a value recorded *outside* the array notices.
    ///
    /// **If this test fails, do not update the list.** Put the change in a new migration.
    /// A migration that has run somewhere is finished, and editing one adds its change to
    /// every new database and to no existing one - which is how the app came to fail at
    /// startup with "no such column: account_public".
    ///
    /// A new migration appended to the end is fine; add its hash here when it ships.
    const APPLIED: &[&str] = &[
        "1a0aa6fe3a61d8e3b32f01b51e86026ffcce2fd422deaeb8f694f6edc223aaf6",  // 1
        "2d4fada17534bcf1e7baae824f0e3dd303988124cbdbcca4eb71452b58005606",  // 2
        "d95a383da9fd4c03162f595d86a6719070ca938c001d0eaa2a2f313c06d8d44b",  // 3
        "83201559cfeaf8f0036b6441cf350eb8a4f27e1ff43174a938700ab7dfc6e7cd",  // 4
        "22dbe8a9b33096271a296249a12a5ae2209f76ae55abc75ddf28a7da20c105a0",  // 5
        "ed4ae499ea8f4d148fee02f882d4dd4cb94ee1a418c475aa659d83ae19ff5c99",  // 6
        "48e5a9223a756b84283ef4f4945d3725ff042889809a0314c76d9ab38d8c0f1c",  // 7
        "25fc38322ff7817e92be9a6a7c2c97a4af67a811360984cc298303ec550e8920",  // 8
    ];

    fn fingerprint(sql: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(sql.as_bytes());
        hex::encode(hasher.finalize())
    }

    #[test]
    fn a_migration_that_has_run_is_never_edited() {
        for (index, sql) in MIGRATIONS.iter().enumerate() {
            let Some(expected) = APPLIED.get(index) else {
                // Newer than the recorded set: not yet pinned, nothing to check.
                continue;
            };
            let actual = fingerprint(sql);
            assert_eq!(
                &actual.as_str(),
                expected,
                "migration {} was edited after it had already been applied.\n\
                 Add a new migration instead - editing this one changes every new \n\
                 database and no existing one.",
                index + 1
            );
        }

        assert!(
            MIGRATIONS.len() >= APPLIED.len(),
            "a migration was removed; databases that ran it cannot be downgraded"
        );
    }
}
