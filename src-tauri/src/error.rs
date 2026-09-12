use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    // keyring::Error carries non-Sync payloads in some variants, so it is flattened to
    // a message here rather than wrapped.
    #[error("keychain error: {0}")]
    Keychain(String),
    #[error("vault: {0}")]
    Vault(String),
    #[error("{0} not found: {1}")]
    NotFound(&'static str, String),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("unresolved variables: {}", .0.join(", "))]
    UnresolvedVariables(Vec<String>),
    #[error("ssh error: {0}")]
    Ssh(String),
    #[error("authentication failed: {0}")]
    Auth(String),
    #[error("the security key needs its PIN")]
    PinRequired,
    #[error("host key for {host} is not known (fingerprint {fingerprint})")]
    UnknownHostKey { host: String, fingerprint: String },
    #[error("HOST KEY CHANGED for {host}: got {fingerprint}, known_hosts line {line}")]
    ChangedHostKey {
        host: String,
        fingerprint: String,
        line: usize,
    },
    #[error("no such session: {0}")]
    NoSession(String),
    #[error("a password is required for {username}@{host}")]
    PasswordRequired { username: String, host: String },

    #[error("sync: {0}")]
    Sync(String),
    /// The server refused the session. The UI signs out rather than retrying, because
    /// nothing this device can do on its own will make the token valid again.
    #[error("your sync session has expired - sign in again")]
    SyncUnauthorised,
    /// Fetching an app icon or the catalog failed. Never fatal: the UI shows the built-in
    /// icon instead.
    #[error("icons: {0}")]
    Icon(String),
    /// Checking for or installing a new version failed. Never fatal: the app in hand goes
    /// on working, which is why the UI reports this quietly.
    #[error("update: {0}")]
    Update(String),
    #[error("internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<keyring::Error> for Error {
    fn from(e: keyring::Error) -> Self {
        Error::Keychain(e.to_string())
    }
}

impl From<russh::Error> for Error {
    fn from(e: russh::Error) -> Self {
        Error::Ssh(e.to_string())
    }
}

impl From<russh::keys::Error> for Error {
    fn from(e: russh::keys::Error) -> Self {
        Error::Ssh(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Invalid(e.to_string())
    }
}

impl From<remotier_sync_proto::crypto::Error> for Error {
    fn from(e: remotier_sync_proto::crypto::Error) -> Self {
        Error::Sync(e.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        // Whatever the transport says, phrased as something a user can act on. The
        // detail still reaches the log.
        log::debug!("sync transport error: {e}");
        if e.is_timeout() {
            Error::Sync("the sync server did not answer in time".into())
        } else if e.is_connect() {
            Error::Sync("could not reach the sync server".into())
        } else {
            Error::Sync(e.to_string())
        }
    }
}

impl Error {
    /// Stable discriminant for the frontend to branch on, so it never has to match on
    /// message text.
    pub fn kind(&self) -> &'static str {
        match self {
            Error::Db(_) => "database",
            Error::Io(_) => "io",
            Error::Keychain(_) => "keychain",
            Error::Vault(_) => "vault",
            Error::NotFound(..) => "notFound",
            Error::Invalid(_) => "invalid",
            Error::UnresolvedVariables(_) => "unresolvedVariables",
            Error::Ssh(_) => "ssh",
            Error::Auth(_) => "auth",
            Error::PinRequired => "pinRequired",
            Error::UnknownHostKey { .. } => "unknownHostKey",
            Error::ChangedHostKey { .. } => "changedHostKey",
            Error::NoSession(_) => "noSession",
            Error::PasswordRequired { .. } => "passwordRequired",
            Error::Sync(_) => "sync",
            Error::SyncUnauthorised => "syncUnauthorised",
            Error::Icon(_) => "icon",
            Error::Update(_) => "update",
            Error::Internal(_) => "internal",
        }
    }
}

/// Serialized as a tagged object rather than a bare string: the host key and unresolved
/// variable cases carry data the UI has to act on, and parsing that back out of a
/// message would be fragile.
impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("kind", self.kind())?;
        map.serialize_entry("message", &self.to_string())?;

        match self {
            Error::UnresolvedVariables(names) => {
                map.serialize_entry("variables", names)?;
            }
            Error::UnknownHostKey { host, fingerprint } => {
                map.serialize_entry("host", host)?;
                map.serialize_entry("fingerprint", fingerprint)?;
            }
            Error::PasswordRequired { username, host } => {
                map.serialize_entry("username", username)?;
                map.serialize_entry("host", host)?;
            }
            Error::ChangedHostKey {
                host,
                fingerprint,
                line,
            } => {
                map.serialize_entry("host", host)?;
                map.serialize_entry("fingerprint", fingerprint)?;
                map.serialize_entry("line", line)?;
            }
            _ => {}
        }

        map.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(error: &Error) -> serde_json::Value {
        serde_json::to_value(error).expect("errors must serialize")
    }

    #[test]
    fn carries_the_fingerprint_for_an_unknown_host() {
        let value = json(&Error::UnknownHostKey {
            host: "example.com:22".into(),
            fingerprint: "SHA256:abc".into(),
        });

        assert_eq!(value["kind"], "unknownHostKey");
        assert_eq!(value["fingerprint"], "SHA256:abc");
        assert_eq!(value["host"], "example.com:22");
    }

    #[test]
    fn carries_the_known_hosts_line_for_a_changed_key() {
        let value = json(&Error::ChangedHostKey {
            host: "example.com:22".into(),
            fingerprint: "SHA256:abc".into(),
            line: 42,
        });

        assert_eq!(value["kind"], "changedHostKey");
        assert_eq!(value["line"], 42);
    }

    #[test]
    fn lists_every_unresolved_variable() {
        let value = json(&Error::UnresolvedVariables(vec!["wg_user".into(), "target".into()]));

        assert_eq!(value["kind"], "unresolvedVariables");
        assert_eq!(value["variables"], serde_json::json!(["wg_user", "target"]));
    }

    #[test]
    fn asks_for_a_password_with_enough_context_to_prompt() {
        let value = json(&Error::PasswordRequired {
            username: "root".into(),
            host: "example.com:22".into(),
        });

        // The prompt has to say who it is asking for, on which host.
        assert_eq!(value["kind"], "passwordRequired");
        assert_eq!(value["username"], "root");
        assert_eq!(value["host"], "example.com:22");
    }

    #[test]
    fn plain_errors_still_carry_a_message() {
        let value = json(&Error::Auth("nope".into()));

        assert_eq!(value["kind"], "auth");
        assert_eq!(value["message"], "authentication failed: nope");
    }
}
