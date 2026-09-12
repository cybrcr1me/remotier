//! Group content keys: which records they cover, and where they are kept.
//!
//! A shared group has a key of its own. Everything in that group - and in every group
//! beneath it - is encrypted under that key rather than the personal one, which is what
//! lets a colleague read exactly one branch of the tree and nothing else.

use std::collections::HashMap;

use remotier_sync_proto::crypto::ContentKey;
use remotier_sync_proto::record::KeyRef;
use rusqlite::{Connection, OptionalExtension, Transaction};
use zeroize::Zeroizing;

use crate::commands::secrets;
use crate::crypto::vault::Vault;
use crate::db::{now_ms, query_all};
use crate::error::{Error, Result};

/// A group key held on this machine.
pub struct GroupKey {
    pub group_id: String,
    pub key: ContentKey,
    /// `None` when this account owns the group. Only an owner may share or rotate.
    pub owner_id: Option<String>,
}

/// Every key this machine can seal or open a record with.
///
/// Built once per cycle. `collect` asks it which key a record should use; `apply` asks it
/// whether it holds the key a record arrived under.
pub struct Keyring {
    personal: ContentKey,
    groups: HashMap<String, GroupKey>,
    /// Group id to the group whose key covers it - itself, or the nearest shared
    /// ancestor. Absent means the personal key.
    cover: HashMap<String, String>,
}

impl Keyring {
    pub fn new(
        conn: &Connection,
        vault: &Vault,
        personal: ContentKey,
    ) -> Result<Self> {
        let groups = load_keys(conn, vault)?;
        let cover = key_owner_map(conn, &groups)?;
        Ok(Self {
            personal,
            groups,
            cover,
        })
    }

    /// Personal-only, for a machine that is in no shared group.
    pub fn personal_only(personal: ContentKey) -> Self {
        Self {
            personal,
            groups: HashMap::new(),
            cover: HashMap::new(),
        }
    }

    /// Which key seals a record living in `group_id`, and how to label it on the wire.
    pub fn select(&self, group_id: Option<&str>) -> (&ContentKey, KeyRef) {
        let covering = group_id.and_then(|id| self.cover.get(id));
        match covering.and_then(|id| self.groups.get(id)) {
            Some(group) => (
                &group.key,
                KeyRef::Group {
                    group_id: group.group_id.clone(),
                },
            ),
            None => (&self.personal, KeyRef::Personal),
        }
    }

    /// The key a pulled record was sealed with, or `None` when this machine does not hold
    /// it - a share that has been revoked, or a rotation not yet fetched.
    pub fn open_with(&self, key_ref: &KeyRef) -> Option<&ContentKey> {
        match key_ref {
            KeyRef::Personal => Some(&self.personal),
            KeyRef::Group { group_id } => self.groups.get(group_id).map(|g| &g.key),
        }
    }

    pub fn personal(&self) -> &ContentKey {
        &self.personal
    }

    /// Groups this account owns a key for and did not receive from someone else. Only an
    /// owner may share or rotate.
    pub fn owned(&self) -> impl Iterator<Item = &GroupKey> {
        self.groups.values().filter(|g| g.owner_id.is_none())
    }

    pub fn get(&self, group_id: &str) -> Option<&GroupKey> {
        self.groups.get(group_id)
    }
}

/// Every group key this machine holds, ready to seal and open with.
pub fn load_keys(conn: &Connection, vault: &Vault) -> Result<HashMap<String, GroupKey>> {
    let rows = query_all(
        conn,
        "SELECT group_id, key_ref, owner_id FROM group_keys",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        },
    )?;

    let mut out = HashMap::with_capacity(rows.len());
    for (group_id, key_ref, owner_id) in rows {
        // A key that will not unseal is a key store problem, not a reason to fail the
        // whole sync: the records under it are skipped and everything else still moves.
        match secrets::get(conn, vault, &key_ref) {
            Ok(hex_key) => match decode_key(&hex_key) {
                Ok(key) => {
                    out.insert(
                        group_id.clone(),
                        GroupKey {
                            group_id,
                            key,
                            owner_id,
                        },
                    );
                }
                Err(e) => log::warn!("sync: group key for {group_id} is unusable: {e}"),
            },
            Err(e) => log::warn!("sync: could not unseal the group key for {group_id}: {e}"),
        }
    }
    Ok(out)
}

