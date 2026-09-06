//! Dialling a host: transport, host key check, authentication, then a PTY shell.

use std::sync::Arc;
use std::time::Duration;

use russh::client::{self, Handle};
use russh::keys::ssh_key::PublicKey;
use russh::keys::PrivateKeyWithHashAlg;
use russh::Channel as SshChannel;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::ssh::client::{Handler, HostKeyPolicy};
use crate::ssh::known_hosts::Verdict;
use crate::ssh::resolve::{AuthMaterial, Target, DEFAULT_TERM};
use crate::ssh::{agent, keys};

/// Keep NAT and idle-timeout devices from silently dropping a quiet shell.
const KEEPALIVE: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

pub struct Connection {
    pub handle: Handle<Handler>,
    pub channel: SshChannel<client::Msg>,
}

/// Progress reported while a connection is being established.
///
/// The UI shows these as they arrive, so a slow DNS lookup or a server that stalls during
/// authentication looks different from an app that has simply frozen.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "stage")]
pub enum Stage {
    /// Opening the TCP connection and performing the SSH handshake.
    Connecting { host: String, port: u16 },
    /// The server's key was accepted, with the fingerprint that was checked.
    HostKeyAccepted { fingerprint: String },
    /// Offering one authentication method.
    Authenticating { method: String, username: String },
    Authenticated { method: String },
    /// Requesting the pseudo-terminal and shell.
    OpeningShell { term: String },
    Ready,
}

/// Called as each stage is reached. Reporting is best effort and never fails a connection.
pub trait Progress: Fn(Stage) + Send + Sync {}
impl<T: Fn(Stage) + Send + Sync> Progress for T {}

/// Connect, authenticate, and open an interactive shell.
///
/// # Errors
///
/// - [`Error::UnknownHostKey`] when the host is not in `known_hosts` and the policy is
///   strict. Retry with [`HostKeyPolicy::TrustOnce`] or `TrustAndSave` once the user has
///   seen the fingerprint.
/// - [`Error::ChangedHostKey`] when the key does not match the recorded one. This is
///   never retryable through a policy.
/// - [`Error::Auth`] when every applicable authentication method is refused.
pub async fn open(
    target: &Target,
    policy: HostKeyPolicy,
    term: &str,
    cols: u32,
    rows: u32,
    progress: &impl Progress,
) -> Result<Connection> {
    let config = Arc::new(client::Config {
        keepalive_interval: Some(KEEPALIVE),
        ..Default::default()
    });

    let handler = Handler::new(target.hostname.clone(), target.port, policy);
    let observed = handler.observed();

    progress(Stage::Connecting {
        host: target.hostname.clone(),
        port: target.port,
    });

    let connecting = client::connect(config, (target.hostname.as_str(), target.port), handler);
    let mut handle = match tokio::time::timeout(CONNECT_TIMEOUT, connecting).await {
        Ok(Ok(handle)) => handle,
        Ok(Err(e)) => return Err(host_key_error(&observed, target).unwrap_or_else(|| e.into())),
        Err(_) => {
            return Err(Error::Ssh(format!(
                "timed out connecting to {}:{}",
                target.hostname, target.port
            )))
        }
    };

    if let Some(seen) = observed.lock().ok().and_then(|slot| slot.clone()) {
        progress(Stage::HostKeyAccepted {
            fingerprint: seen.fingerprint,
        });
    }

    let method = auth_method_name(&target.auth);
    progress(Stage::Authenticating {
        method: method.to_string(),
        username: target.username.clone(),
    });
    authenticate(&mut handle, target).await?;
    progress(Stage::Authenticated {
        method: method.to_string(),
    });

    progress(Stage::OpeningShell {
        term: pick_term(term).to_string(),
    });
    let channel = handle.channel_open_session().await?;
    channel
        .request_pty(true, pick_term(term), cols, rows, 0, 0, &[])
        .await?;
    channel.request_shell(true).await?;

    progress(Stage::Ready);
    Ok(Connection { handle, channel })
}

/// Human-readable name for the method being offered, for the progress panel.
fn auth_method_name(auth: &AuthMaterial) -> &'static str {
    match auth {
        AuthMaterial::Password(_) => "password",
        AuthMaterial::Interactive(_) => "keyboard-interactive",
        AuthMaterial::Key { .. } => "public key",
        AuthMaterial::KeyPath { .. } => "public key (from disk)",
        AuthMaterial::Agent { .. } => "ssh-agent",
    }
}

fn pick_term(term: &str) -> &str {
    if term.trim().is_empty() {
        DEFAULT_TERM
    } else {
        term
    }
}

/// Translate a failed handshake into a precise host key error when that was the cause.
fn host_key_error(
    observed: &Arc<std::sync::Mutex<Option<crate::ssh::client::ObservedHostKey>>>,
    target: &Target,
) -> Option<Error> {
    let seen = observed.lock().ok()?.clone()?;
    let host = format!("{}:{}", target.hostname, target.port);

    match seen.verdict {
        Verdict::Known => None,
        Verdict::Unknown => Some(Error::UnknownHostKey {
            host,
            fingerprint: seen.fingerprint,
        }),
        Verdict::Changed { line } => Some(Error::ChangedHostKey {
            host,
            fingerprint: seen.fingerprint,
            line,
        }),
    }
}

