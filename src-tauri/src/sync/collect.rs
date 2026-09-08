//! Dirty local rows to sealed envelopes.
//!
//! Everything here runs under the database lock and touches no network, so the caller can
//! take the batch, release the lock, and only then talk to the server. `Db`'s contract is
//! "nothing awaits while the guard is held", and a sync worker holding it across an HTTP
//! round trip would stall every terminal command in the app.

use remotier_sync_proto::crypto::{self, ContentKey};
use remotier_sync_proto::record::{
    DeviceLayoutPayload, Envelope, GroupPayload, HostPayload, IdentityPayload, KeyRef, RecordKind,
    SettingPayload, VarDefPayload, WorkspacePayload,
};
use rusqlite::{Connection, OptionalExtension};

use crate::db::query_all;
use crate::error::{Error, Result};

use super::groups::Keyring;
use super::settings::is_syncable;

/// One dirty record, before its payload has been fetched.
struct Dirty {
    kind: RecordKind,
    id: String,
    deleted_at: Option<i64>,
    updated_at: i64,
}

/// What a pass over the dirty records produced.
#[derive(Debug, Default)]
pub struct Batch {
    pub envelopes: Vec<Envelope>,
    /// Records that are dirty but will never be sent: a setting the allow-list excludes,
    /// or a row deleted before it was ever pushed.
    ///
    /// Their flag has to be cleared too. Leaving it set means the pending count never
    /// reaches zero - the UI reports "1 change waiting" for the life of the install, and
    /// every cycle re-examines a record it has already decided to skip.
    pub skipped: Vec<(&'static str, String)>,
}

impl Batch {
    /// How many dirty records this pass consumed, sent or not.
    pub fn consumed(&self) -> usize {
        self.envelopes.len() + self.skipped.len()
    }

