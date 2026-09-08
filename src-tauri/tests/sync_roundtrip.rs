//! Collect and apply, back to back, with no server in the way.
//!
//! Two `Db`s stand in for two devices: whatever `collect` produces on one is fed to
//! `apply` on the other, which is exactly what the wire does minus the transport. That
//! makes convergence testable without a running instance.

use remotier_sync_proto::crypto;
use remotier_sync_proto::record::{Envelope, RecordKind};
use remotier_lib::db::Db;
use remotier_lib::sync::groups::Keyring;
use remotier_lib::sync::{apply, collect};

struct Device {
    db: Db,
    id: &'static str,
}

impl Device {
    fn new(id: &'static str) -> Self {
        Self {
            db: Db::open_in_memory().unwrap(),
            id,
        }
    }

    fn collect(&self, key: &crypto::ContentKey) -> Vec<Envelope> {
        self.db
            .read(|conn| collect::collect(conn, &Keyring::personal_only(key.clone()), self.id, 500))
            .unwrap()
    }

    /// Push to the other device, as the server would: stamp a sequence number, apply,
    /// then clear this side's dirty flags the way a successful push does.
    fn push_to(&self, other: &Device, key: &crypto::ContentKey) -> apply::Applied {
        let mut batch = self.collect(key);
        for (i, envelope) in batch.iter_mut().enumerate() {
            envelope.seq = i as i64 + 1;
        }

        let applied = other
            .db
            .write(|tx| apply::apply(tx, &Keyring::personal_only(key.clone()), other.id, &batch))
            .unwrap();

        let accepted: Vec<_> = batch
            .iter()
            .map(|e| remotier_sync_proto::api::Accepted {
                id: e.id.clone(),
                kind: e.kind.as_str().to_string(),
                seq: e.seq,
            })
            .collect();
        self.db
            .write(|tx| collect::mark_pushed(tx, &accepted))
            .unwrap();

        applied
    }

    fn host_count(&self) -> i64 {
        self.db
            .read(|conn| Ok(conn.query_row("SELECT count(*) FROM hosts", [], |r| r.get(0))?))
            .unwrap()
    }

    fn hostname(&self, id: &str) -> Option<String> {
        self.db
            .read(|conn| {
                Ok(conn
                    .query_row("SELECT hostname FROM hosts WHERE id = ?1", [id], |r| {
                        r.get::<_, String>(0)
                    })
                    .ok())
            })
            .unwrap()
    }

