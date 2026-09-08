//! Dialling a host: transport, host key check, authentication, then a PTY shell.

use std::sync::Arc;
use std::time::Duration;

use russh::client::{self, Handle};
use russh::keys::ssh_key::PublicKey;
use russh::keys::PrivateKeyWithHashAlg;
use russh::Channel as SshChannel;
use russh::client::AuthResult;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::ssh::client::{Handler, HostKeyPolicy};
use crate::ssh::known_hosts::Verdict;
use crate::ssh::resolve::{AuthMaterial, Target, DEFAULT_TERM};
use zeroize::Zeroizing;

use crate::ssh::{agent, fido, keys};

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
    /// The security key is waiting to be touched. Nothing happens until it is, so this
    /// has to reach the user - it is the one stage that is blocked on them, not on us.
    TouchRequired,
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
        method: method.clone(),
        username: target.username.clone(),
    });
    authenticate(&mut handle, target, progress).await?;
    progress(Stage::Authenticated { method });

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
///
/// A hardware-backed key is called out, because "public key (from disk)" would be a lie
/// about the one case where the disk holds no key at all - and that is exactly the case a
/// user is trying to diagnose when they read this panel.
fn auth_method_name(auth: &AuthMaterial) -> String {
    let token = |pem: &str| {
        keys::private_algorithm(pem).is_ok_and(|algorithm| keys::is_hardware_backed(&algorithm))
    };

    match auth {
        AuthMaterial::Password(_) => "password".to_string(),
        AuthMaterial::Interactive(_) => "keyboard-interactive".to_string(),
        AuthMaterial::Key { pem, .. } => if token(pem) {
            "public key (security key)"
        } else {
            "public key"
        }
        .to_string(),
        AuthMaterial::KeyPath { path, .. } => {
            let hardware = std::fs::read_to_string(path).is_ok_and(|pem| token(&pem));
            if hardware {
                "public key (security key)"
            } else {
                "public key (from disk)"
            }
            .to_string()
        }
        AuthMaterial::Agent { .. } => "ssh-agent".to_string(),
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

async fn authenticate(
    handle: &mut Handle<Handler>,
    target: &Target,
    progress: &impl Progress,
) -> Result<()> {
    let user = target.username.as_str();



    match &target.auth {
        AuthMaterial::Password(Some(password)) => {
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

        // Nothing stored. Try what does not need a secret before troubling the user:
        // plenty of hosts accept `none` or an empty keyboard-interactive exchange.
        AuthMaterial::Password(None) | AuthMaterial::Interactive(None) => {
            if handle.authenticate_none(user).await?.success() {
                return Ok(());
            }
            if keyboard_interactive(handle, user, None).await? {
                return Ok(());
            }
            Err(Error::PasswordRequired {
                username: user.to_string(),
                host: format!("{}:{}", target.hostname, target.port),
            })
        }

        AuthMaterial::Interactive(Some(password)) => {
            if keyboard_interactive(handle, user, Some(password.as_str())).await? {
                return Ok(());
            }
            Err(Error::Auth("keyboard-interactive authentication failed".into()))
        }

        AuthMaterial::Key { pem, passphrase } => {
            authenticate_with_pem(
                handle,
                target,
                pem,
                passphrase.as_ref().map(|p| p.as_str()),
                target.pin.as_ref(),
                progress,
            )
            .await
        }

        AuthMaterial::KeyPath { path, passphrase } => {
            let pem = std::fs::read_to_string(path)
                .map_err(|e| Error::Auth(format!("could not read key at {path}: {e}")))?;
            authenticate_with_pem(
                handle,
                target,
                &pem,
                passphrase.as_ref().map(|p| p.as_str()),
                target.pin.as_ref(),
                progress,
            )
            .await
        }

        AuthMaterial::Agent {
            public_openssh,
            socket,
        } => authenticate_agent(handle, user, public_openssh.as_deref(), socket.as_deref()).await,
    }
}

/// Authenticate with a stored key, sending it to the token's handler when it has one.
///
/// The algorithm is read before any decryption, because a FIDO key needs no passphrase
/// from us - whatever protects it is on the token - and asking for one would be a prompt
/// with no answer.
async fn authenticate_with_pem(
    handle: &mut Handle<Handler>,
    target: &Target,
    pem: &str,
    passphrase: Option<&str>,
    pin: Option<&Zeroizing<String>>,
    progress: &impl Progress,
) -> Result<()> {
    let algorithm = keys::private_algorithm(pem)?;

    if keys::is_hardware_backed(&algorithm) {
        return authenticate_token_key(handle, target, pem, pin, progress).await;
    }

    let key = keys::parse_private(pem, passphrase)?;
    authenticate_key(handle, target, key).await
}

/// Authenticate a FIDO key by asking the token itself to sign.
///
/// No agent, no `ssh-add`, no `ssh-sk-helper`: russh hands the bytes to sign to a
/// [`fido::TokenSigner`] exactly as it would to the agent client, and the token answers.
/// That is what makes this work identically on macOS, Linux and Windows.
async fn authenticate_token_key(
    handle: &mut Handle<Handler>,
    target: &Target,
    pem: &str,
    pin: Option<&Zeroizing<String>>,
    progress: &impl Progress,
) -> Result<()> {
    let user = target.username.as_str();
    // No passphrase: whatever protects a FIDO key lives on the token, not in the file.
    let key = keys::parse_private(pem, None)?;
    let security_key = fido::SecurityKey::from_private(&key)?;

    if security_key.needs_touch() {
        progress(Stage::TouchRequired);
    }

    let mut signer = fido::TokenSigner::new(security_key, pin.cloned());
    let public = key.public_key().clone();

    log::debug!("security key: offering {} to the server", key.algorithm());

    let result = handle
        .authenticate_publickey_with(user, public, None, &mut signer)
        .await;

    // russh's signer error says nothing; the signer kept the real one.
    if let Some(failure) = signer.take_failure() {
        log::warn!("security key: signing failed: {failure}");
        return Err(failure);
    }

    match &result {
        Ok(outcome) => log::debug!("security key: server said success={}", outcome.success()),
        Err(e) => log::warn!("security key: authentication errored: {e}"),
    }

    if result
        .map_err(|e| Error::Auth(format!("security key authentication failed: {e}")))?
        .success()
    {
        Ok(())
    } else {
        after_key_rejected(handle, target, "this security key").await
    }
}

/// What to do when the server turns a key down.
///
/// OpenSSH moves on to the next method the server will still accept, and a user whose key
/// is simply not in `authorized_keys` on this host expects the same - otherwise a host
/// that would happily take a password just fails. When a password has already been typed
/// it is tried; when not, the caller is asked for one.
///
/// If the server offers nothing but public keys, say so rather than prompting for a
/// password it would refuse anyway.
async fn after_key_rejected(
    handle: &mut Handle<Handler>,
    target: &Target,
    what: &str,
) -> Result<()> {
    let methods = match handle.authenticate_none(target.username.as_str()).await {
        Ok(AuthResult::Failure {
            remaining_methods, ..
        }) => remaining_methods,
        // Already in, somehow: the server accepted `none`.
        Ok(AuthResult::Success) => return Ok(()),
        Err(_) => russh::MethodSet::empty(),
    };

    let offers_password = methods.contains(&russh::MethodKind::Password)
        || methods.contains(&russh::MethodKind::KeyboardInteractive);

    if !offers_password {
        return Err(Error::Auth(format!(
            "the server rejected {what} and accepts no other method - add the key to \
             authorized_keys on that host"
        )));
    }

    let user = target.username.as_str();

    if let Some(password) = &target.typed_password {
        if handle.authenticate_password(user, password.as_str()).await?.success() {
            return Ok(());
        }
        if keyboard_interactive(handle, user, Some(password)).await? {
            return Ok(());
        }
        return Err(Error::Auth("the server rejected that password".into()));
    }

    Err(Error::PasswordRequired {
        username: user.to_string(),
        host: format!("{}:{}", target.hostname, target.port),
    })
}

async fn authenticate_key(
    handle: &mut Handle<Handler>,
    target: &Target,
    key: russh::keys::PrivateKey,
) -> Result<()> {
    let user = target.username.as_str();
    // RSA keys must be signed with a hash the server actually accepts; SHA-1 is refused
    // by anything modern.
    let hash_alg = handle.best_supported_rsa_hash().await?.flatten();
    let result = handle
        .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg))
        .await?;

    if result.success() {
        Ok(())
    } else {
        after_key_rejected(handle, target, "this key").await
    }
}

async fn authenticate_agent(
    handle: &mut Handle<Handler>,
    user: &str,
    public_openssh: Option<&str>,
    socket: Option<&str>,
) -> Result<()> {
    let mut client = agent::connect(socket).await?;
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

        let hardware = keys::is_hardware_backed(&key.algorithm());

        let result = handle
            .authenticate_publickey_with(user, key.clone(), hash_alg, &mut client)
            .await
            .map_err(|e| {
                if hardware {
                    // The agent took the key and cannot sign with it. On macOS this is
                    // near-certain: every GUI app is handed launchd's agent, and Apple's
                    // OpenSSH ships no `ssh-sk-helper`, so it accepts a FIDO key and then
                    // fails every signature. Naming the cause saves a long hunt.
                    Error::Auth(format!(
                        "the ssh-agent holds this hardware key but cannot sign with it \
                         ({e}). Its OpenSSH build has no ssh-sk-helper - Apple's does not. \
                         Point Settings > ssh-agent socket at an agent that does."
                    ))
                } else {
                    Error::Auth(format!("agent authentication failed: {e}"))
                }
            })?;

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
