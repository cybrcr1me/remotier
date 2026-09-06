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
pub async fn connect() -> Result<DynAgentClient> {
    #[cfg(unix)]
    {
        let client = AgentClient::connect_env()
            .await
            .map_err(|e| Error::Ssh(format!("no ssh-agent available: {e}")))?;
        Ok(client.dynamic())
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
pub async fn identities() -> Result<Vec<AgentKey>> {
    let mut client = connect().await?;
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
