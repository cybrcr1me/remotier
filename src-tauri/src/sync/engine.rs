//! The sync engine: what the commands call, and what the background worker drives.
//!
//! # The lock rule
//!
//! `Db` is a single `Mutex<Connection>` whose contract is "nothing awaits while the guard
//! is held". Every cycle here is therefore three phases: read under the lock, **release**,
//! talk to the server, reacquire to write. A worker that held the guard across an HTTP
//! round trip would stall every terminal command in the app for as long as the network
//! took, which on a bad connection is thirty seconds.

use std::sync::Arc;

use remotier_sync_proto::api::{
    KeyMaterial, LoginRequest, RecoverRequest, RegisterRequest, SealedBlob, Session, Share,
    ShareRequest,
};
use remotier_sync_proto::crypto::{self, ContentKey};
use remotier_sync_proto::record::Envelope;
use serde::Serialize;
use tokio::sync::{Mutex, RwLock};
use zeroize::Zeroizing;

use crate::crypto::vault::Vault;
use crate::db::{now_ms, Db};
use crate::error::{Error, Result};

use super::client::Client;
use super::groups::Keyring;
use super::history::{self, Action, Entry};
use super::{apply, collect, groups, state};

/// How many records go in one push. Bounds the request a proxy has to accept, and keeps
/// a first sync of a large inventory from being one enormous body.
const PUSH_BATCH: usize = 200;

/// Another machine, and when it last saved its tabs.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceLayout {
    pub device_id: String,
    pub name: String,
    pub updated_at: i64,
}

/// What the UI shows.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub signed_in: bool,
    pub instance_url: Option<String>,
    pub email: Option<String>,
    pub device_id: String,
    pub last_sync_at: Option<i64>,
    /// Records waiting to be pushed.
    pub pending: i64,
    pub syncing: bool,
    /// Local rows written or deleted by the last cycle.
    ///
    /// The engine writes SQLite directly, so the Pinia stores hold a stale copy until
    /// they reload. Rather than have the frontend poll or reload on every tick, the
    /// status says when there was actually something to see.
    pub applied: i64,
    /// The last failure, cleared by the next success. Sync failing is not an error state
    /// the user has to dismiss - it is a condition that usually fixes itself.
    pub error: Option<String>,
}

/// The account keys, in memory while signed in.
struct Keys {
    personal: ContentKey,
    /// Held for group sharing, which unwraps a group key sealed to this account.
    #[allow(dead_code)]
    account_secret: Zeroizing<Vec<u8>>,
}

pub struct SyncEngine {
    db: Arc<Db>,
    /// `None` when the key store could not be opened. Sync then stays off, the same way
    /// every other secret-touching feature degrades - `AppState::vault` never panics.
    vault: Option<Arc<Vault>>,
    keys: RwLock<Option<Keys>>,
    /// One cycle at a time. Two overlapping cycles would push the same records twice and
    /// race each other's cursor writes.
    cycle: Mutex<()>,
    last_error: RwLock<Option<String>>,
    syncing: RwLock<bool>,
    applied: RwLock<i64>,
}

impl SyncEngine {
    pub fn new(db: Arc<Db>, vault: Option<Arc<Vault>>) -> Self {
        Self {
            db,
            vault,
            keys: RwLock::new(None),
            cycle: Mutex::new(()),
            last_error: RwLock::new(None),
            syncing: RwLock::new(false),
            applied: RwLock::new(0),
        }
    }

    /// Load the account keys from the local vault, if this machine is signed in.
    ///
    /// Called at startup. A missing or unreadable vault is not fatal: sync simply stays
    /// off, the same way every other secret-touching feature degrades.
    pub async fn restore(&self) -> Result<()> {
        let state = state::load(&self.db)?;
        if !state.signed_in() {
            return Ok(());
        }
        let Some(vault) = self.vault.as_deref() else {
            log::warn!("sync: key store unavailable, staying signed out");
            return Ok(());
        };

        let (personal, secret) = self.db.read(|conn| {
            let personal = state::unseal(conn, vault, state.personal_key_ref.as_deref())?;
            let secret = state::unseal(conn, vault, state.account_secret_ref.as_deref())?;
            Ok((personal, secret))
        })?;

        let personal = crypto_key(&state::hex_decode(&personal)?)?;
        let account_secret = state::hex_decode(&secret)?;

        *self.keys.write().await = Some(Keys {
            personal,
            account_secret,
        });
        Ok(())
    }