pub fn save_key(
    tx: &Transaction,
    vault: &Vault,
    group_id: &str,
    key: &ContentKey,
    owner_id: Option<&str>,
) -> Result<()> {
    let existing: Option<String> = tx
        .query_row(
            "SELECT key_ref FROM group_keys WHERE group_id = ?1",
            [group_id],
            |row| row.get(0),
        )
        .optional()?;

    let key_ref = secrets::put(tx, vault, existing.as_deref(), &hex::encode(key.as_slice()))?;

    tx.execute(
        "INSERT INTO group_keys (group_id, key_ref, owner_id, generation, updated_at)
         VALUES (?1, ?2, ?3, 1, ?4)
         ON CONFLICT(group_id) DO UPDATE SET
             key_ref = excluded.key_ref, owner_id = excluded.owner_id,
             generation = group_keys.generation + 1, updated_at = excluded.updated_at",
        rusqlite::params![group_id, key_ref, owner_id, now_ms()],
    )?;
    Ok(())
}

pub fn forget_key(tx: &Transaction, group_id: &str) -> Result<()> {
    let existing: Option<String> = tx
        .query_row(
            "SELECT key_ref FROM group_keys WHERE group_id = ?1",
            [group_id],
            |row| row.get(0),
        )
        .optional()?;

    tx.execute("DELETE FROM group_keys WHERE group_id = ?1", [group_id])?;
    secrets::delete(tx, existing.as_deref())?;
    Ok(())
}

/// Every group id that sits at or beneath `group_id`, itself included.
///
/// Sharing covers a subtree, so rotation and re-encryption have to reach all of it.
pub fn subtree(conn: &Connection, group_id: &str) -> Result<Vec<String>> {
    let edges = query_all(
        conn,
        "SELECT id, parent_id FROM groups",
        [],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
    )?;

    let mut children: HashMap<Option<String>, Vec<String>> = HashMap::new();
    for (id, parent) in edges {
        children.entry(parent).or_default().push(id);
    }

    let mut out = vec![group_id.to_string()];
    let mut queue = vec![group_id.to_string()];
    // A malformed parent chain must not spin here: a group is only ever visited once.
    let mut seen: std::collections::HashSet<String> = std::iter::once(group_id.to_string()).collect();

    while let Some(current) = queue.pop() {
        for child in children.get(&Some(current)).into_iter().flatten() {
            if seen.insert(child.clone()) {
                out.push(child.clone());
                queue.push(child.clone());
            }
        }
    }
    Ok(out)
}

/// For every group, the group whose key encrypts it: itself, or the nearest ancestor
/// that has one.
///
/// Built once per cycle rather than walked per record. A group with no shared ancestor
/// is absent from the map, which means "use the personal key".
pub fn key_owner_map(
    conn: &Connection,
    keys: &HashMap<String, GroupKey>,
) -> Result<HashMap<String, String>> {
    let parents: HashMap<String, Option<String>> = query_all(
        conn,
        "SELECT id, parent_id FROM groups",
        [],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
    )?
    .into_iter()
    .collect();

    let mut out = HashMap::new();
    for id in parents.keys() {
        if let Some(owner) = nearest_keyed(id, &parents, keys) {
            out.insert(id.clone(), owner);
        }
    }
    Ok(out)
}

fn nearest_keyed(
    start: &str,
    parents: &HashMap<String, Option<String>>,
    keys: &HashMap<String, GroupKey>,
) -> Option<String> {
    let mut current = Some(start.to_string());
    // Bounded by the number of groups: a cycle in the parent chain would otherwise hang
    // the sync worker, and `buildTree` on the frontend already tolerates one.
    let mut steps = 0;
    while let Some(id) = current {
        if keys.contains_key(&id) {
            return Some(id);
        }
        if steps > parents.len() {
            log::warn!("sync: group parent chain from {start} does not terminate");
            return None;
        }
        steps += 1;
        current = parents.get(&id).cloned().flatten();
    }
    None
}

