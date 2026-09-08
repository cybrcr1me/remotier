//! The single `sync_state` row, and the account keys it points at.

use rusqlite::{Connection, OptionalExtension, Transaction};
use zeroize::Zeroizing;

use crate::commands::secrets;
use crate::crypto::vault::Vault;
use crate::db::{new_id, Db};
use crate::error::{Error, Result};

/// What this machine knows about its account, minus anything sealed.
#[derive(Debug, Clone, Default)]
pub struct SyncState {
    pub instance_url: Option<String>,
    pub account_id: Option<String>,
    pub account_email: Option<String>,
    pub account_public: Option<String>,
    pub device_id: String,
    pub device_name: Option<String>,
    /// `session_state.updated_at` as of the last layout push. The layout has no dirty
    /// flag, so this is what stops it being sent on every cycle regardless.
    pub layout_pushed_at: i64,
    pub cursor: i64,
    pub last_sync_at: Option<i64>,
    pub personal_key_ref: Option<String>,
    pub account_secret_ref: Option<String>,
    pub access_token_ref: Option<String>,
    pub refresh_token_ref: Option<String>,
}

impl SyncState {
    pub fn signed_in(&self) -> bool {
        self.account_id.is_some() && self.personal_key_ref.is_some()
    }
}

/// Read the row, creating it with a fresh device id on first call.
///
/// The device id is minted here rather than at sign-in because it identifies the machine,
/// not the account: signing out and back in must not change how this device breaks a
/// last-write-wins tie, or two devices could swap identities mid-conflict.
pub fn load(db: &Db) -> Result<SyncState> {
    if let Some(state) = db.read(read_row)? {
        return Ok(state);
    }

    let device_id = new_id();
    db.write(|tx| {
        tx.execute(
            "INSERT INTO sync_state (id, device_id, cursor) VALUES (1, ?1, 0)
             ON CONFLICT(id) DO NOTHING",
            [&device_id],
        )?;
        Ok(())
    })?;

    db.read(read_row)?.ok_or_else(|| {
        Error::Internal("sync_state row missing immediately after insert".into())
    })
}

fn read_row(conn: &Connection) -> Result<Option<SyncState>> {
    let row = conn
        .query_row(
            "SELECT instance_url, account_id, account_email, account_public, device_id,
                    cursor, last_sync_at, personal_key_ref, account_secret_ref,
                    access_token_ref, refresh_token_ref, device_name, layout_pushed_at
             FROM sync_state WHERE id = 1",
            [],
            |row| {
                Ok(SyncState {
                    instance_url: row.get(0)?,
                    account_id: row.get(1)?,
                    account_email: row.get(2)?,
                    account_public: row.get(3)?,
                    device_id: row.get(4)?,
                    cursor: row.get(5)?,
                    last_sync_at: row.get(6)?,
                    personal_key_ref: row.get(7)?,
                    account_secret_ref: row.get(8)?,
                    access_token_ref: row.get(9)?,
                    refresh_token_ref: row.get(10)?,
                    device_name: row.get(11)?,
                    layout_pushed_at: row.get(12)?,
                })
            },
        )
        .optional()?;
    Ok(row)
}