async fn authenticate(handle: &mut Handle<Handler>, target: &Target) -> Result<()> {
    let user = target.username.as_str();

    match &target.auth {
        AuthMaterial::Password(password) => {
            if handle.authenticate_password(user, password.as_str()).await?.success() {
                return Ok(());
            }
            // Plenty of servers disable `password` but still accept the same secret over
            // keyboard-interactive, so this is a fallback rather than a separate mode.
            if keyboard_interactive(handle, user, Some(password.as_str())).await? {
                return Ok(());
            }
            Err(Error::Auth("the server rejected the password".into()))
        }

        AuthMaterial::Interactive(password) => {
            if keyboard_interactive(handle, user, password.as_ref().map(|p| p.as_str())).await? {
                return Ok(());
            }
            Err(Error::Auth("keyboard-interactive authentication failed".into()))
        }

        AuthMaterial::Key { pem, passphrase } => {
            let key = keys::parse_private(pem, passphrase.as_ref().map(|p| p.as_str()))?;
            authenticate_key(handle, user, key).await
        }

        AuthMaterial::KeyPath { path, passphrase } => {
            let pem = std::fs::read_to_string(path)
                .map_err(|e| Error::Auth(format!("could not read key at {path}: {e}")))?;
            let key = keys::parse_private(&pem, passphrase.as_ref().map(|p| p.as_str()))?;
            authenticate_key(handle, user, key).await
        }

        AuthMaterial::Agent { public_openssh } => {
            authenticate_agent(handle, user, public_openssh.as_deref()).await
        }
    }
}

async fn authenticate_key(
    handle: &mut Handle<Handler>,
    user: &str,
    key: russh::keys::PrivateKey,
) -> Result<()> {
    // RSA keys must be signed with a hash the server actually accepts; SHA-1 is refused
    // by anything modern.
    let hash_alg = handle.best_supported_rsa_hash().await?.flatten();
    let result = handle
        .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg))
        .await?;

    if result.success() {
        Ok(())
    } else {
        Err(Error::Auth("the server rejected this key".into()))
    }
}

async fn authenticate_agent(
    handle: &mut Handle<Handler>,
    user: &str,
    public_openssh: Option<&str>,
) -> Result<()> {
    let mut client = agent::connect().await?;
    let identities = client
        .request_identities()
        .await
        .map_err(|e| Error::Auth(format!("could not list agent identities: {e}")))?;

    if identities.is_empty() {
        return Err(Error::Auth("the ssh-agent is not holding any keys".into()));
    }

    // When the identity names a key, offer only that one; otherwise try each in turn,
    // which is what OpenSSH does.
    let wanted = match public_openssh {
        Some(line) => Some(
            PublicKey::from_openssh(line.trim())
                .map_err(|e| Error::Auth(format!("stored public key is unreadable: {e}")))?,
        ),
        None => None,
    };

    let hash_alg = handle.best_supported_rsa_hash().await?.flatten();

    for identity in identities {
        let russh::keys::agent::AgentIdentity::PublicKey { key, .. } = &identity else {
            continue;
        };
        if let Some(wanted) = &wanted {
            if key != wanted {
                continue;
            }
        }

        let result = handle
            .authenticate_publickey_with(user, key.clone(), hash_alg, &mut client)
            .await
            .map_err(|e| Error::Auth(format!("agent authentication failed: {e}")))?;

        if result.success() {
            return Ok(());
        }
    }

    Err(Error::Auth(
        "no key held by the ssh-agent was accepted".into(),
    ))
}

/// Answer the server's prompts. Prompts that ask for a password get the stored one;
/// anything else (a 2FA code, say) cannot be answered without a UI round-trip yet.
async fn keyboard_interactive(
    handle: &mut Handle<Handler>,
    user: &str,
    password: Option<&str>,
) -> Result<bool> {
    use russh::client::KeyboardInteractiveAuthResponse as Response;

    let mut response = handle
        .authenticate_keyboard_interactive_start(user, None)
        .await?;

    loop {
        match response {
            Response::Success => return Ok(true),
            Response::Failure { .. } => return Ok(false),
            Response::InfoRequest { prompts, .. } => {
                let mut answers = Vec::with_capacity(prompts.len());
                for prompt in &prompts {
                    let looks_like_password = prompt.prompt.to_lowercase().contains("password");
                    match (looks_like_password, password) {
                        (true, Some(password)) => answers.push(password.to_string()),
                        // An empty answer keeps the exchange going; the server decides.
                        _ => answers.push(String::new()),
                    }
                }
                response = handle
                    .authenticate_keyboard_interactive_respond(answers)
                    .await?;
            }
        }
    }
}