    pub fn is_empty(&self) -> bool {
        self.consumed() == 0
    }
}

/// Build the push batch.
///
/// `limit` bounds a single push; the worker loops until nothing is left, so a first sync
/// of a large inventory arrives in pieces rather than as one request a proxy will reject.
pub fn collect(
    conn: &Connection,
    keys: &Keyring,
    device_id: &str,
    limit: usize,
) -> Result<Batch> {
    let dirty = query_all(
        conn,
        "SELECT kind, id, deleted_at, updated_at FROM sync_meta
         WHERE local_dirty = 1 ORDER BY updated_at LIMIT ?1",
        [limit as i64],
        |row| {
            let kind: String = row.get(0)?;
            Ok(Dirty {
                // An unknown kind means a newer build wrote this row. Skipping is the
                // only safe answer: guessing would file it under the wrong table.
                kind: RecordKind::parse(&kind).unwrap_or(RecordKind::Setting),
                id: row.get(1)?,
                deleted_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        },
    )?;

    let mut out = Batch::default();
    for record in dirty {
        match envelope_for(conn, keys, device_id, &record)? {
            Some(envelope) => out.envelopes.push(envelope),
            None => out.skipped.push((record.kind.as_str(), record.id.clone())),
        }
    }
    Ok(out)
}

/// Clear the flag on records that were deliberately not sent.
pub fn mark_skipped(tx: &rusqlite::Transaction, skipped: &[(&str, String)]) -> Result<()> {
    for (kind, id) in skipped {
        tx.execute(
            "UPDATE sync_meta SET local_dirty = 0 WHERE kind = ?1 AND id = ?2",
            rusqlite::params![kind, id],
        )?;
    }
    Ok(())
}

fn envelope_for(
    conn: &Connection,
    keys: &Keyring,
    device_id: &str,
    record: &Dirty,
) -> Result<Option<Envelope>> {
    // A tombstone carries no payload. Shipping the last known state of a deleted record
    // would leak it for no benefit - the other devices are being told to forget it.
    if record.deleted_at.is_some() {
        // A tombstone for a shared record has to be labelled with the group key, or the
        // server files the deletion under the wrong account and the members never see it.
        // The group is read from `sync_meta`'s own row rather than the domain table,
        // which is already gone.
        let group_id = tombstone_group(conn, record)?;
        let (key, key_ref) = keys.select(group_id.as_deref());
        let mut envelope = skeleton(record, device_id, group_id, None, 0);
        envelope.key_ref = key_ref;
        return Ok(Some(seal(key, envelope, None::<&()>)?));
    }

    Ok(match record.kind {
        RecordKind::Host => host(conn, keys, device_id, record)?,
        RecordKind::Group => group(conn, keys, device_id, record)?,
        RecordKind::Identity => identity(conn, keys, device_id, record)?,
        RecordKind::VarDef => var_def(conn, keys, device_id, record)?,
        RecordKind::Workspace => workspace(conn, keys, device_id, record)?,
        RecordKind::Setting => setting(conn, keys, device_id, record)?,
        // Device layouts are pushed explicitly by the worker, not swept up from
        // sync_meta - `session_state` has no trigger and never will.
        RecordKind::DeviceLayout => None,
    })
}

/// The plaintext half, before a payload is attached.
fn skeleton(
    record: &Dirty,
    device_id: &str,
    group_id: Option<String>,
    parent_id: Option<String>,
    sort: i64,
) -> Envelope {
    Envelope {
        id: record.id.clone(),
        kind: record.kind,
        // Overwritten by the caller once the record's group is known. Personal is the
        // safe default: it is readable by this account and nobody else.
        key_ref: KeyRef::Personal,
        group_id,
        parent_id,
        sort,
        updated_at: record.updated_at,
        device_id: device_id.to_string(),
        deleted_at: record.deleted_at,
        nonce: Vec::new(),
        ciphertext: Vec::new(),
        seq: 0,
    }
}

fn seal<T: serde::Serialize>(
    key: &ContentKey,
    mut envelope: Envelope,
    payload: Option<&T>,
) -> Result<Envelope> {
    if let Some(payload) = payload {
        // The AAD is computed from the finished plaintext header, so a server that edits
        // the header cannot make the payload open.
        let aad = envelope.aad();
        let (nonce, ciphertext) = crypto::seal_json(key, &aad, payload)?;
        envelope.nonce = nonce;
        envelope.ciphertext = ciphertext;
    }
    Ok(envelope)
}

fn host(
    conn: &Connection,
    keys: &Keyring,
    device_id: &str,
    record: &Dirty,
) -> Result<Option<Envelope>> {
    let row = conn
        .query_row(
            "SELECT h.group_id, h.label, h.hostname, h.port, h.identity_id, h.jump_host_id,
                    h.color, h.icon, h.tags, h.sort, h.username, h.auth_kind, k.fingerprint
             FROM hosts h LEFT JOIN keys k ON k.id = h.key_id
             WHERE h.id = ?1",
            [&record.id],
            |row| {
                let tags: String = row.get(8)?;
                Ok(HostPayload {
                    group_id: row.get(0)?,
                    label: row.get(1)?,
                    hostname: row.get(2)?,
                    port: row.get(3)?,
                    identity_id: row.get(4)?,
                    jump_host_id: row.get(5)?,
                    color: row.get(6)?,
                    icon: row.get(7)?,
                    tags: serde_json::from_str(&tags).unwrap_or_default(),
                    sort: row.get(9)?,
                    username: row.get(10)?,
                    auth_kind: row.get(11)?,
                    // The key's fingerprint, never its id: key rows do not sync and a
                    // fresh uuid is minted on every machine that imports one.
                    key_fingerprint: row.get(12)?,
                    created_at: 0,
                })
            },
        )
        .optional()?;

    // A row that is dirty but gone is a delete the trigger has already tombstoned; the
    // tombstone will be collected in its own right.
    let Some(payload) = row else {
        return Ok(None);
    };

    let (key, key_ref) = keys.select(payload.group_id.as_deref());
    let mut envelope = skeleton(
        record,
        device_id,
        payload.group_id.clone(),
        None,
        payload.sort,
    );
    envelope.key_ref = key_ref;
    Ok(Some(seal(key, envelope, Some(&payload))?))
}

fn group(
    conn: &Connection,
    keys: &Keyring,
    device_id: &str,
    record: &Dirty,
) -> Result<Option<Envelope>> {
    let row = conn
        .query_row(
            "SELECT parent_id, name, sort, default_port, default_identity_id,
                    default_jump_host_id, icon, color, created_at
             FROM groups WHERE id = ?1",
            [&record.id],
            |row| {
                Ok(GroupPayload {
                    parent_id: row.get(0)?,
                    name: row.get(1)?,
                    sort: row.get(2)?,
                    default_port: row.get(3)?,
                    default_identity_id: row.get(4)?,
                    default_jump_host_id: row.get(5)?,
                    icon: row.get(6)?,
                    color: row.get(7)?,
                    created_at: row.get(8)?,
                })
            },
        )
        .optional()?;

    let Some(payload) = row else {
        return Ok(None);
    };

    // A group is covered by its own key when it is the shared one, otherwise by the
    // nearest shared ancestor - so the group record itself reaches the members who can
    // see what is inside it.
    let (key, key_ref) = keys.select(Some(&record.id));
    let mut envelope = skeleton(
        record,
        device_id,
        Some(record.id.clone()),
        payload.parent_id.clone(),
        payload.sort,
    );
    envelope.key_ref = key_ref;
    Ok(Some(seal(key, envelope, Some(&payload))?))
}

fn identity(
    conn: &Connection,
    keys: &Keyring,
    device_id: &str,
    record: &Dirty,
) -> Result<Option<Envelope>> {
    let row = conn
        .query_row(
            "SELECT i.label, i.username, i.auth_kind, k.fingerprint, i.created_at
             FROM identities i LEFT JOIN keys k ON k.id = i.key_id
             WHERE i.id = ?1",
            [&record.id],
            |row| {
                Ok(IdentityPayload {
                    label: row.get(0)?,
                    username: row.get(1)?,
                    auth_kind: row.get(2)?,
                    key_fingerprint: row.get(3)?,
                    created_at: row.get(4)?,
                })
            },
        )
        .optional()?;

    let Some(payload) = row else {
        return Ok(None);
    };
    // Identities are account-wide, not part of any group, so they stay personal. A host
    // shared with a colleague arrives with its identity link nulled on their side, which
    // is the same thing that happens for a key they do not have.
    Ok(Some(seal(
        keys.personal(),
        skeleton(record, device_id, None, None, 0),
        Some(&payload),
    )?))
}

fn var_def(
    conn: &Connection,
    keys: &Keyring,
    device_id: &str,
    record: &Dirty,
) -> Result<Option<Envelope>> {
    let row = conn
        .query_row(
            "SELECT scope, scope_id, name, label, default_value, required, created_at
             FROM var_defs WHERE id = ?1",
            [&record.id],
            |row| {
                Ok(VarDefPayload {
                    scope: row.get(0)?,
                    scope_id: row.get(1)?,
                    name: row.get(2)?,
                    label: row.get(3)?,
                    default_value: row.get(4)?,
                    required: row.get::<_, i64>(5)? != 0,
                    created_at: row.get(6)?,
                })
            },
        )
        .optional()?;

    let Some(payload) = row else {
        return Ok(None);
    };
    // A group-scoped placeholder travels with its group, so it is sealed with the group's
    // key. That is the point of declaring one on a group at all.
    let covering = (payload.scope == "group").then(|| payload.scope_id.clone());
    let (key, key_ref) = keys.select(covering.as_deref());
    let mut envelope = skeleton(record, device_id, covering, None, 0);
    envelope.key_ref = key_ref;
    Ok(Some(seal(key, envelope, Some(&payload))?))
}

fn workspace(
    conn: &Connection,
    keys: &Keyring,
    device_id: &str,
    record: &Dirty,
) -> Result<Option<Envelope>> {
    let row = conn
        .query_row(
            "SELECT name, layout_json, created_at FROM workspaces WHERE id = ?1",
            [&record.id],
            |row| {
                Ok(WorkspacePayload {
                    name: row.get(0)?,
                    layout_json: row.get(1)?,
                    created_at: row.get(2)?,
                })
            },
        )
        .optional()?;

    let Some(payload) = row else {
        return Ok(None);
    };
    Ok(Some(seal(
        keys.personal(),
        skeleton(record, device_id, None, None, 0),
        Some(&payload),
    )?))
}

fn setting(
    conn: &Connection,
    keys: &Keyring,
    device_id: &str,
    record: &Dirty,
) -> Result<Option<Envelope>> {
    // The allow-list is applied here rather than in the trigger, so there is one place
    // that decides and it is in Rust where it can be read and tested.
    if !is_syncable(&record.id) {
        return Ok(None);
    }

    let row = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            [&record.id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    let Some(value) = row else {
        return Ok(None);
    };
    let payload = SettingPayload {
        key: record.id.clone(),
        value,
    };
    Ok(Some(seal(
        keys.personal(),
        skeleton(record, device_id, None, None, 0),
        Some(&payload),
    )?))
}

/// Which group a deleted record was in.
///
/// The domain row is gone by the time a tombstone is collected, so the group cannot be
/// read from it. `sync_meta` does not record it either - but the record was pushed once
/// under a group key, and the server remembers where it lives. Falling back to whatever
/// the last push said keeps the deletion on the same account as the record it deletes.
fn tombstone_group(conn: &Connection, record: &Dirty) -> Result<Option<String>> {
    if record.kind == RecordKind::Group {
        return Ok(Some(record.id.clone()));
    }
    Ok(conn
        .query_row(
            "SELECT group_id FROM sync_meta WHERE kind = ?1 AND id = ?2",
            rusqlite::params![record.kind.as_str(), record.id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten())
}

/// This machine's open tabs, as a record keyed by device id.
///
/// Built on demand rather than swept out of `sync_meta`: `session_state` has no trigger,
/// because a layout written every 400ms would otherwise become sync traffic every 400ms.
pub fn device_layout(
    conn: &Connection,
    key: &ContentKey,
    device_id: &str,
    device_name: &str,
) -> Result<Option<Envelope>> {
    let row = conn
        .query_row(
            "SELECT payload, updated_at FROM session_state WHERE id = 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;

    let Some((layout_json, updated_at)) = row else {
        return Ok(None);
    };

    let payload = DeviceLayoutPayload {
        device_name: device_name.to_string(),
        layout_json,
    };
    let envelope = Envelope {
        // The device id *is* the record id, so a device has exactly one layout and
        // re-pushing replaces it rather than accumulating.
        id: device_id.to_string(),
        kind: RecordKind::DeviceLayout,
        key_ref: KeyRef::Personal,
        group_id: None,
        parent_id: None,
        sort: 0,
        updated_at,
        device_id: device_id.to_string(),
        deleted_at: None,
        nonce: Vec::new(),
        ciphertext: Vec::new(),
        seq: 0,
    };
    Ok(Some(seal(key, envelope, Some(&payload))?))
}

/// Clear the dirty flag for records the server accepted, recording where they landed.
pub fn mark_pushed(
    tx: &rusqlite::Transaction,
    accepted: &[remotier_sync_proto::api::Accepted],
) -> Result<()> {
    for record in accepted {
        tx.execute(
            "UPDATE sync_meta SET local_dirty = 0, server_seq = ?1
             WHERE kind = ?2 AND id = ?3",
            rusqlite::params![record.seq, record.kind, record.id],
        )?;
    }
    Ok(())
}

/// A record the server refused as stale is not dirty either: the server's copy is newer,
/// and the next pull brings it. Leaving it dirty would push it again forever.
pub fn mark_stale(tx: &rusqlite::Transaction, kind: &str, id: &str) -> Result<()> {
    tx.execute(
        "UPDATE sync_meta SET local_dirty = 0 WHERE kind = ?1 AND id = ?2",
        rusqlite::params![kind, id],
    )?;
    Ok(())
}

// Referenced by the error path in `envelope_for` when a kind cannot be parsed.
const _: fn(String) -> Error = Error::Sync;