fn decode_key(hex_key: &str) -> Result<ContentKey> {
    let bytes = hex::decode(hex_key)
        .map_err(|_| Error::Sync("a stored group key is not valid hex".into()))?;
    let array: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| Error::Sync("a stored group key is the wrong length".into()))?;
    Ok(Zeroizing::new(array))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn tree(db: &Db, rows: &[(&str, Option<&str>)]) {
        db.write(|tx| {
            for (id, parent) in rows {
                tx.execute(
                    "INSERT INTO groups (id, parent_id, name, sort, created_at, updated_at)
                     VALUES (?1, ?2, ?1, 0, 0, 0)",
                    rusqlite::params![id, parent],
                )?;
            }
            Ok(())
        })
        .unwrap()
    }

    fn keyed(ids: &[&str]) -> HashMap<String, GroupKey> {
        ids.iter()
            .map(|id| {
                (
                    id.to_string(),
                    GroupKey {
                        group_id: id.to_string(),
                        key: remotier_sync_proto::crypto::random_key(),
                        owner_id: None,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn a_subtree_is_the_group_and_everything_under_it() {
        let db = Db::open_in_memory().unwrap();
        tree(
            &db,
            &[
                ("root", None),
                ("a", Some("root")),
                ("b", Some("root")),
                ("a1", Some("a")),
                ("elsewhere", None),
            ],
        );

        let mut found = db.read(|conn| subtree(conn, "root")).unwrap();
        found.sort();
        assert_eq!(found, ["a", "a1", "b", "root"]);
    }

    #[test]
    fn a_leaf_subtree_is_just_itself() {
        let db = Db::open_in_memory().unwrap();
        tree(&db, &[("root", None), ("a", Some("root"))]);
        assert_eq!(db.read(|conn| subtree(conn, "a")).unwrap(), ["a"]);
    }

    #[test]
    fn a_group_below_a_shared_one_uses_the_shared_key() {
        // Sharing covers a subtree. A host three levels down is still in the group that
        // was shared, and must be encrypted so the member can read it.
        let db = Db::open_in_memory().unwrap();
        tree(
            &db,
            &[("root", None), ("a", Some("root")), ("a1", Some("a"))],
        );

        let keys = keyed(&["a"]);
        let map = db.read(|conn| key_owner_map(conn, &keys)).unwrap();

        assert_eq!(map.get("a").map(String::as_str), Some("a"));
        assert_eq!(map.get("a1").map(String::as_str), Some("a"));
        // Above the shared group is not covered by it.
        assert_eq!(map.get("root"), None);
    }

    #[test]
    fn the_nearest_shared_ancestor_wins() {
        // Sharing a subgroup of an already shared group hands out a narrower key; the
        // records below must use that one, not the wider one.
        let db = Db::open_in_memory().unwrap();
        tree(
            &db,
            &[("root", None), ("a", Some("root")), ("a1", Some("a"))],
        );

        let keys = keyed(&["root", "a"]);
        let map = db.read(|conn| key_owner_map(conn, &keys)).unwrap();

        assert_eq!(map.get("a1").map(String::as_str), Some("a"));
        assert_eq!(map.get("root").map(String::as_str), Some("root"));
    }

    #[test]
    fn nothing_shared_means_nothing_mapped() {
        let db = Db::open_in_memory().unwrap();
        tree(&db, &[("root", None), ("a", Some("root"))]);
        let map = db.read(|conn| key_owner_map(conn, &HashMap::new())).unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn a_parent_cycle_does_not_hang() {
        // SQLite will not stop a group being made its own ancestor by two edits that are
        // each valid. The walk has to survive it, as buildTree already does.
        let db = Db::open_in_memory().unwrap();
        tree(&db, &[("a", None), ("b", Some("a"))]);
        db.write(|tx| {
            tx.execute("UPDATE groups SET parent_id = 'b' WHERE id = 'a'", [])?;
            Ok(())
        })
        .unwrap();

        let map = db.read(|conn| key_owner_map(conn, &keyed(&["zzz"]))).unwrap();
        assert!(map.is_empty());

        let found = db.read(|conn| subtree(conn, "a")).unwrap();
        assert_eq!(found.len(), 2, "each group visited once");
    }
}
