//! The russh client handler, which is where host key trust is decided.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use russh::client;
use russh::keys::ssh_key::PublicKey;
use russh::keys::PublicKeyOrCertificate;
use serde::{Deserialize, Serialize};

use crate::ssh::known_hosts::{self, Verdict};

/// How to treat a host key that `known_hosts` does not vouch for.
///
/// A *changed* key is never covered by this: it is refused under every policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HostKeyPolicy {
    /// Refuse unknown hosts. The caller shows a fingerprint prompt and retries.
    Strict,
    /// Accept for this connection only; nothing is written to `known_hosts`.
    TrustOnce,
    /// Accept and append to `~/.ssh/known_hosts`, so `ssh` trusts it afterwards too.
    TrustAndSave,
}

/// What the handler saw, readable after the connection attempt finishes.
#[derive(Debug, Clone)]
pub struct ObservedHostKey {
    pub verdict: Verdict,
    pub fingerprint: String,
}

#[derive(Clone)]
pub struct Handler {
    host: String,
    port: u16,
    known_hosts_path: Option<PathBuf>,
    policy: HostKeyPolicy,
    observed: Arc<Mutex<Option<ObservedHostKey>>>,
}

impl Handler {
    pub fn new(host: impl Into<String>, port: u16, policy: HostKeyPolicy) -> Self {
        Self {
            host: host.into(),
            port,
            known_hosts_path: known_hosts::default_path(),
            policy,
            observed: Arc::new(Mutex::new(None)),
        }
    }

    /// Shared handle to what the handler recorded, so a failed connect can be turned
    /// into a precise error instead of a generic protocol failure.
    pub fn observed(&self) -> Arc<Mutex<Option<ObservedHostKey>>> {
        Arc::clone(&self.observed)
    }

    fn record(&self, verdict: Verdict, fingerprint: String) {
        if let Ok(mut slot) = self.observed.lock() {
            *slot = Some(ObservedHostKey {
                verdict,
                fingerprint,
            });
        }
    }

    fn decide(&mut self, key: &PublicKey) -> bool {
        let fingerprint = known_hosts::fingerprint(key);

        let Some(path) = self.known_hosts_path.clone() else {
            // No home directory means no known_hosts to consult; refuse rather than
            // silently trusting whatever answered.
            self.record(Verdict::Unknown, fingerprint);
            return false;
        };

        let verdict = match known_hosts::verify(&self.host, self.port, key, &path) {
            Ok(verdict) => verdict,
            Err(e) => {
                log::warn!("could not read known_hosts: {e}");
                self.record(Verdict::Unknown, fingerprint);
                return false;
            }
        };
        self.record(verdict.clone(), fingerprint);

        match verdict {
            Verdict::Known => true,
            // A changed key is the MITM signature. No policy overrides it; clearing it
            // is a deliberate action the user takes in the known hosts screen.
            Verdict::Changed { .. } => false,
            Verdict::Unknown => match self.policy {
                HostKeyPolicy::Strict => false,
                HostKeyPolicy::TrustOnce => true,
                HostKeyPolicy::TrustAndSave => {
                    if let Err(e) = known_hosts::learn(&self.host, self.port, key, &path) {
                        log::warn!("could not write known_hosts: {e}");
                    }
                    true
                }
            },
        }
    }
}

impl client::Handler for Handler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        match server_public_key {
            PublicKeyOrCertificate::PublicKey { key, .. } => Ok(self.decide(key)),
            // Host certificates would need CA trust handling; refuse clearly instead of
            // pretending to have verified something.
            PublicKeyOrCertificate::Certificate(_) => {
                log::warn!("{} presented a host certificate, which is not supported", self.host);
                Ok(false)
            }
        }
    }
}
