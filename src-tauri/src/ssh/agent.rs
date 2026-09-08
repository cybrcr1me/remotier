//! ssh-agent support.
//!
//! russh already implements [`russh::Signer`] for its agent client, so a connected
//! client can be handed straight to `authenticate_publickey_with`. The private key never
//! enters this process - only the blob to sign does.

use russh::keys::agent::client::{AgentClient, AgentStream};
use russh::keys::ssh_key::{HashAlg, PublicKey};
use russh::keys::agent::AgentIdentity;
use serde::Serialize;

use crate::error::{Error, Result};

/// Type-erased so unix sockets, Windows named pipes and Pageant share one code path.
pub type DynAgentClient = AgentClient<Box<dyn AgentStream + Send + Unpin + 'static>>;

/// Connect to the agent the environment points at.
///
/// # Errors
///
/// Fails when no agent is running or `SSH_AUTH_SOCK` is unset, which is an ordinary
/// situation rather than a bug - callers fall back to another auth method.
pub async fn connect(socket: Option<&str>) -> Result<DynAgentClient> {
    #[cfg(unix)]
    {
        // An explicit socket exists because the environment's agent is not always the one
        // that can help: macOS hands every GUI app launchd's agent, and Apple's build has
        // no `ssh-sk-helper`, so it cannot sign for a FIDO key however the key got in.
        let client = match socket {
            Some(path) => AgentClient::connect_uds(path)
                .await
                .map_err(|e| Error::Ssh(format!("no ssh-agent at {path}: {e}")))?
                .dynamic(),
            None => AgentClient::connect_env()
                .await
                .map_err(|e| Error::Ssh(format!("no ssh-agent available: {e}")))?
                .dynamic(),
        };
        Ok(client)
    }

    #[cfg(windows)]
    {
        // Win32-OpenSSH exposes a named pipe; SSH_AUTH_SOCK holds its path when set.
        let pipe = std::env::var("SSH_AUTH_SOCK")
            .unwrap_or_else(|_| r"\\.\pipe\openssh-ssh-agent".to_string());
        match AgentClient::connect_named_pipe(&pipe).await {
            Ok(client) => Ok(client.dynamic()),
            Err(pipe_err) => match AgentClient::connect_pageant().await {
                Ok(client) => Ok(client.dynamic()),
                Err(_) => Err(Error::Ssh(format!("no ssh-agent available: {pipe_err}"))),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentKey {
    pub fingerprint: String,
    pub algorithm: String,
    pub comment: String,
    pub openssh: String,
}

/// Public keys the agent is holding.
pub async fn identities(socket: Option<&str>) -> Result<Vec<AgentKey>> {
    let mut client = connect(socket).await?;
    let identities = client
        .request_identities()
        .await
        .map_err(|e| Error::Ssh(format!("could not list agent identities: {e}")))?;

    Ok(identities
        .iter()
        .filter_map(|identity| match identity {
            AgentIdentity::PublicKey { key, comment } => Some(AgentKey {
                fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
                algorithm: key.algorithm().to_string(),
                comment: comment.clone(),
                openssh: key.to_openssh().unwrap_or_default(),
            }),
            // Certificates are not offered as pickable identities yet.
            AgentIdentity::Certificate { .. } => None,
        })
        .collect())
}

/// Pick the agent identity matching `public`, if the agent still holds it.
pub async fn find_identity(
    client: &mut DynAgentClient,
    public: &PublicKey,
) -> Result<Option<AgentIdentity>> {
    let identities = client
        .request_identities()
        .await
        .map_err(|e| Error::Ssh(format!("could not list agent identities: {e}")))?;

    Ok(identities.into_iter().find(|identity| match identity {
        AgentIdentity::PublicKey { key, .. } => key == public,
        AgentIdentity::Certificate { .. } => false,
    }))
}
