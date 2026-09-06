//! The SSH command surface.
//!
//! Terminal output does not come back through these commands - it is pushed over the
//! `onData` channel supplied to [`ssh_connect`].

use serde::Deserialize;
use tauri::ipc::{Channel as IpcChannel, InvokeResponseBody};
use tauri::{AppHandle, State};

use serde::Serialize;
use tauri::Emitter;

use crate::db::new_id;
use crate::error::Result;
use crate::ssh::client::HostKeyPolicy;
use crate::ssh::resolve::{self, TargetPreview, DEFAULT_TERM};
use crate::ssh::session::{self, Command};
use crate::ssh::{connect, keys};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    pub host_id: String,
    pub cols: u32,
    pub rows: u32,
    /// Defaults to strict: unknown hosts are refused until the user sees the fingerprint.
    #[serde(default = "strict_policy")]
    pub policy: HostKeyPolicy,
    pub term: Option<String>,
    /// Correlates progress events with the pane that asked for the connection. The
    /// session id does not exist yet while connecting, so the caller supplies this.
    pub attempt_id: String,
}

pub const PROGRESS_EVENT: &str = "ssh://progress";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    attempt_id: String,
    #[serde(flatten)]
    stage: connect::Stage,
}

fn strict_policy() -> HostKeyPolicy {
    HostKeyPolicy::Strict
}

/// Resolve a host without connecting. Safe for previews - returns no secrets.
#[tauri::command(async)]
pub fn resolve_host(state: State<'_, AppState>, host_id: String) -> Result<TargetPreview> {
    resolve::preview(&state.db, &host_id)
}

/// Open a session and start streaming its output to `on_data`.
///
/// Returns the session id used by every other command here.
#[tauri::command]
pub async fn ssh_connect(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ConnectRequest,
    on_data: IpcChannel<InvokeResponseBody>,
) -> Result<String> {
    let target = resolve::target(&state.db, state.vault()?, &request.host_id)?;
    let term = request.term.unwrap_or_else(|| DEFAULT_TERM.to_string());

    log::info!(
        "connecting to {}@{}:{}",
        target.username,
        target.hostname,
        target.port
    );

    let attempt_id = request.attempt_id.clone();
    let reporter = app.clone();
    let on_stage = move |stage: connect::Stage| {
        // Progress is advisory: a failed emit must not fail the connection.
        let _ = reporter.emit(
            PROGRESS_EVENT,
            ProgressEvent {
                attempt_id: attempt_id.clone(),
                stage,
            },
        );
    };

    let connection = connect::open(
        &target,
        request.policy,
        &term,
        request.cols,
        request.rows,
        &on_stage,
    )
    .await?;

    let session_id = new_id();
    session::spawn(
        app,
        state.sessions(),
        session_id.clone(),
        connection.handle,
        connection.channel,
        on_data,
    );

    Ok(session_id)
}

/// Send keystrokes (or pasted text) to the remote shell.
#[tauri::command]
pub async fn ssh_write(
    state: State<'_, AppState>,
    session_id: String,
    data: Vec<u8>,
) -> Result<()> {
    state
        .sessions()
        .send(&session_id, Command::Data(data))
        .await
}

#[tauri::command]
pub async fn ssh_resize(
    state: State<'_, AppState>,
    session_id: String,
    cols: u32,
    rows: u32,
) -> Result<()> {
    state
        .sessions()
        .send(&session_id, Command::Resize { cols, rows })
        .await
}

#[tauri::command]
pub async fn ssh_disconnect(state: State<'_, AppState>, session_id: String) -> Result<()> {
    state.sessions().send(&session_id, Command::Close).await
}

#[tauri::command(async)]
pub fn ssh_sessions(state: State<'_, AppState>) -> Result<Vec<String>> {
    Ok(state.sessions().ids())
}

/// Public keys currently held by the ssh-agent.
#[tauri::command]
pub async fn list_agent_keys() -> Result<Vec<crate::ssh::agent::AgentKey>> {
    crate::ssh::agent::identities().await
}

/// Key pairs found in `~/.ssh`, for the "use a key I already have" flow.
#[tauri::command(async)]
pub fn scan_system_keys() -> Result<Vec<keys::DiscoveredKey>> {
    match keys::default_ssh_dir() {
        Some(dir) => keys::scan(&dir),
        None => Ok(Vec::new()),
    }
}

/// Entries in `~/.ssh/known_hosts`, for the management screen.
#[tauri::command(async)]
pub fn list_known_hosts() -> Result<Vec<crate::ssh::known_hosts::Entry>> {
    match crate::ssh::known_hosts::default_path() {
        Some(path) => crate::ssh::known_hosts::list(&path),
        None => Ok(Vec::new()),
    }
}

/// Remove one entry from `~/.ssh/known_hosts`.
///
/// This edits the user's real file, which `ssh` also reads, so the UI must confirm first.
#[tauri::command(async)]
pub fn revoke_known_host(line: usize) -> Result<()> {
    let path = crate::ssh::known_hosts::default_path()
        .ok_or_else(|| crate::error::Error::Invalid("no home directory".into()))?;
    crate::ssh::known_hosts::revoke(&path, line)
}

/// Read `~/.ssh/config` without importing anything, so the user can review first.
#[tauri::command(async)]
pub fn preview_ssh_config() -> Result<Vec<crate::ssh::ssh_config::ConfigHost>> {
    match crate::ssh::ssh_config::default_path() {
        Some(path) => crate::ssh::ssh_config::read(&path),
        None => Ok(Vec::new()),
    }
}

/// Import the chosen aliases from `~/.ssh/config` as hosts.
///
/// Aliases that already exist as a host label are skipped rather than duplicated. An
/// identity is created per distinct username so credentials can be filled in afterwards.
#[tauri::command(async)]
pub fn import_ssh_config(state: State<'_, AppState>, aliases: Vec<String>) -> Result<usize> {
    let available = preview_ssh_config()?;
    let wanted: Vec<_> = available
        .into_iter()
        .filter(|host| aliases.contains(&host.alias))
        .collect();

    let now = crate::db::now_ms();
    let mut imported = 0usize;

    state.db.write(|tx| {
        for host in &wanted {
            let exists: i64 = tx.query_row(
                "SELECT count(*) FROM hosts WHERE label = ?1",
                rusqlite::params![host.alias],
                |row| row.get(0),
            )?;
            if exists > 0 {
                continue;
            }

            let identity_id = match &host.user {
                Some(user) => {
                    // Reuse an identity with the same username rather than making a
                    // near-duplicate for every imported host.
                    let existing: Option<String> = tx
                        .query_row(
                            "SELECT id FROM identities WHERE username = ?1 LIMIT 1",
                            rusqlite::params![user],
                            |row| row.get(0),
                        )
                        .ok();

                    match existing {
                        Some(id) => Some(id),
                        None => {
                            let id = new_id();
                            tx.execute(
                                "INSERT INTO identities (id, label, username, auth_kind,
                                                         created_at, updated_at)
                                 VALUES (?1, ?2, ?3, 'agent', ?4, ?4)",
                                rusqlite::params![id, user, user, now],
                            )?;
                            Some(id)
                        }
                    }
                }
                None => None,
            };

            tx.execute(
                "INSERT INTO hosts (id, label, hostname, port, identity_id, tags, sort,
                                    created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, '[]', 0, ?6, ?6)",
                rusqlite::params![
                    new_id(),
                    host.alias,
                    host.hostname,
                    host.port.map(i64::from),
                    identity_id,
                    now,
                ],
            )?;
            imported += 1;
        }
        Ok(())
    })?;

    Ok(imported)
}
