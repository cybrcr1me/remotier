//! Pulled envelopes into local rows.
//!
//! Two rules run through all of it:
//!
//! **The whole pull is applied in one transaction with foreign keys deferred.** A page
//! boundary can put a host before the group it belongs to, and a child group before its
//! parent. Deferring the checks to commit lets the batch settle in any order; anything
//! still dangling at commit is a genuine inconsistency and the whole apply is rolled back
//! rather than half-written.
//!
//! **A reference to something this device does not have becomes NULL, not a failure.** A
//! host may point at an identity inside a group that was never shared with this account,
//! and refusing the host over it would leave the user with nothing.

use remotier_sync_proto::crypto;
use remotier_sync_proto::merge::{resolve, Clock, Resolution};
use remotier_sync_proto::record::{
    DeviceLayoutPayload, Envelope, GroupPayload, HostPayload, IdentityPayload, RecordKind,
    SettingPayload, VarDefPayload, WorkspacePayload,
};
use rusqlite::{OptionalExtension, Transaction};

use crate::error::Result;

use super::groups::Keyring;
use super::settings::is_syncable;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Applied {
    pub written: usize,
    pub deleted: usize,
    /// Records this build could not read: an unknown kind, a payload that would not
    /// open, or a group key this machine does not hold. Counted rather than thrown, so
    /// one unreadable record does not lose the batch.
    pub skipped: usize,
}

/// Apply a whole pull.
///
/// Ordering matters even with deferred foreign keys, because the "does the target exist"
/// checks below run as rows are written: groups and identities first means a host in the
/// same batch can see them.
pub fn apply(
    tx: &Transaction,
    keys: &Keyring,
    device_id: &str,
    batch: &[Envelope],
) -> Result<Applied> {
    tx.execute_batch("PRAGMA defer_foreign_keys = ON")?;

    let mut applied = Applied::default();

    for kind in [
        RecordKind::Group,
        RecordKind::Identity,
        RecordKind::Host,
        RecordKind::VarDef,
        RecordKind::Workspace,
        RecordKind::Setting,
        RecordKind::DeviceLayout,
    ] {
        for envelope in batch.iter().filter(|e| e.kind == kind) {
            match one(tx, keys, device_id, envelope) {
                Ok(Outcome::Written) => applied.written += 1,
                Ok(Outcome::Deleted) => applied.deleted += 1,
                Ok(Outcome::Skipped) => applied.skipped += 1,
                Err(e) => {
                    // A record that will not open is almost always a key mismatch on one
                    // record, not a broken batch. Log it and keep the rest.
                    log::warn!(
                        "sync: skipping {} {}: {e}",
                        envelope.kind.as_str(),
                        envelope.id
                    );
                    applied.skipped += 1;
                }
            }
        }
    }

    Ok(applied)
}

enum Outcome {
    Written,
    Deleted,
    Skipped,
}