/// Store the account, its keys and its session after a successful sign-in.
#[allow(clippy::too_many_arguments)]
pub fn save_account(
    tx: &Transaction,
    vault: &Vault,
    existing: &SyncState,
    instance_url: &str,
    account_id: &str,
    email: &str,
    device_name: &str,
    account_public: &str,
    personal_key: &[u8],
    account_secret: &[u8],
    access: &str,
    refresh: &str,
) -> Result<()> {
    // Reusing the existing refs keeps one secrets row per slot rather than orphaning one
    // on every sign-in - `secrets::put` updates in place when given a ref.
    let personal_ref = secrets::put(
        tx,
        vault,
        existing.personal_key_ref.as_deref(),
        &hex_encode(personal_key),
    )?;
    let secret_ref = secrets::put(
        tx,
        vault,
        existing.account_secret_ref.as_deref(),
        &hex_encode(account_secret),
    )?;
    let access_ref = secrets::put(tx, vault, existing.access_token_ref.as_deref(), access)?;
    let refresh_ref = secrets::put(tx, vault, existing.refresh_token_ref.as_deref(), refresh)?;

    tx.execute(
        "UPDATE sync_state SET instance_url = ?1, account_id = ?2, account_email = ?3,
                account_public = ?4, personal_key_ref = ?5, account_secret_ref = ?6,
                access_token_ref = ?7, refresh_token_ref = ?8, device_name = ?9
         WHERE id = 1",
        rusqlite::params![
            instance_url,
            account_id,
            email,
            account_public,
            personal_ref,
            secret_ref,
            access_ref,
            refresh_ref,
            device_name
        ],
    )?;
    Ok(())
}

/// Remember that this machine's layout has been sent up to this point.
pub fn save_layout_pushed(tx: &Transaction, at: i64) -> Result<()> {
    tx.execute(
        "UPDATE sync_state SET layout_pushed_at = ?1 WHERE id = 1",
        [at],
    )?;
    Ok(())
}

/// Replace just the session, after a token refresh.
pub fn save_session(
    tx: &Transaction,
    vault: &Vault,
    existing: &SyncState,
    access: &str,
    refresh: &str,
) -> Result<()> {
    let access_ref = secrets::put(tx, vault, existing.access_token_ref.as_deref(), access)?;
    let refresh_ref = secrets::put(tx, vault, existing.refresh_token_ref.as_deref(), refresh)?;
    tx.execute(
        "UPDATE sync_state SET access_token_ref = ?1, refresh_token_ref = ?2 WHERE id = 1",
        rusqlite::params![access_ref, refresh_ref],
    )?;
    Ok(())
}

pub fn save_cursor(tx: &Transaction, cursor: i64, at: i64) -> Result<()> {
    tx.execute(
        "UPDATE sync_state SET cursor = ?1, last_sync_at = ?2 WHERE id = 1",
        rusqlite::params![cursor, at],
    )?;
    Ok(())
}

/// Forget the account. The device id survives, because the machine has not changed.
///
/// Every synced record is left in place and marked dirty again, so signing back in - to
/// this instance or another - pushes what is here rather than silently dropping it. The
/// alternative, wiping local data on sign-out, turns a mis-click into data loss.
pub fn sign_out(tx: &Transaction, existing: &SyncState) -> Result<()> {
    for reference in [
        &existing.personal_key_ref,
        &existing.account_secret_ref,
        &existing.access_token_ref,
        &existing.refresh_token_ref,
    ] {
        secrets::delete(tx, reference.as_deref())?;
    }

    tx.execute(
        "UPDATE sync_state SET instance_url = NULL, account_id = NULL, account_email = NULL,
                account_public = NULL, personal_key_ref = NULL, account_secret_ref = NULL,
                access_token_ref = NULL, refresh_token_ref = NULL, cursor = 0,
                last_sync_at = NULL, layout_pushed_at = 0
         WHERE id = 1",
        [],
    )?;
    // Other machines' layouts are meaningless without the account that carried them.
    tx.execute("DELETE FROM device_layouts", [])?;
    tx.execute(
        "UPDATE sync_meta SET local_dirty = 1, server_seq = NULL",
        [],
    )?;
    Ok(())
}

/// Read a sealed secret back, or explain what is missing.
pub fn unseal(conn: &Connection, vault: &Vault, reference: Option<&str>) -> Result<Zeroizing<String>> {
    let reference = reference.ok_or_else(|| Error::Sync("not signed in".into()))?;
    secrets::get(conn, vault, reference)
}

fn hex_encode(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

pub fn hex_decode(s: &str) -> Result<Zeroizing<Vec<u8>>> {
    hex::decode(s)
        .map(Zeroizing::new)
        .map_err(|_| Error::Sync("stored key material is not valid hex".into()))
}