    pub async fn status(&self) -> Result<SyncStatus> {
        let state = state::load(&self.db)?;
        let pending = self.db.read(|conn| {
            Ok(conn.query_row(
                "SELECT count(*) FROM sync_meta WHERE local_dirty = 1",
                [],
                |row| row.get::<_, i64>(0),
            )?)
        })?;

        Ok(SyncStatus {
            signed_in: state.signed_in() && self.keys.read().await.is_some(),
            instance_url: state.instance_url,
            email: state.account_email,
            device_id: state.device_id,
            last_sync_at: state.last_sync_at,
            pending,
            syncing: *self.syncing.read().await,
            applied: *self.applied.read().await,
            error: self.last_error.read().await.clone(),
        })
    }

    /// The most recent records this device sent or received.
    ///
    /// Readable while signed out - signing out clears it, so anything still here belongs
    /// to the session the user is looking at.
    pub fn history(&self, limit: usize) -> Result<Vec<history::HistoryEntry>> {
        self.db.read(|conn| history::recent(conn, limit))
    }

    /// Create an account. Returns the recovery code, which is shown once and never again.
    pub async fn register(
        &self,
        instance_url: &str,
        email: &str,
        password: &str,
        device_name: &str,
    ) -> Result<Zeroizing<String>> {
        let state = state::load(&self.db)?;
        let client = Client::new(instance_url)?;

        let keys = crypto::derive_account_keys(password, email)?;
        let recovery_code = crypto::generate_recovery_code();
        let recovery = crypto::derive_recovery_keys(&recovery_code)?;

        // One keypair and one content key, wrapped twice: once under the password and
        // once under the recovery code. Both doors open the same room, which is why a
        // password change later re-encrypts no records at all.
        let (account_secret, account_public) = crypto::generate_account_keypair();
        let personal = crypto::random_key();
        let secret_bytes = Zeroizing::new(account_secret.to_bytes().to_vec());

        let session = client
            .register(&RegisterRequest {
                email: email.trim().to_lowercase(),
                auth_key: hex::encode(keys.auth.as_slice()),
                key_material: wrap_material(&keys.wrap, &account_public, &secret_bytes, &personal)?,
                recovery_material: wrap_material(
                    &recovery.wrap,
                    &account_public,
                    &secret_bytes,
                    &personal,
                )?,
                recovery_auth_key: hex::encode(recovery.auth.as_slice()),
                device_id: state.device_id.clone(),
                device_name: device_name.to_string(),
            })
            .await?;

        self.adopt(&state, instance_url, &session, device_name, personal, secret_bytes)
            .await?;
        Ok(recovery_code)
    }

    pub async fn login(
        &self,
        instance_url: &str,
        email: &str,
        password: &str,
        device_name: &str,
    ) -> Result<()> {
        let state = state::load(&self.db)?;
        let client = Client::new(instance_url)?;
        let keys = crypto::derive_account_keys(password, email)?;

        let session = client
            .login(&LoginRequest {
                email: email.trim().to_lowercase(),
                auth_key: hex::encode(keys.auth.as_slice()),
                device_id: state.device_id.clone(),
                device_name: device_name.to_string(),
            })
            .await?;

        let (personal, secret) = unwrap_material(&keys.wrap, &session.key_material)?;
        self.adopt(&state, instance_url, &session, device_name, personal, secret)
            .await
    }