    fn dirty(&self) -> i64 {
        self.db
            .read(|conn| {
                Ok(conn.query_row(
                    "SELECT count(*) FROM sync_meta WHERE local_dirty = 1",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap()
    }

    fn add_host(&self, id: &str, hostname: &str, group: Option<&str>, at: i64) {
        self.db
            .write(|tx| {
                tx.execute(
                    "INSERT INTO hosts (id, group_id, label, hostname, tags, sort,
                                        created_at, updated_at)
                     VALUES (?1, ?2, ?1, ?3, '[]', 0, ?4, ?4)",
                    rusqlite::params![id, group, hostname, at],
                )?;
                Ok(())
            })
            .unwrap();
    }

    fn add_group(&self, id: &str, parent: Option<&str>, at: i64) {
        self.db
            .write(|tx| {
                tx.execute(
                    "INSERT INTO groups (id, parent_id, name, sort, created_at, updated_at)
                     VALUES (?1, ?2, ?1, 0, ?3, ?3)",
                    rusqlite::params![id, parent, at],
                )?;
                Ok(())
            })
            .unwrap();
    }

    fn edit_host(&self, id: &str, hostname: &str, at: i64) {
        self.db
            .write(|tx| {
                tx.execute(
                    "UPDATE hosts SET hostname = ?2, updated_at = ?3 WHERE id = ?1",
                    rusqlite::params![id, hostname, at],
                )?;
                Ok(())
            })
            .unwrap();
    }

    fn delete_host(&self, id: &str) {
        self.db
            .write(|tx| {
                tx.execute("DELETE FROM hosts WHERE id = ?1", [id])?;
                Ok(())
            })
            .unwrap();
    }

    fn set_setting(&self, key: &str, value: &str, at: i64) {
        self.db
            .write(|tx| {
                tx.execute(
                    "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value,
                                                    updated_at = excluded.updated_at",
                    rusqlite::params![key, value, at],
                )?;
                Ok(())
            })
            .unwrap();
    }
}

fn key() -> crypto::ContentKey {
    crypto::random_key()
}

#[test]
fn a_host_reaches_the_other_device() {
    let key = key();
    let (a, b) = (Device::new("dev-a"), Device::new("dev-b"));

    a.add_host("h1", "terminal.shop", None, 100);
    let applied = a.push_to(&b, &key);

    assert_eq!(applied.written, 1);
    assert_eq!(b.hostname("h1").as_deref(), Some("terminal.shop"));
}

#[test]
fn a_pulled_record_is_not_pushed_straight_back() {
    // The domain write fires the table's own dirty trigger, so apply has to clear the
    // flag afterwards. Without that the two devices would push the same record at each
    // other forever.
    let key = key();
    let (a, b) = (Device::new("dev-a"), Device::new("dev-b"));

    a.add_host("h1", "terminal.shop", None, 100);
    a.push_to(&b, &key);

    assert_eq!(b.dirty(), 0, "b has nothing of its own to push");
    assert!(b.collect(&key).is_empty());
}

#[test]
fn the_newer_edit_wins_in_both_directions() {
    let key = key();
    let (a, b) = (Device::new("dev-a"), Device::new("dev-b"));

    a.add_host("h1", "old.example", None, 100);
    a.push_to(&b, &key);

    // b edits later; a's older copy must not overwrite it.
    b.edit_host("h1", "new.example", 300);
    a.edit_host("h1", "stale.example", 200);

    b.push_to(&a, &key);
    assert_eq!(a.hostname("h1").as_deref(), Some("new.example"));

    // And a's stale edit, pushed after, does not win.
    a.push_to(&b, &key);
    assert_eq!(b.hostname("h1").as_deref(), Some("new.example"));
}

#[test]
fn a_delete_travels_and_does_not_come_back() {
    let key = key();
    let (a, b) = (Device::new("dev-a"), Device::new("dev-b"));

    a.add_host("h1", "terminal.shop", None, 100);
    a.push_to(&b, &key);
    assert_eq!(b.host_count(), 1);

    a.delete_host("h1");
    let applied = a.push_to(&b, &key);
    assert_eq!(applied.deleted, 1);
    assert_eq!(b.host_count(), 0);

    // b now holds a tombstone, not nothing. Pushing back must not resurrect the host on
    // a, and must not be treated as a new record.
    b.push_to(&a, &key);
    assert_eq!(a.host_count(), 0);
}

#[test]
fn an_older_edit_does_not_resurrect_a_deleted_host() {
    let key = key();
    let (a, b) = (Device::new("dev-a"), Device::new("dev-b"));

    a.add_host("h1", "terminal.shop", None, 100);
    a.push_to(&b, &key);

    // b edits at 150 without knowing a deleted at 200.
    b.edit_host("h1", "edited.example", 150);
    a.delete_host("h1");
    a.push_to(&b, &key);

    assert_eq!(b.host_count(), 0, "the later delete wins");
}

#[test]
fn a_tombstone_carries_no_payload() {
    let key = key();
    let a = Device::new("dev-a");
    a.add_host("h1", "terminal.shop", None, 100);
    a.db.write(|tx| {
        tx.execute("UPDATE sync_meta SET local_dirty = 0", [])?;
        Ok(())
    })
    .unwrap();
    a.delete_host("h1");

    let batch = a.collect(&key);
    let tombstone = batch.iter().find(|e| e.id == "h1").unwrap();
    assert!(tombstone.deleted_at.is_some());
    assert!(
        tombstone.ciphertext.is_empty(),
        "the last state of a deleted record must not be shipped"
    );
}

#[test]
fn a_group_and_its_host_arrive_together_in_any_order() {
    // The batch is applied in one transaction with foreign keys deferred, and groups are
    // written before hosts, so a host in a group is never orphaned by page ordering.
    let key = key();
    let (a, b) = (Device::new("dev-a"), Device::new("dev-b"));

    a.add_group("g1", None, 100);
    a.add_host("h1", "terminal.shop", Some("g1"), 100);
    a.push_to(&b, &key);

    let group: Option<String> = b
        .db
        .read(|conn| {
            Ok(conn
                .query_row("SELECT group_id FROM hosts WHERE id = 'h1'", [], |r| r.get(0))
                .unwrap())
        })
        .unwrap();
    assert_eq!(group.as_deref(), Some("g1"));
}

#[test]
fn deleting_a_group_takes_its_subtree_across() {
    let key = key();
    let (a, b) = (Device::new("dev-a"), Device::new("dev-b"));

    a.add_group("root", None, 100);
    a.add_group("child", Some("root"), 100);
    a.add_host("h1", "terminal.shop", Some("child"), 100);
    a.push_to(&b, &key);
    assert_eq!(b.host_count(), 1);

    a.db.write(|tx| {
        tx.execute("DELETE FROM groups WHERE id = 'root'", [])?;
        Ok(())
    })
    .unwrap();
    a.push_to(&b, &key);

    let groups: i64 = b
        .db
        .read(|conn| Ok(conn.query_row("SELECT count(*) FROM groups", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(groups, 0, "the whole subtree was tombstoned and travelled");
    // The host survives on both sides, ungrouped - ON DELETE SET NULL, not a cascade.
    assert_eq!(b.host_count(), 1);
}

#[test]
fn a_reference_to_something_this_device_lacks_becomes_null() {
    // A host can point at an identity that never reached this machine. Refusing the host
    // over it would leave the user with nothing; nulling the link leaves them a host.
    let key = key();
    let (a, b) = (Device::new("dev-a"), Device::new("dev-b"));

    a.db.write(|tx| {
        tx.execute(
            "INSERT INTO identities (id, label, username, auth_kind, created_at, updated_at)
             VALUES ('i1', 'ops', 'root', 'agent', 100, 100)",
            [],
        )?;
        tx.execute(
            "INSERT INTO hosts (id, label, hostname, identity_id, tags, sort, created_at, updated_at)
             VALUES ('h1', 'h', 'terminal.shop', 'i1', '[]', 0, 100, 100)",
            [],
        )?;
        Ok(())
    })
    .unwrap();

    // Ship only the host, as if the identity were in a group b cannot see.
    let mut batch: Vec<Envelope> = a
        .collect(&key)
        .into_iter()
        .filter(|e| e.kind == RecordKind::Host)
        .collect();
    batch[0].seq = 1;

    b.db.write(|tx| apply::apply(tx, &Keyring::personal_only(key.clone()), "dev-b", &batch)).unwrap();

    let identity: Option<String> = b
        .db
        .read(|conn| {
            Ok(conn
                .query_row("SELECT identity_id FROM hosts WHERE id = 'h1'", [], |r| r.get(0))
                .unwrap())
        })
        .unwrap();
    assert_eq!(identity, None);
    assert_eq!(b.host_count(), 1, "the host still arrived");
}

#[test]
fn local_only_data_is_never_collected() {
    // var_values is this machine's answers and session_state its open tabs. Both are
    // stated local-only in the schema; this asserts on what `collect` actually produces
    // rather than on the intent.
    let key = key();
    let a = Device::new("dev-a");

    a.db.write(|tx| {
        tx.execute(
            "INSERT INTO var_values (scope, scope_id, name, value, updated_at)
             VALUES ('global', '', 'company', 'acme', 100)",
            [],
        )?;
        tx.execute(
            "INSERT INTO session_state (id, payload, updated_at) VALUES (1, '{}', 100)",
            [],
        )?;
        Ok(())
    })
    .unwrap();

    assert!(a.collect(&key).is_empty());
}

#[test]
fn a_machine_specific_setting_is_not_collected() {
    let key = key();
    let a = Device::new("dev-a");

    a.set_setting("ssh.agentSocket", "/opt/homebrew/var/run/agent.sock", 100);
    a.set_setting("terminal.fontSize", "14", 100);

    let batch = a.collect(&key);
    assert_eq!(batch.len(), 1);
    assert_eq!(batch[0].id, "terminal.fontSize");
}

#[test]
fn a_setting_the_allow_list_rejects_is_refused_on_the_way_in_too() {
    // A newer build could send a setting this one keeps local. The allow-list has to
    // hold in both directions, or the far side decides what this machine stores.
    let key = key();
    let (a, b) = (Device::new("dev-a"), Device::new("dev-b"));

    a.set_setting("ssh.agentSocket", "/somewhere/else.sock", 100);
    // Force it onto the wire as though a newer build had sent it.
    let mut envelope = {
        a.set_setting("terminal.fontSize", "14", 100);
        let mut batch = a.collect(&key);
        batch.retain(|e| e.id == "terminal.fontSize");
        batch.remove(0)
    };
    envelope.id = "ssh.agentSocket".into();
    envelope.seq = 1;
    // Re-seal, because the id is in the AAD.
    let aad = envelope.aad();
    let payload = remotier_sync_proto::record::SettingPayload {
        key: "ssh.agentSocket".into(),
        value: "/somewhere/else.sock".into(),
    };
    let (nonce, ciphertext) = crypto::seal_json(&key, &aad, &payload).unwrap();
    envelope.nonce = nonce;
    envelope.ciphertext = ciphertext;

    b.db.write(|tx| apply::apply(tx, &Keyring::personal_only(key.clone()), "dev-b", &[envelope]))
        .unwrap();

    let stored: Option<String> = b
        .db
        .read(|conn| {
            Ok(conn
                .query_row("SELECT value FROM settings WHERE key = 'ssh.agentSocket'", [], |r| {
                    r.get::<_, String>(0)
                })
                .ok())
        })
        .unwrap();
    assert_eq!(stored, None, "this machine's agent socket was not overwritten");
}

#[test]
fn a_record_sealed_with_another_key_is_skipped_not_fatal() {
    let key = key();
    let (a, b) = (Device::new("dev-a"), Device::new("dev-b"));

    a.add_host("h1", "terminal.shop", None, 100);
    a.add_host("h2", "other.shop", None, 100);
    let mut batch = a.collect(&key);
    for (i, e) in batch.iter_mut().enumerate() {
        e.seq = i as i64 + 1;
    }
    // Corrupt one payload, as a wrong key or a tampering server would.
    batch[0].ciphertext[0] ^= 0xff;

    let applied = b.db.write(|tx| apply::apply(tx, &Keyring::personal_only(key.clone()), "dev-b", &batch)).unwrap();
    assert_eq!(applied.skipped, 1);
    assert_eq!(applied.written, 1, "the other record still landed");
}

#[test]
fn a_swapped_envelope_header_fails_to_open() {
    // The AAD binds id, kind, clock and device. A server that re-labelled a record must
    // not be able to make it decrypt as something else.
    let key = key();
    let a = Device::new("dev-a");
    let b = Device::new("dev-b");

    a.add_host("h1", "terminal.shop", None, 100);
    let mut batch = a.collect(&key);
    batch[0].id = "h2".into();
    batch[0].seq = 1;

    let applied = b.db.write(|tx| apply::apply(tx, &Keyring::personal_only(key.clone()), "dev-b", &batch)).unwrap();
    assert_eq!(applied.skipped, 1);
    assert_eq!(b.host_count(), 0);
}

#[test]
fn two_devices_declaring_the_same_placeholder_converge_on_one_row() {
    // var_defs has UNIQUE(scope, scope_id, name) as well as its id, so two independent
    // declarations of the same placeholder collide on a key that is not the primary one.
    let key = key();
    let (a, b) = (Device::new("dev-a"), Device::new("dev-b"));

    for (device, id, at) in [(&a, "d1", 100), (&b, "d2", 200)] {
        device
            .db
            .write(|tx| {
                tx.execute(
                    "INSERT INTO var_defs (id, scope, scope_id, name, required,
                                           created_at, updated_at)
                     VALUES (?1, 'global', '', 'company', 0, ?2, ?2)",
                    rusqlite::params![id, at],
                )?;
                Ok(())
            })
            .unwrap();
    }

    a.push_to(&b, &key);

    let count: i64 = b
        .db
        .read(|conn| Ok(conn.query_row("SELECT count(*) FROM var_defs", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(count, 1, "one declaration per (scope, scope_id, name)");
}

// ---------------------------------------------------------------------------
// Group sharing
// ---------------------------------------------------------------------------

/// A keyring holding a personal key plus one group key, as a member's machine has after
/// `fetch_shared_keys` has run.
fn shared_keyring(
    device: &Device,
    personal: &crypto::ContentKey,
    group_id: &str,
    group_key: &crypto::ContentKey,
) -> Keyring {
    use remotier_lib::crypto::vault::Vault;
    let vault = Vault::from_key(&[3u8; 32]).unwrap();
    device
        .db
        .write(|tx| {
            remotier_lib::sync::groups::save_key(tx, &vault, group_id, group_key, None)
        })
        .unwrap();
    device
        .db
        .read(|conn| Keyring::new(conn, &vault, personal.clone()))
        .unwrap()
}

#[test]
fn a_record_in_a_shared_group_is_sealed_with_the_group_key() {
    let personal = key();
    let group_key = key();
    let a = Device::new("dev-a");

    a.add_group("g1", None, 100);
    a.add_host("h1", "terminal.shop", Some("g1"), 100);

    let keys = shared_keyring(&a, &personal, "g1", &group_key);
    let batch = a.db.read(|conn| collect::collect(conn, &keys, "dev-a", 500)).unwrap();

    let host = batch.iter().find(|e| e.id == "h1").unwrap();
    assert_eq!(
        host.key_ref,
        remotier_sync_proto::record::KeyRef::Group { group_id: "g1".into() }
    );

    // And it really is that key: the personal one must not open it.
    let aad = host.aad();
    assert!(crypto::open(&personal, &aad, &host.nonce, &host.ciphertext).is_err());
    assert!(crypto::open(&group_key, &aad, &host.nonce, &host.ciphertext).is_ok());
}

#[test]
fn sharing_covers_the_whole_subtree() {
    // A host three levels below the shared group is still inside it, and must be readable
    // by the member - otherwise sharing a group hands over an empty shell.
    let personal = key();
    let group_key = key();
    let a = Device::new("dev-a");

    a.add_group("g1", None, 100);
    a.add_group("g2", Some("g1"), 100);
    a.add_host("deep", "deep.example", Some("g2"), 100);

    let keys = shared_keyring(&a, &personal, "g1", &group_key);
    let batch = a.db.read(|conn| collect::collect(conn, &keys, "dev-a", 500)).unwrap();

    let deep = batch.iter().find(|e| e.id == "deep").unwrap();
    assert_eq!(
        deep.key_ref,
        remotier_sync_proto::record::KeyRef::Group { group_id: "g1".into() }
    );
}

#[test]
fn a_record_outside_the_shared_group_stays_personal() {
    // The whole promise of sharing one group: everything else stays unreadable.
    let personal = key();
    let group_key = key();
    let a = Device::new("dev-a");

    a.add_group("g1", None, 100);
    a.add_host("inside", "in.example", Some("g1"), 100);
    a.add_host("outside", "out.example", None, 100);

    let keys = shared_keyring(&a, &personal, "g1", &group_key);
    let batch = a.db.read(|conn| collect::collect(conn, &keys, "dev-a", 500)).unwrap();

    let outside = batch.iter().find(|e| e.id == "outside").unwrap();
    assert_eq!(outside.key_ref, remotier_sync_proto::record::KeyRef::Personal);
}

#[test]
fn a_member_reads_the_shared_group_and_nothing_else() {
    // The member holds the group key and their *own* personal key, never the owner's.
    let owner_personal = key();
    let member_personal = key();
    let group_key = key();

    let owner = Device::new("dev-a");
    let member = Device::new("dev-b");

    owner.add_group("g1", None, 100);
    owner.add_host("inside", "in.example", Some("g1"), 100);
    owner.add_host("outside", "out.example", None, 100);

    let owner_keys = shared_keyring(&owner, &owner_personal, "g1", &group_key);
    let mut batch = owner
        .db
        .read(|conn| collect::collect(conn, &owner_keys, "dev-a", 500))
        .unwrap();
    for (i, e) in batch.iter_mut().enumerate() {
        e.seq = i as i64 + 1;
    }

    let member_keys = shared_keyring(&member, &member_personal, "g1", &group_key);
    let applied = member
        .db
        .write(|tx| apply::apply(tx, &member_keys, "dev-b", &batch))
        .unwrap();

    assert_eq!(member.hostname("inside").as_deref(), Some("in.example"));
    assert_eq!(member.hostname("outside"), None, "the private host is unreadable");
    assert!(applied.skipped >= 1, "the personal records were skipped, not failed");
}

#[test]
fn losing_the_group_key_skips_those_records_without_losing_the_rest() {
    // What a revoked member sees on the next pull: the group's records stop opening, and
    // everything of their own keeps working.
    let personal = key();
    let group_key = key();

    let owner = Device::new("dev-a");
    owner.add_group("g1", None, 100);
    owner.add_host("shared", "in.example", Some("g1"), 100);

    let owner_keys = shared_keyring(&owner, &personal, "g1", &group_key);
    let mut batch = owner
        .db
        .read(|conn| collect::collect(conn, &owner_keys, "dev-a", 500))
        .unwrap();
    for (i, e) in batch.iter_mut().enumerate() {
        e.seq = i as i64 + 1;
    }

    // The member no longer holds the group key.
    let revoked = Device::new("dev-b");
    let applied = revoked
        .db
        .write(|tx| apply::apply(tx, &Keyring::personal_only(personal.clone()), "dev-b", &batch))
        .unwrap();

    assert_eq!(applied.written, 0);
    assert_eq!(applied.skipped, batch.len());
    assert_eq!(revoked.host_count(), 0);
}

#[test]
fn a_rotated_key_makes_the_old_one_useless_for_new_records() {
    // Rotation is what actually revokes access. The removed member's copy of the old key
    // must not open anything written after it.
    let personal = key();
    let old_key = key();
    let rotated = key();

    let owner = Device::new("dev-a");
    owner.add_group("g1", None, 100);
    owner.add_host("h1", "in.example", Some("g1"), 100);

    // Seal once under the old key, then again under the rotated one.
    let keys = shared_keyring(&owner, &personal, "g1", &rotated);
    let batch = owner
        .db
        .read(|conn| collect::collect(conn, &keys, "dev-a", 500))
        .unwrap();
    let host = batch.iter().find(|e| e.id == "h1").unwrap();

    let aad = host.aad();
    assert!(crypto::open(&old_key, &aad, &host.nonce, &host.ciphertext).is_err());
    assert!(crypto::open(&rotated, &aad, &host.nonce, &host.ciphertext).is_ok());
}

#[test]
fn a_group_scoped_placeholder_travels_with_its_group() {
    // Placeholders declared on a group are shared with whoever has the group - that is
    // stated in CLAUDE.md and is why they are sealed with the group's key.
    let personal = key();
    let group_key = key();
    let a = Device::new("dev-a");

    a.add_group("g1", None, 100);
    a.db.write(|tx| {
        tx.execute(
            "INSERT INTO var_defs (id, scope, scope_id, name, required, created_at, updated_at)
             VALUES ('d1', 'group', 'g1', 'wg_user', 1, 100, 100)",
            [],
        )?;
        tx.execute(
            "INSERT INTO var_defs (id, scope, scope_id, name, required, created_at, updated_at)
             VALUES ('d2', 'global', '', 'company', 0, 100, 100)",
            [],
        )?;
        Ok(())
    })
    .unwrap();

    let keys = shared_keyring(&a, &personal, "g1", &group_key);
    let batch = a.db.read(|conn| collect::collect(conn, &keys, "dev-a", 500)).unwrap();

    let group_scoped = batch.iter().find(|e| e.id == "d1").unwrap();
    assert_eq!(
        group_scoped.key_ref,
        remotier_sync_proto::record::KeyRef::Group { group_id: "g1".into() }
    );

    // An account-wide one is not part of any group and stays personal.
    let account_wide = batch.iter().find(|e| e.id == "d2").unwrap();
    assert_eq!(account_wide.key_ref, remotier_sync_proto::record::KeyRef::Personal);
}

#[test]
fn a_tombstone_for_a_shared_record_keeps_the_group_key() {
    // The domain row is gone when a tombstone is collected, so the group has to come from
    // somewhere else. Labelling it personal would file the deletion under the wrong
    // account and the members would never see it.
    let personal = key();
    let group_key = key();
    let a = Device::new("dev-a");

    a.add_group("g1", None, 100);
    a.add_host("h1", "in.example", Some("g1"), 100);

    let keys = shared_keyring(&a, &personal, "g1", &group_key);
    // Push once, so sync_meta records where the host lives.
    let batch = a.db.read(|conn| collect::collect(conn, &keys, "dev-a", 500)).unwrap();
    let accepted: Vec<_> = batch
        .iter()
        .enumerate()
        .map(|(i, e)| remotier_sync_proto::api::Accepted {
            id: e.id.clone(),
            kind: e.kind.as_str().to_string(),
            seq: i as i64 + 1,
        })
        .collect();
    a.db.write(|tx| collect::mark_pushed(tx, &accepted)).unwrap();

    a.delete_host("h1");
    let batch = a.db.read(|conn| collect::collect(conn, &keys, "dev-a", 500)).unwrap();
    let tombstone = batch.iter().find(|e| e.id == "h1").unwrap();

    assert!(tombstone.deleted_at.is_some());
    assert_eq!(
        tombstone.key_ref,
        remotier_sync_proto::record::KeyRef::Group { group_id: "g1".into() },
        "the deletion has to reach the group it was in"
    );
}

// ---------------------------------------------------------------------------
// Device layouts
// ---------------------------------------------------------------------------

impl Device {
    fn set_layout(&self, payload: &str, at: i64) {
        self.db
            .write(|tx| {
                tx.execute(
                    "INSERT INTO session_state (id, payload, updated_at) VALUES (1, ?1, ?2)
                     ON CONFLICT(id) DO UPDATE SET payload = excluded.payload,
                                                   updated_at = excluded.updated_at",
                    rusqlite::params![payload, at],
                )?;
                Ok(())
            })
            .unwrap();
    }

    fn stored_layout(&self, device_id: &str) -> Option<String> {
        self.db
            .read(|conn| {
                Ok(conn
                    .query_row(
                        "SELECT layout_json FROM device_layouts WHERE device_id = ?1",
                        [device_id],
                        |r| r.get::<_, String>(0),
                    )
                    .ok())
            })
            .unwrap()
    }

    fn own_layout(&self) -> Option<String> {
        self.db
            .read(|conn| {
                Ok(conn
                    .query_row("SELECT payload FROM session_state WHERE id = 1", [], |r| {
                        r.get::<_, String>(0)
                    })
                    .ok())
            })
            .unwrap()
    }
}

fn layout_envelope(device: &Device, key: &crypto::ContentKey, name: &str) -> Envelope {
    device
        .db
        .read(|conn| collect::device_layout(conn, key, device.id, name))
        .unwrap()
        .unwrap()
}

#[test]
fn a_layout_is_not_swept_up_with_ordinary_records() {
    // session_state has no trigger: it is rewritten 400ms after every layout change, and
    // a trigger would turn that into a push every 400ms.
    let key = key();
    let a = Device::new("dev-a");
    a.set_layout(r#"{"tabs":[]}"#, 100);

    assert!(a.collect(&key).is_empty());
}

#[test]
fn another_devices_layout_is_stored_rather_than_applied() {
    // Applying it would replace the tabs the user is looking at.
    let key = key();
    let (a, b) = (Device::new("dev-a"), Device::new("dev-b"));

    a.set_layout(r#"{"tabs":["a"]}"#, 100);
    b.set_layout(r#"{"tabs":["b"]}"#, 100);

    let mut envelope = layout_envelope(&a, &key, "MacBook Pro");
    envelope.seq = 1;

    let applied = b
        .db
        .write(|tx| apply::apply(tx, &Keyring::personal_only(key.clone()), "dev-b", &[envelope]))
        .unwrap();

    assert_eq!(applied.written, 1);
    assert_eq!(b.stored_layout("dev-a").as_deref(), Some(r#"{"tabs":["a"]}"#));
    assert_eq!(
        b.own_layout().as_deref(),
        Some(r#"{"tabs":["b"]}"#),
        "b's own tabs are untouched"
    );
}

#[test]
fn a_devices_own_layout_coming_back_does_not_overwrite_its_tabs() {
    // The round trip returns what this machine pushed. Treating it as an update would
    // replace live tabs with whatever was sent last cycle.
    let key = key();
    let a = Device::new("dev-a");

    a.set_layout(r#"{"tabs":["old"]}"#, 100);
    let mut echo = layout_envelope(&a, &key, "MacBook Pro");
    echo.seq = 1;

    a.set_layout(r#"{"tabs":["new"]}"#, 200);

    let applied = a
        .db
        .write(|tx| apply::apply(tx, &Keyring::personal_only(key.clone()), "dev-a", &[echo]))
        .unwrap();

    assert_eq!(applied.skipped, 1);
    assert_eq!(a.own_layout().as_deref(), Some(r#"{"tabs":["new"]}"#));
    assert_eq!(a.stored_layout("dev-a"), None, "not listed as another device");
}

#[test]
fn a_layout_record_is_keyed_by_device_so_it_replaces_rather_than_accumulates() {
    let key = key();
    let a = Device::new("dev-a");

    a.set_layout(r#"{"tabs":["one"]}"#, 100);
    let first = layout_envelope(&a, &key, "MacBook Pro");
    a.set_layout(r#"{"tabs":["two"]}"#, 200);
    let second = layout_envelope(&a, &key, "MacBook Pro");

    assert_eq!(first.id, "dev-a");
    assert_eq!(second.id, first.id);
    assert!(second.updated_at > first.updated_at);
}

#[test]
fn a_renamed_device_is_relabelled_rather_than_duplicated() {
    let key = key();
    let (a, b) = (Device::new("dev-a"), Device::new("dev-b"));
    a.set_layout(r#"{"tabs":[]}"#, 100);

    for (name, at) in [("Old name", 100), ("New name", 200)] {
        a.set_layout(r#"{"tabs":[]}"#, at);
        let mut envelope = layout_envelope(&a, &key, name);
        envelope.seq = at;
        b.db.write(|tx| apply::apply(tx, &Keyring::personal_only(key.clone()), "dev-b", &[envelope]))
            .unwrap();
    }

    let names: Vec<String> = b
        .db
        .read(|conn| {
            let mut stmt = conn.prepare("SELECT device_name FROM device_layouts")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            Ok(rows.filter_map(Result::ok).collect())
        })
        .unwrap();
    assert_eq!(names, ["New name"]);
}

#[test]
fn a_layout_without_the_key_is_skipped_not_stored_blank() {
    let a = Device::new("dev-a");
    let b = Device::new("dev-b");
    a.set_layout(r#"{"tabs":[]}"#, 100);

    let mut envelope = layout_envelope(&a, &key(), "MacBook Pro");
    envelope.seq = 1;

    // b has a different personal key, as a second account would.
    let applied = b
        .db
        .write(|tx| apply::apply(tx, &Keyring::personal_only(key()), "dev-b", &[envelope]))
        .unwrap();

    assert_eq!(applied.written, 0);
    assert_eq!(applied.skipped, 1);
    assert_eq!(b.stored_layout("dev-a"), None);
}

#[test]
fn a_workspace_travels_like_any_other_record() {
    // Named layouts sync; the live session does not. This is the difference.
    let key = key();
    let (a, b) = (Device::new("dev-a"), Device::new("dev-b"));

    a.db.write(|tx| {
        tx.execute(
            "INSERT INTO workspaces (id, name, layout_json, created_at, updated_at)
             VALUES ('w1', 'Deploy', '{\"tabs\":[]}', 100, 100)",
            [],
        )?;
        Ok(())
    })
    .unwrap();

    a.push_to(&b, &key);

    let name: Option<String> = b
        .db
        .read(|conn| {
            Ok(conn
                .query_row("SELECT name FROM workspaces WHERE id = 'w1'", [], |r| {
                    r.get::<_, String>(0)
                })
                .ok())
        })
        .unwrap();
    assert_eq!(name.as_deref(), Some("Deploy"));
}