fn one(
    tx: &Transaction,
    keys: &Keyring,
    device_id: &str,
    envelope: &Envelope,
) -> Result<Outcome> {
    // A layout is stored, never applied. It is another machine's open tabs; dropping it
    // into this session would replace what the user is looking at, so it waits in
    // `device_layouts` until they ask for it.
    if envelope.kind == RecordKind::DeviceLayout {
        record_meta(tx, envelope)?;
        // Our own layout coming back from the server. `session_state` is already the
        // authority for this machine, and taking the round trip as an update would
        // overwrite live tabs with whatever was pushed last.
        if envelope.id == device_id {
            return Ok(Outcome::Skipped);
        }
        if envelope.deleted_at.is_some() {
            tx.execute("DELETE FROM device_layouts WHERE device_id = ?1", [&envelope.id])?;
            return Ok(Outcome::Deleted);
        }

        let Some(key) = keys.open_with(&envelope.key_ref) else {
            return Ok(Outcome::Skipped);
        };
        let payload: DeviceLayoutPayload =
            crypto::open_json(key, &envelope.aad(), &envelope.nonce, &envelope.ciphertext)?;

        tx.execute(
            "INSERT INTO device_layouts (device_id, device_name, layout_json, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(device_id) DO UPDATE SET
                 device_name = excluded.device_name, layout_json = excluded.layout_json,
                 updated_at = excluded.updated_at",
            rusqlite::params![
                envelope.id,
                payload.device_name,
                payload.layout_json,
                envelope.updated_at
            ],
        )?;
        return Ok(Outcome::Written);
    }

    if !wins(tx, device_id, envelope)? {
        // The local copy is newer. It stays dirty and will be pushed; nothing to do but
        // note how far the log has been read.
        return Ok(Outcome::Skipped);
    }

    if envelope.deleted_at.is_some() {
        delete_row(tx, envelope)?;
        record_meta(tx, envelope)?;
        return Ok(Outcome::Deleted);
    }

    // A record sealed under a group key this machine does not hold: a share that was
    // revoked, or a rotation not fetched yet. Skipping leaves the local copy alone rather
    // than replacing it with something unreadable.
    let Some(key) = keys.open_with(&envelope.key_ref) else {
        log::debug!(
            "sync: no key for {} {} yet",
            envelope.kind.as_str(),
            envelope.id
        );
        return Ok(Outcome::Skipped);
    };

    let aad = envelope.aad();
    match envelope.kind {
        RecordKind::Host => {
            let p: HostPayload =
                crypto::open_json(key, &aad, &envelope.nonce, &envelope.ciphertext)?;
            write_host(tx, envelope, &p)?;
        }
        RecordKind::Group => {
            let p: GroupPayload =
                crypto::open_json(key, &aad, &envelope.nonce, &envelope.ciphertext)?;
            write_group(tx, envelope, &p)?;
        }
        RecordKind::Identity => {
            let p: IdentityPayload =
                crypto::open_json(key, &aad, &envelope.nonce, &envelope.ciphertext)?;
            write_identity(tx, envelope, &p)?;
        }
        RecordKind::VarDef => {
            let p: VarDefPayload =
                crypto::open_json(key, &aad, &envelope.nonce, &envelope.ciphertext)?;
            write_var_def(tx, envelope, &p)?;
        }
        RecordKind::Workspace => {
            let p: WorkspacePayload =
                crypto::open_json(key, &aad, &envelope.nonce, &envelope.ciphertext)?;
            write_workspace(tx, envelope, &p)?;
        }
        RecordKind::Setting => {
            let p: SettingPayload =
                crypto::open_json(key, &aad, &envelope.nonce, &envelope.ciphertext)?;
            if !is_syncable(&p.key) {
                // A newer build sent a setting this one keeps local. Refusing it is the
                // point of the allow-list.
                record_meta(tx, envelope)?;
                return Ok(Outcome::Skipped);
            }
            tx.execute(
                "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value,
                                                updated_at = excluded.updated_at",
                rusqlite::params![p.key, p.value, envelope.updated_at],
            )?;
        }
        RecordKind::DeviceLayout => unreachable!("handled above"),
    }

    record_meta(tx, envelope)?;
    Ok(Outcome::Written)
}

/// Does the incoming record beat what is here?
///
/// Compares against `sync_meta`, not the domain row, because a tombstone has no domain
/// row - and an older edit arriving after a delete must not resurrect it.
///
/// The local side of the tie-break is always *this* device: `sync_meta` records when a
/// row last changed here, and a row only changes here because this machine changed it.
/// (A pulled record is stamped with the sender's clock but is not dirty, so it is not
/// competing.) That is why `sync_meta` needs no `device_id` column.
fn wins(tx: &Transaction, device_id: &str, envelope: &Envelope) -> Result<bool> {
    let local: Option<i64> = tx
        .query_row(
            "SELECT updated_at FROM sync_meta WHERE kind = ?1 AND id = ?2",
            rusqlite::params![envelope.kind.as_str(), envelope.id],
            |row| row.get(0),
        )
        .optional()?;

    let Some(updated_at) = local else {
        // Never seen it. Nothing to lose.
        return Ok(true);
    };

    Ok(resolve(
        Clock {
            updated_at,
            device_id,
        },
        Clock {
            updated_at: envelope.updated_at,
            device_id: &envelope.device_id,
        },
    ) == Resolution::TakeRemote)
}

