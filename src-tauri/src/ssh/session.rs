//! Live SSH sessions.
//!
//! Each session owns a tokio task holding the russh channel. Terminal output goes back
//! over a binary [`tauri::ipc::Channel`] rather than Tauri events: events are JSON, and
//! JSON-encoding every burst of terminal output is the classic bottleneck here.
//!
//! Output is coalesced - a `cat` of a large file produces a handful of IPC frames instead
//! of thousands.

use std::time::Duration;

use dashmap::DashMap;
use russh::client::Msg;
use russh::{Channel as SshChannel, ChannelMsg, Disconnect};
use serde::Serialize;
use tauri::ipc::{Channel as IpcChannel, InvokeResponseBody};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use crate::error::{Error, Result};
use crate::ssh::liveness::{sleep_or_suspend, Wakeup};

/// Flush pending output after this long, even if little has arrived.
const FLUSH_INTERVAL: Duration = Duration::from_millis(4);
/// Flush immediately once this much output is buffered.
const FLUSH_BYTES: usize = 64 * 1024;
/// Bound on queued input, so a wedged connection cannot grow memory without limit.
const INPUT_QUEUE: usize = 256;

/// How often the watchdog proves the connection is still there.
const PING_INTERVAL: Duration = Duration::from_secs(30);
/// How long the server has to answer a ping before the link is called dead.
///
/// Generous enough for a slow link that has just come back, short enough that nobody is
/// left typing into a terminal that is not connected to anything.
const PING_TIMEOUT: Duration = Duration::from_secs(10);

pub const SESSION_EVENT: &str = "ssh://session";

#[derive(Debug)]
pub enum Command {
    Data(Vec<u8>),
    Resize { cols: u32, rows: u32 },
    Close,
    /// The watchdog found the connection dead. Carries what to tell the user.
    Lost(String),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum SessionEvent {
    Closed {
        session_id: String,
        /// Present when the remote shell reported an exit status.
        exit_status: Option<u32>,
    },
    Failed {
        session_id: String,
        message: String,
    },
    /// The connection died under us, rather than the shell exiting.
    ///
    /// Separate from `Failed` so the UI can offer to reconnect instead of reporting a
    /// failure the user did nothing to cause.
    Lost {
        session_id: String,
        message: String,
    },
}

struct SessionHandle {
    tx: mpsc::Sender<Command>,
}

/// All live sessions, keyed by the id handed to the frontend.
#[derive(Default)]
pub struct Sessions {
    inner: DashMap<String, SessionHandle>,
}

impl Sessions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ids(&self) -> Vec<String> {
        self.inner.iter().map(|entry| entry.key().clone()).collect()
    }

    pub fn count(&self) -> usize {
        self.inner.len()
    }

    fn insert(&self, id: String, tx: mpsc::Sender<Command>) {
        self.inner.insert(id, SessionHandle { tx });
    }

    fn remove(&self, id: &str) {
        self.inner.remove(id);
    }

    /// # Errors
    ///
    /// [`Error::NoSession`] when the id is unknown or the session has already ended.
    pub async fn send(&self, id: &str, command: Command) -> Result<()> {
        let tx = {
            let Some(handle) = self.inner.get(id) else {
                return Err(Error::NoSession(id.to_string()));
            };
            handle.tx.clone()
        };

        tx.send(command)
            .await
            .map_err(|_| Error::NoSession(id.to_string()))
    }
}

/// Take ownership of a connected channel and start pumping it.
///
/// `handle` is moved in purely to keep the connection alive for as long as the session
/// lives; dropping it would tear the transport down.
pub fn spawn(
    app: AppHandle,
    sessions: std::sync::Arc<Sessions>,
    id: String,
    handle: russh::client::Handle<crate::ssh::client::Handler>,
    channel: SshChannel<Msg>,
    on_data: IpcChannel<InvokeResponseBody>,
) {
    let (tx, rx) = mpsc::channel(INPUT_QUEUE);
    let handle = std::sync::Arc::new(handle);

    sessions.insert(id.clone(), tx.clone());
    tauri::async_runtime::spawn(watchdog(id.clone(), handle.clone(), tx));

    tauri::async_runtime::spawn(async move {
        let outcome = pump(&id, &handle, channel, &on_data, rx).await;
        sessions.remove(&id);

        let event = match outcome {
            Ok(Ended::Shell { exit_status }) => SessionEvent::Closed {
                session_id: id.clone(),
                exit_status,
            },
            Ok(Ended::Lost { message }) => {
                log::info!("session {id} lost: {message}");
                SessionEvent::Lost {
                    session_id: id.clone(),
                    message,
                }
            }
            Err(e) => {
                log::warn!("session {id} ended with an error: {e}");
                SessionEvent::Failed {
                    session_id: id.clone(),
                    message: e.to_string(),
                }
            }
        };

        if let Err(e) = app.emit(SESSION_EVENT, event) {
            log::warn!("could not emit session event for {id}: {e}");
        }
    });
}

/// How a session came to an end.
enum Ended {
    /// The remote shell finished, or the channel was closed from either side.
    Shell { exit_status: Option<u32> },
    /// The watchdog found the connection gone.
    Lost { message: String },
}