    /// Sign in with the recovery code. The server returns the recovery-wrapped blobs,
    /// which hold the same two keys sealed under a different wrapping key.
    pub async fn recover(
        &self,
        instance_url: &str,
        email: &str,
        recovery_code: &str,
        device_name: &str,
    ) -> Result<()> {
        let state = state::load(&self.db)?;
        let client = Client::new(instance_url)?;
        let keys = crypto::derive_recovery_keys(recovery_code)?;

        let session = client
            .recover(&RecoverRequest {
                email: email.trim().to_lowercase(),
                recovery_auth_key: hex::encode(keys.auth.as_slice()),
                device_id: state.device_id.clone(),
                device_name: device_name.to_string(),
            })
            .await?;

        let (personal, secret) = unwrap_material(&keys.wrap, &session.key_material)?;
        self.adopt(&state, instance_url, &session, device_name, personal, secret)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn adopt(
        &self,
        state: &state::SyncState,
        instance_url: &str,
        session: &Session,
        device_name: &str,
        personal: ContentKey,
        account_secret: Zeroizing<Vec<u8>>,
    ) -> Result<()> {
        let vault = self.vault()?;
        self.db.write(|tx| {
            state::save_account(
                tx,
                vault,
                state,
                instance_url,
                &session.account_id,
                &session.email,
                device_name,
                &session.key_material.account_public,
                personal.as_slice(),
                &account_secret,
                &session.access_token,
                &session.refresh_token,
            )
        })?;

        *self.keys.write().await = Some(Keys {
            personal,
            account_secret,
        });
        *self.last_error.write().await = None;
        Ok(())
    }

    /// Sign out. Local data stays and is marked dirty again, so signing back in pushes it
    /// rather than losing it - a mis-click must not be a way to delete everything.
    pub async fn logout(&self) -> Result<()> {
        let state = state::load(&self.db)?;

        // Best effort: the server should forget the session, but a server that cannot be
        // reached must not trap the user signed in on this device.
        if let (Some(url), Some(_)) = (&state.instance_url, &state.access_token_ref) {
            if let Ok(client) = Client::new(url) {
                if let Ok(token) = self.access_token(&state) {
                    let _ = client.logout(&token).await;
                }
            }
        }

        self.db.write(|tx| state::sign_out(tx, &state))?;
        *self.keys.write().await = None;
        *self.last_error.write().await = None;
        Ok(())
    }

    /// One full cycle: push what is dirty, pull what is new, apply it.
    pub async fn sync_now(&self) -> Result<SyncStatus> {
        let _guard = self.cycle.lock().await;
        *self.syncing.write().await = true;
        // Reset before the cycle: `applied` describes this cycle, so a stale count from
        // the last one must not make the UI reload for nothing.
        *self.applied.write().await = 0;

        let outcome = self.cycle().await;

        *self.syncing.write().await = false;
        match outcome {
            Ok(()) => *self.last_error.write().await = None,
            Err(ref e) => *self.last_error.write().await = Some(e.to_string()),
        }
        // A failed cycle still reports status: the UI shows the error beside the last
        // successful sync rather than losing both.
        outcome?;
        self.status().await
    }

    async fn cycle(&self) -> Result<()> {
        let state = state::load(&self.db)?;
        if !state.signed_in() {
            return Err(Error::Sync("not signed in".into()));
        }
        let url = state
            .instance_url
            .clone()
            .ok_or_else(|| Error::Sync("no instance configured".into()))?;

        let client = Client::new(&url)?;
        let access = self.access_token(&state)?;

        match self.run(&client, &access, &state).await {
            // The access token lives an hour and this app stays open for days, so an
            // expired one is the normal case, not an exception. Refresh once and retry;
            // a second failure means the session is genuinely gone.
            Err(Error::SyncUnauthorised) => {
                let access = self.refresh_session(&state).await?;
                // Re-read: the refresh replaced the stored token refs.
                let state = state::load(&self.db)?;
                self.run(&client, &access, &state).await
            }
            other => other,
        }
    }

    async fn run(&self, client: &Client, access: &str, state: &state::SyncState) -> Result<()> {
        // Keys first: a record sealed under a group key this machine has not collected
        // yet would be skipped, and skipped records are not retried until they change.
        //
        // Not fatal, though. Sharing is optional and this is the only call in a cycle
        // that a server without it would refuse - letting it abort would mean an older
        // instance, or one blip on this endpoint, stops a device syncing its own data at
        // all. The worst case is that a shared group stays unreadable for another minute.
        if let Err(e) = self.fetch_shared_keys(client, access).await {
            log::warn!("sync: could not refresh shared group keys: {e}");
        }

        self.push(client, access, state).await?;
        self.push_layout(client, access, state).await?;
        self.pull(client, access, state).await
    }

    /// Send this machine's open tabs, if they have moved since the last time.
    ///
    /// `session_state` has no dirty flag on purpose - it is rewritten 400ms after every
    /// layout change, and a trigger would turn that into a push every 400ms. Comparing
    /// its clock against the last one sent gets the same result at cycle granularity.
    async fn push_layout(
        &self,
        client: &Client,
        access: &str,
        state: &state::SyncState,
    ) -> Result<()> {
        let Some(device_name) = state.device_name.clone() else {
            // Signed in before this build knew how to name a device. The next sign-in
            // fills it in; sending an unnamed layout would leave the other machines with
            // a row they cannot label.
            return Ok(());
        };

        let envelope = {
            let keyring = self.keyring().await?;
            self.db.read(|conn| {
                collect::device_layout(conn, keyring.personal(), &state.device_id, &device_name)
            })?
        };
        let Some(envelope) = envelope else {
            return Ok(());
        };
        if envelope.updated_at <= state.layout_pushed_at {
            return Ok(());
        }

        let sent_at = envelope.updated_at;
        let response = client.push(access, vec![envelope]).await?;
        if !response.accepted.is_empty() {
            self.db
                .write(|tx| state::save_layout_pushed(tx, sent_at))?;
        }
        Ok(())
    }

    async fn push(&self, client: &Client, access: &str, state: &state::SyncState) -> Result<()> {
        loop {
            // Phase one: read under the lock and get out.
            let batch = {
                let keyring = self.keyring().await?;
                self.db
                    .read(|conn| collect::collect(conn, &keyring, &state.device_id, PUSH_BATCH))?
            };
            if batch.is_empty() {
                return Ok(());
            }
            let consumed = batch.consumed();

            // Records that will never be sent - a setting the allow-list excludes, say -
            // are cleared here rather than left dirty. Leaving them set makes the pending
            // count stick above zero permanently, so the UI reports work that is never
            // going to happen and every cycle re-examines the same rows.
            if !batch.skipped.is_empty() {
                let skipped = batch.skipped;
                self.db.write(|tx| collect::mark_skipped(tx, &skipped))?;
            }
            if batch.envelopes.is_empty() {
                return Ok(());
            }

            // Which of these were deletions, kept for the history: the envelopes
            // themselves are handed to the client, and a tombstone is indistinguishable
            // from a write once only its id has come back.
            let tombstones: Vec<String> = batch
                .envelopes
                .iter()
                .filter(|e| e.deleted_at.is_some())
                .map(|e| format!("{}:{}", e.kind.as_str(), e.id))
                .collect();

            // Phase two: the network, with no lock held.
            let response = client.push(access, batch.envelopes).await?;

            // Phase three: record what happened.
            self.db.write(|tx| {
                collect::mark_pushed(tx, &response.accepted)?;
                let sent = pushed_entries(tx, &response.accepted, &tombstones)?;
                history::record(tx, history::Direction::Push, &sent)?;
                for refused in &response.rejected {
                    use remotier_sync_proto::api::RejectReason;
                    match refused.reason {
                        // The server has a newer copy. Not dirty any more; the pull
                        // brings it. Leaving it dirty would push it forever.
                        RejectReason::Stale => {
                            collect::mark_stale(tx, &refused.kind, &refused.id)?;
                        }
                        // These need a human. Leave them dirty and say so.
                        reason => log::warn!(
                            "sync: server refused {} {} ({reason:?})",
                            refused.kind,
                            refused.id
                        ),
                    }
                }
                Ok(())
            })?;

            let refused_permanently = response
                .rejected
                .iter()
                .filter(|r| !matches!(r.reason, remotier_sync_proto::api::RejectReason::Stale))
                .count();

            // Everything in this batch was refused for a reason retrying will not fix.
            // Without this the loop would collect the same batch forever.
            if response.accepted.is_empty() && refused_permanently > 0 {
                return Err(Error::Sync(format!(
                    "the server refused {refused_permanently} record(s) - check this device's clock"
                )));
            }
            if consumed < PUSH_BATCH {
                return Ok(());
            }
        }
    }

    async fn pull(&self, client: &Client, access: &str, state: &state::SyncState) -> Result<()> {
        // Every page is gathered before anything is applied, so the whole set lands in
        // one transaction. A host and the group it belongs to can arrive on either side
        // of a page boundary, and applying page by page would fail on the foreign key.
        let mut cursor = state.cursor;
        let mut batch: Vec<Envelope> = Vec::new();

        loop {
            let page = client.pull(access, cursor).await?;
            let more = page.more;
            cursor = page.cursor;
            batch.extend(page.envelopes);
            if !more {
                break;
            }
            // A runaway server that always says "more" must not fill memory.
            if batch.len() > 50_000 {
                return Err(Error::Sync("the server sent an implausible number of records".into()));
            }
        }

        if batch.is_empty() {
            self.db.write(|tx| state::save_cursor(tx, cursor, now_ms()))?;
            return Ok(());
        }

        let keyring = self.keyring().await?;

        let applied = self.db.write(|tx| {
            let applied = apply::apply(tx, &keyring, &state.device_id, &batch)?;
            history::record(tx, history::Direction::Pull, &applied.entries)?;
            state::save_cursor(tx, cursor, now_ms())?;
            Ok(applied)
        })?;

        log::info!(
            "sync: applied {} wrote, {} deleted, {} skipped",
            applied.written,
            applied.deleted,
            applied.skipped
        );
        *self.applied.write().await = (applied.written + applied.deleted) as i64;
        Ok(())
    }

    /// Build the cycle's keyring: the personal key plus every group key held here.
    ///
    /// Rebuilt per cycle rather than cached, because a share can arrive or be revoked
    /// between two cycles and a stale keyring would seal records with a key the far side
    /// can no longer open.
    async fn keyring(&self) -> Result<Keyring> {
        let keys = self.keys.read().await;
        let keys = keys.as_ref().ok_or(Error::SyncUnauthorised)?;
        let personal = keys.personal.clone();

        match self.vault.as_deref() {
            Some(vault) => self.db.read(|conn| Keyring::new(conn, vault, personal.clone())),
            None => Ok(Keyring::personal_only(personal)),
        }
    }

    fn vault(&self) -> Result<&Vault> {
        self.vault
            .as_deref()
            .ok_or_else(|| Error::Sync("the key store is unavailable, so sync is off".into()))
    }

    // ---------------------------------------------------------------------------
    // Group sharing
    // ---------------------------------------------------------------------------

    /// Share a group, and everything beneath it, with another account on this instance.
    ///
    /// The group's content key is wrapped to the recipient's X25519 public key, so the
    /// server routes a blob it cannot open. Every record in the subtree is then marked
    /// dirty, because it has to be re-sealed under the group key before the recipient can
    /// read any of it.
    pub async fn share_group(&self, group_id: &str, email: &str) -> Result<()> {
        let state = state::load(&self.db)?;
        let vault = self.vault()?;
        let url = self.instance(&state)?;
        let client = Client::new(&url)?;
        let access = self.access_token(&state)?;

        let recipient = client.lookup(&access, email).await?;
        if Some(&recipient.user_id) == state.account_id.as_ref() {
            return Err(Error::Sync("that group is already yours".into()));
        }

        // Reuse the group's key if it has one, so adding a second member does not lock
        // the first one out.
        let existing = self
            .db
            .read(|conn| groups::load_keys(conn, vault))?
            .remove(group_id);
        let group_key = match existing {
            Some(held) => {
                if held.owner_id.is_some() {
                    return Err(Error::Sync(
                        "only the owner of a shared group can share it further".into(),
                    ));
                }
                held.key
            }
            None => {
                let fresh = crypto::random_key();
                self.db
                    .write(|tx| groups::save_key(tx, vault, group_id, &fresh, None))?;
                fresh
            }
        };

        let public = crypto::public_from_bytes(&decode_hex(&recipient.account_public)?)?;
        let wrapped = crypto::wrap_for(&public, &group_key)?;

        client
            .share(
                &access,
                &ShareRequest {
                    group_id: group_id.to_string(),
                    user_id: recipient.user_id,
                    wrapped_group_key: hex::encode(wrapped),
                },
            )
            .await?;

        self.reseal_subtree(group_id)?;
        Ok(())
    }

    pub async fn shares(&self, group_id: &str) -> Result<Vec<Share>> {
        let state = state::load(&self.db)?;
        let client = Client::new(&self.instance(&state)?)?;
        client
            .shares(&self.access_token(&state)?, group_id)
            .await
    }

    /// Stop sharing a group with someone, and rotate its key.
    ///
    /// Rotation is the whole point. The removed member already holds the old key and may
    /// have kept a copy of everything under it; deleting the share row only stops them
    /// receiving *new* records. A fresh key, re-wrapped for whoever remains and applied to
    /// the whole subtree, is what actually revokes access going forward.
    pub async fn unshare_group(&self, group_id: &str, user_id: &str) -> Result<()> {
        let state = state::load(&self.db)?;
        let vault = self.vault()?;
        let url = self.instance(&state)?;
        let client = Client::new(&url)?;
        let access = self.access_token(&state)?;

        client.unshare(&access, group_id, user_id).await?;

        let remaining = client.shares(&access, group_id).await?;
        if remaining.is_empty() {
            // Nobody left. Keep the key rather than dropping it: the records under it are
            // still sealed with it until the next push re-seals them personally.
            let rotated = crypto::random_key();
            self.db
                .write(|tx| groups::save_key(tx, vault, group_id, &rotated, None))?;
            self.reseal_subtree(group_id)?;
            return Ok(());
        }

        let rotated = crypto::random_key();
        for share in &remaining {
            let account = client.lookup(&access, &share.email).await?;
            let public = crypto::public_from_bytes(&decode_hex(&account.account_public)?)?;
            let wrapped = crypto::wrap_for(&public, &rotated)?;
            client
                .share(
                    &access,
                    &ShareRequest {
                        group_id: group_id.to_string(),
                        user_id: share.user_id.clone(),
                        wrapped_group_key: hex::encode(wrapped),
                    },
                )
                .await?;
        }

        self.db
            .write(|tx| groups::save_key(tx, vault, group_id, &rotated, None))?;
        self.reseal_subtree(group_id)?;
        Ok(())
    }

    /// Fetch the group keys other people have shared with this account.
    ///
    /// Run at the start of every cycle: a share can arrive at any time, and a record
    /// sealed under a key this machine has not collected yet is simply skipped.
    async fn fetch_shared_keys(&self, client: &Client, access: &str) -> Result<()> {
        let vault = self.vault()?;
        let account_secret = {
            let keys = self.keys.read().await;
            let keys = keys.as_ref().ok_or(Error::SyncUnauthorised)?;
            crypto::secret_from_bytes(&keys.account_secret)?
        };

        let mine = client.shares_for_me(access).await?;
        let held: Vec<String> = self
            .db
            .read(|conn| Ok(groups::load_keys(conn, vault)?.keys().cloned().collect()))?;

        for share in &mine {
            let wrapped = decode_hex(&share.wrapped_group_key)?;
            let key = match crypto::unwrap_with(&account_secret, &wrapped) {
                Ok(key) => key,
                Err(e) => {
                    // A wrap sealed to a different account, or a corrupt row. Skipping is
                    // right: the records under it stay unreadable rather than the whole
                    // sync failing.
                    log::warn!("sync: could not unwrap the key for group {}: {e}", share.group_id);
                    continue;
                }
            };
            self.db.write(|tx| {
                groups::save_key(tx, vault, &share.group_id, &key, Some(&share.owner_id))
            })?;
        }

        // A share that has gone means access was revoked. Dropping the key stops this
        // machine sealing anything new under it - the records themselves stay until the
        // owner's rotation reaches us.
        let still_shared: std::collections::HashSet<&str> =
            mine.iter().map(|s| s.group_id.as_str()).collect();
        for group_id in held {
            let ours = self
                .db
                .read(|conn| {
                    Ok(conn
                        .query_row(
                            "SELECT owner_id FROM group_keys WHERE group_id = ?1",
                            [&group_id],
                            |row| row.get::<_, Option<String>>(0),
                        )
                        .ok()
                        .flatten())
                })?
                .is_none();
            if !ours && !still_shared.contains(group_id.as_str()) {
                log::info!("sync: access to group {group_id} was revoked");
                self.db.write(|tx| groups::forget_key(tx, &group_id))?;
            }
        }
        Ok(())
    }

    /// Mark every record in a group's subtree dirty, so the next push re-seals it under
    /// whatever key now covers it.
    fn reseal_subtree(&self, group_id: &str) -> Result<()> {
        let ids = self.db.read(|conn| groups::subtree(conn, group_id))?;

        self.db.write(|tx| {
            for id in &ids {
                tx.execute(
                    "UPDATE sync_meta SET local_dirty = 1
                     WHERE kind = 'group' AND id = ?1",
                    [id],
                )?;
                tx.execute(
                    "UPDATE sync_meta SET local_dirty = 1
                     WHERE kind = 'host'
                       AND id IN (SELECT id FROM hosts WHERE group_id = ?1)",
                    [id],
                )?;
                tx.execute(
                    "UPDATE sync_meta SET local_dirty = 1
                     WHERE kind = 'var_def'
                       AND id IN (SELECT id FROM var_defs
                                  WHERE scope = 'group' AND scope_id = ?1)",
                    [id],
                )?;
            }
            Ok(())
        })
    }

    // ---------------------------------------------------------------------------
    // Other devices' layouts
    // ---------------------------------------------------------------------------

    /// The machines that have a saved layout, most recent first. This one is excluded -
    /// its tabs are already on screen.
    pub fn devices(&self) -> Result<Vec<DeviceLayout>> {
        let state = state::load(&self.db)?;
        self.db.read(|conn| {
            crate::db::query_all(
                conn,
                "SELECT device_id, device_name, updated_at FROM device_layouts
                 WHERE device_id <> ?1 ORDER BY updated_at DESC",
                [&state.device_id],
                |row| {
                    Ok(DeviceLayout {
                        device_id: row.get(0)?,
                        name: row.get(1)?,
                        updated_at: row.get(2)?,
                    })
                },
            )
        })
    }

    /// One device's stored layout, for the user to open deliberately.
    pub fn device_layout(&self, device_id: &str) -> Result<String> {
        self.db.read(|conn| {
            conn.query_row(
                "SELECT layout_json FROM device_layouts WHERE device_id = ?1",
                [device_id],
                |row| row.get::<_, String>(0),
            )
            .map_err(|_| Error::NotFound("device layout", device_id.to_string()))
        })
    }

    fn instance(&self, state: &state::SyncState) -> Result<String> {
        state
            .instance_url
            .clone()
            .ok_or_else(|| Error::Sync("no instance configured".into()))
    }

    /// Read the access token back out of the vault-sealed `secrets` row.
    fn access_token(&self, state: &state::SyncState) -> Result<String> {
        let vault = self.vault()?;
        let token = self
            .db
            .read(|conn| state::unseal(conn, vault, state.access_token_ref.as_deref()))?;
        Ok(token.to_string())
    }

    /// Trade the refresh token for a new session.
    ///
    /// Refresh tokens are single-use on the server, so the new pair must be stored before
    /// anything else can go wrong - losing it here would sign the device out for good.
    async fn refresh_session(&self, state: &state::SyncState) -> Result<String> {
        let vault = self.vault()?;
        let url = state
            .instance_url
            .clone()
            .ok_or_else(|| Error::Sync("no instance configured".into()))?;

        let refresh = self
            .db
            .read(|conn| state::unseal(conn, vault, state.refresh_token_ref.as_deref()))?;

        let session = Client::new(&url)?.refresh(&refresh).await?;
        self.db.write(|tx| {
            state::save_session(tx, vault, state, &session.access_token, &session.refresh_token)
        })?;
        Ok(session.access_token)
    }
}

/// History entries for the records the server took.
///
/// Only the accepted ones: a refused record did not sync, and listing it as though it
/// had is worse than not listing it. A tombstone has no local row left to name, so it is
/// recorded without a label and the panel shows its kind.
fn pushed_entries(
    tx: &rusqlite::Transaction,
    accepted: &[remotier_sync_proto::api::Accepted],
    tombstones: &[String],
) -> Result<Vec<Entry>> {
    let mut entries = Vec::with_capacity(accepted.len());
    for record in accepted {
        // An unrecognised kind is a newer build's record echoed back. Nothing sensible to
        // show, and `Entry` names the kinds this build knows.
        let Some(kind) = remotier_sync_proto::record::RecordKind::parse(&record.kind) else {
            continue;
        };
        let deleted = tombstones
            .iter()
            .any(|t| t == &format!("{}:{}", kind.as_str(), record.id));
        entries.push(Entry {
            kind: kind.as_str(),
            id: record.id.clone(),
            action: if deleted { Action::Deleted } else { Action::Written },
            label: if deleted {
                None
            } else {
                history::label_of(tx, kind.as_str(), &record.id)?
            },
        });
    }
    Ok(entries)
}

fn decode_hex(s: &str) -> Result<Vec<u8>> {
    hex::decode(s).map_err(|_| Error::Sync("the server sent malformed key material".into()))
}

fn crypto_key(bytes: &[u8]) -> Result<ContentKey> {
    let array: [u8; 32] = bytes
        .try_into()
        .map_err(|_| Error::Sync("stored key is the wrong length".into()))?;
    Ok(Zeroizing::new(array))
}

/// Seal the account keypair and the personal content key under a wrapping key.
fn wrap_material(
    wrap: &ContentKey,
    public: &crypto::PublicKey,
    secret: &[u8],
    personal: &ContentKey,
) -> Result<KeyMaterial> {
    let (secret_nonce, secret_ct) = crypto::seal(wrap, WRAP_AAD, secret)?;
    let (personal_nonce, personal_ct) = crypto::seal(wrap, WRAP_AAD, personal.as_slice())?;

    Ok(KeyMaterial {
        account_public: hex::encode(public.as_bytes()),
        wrapped_account_secret: blob(secret_nonce, secret_ct),
        wrapped_personal_key: blob(personal_nonce, personal_ct),
    })
}

fn unwrap_material(
    wrap: &ContentKey,
    material: &KeyMaterial,
) -> Result<(ContentKey, Zeroizing<Vec<u8>>)> {
    let secret = open_blob(wrap, &material.wrapped_account_secret)?;
    let personal = open_blob(wrap, &material.wrapped_personal_key)?;
    Ok((crypto_key(&personal)?, secret))
}

fn blob(nonce: Vec<u8>, ciphertext: Vec<u8>) -> SealedBlob {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    SealedBlob {
        nonce: STANDARD.encode(nonce),
        ciphertext: STANDARD.encode(ciphertext),
    }
}

fn open_blob(wrap: &ContentKey, blob: &SealedBlob) -> Result<Zeroizing<Vec<u8>>> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    let nonce = STANDARD
        .decode(&blob.nonce)
        .map_err(|_| Error::Sync("the server sent a malformed key blob".into()))?;
    let ciphertext = STANDARD
        .decode(&blob.ciphertext)
        .map_err(|_| Error::Sync("the server sent a malformed key blob".into()))?;

    crypto::open(wrap, WRAP_AAD, &nonce, &ciphertext).map_err(|_| {
        // The one error here a user can act on: this is what a wrong password looks like
        // after the server has already accepted the auth half.
        Error::Sync("could not unlock the account keys - wrong password?".into())
    })
}

/// Fixed associated data for the two wrapped blobs. They are not records and have no
/// envelope header to bind to, but passing something rather than nothing keeps every
/// sealing call in the app on the same code path.
const WRAP_AAD: &[u8] = b"remotier.keymaterial.v1";
