//! The background sync task.
//!
//! One task, three reasons to wake: something changed locally, the poll interval elapsed,
//! or the UI asked. It never holds the database lock across a network call - see the lock
//! rule on [`super::engine::SyncEngine`].

use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use crate::commands::sync::SYNC_STATUS_EVENT;
use crate::db::Db;

use super::engine::SyncEngine;

/// How long to wait after a local change before pushing.
///
/// Long enough that renaming a host character by character is one push rather than
/// twelve, short enough that switching to the other machine feels immediate.
const DEBOUNCE: Duration = Duration::from_secs(2);

/// How often to look for other devices' changes when nothing happens here.
///
/// There is no push channel from the server, so this is the whole latency of an incoming
/// change. A minute is a compromise: a websocket would be better and is not worth the
/// reconnection machinery until someone asks for it.
const POLL: Duration = Duration::from_secs(60);

/// How long to wait after a failure before trying again, and the ceiling it backs off to.
const RETRY_MIN: Duration = Duration::from_secs(15);
const RETRY_MAX: Duration = Duration::from_secs(15 * 60);

pub fn spawn(app: AppHandle, engine: Arc<SyncEngine>, db: Arc<Db>) {
    // `tauri::async_runtime::spawn`, not `tokio::spawn`. This is called from `setup`,
    // where no tokio runtime is entered on the calling thread, so `tokio::spawn` panics
    // with "there is no reactor running" - and a panic in `setup` stops the app starting
    // at all. Every other task in this codebase goes through the same door.
    tauri::async_runtime::spawn(async move {
        // Restore the account keys before anything else, or the first cycle would think
        // the device is signed out.
        if let Err(e) = engine.restore().await {
            log::warn!("sync: could not restore the account keys: {e}");
        }
        let _ = app.emit(SYNC_STATUS_EVENT, engine.status().await.ok());

        let mut backoff = RETRY_MIN;
        let mut pending_seen = pending(&db);

        loop {
            // Wake early if local work is waiting, otherwise sit out the poll interval.
            let wait = if pending_seen > 0 { DEBOUNCE } else { POLL };
            tokio::time::sleep(wait).await;

            let status = match engine.status().await {
                Ok(status) => status,
                Err(e) => {
                    log::debug!("sync: status unavailable: {e}");
                    continue;
                }
            };
            if !status.signed_in {
                pending_seen = 0;
                continue;
            }

            // Nothing to push and the poll interval has not elapsed in vain - a cycle
            // with nothing to do still costs a round trip, so skip it.
            let dirty = pending(&db);
            if dirty == 0 && pending_seen > 0 && wait == DEBOUNCE {
                pending_seen = 0;
                continue;
            }
            pending_seen = dirty;

            match engine.sync_now().await {
                Ok(status) => {
                    backoff = RETRY_MIN;
                    pending_seen = status.pending;
                    let _ = app.emit(SYNC_STATUS_EVENT, status);
                }
                Err(e) => {
                    // Back off rather than hammering an instance that is down. The error
                    // is already on the status the UI reads.
                    log::warn!("sync: cycle failed: {e}");
                    let _ = app.emit(SYNC_STATUS_EVENT, engine.status().await.ok());
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(RETRY_MAX);
                }
            }
        }
    });
}

/// How many records are waiting to go. Cheap enough to ask every tick.
fn pending(db: &Db) -> i64 {
    db.read(|conn| {
        Ok(conn.query_row(
            "SELECT count(*) FROM sync_meta WHERE local_dirty = 1",
            [],
            |row| row.get::<_, i64>(0),
        )?)
    })
    .unwrap_or(0)
}