/// Note that this record is at the server's version, and is not pending a push.
fn record_meta(tx: &Transaction, envelope: &Envelope) -> Result<()> {
    // Runs *after* the domain write, so it overwrites the dirty flag the table's own
    // trigger just set. Without this every pulled record would be pushed straight back.
    tx.execute(
        "INSERT INTO sync_meta (kind, id, server_seq, local_dirty, deleted_at, updated_at)
         VALUES (?1, ?2, ?3, 0, ?4, ?5)
         ON CONFLICT(kind, id) DO UPDATE SET
             server_seq = excluded.server_seq, local_dirty = 0,
             deleted_at = excluded.deleted_at, updated_at = excluded.updated_at",
        rusqlite::params![
            envelope.kind.as_str(),
            envelope.id,
            envelope.seq,
            envelope.deleted_at,
            envelope.updated_at
        ],
    )?;
    Ok(())
}

fn delete_row(tx: &Transaction, envelope: &Envelope) -> Result<()> {
    let sql = match envelope.kind {
        RecordKind::Host => "DELETE FROM hosts WHERE id = ?1",
        RecordKind::Group => "DELETE FROM groups WHERE id = ?1",
        RecordKind::Identity => "DELETE FROM identities WHERE id = ?1",
        RecordKind::VarDef => "DELETE FROM var_defs WHERE id = ?1",
        RecordKind::Workspace => "DELETE FROM workspaces WHERE id = ?1",
        RecordKind::Setting => "DELETE FROM settings WHERE key = ?1",
        RecordKind::DeviceLayout => return Ok(()),
    };
    tx.execute(sql, [&envelope.id])?;
    Ok(())
}

/// `Some(id)` if that row exists here, `None` otherwise - the caller stores NULL.
fn existing(tx: &Transaction, table: &str, id: Option<&String>) -> Result<Option<String>> {
    let Some(id) = id else { return Ok(None) };
    let sql = format!("SELECT id FROM {table} WHERE id = ?1");
    Ok(tx.query_row(&sql, [id], |row| row.get(0)).optional()?)
}

/// Resolve a synced key fingerprint against this machine's keychain.
///
/// Key rows never sync, and `import_key` mints a fresh uuid wherever it runs, so the
/// fingerprint is the only name a key has that means the same thing on two machines.
fn key_id_for(tx: &Transaction, fingerprint: Option<&String>) -> Result<Option<String>> {
    let Some(fingerprint) = fingerprint else {
        return Ok(None);
    };
    Ok(tx
        .query_row(
            "SELECT id FROM keys WHERE fingerprint = ?1",
            [fingerprint],
            |row| row.get(0),
        )
        .optional()?)
}

fn write_host(tx: &Transaction, envelope: &Envelope, p: &HostPayload) -> Result<()> {
    let group_id = existing(tx, "groups", p.group_id.as_ref())?;
    let identity_id = existing(tx, "identities", p.identity_id.as_ref())?;
    let jump_host_id = existing(tx, "hosts", p.jump_host_id.as_ref())?;
    let key_id = key_id_for(tx, p.key_fingerprint.as_ref())?;
    let tags = serde_json::to_string(&p.tags)?;

    tx.execute(
        "INSERT INTO hosts (id, group_id, label, hostname, port, identity_id, jump_host_id,
                            color, icon, tags, sort, username, auth_kind, key_id,
                            created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
         ON CONFLICT(id) DO UPDATE SET
             group_id = excluded.group_id, label = excluded.label,
             hostname = excluded.hostname, port = excluded.port,
             identity_id = excluded.identity_id, jump_host_id = excluded.jump_host_id,
             color = excluded.color, icon = excluded.icon, tags = excluded.tags,
             sort = excluded.sort, username = excluded.username,
             auth_kind = excluded.auth_kind, key_id = excluded.key_id,
             updated_at = excluded.updated_at",
        rusqlite::params![
            envelope.id,
            group_id,
            p.label,
            p.hostname,
            p.port,
            identity_id,
            jump_host_id,
            p.color,
            p.icon,
            tags,
            p.sort,
            p.username,
            p.auth_kind,
            key_id,
            // `created_at` from the payload on insert; the ON CONFLICT arm leaves the
            // local one alone, so a record does not get younger every time it syncs.
            if p.created_at == 0 {
                envelope.updated_at
            } else {
                p.created_at
            },
            envelope.updated_at,
        ],
    )?;
    Ok(())
}