async fn pump(
    id: &str,
    handle: &russh::client::Handle<crate::ssh::client::Handler>,
    mut channel: SshChannel<Msg>,
    on_data: &IpcChannel<InvokeResponseBody>,
    mut rx: mpsc::Receiver<Command>,
) -> Result<Ended> {
    let mut pending: Vec<u8> = Vec::with_capacity(FLUSH_BYTES);
    let mut flush_at: Option<tokio::time::Instant> = None;
    let mut exit_status = None;
    let mut eof = false;
    let mut lost: Option<String> = None;

    loop {
        tokio::select! {
            // Both branches are cancellation-safe: mpsc::Receiver::recv and
            // sleep_until can be dropped mid-poll without losing data.
            command = rx.recv() => {
                match command {
                    Some(Command::Data(data)) => channel.data(&data[..]).await?,
                    Some(Command::Resize { cols, rows }) => {
                        channel.window_change(cols, rows, 0, 0).await?;
                    }
                    Some(Command::Lost(message)) => {
                        lost = Some(message);
                        break;
                    }
                    // Either an explicit disconnect or the last sender going away.
                    Some(Command::Close) | None => break,
                }
            }

            message = channel.wait() => {
                let Some(message) = message else { break };
                match message {
                    ChannelMsg::Data { data } => {
                        pending.extend_from_slice(&data);
                        if pending.len() >= FLUSH_BYTES {
                            flush(on_data, &mut pending)?;
                            flush_at = None;
                        } else if flush_at.is_none() {
                            flush_at = Some(tokio::time::Instant::now() + FLUSH_INTERVAL);
                        }
                    }
                    // stderr on a PTY session is folded into the same stream, which is
                    // what a terminal expects to see.
                    ChannelMsg::ExtendedData { data, .. } => {
                        pending.extend_from_slice(&data);
                        if flush_at.is_none() {
                            flush_at = Some(tokio::time::Instant::now() + FLUSH_INTERVAL);
                        }
                    }
                    ChannelMsg::ExitStatus { exit_status: status } => {
                        exit_status = Some(status);
                        if eof {
                            break;
                        }
                    }
                    // EOF alone is not the end. OpenSSH sends the exit status first, but
                    // Dropbear can send it after, and the status is what tells the UI the
                    // shell exited - which closes the pane - rather than the channel closing.
                    ChannelMsg::Eof if exit_status.is_some() => break,
                    ChannelMsg::Eof => eof = true,
                    ChannelMsg::Close => break,
                    _ => {}
                }
            }

            () = sleep_until(flush_at), if flush_at.is_some() => {
                flush(on_data, &mut pending)?;
                flush_at = None;
            }
        }
    }

    // Anything buffered when the shell exited still belongs on screen.
    flush(on_data, &mut pending)?;

    let _ = channel.close().await;
    let _ = handle
        .disconnect(Disconnect::ByApplication, "", "en")
        .await;

    log::debug!("session {id} closed (exit status {exit_status:?})");
    Ok(match lost {
        Some(message) => Ended::Lost { message },
        None => Ended::Shell { exit_status },
    })
}

/// Prove, repeatedly, that the connection is still there.
///
/// russh's own keepalive counts in monotonic time, which stops while the machine is
/// suspended - so after a wake it needs another couple of minutes before it notices a
/// connection that died hours ago. This watches both clocks, re-tests immediately when the
/// machine has been asleep, and otherwise checks on a fixed interval, which also catches a
/// link lost while awake.
///
/// The ping runs here rather than in the pump's `select!` loop because waiting up to
/// [`PING_TIMEOUT`] for a reply inside that loop would stall terminal output for every
/// other message.
async fn watchdog(
    id: String,
    handle: std::sync::Arc<russh::client::Handle<crate::ssh::client::Handler>>,
    tx: mpsc::Sender<Command>,
) {
    loop {
        let wakeup = sleep_or_suspend(PING_INTERVAL).await;

        // The session ended on its own; the pump has already reported why.
        if handle.is_closed() || tx.is_closed() {
            return;
        }

        // A ping is the only honest test: a connection dropped while the machine slept
        // still looks open from this side until something expects an answer.
        if matches!(
            tokio::time::timeout(PING_TIMEOUT, handle.send_ping()).await,
            Ok(Ok(()))
        ) {
            continue;
        }

        let message = match wakeup {
            Wakeup::Suspended(gap) => format!(
                "The connection did not survive the machine sleeping for {}.",
                describe(gap)
            ),
            Wakeup::Elapsed => "The server stopped responding.".to_string(),
        };

        log::info!("session {id} failed its liveness check: {message}");
        let _ = tx.send(Command::Lost(message)).await;
        return;
    }
}

/// A duration in the roundest terms that are still true, for a sentence.
fn describe(gap: Duration) -> String {
    let minutes = gap.as_secs() / 60;
    match minutes {
        0 => "less than a minute".to_string(),
        1 => "a minute".to_string(),
        2..=59 => format!("{minutes} minutes"),
        60..=119 => "an hour".to_string(),
        _ => format!("{} hours", minutes / 60),
    }
}

async fn sleep_until(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        // Never resolves; the select arm is disabled by its `if` guard anyway.
        None => std::future::pending().await,
    }
}

fn flush(on_data: &IpcChannel<InvokeResponseBody>, pending: &mut Vec<u8>) -> Result<()> {
    if pending.is_empty() {
        return Ok(());
    }
    // Raw keeps the bytes out of JSON; the frontend receives an ArrayBuffer.
    let payload = InvokeResponseBody::Raw(std::mem::take(pending));
    on_data
        .send(payload)
        .map_err(|e| Error::Ssh(format!("could not deliver terminal output: {e}")))?;
    pending.reserve(FLUSH_BYTES);
    Ok(())
}