fn write_group(tx: &Transaction, envelope: &Envelope, p: &GroupPayload) -> Result<()> {
    // A group cannot be its own parent. A batch that says otherwise is corrupt, and
    // `buildTree` in the frontend survives a cycle by dropping the group - which makes it
    // vanish rather than error.
    let parent_id = match p.parent_id.as_ref() {
        Some(id) if *id == envelope.id => None,
        other => existing(tx, "groups", other)?,
    };
    let default_identity_id = existing(tx, "identities", p.default_identity_id.as_ref())?;
    let default_jump_host_id = existing(tx, "hosts", p.default_jump_host_id.as_ref())?;

    tx.execute(
        "INSERT INTO groups (id, parent_id, name, sort, default_port, default_identity_id,
                             default_jump_host_id, icon, color, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(id) DO UPDATE SET
             parent_id = excluded.parent_id, name = excluded.name, sort = excluded.sort,
             default_port = excluded.default_port,
             default_identity_id = excluded.default_identity_id,
             default_jump_host_id = excluded.default_jump_host_id,
             icon = excluded.icon, color = excluded.color, updated_at = excluded.updated_at",
        rusqlite::params![
            envelope.id,
            parent_id,
            p.name,
            p.sort,
            p.default_port,
            default_identity_id,
            default_jump_host_id,
            p.icon,
            p.color,
            if p.created_at == 0 { envelope.updated_at } else { p.created_at },
            envelope.updated_at,
        ],
    )?;
    Ok(())
}

fn write_identity(tx: &Transaction, envelope: &Envelope, p: &IdentityPayload) -> Result<()> {
    let key_id = key_id_for(tx, p.key_fingerprint.as_ref())?;

    tx.execute(
        "INSERT INTO identities (id, label, username, auth_kind, key_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET
             label = excluded.label, username = excluded.username,
             auth_kind = excluded.auth_kind, key_id = excluded.key_id,
             updated_at = excluded.updated_at",
        rusqlite::params![
            envelope.id,
            p.label,
            p.username,
            p.auth_kind,
            key_id,
            if p.created_at == 0 { envelope.updated_at } else { p.created_at },
            envelope.updated_at,
        ],
    )?;
    Ok(())
}

fn write_var_def(tx: &Transaction, envelope: &Envelope, p: &VarDefPayload) -> Result<()> {
    // `var_defs` has a UNIQUE(scope, scope_id, name) as well as its id. Two devices can
    // declare the same placeholder independently and end up with two ids for one triple,
    // so the conflict has to be handled on both keys or the insert simply fails.
    tx.execute(
        "DELETE FROM var_defs WHERE scope = ?1 AND scope_id = ?2 AND name = ?3 AND id <> ?4",
        rusqlite::params![p.scope, p.scope_id, p.name, envelope.id],
    )?;

    tx.execute(
        "INSERT INTO var_defs (id, scope, scope_id, name, label, default_value, required,
                               created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
             scope = excluded.scope, scope_id = excluded.scope_id, name = excluded.name,
             label = excluded.label, default_value = excluded.default_value,
             required = excluded.required, updated_at = excluded.updated_at",
        rusqlite::params![
            envelope.id,
            p.scope,
            p.scope_id,
            p.name,
            p.label,
            p.default_value,
            i64::from(p.required),
            if p.created_at == 0 { envelope.updated_at } else { p.created_at },
            envelope.updated_at,
        ],
    )?;
    Ok(())
}

fn write_workspace(tx: &Transaction, envelope: &Envelope, p: &WorkspacePayload) -> Result<()> {
    tx.execute(
        "INSERT INTO workspaces (id, name, layout_json, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET
             name = excluded.name, layout_json = excluded.layout_json,
             updated_at = excluded.updated_at",
        rusqlite::params![
            envelope.id,
            p.name,
            p.layout_json,
            if p.created_at == 0 { envelope.updated_at } else { p.created_at },
            envelope.updated_at,
        ],
    )?;
    Ok(())
}
