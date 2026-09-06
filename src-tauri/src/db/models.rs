//! Row types shared with the frontend. Field names are camelCased on the wire so the
//! TypeScript side needs no translation layer.

use rusqlite::Row;
use serde::{Deserialize, Serialize};

use crate::error::Result;

fn tags_from_json(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub sort: i64,
    /// Opaque key resolved to an icon by the frontend.
    pub icon: Option<String>,
    /// Opaque key resolved to a border colour by the frontend.
    pub color: Option<String>,
    pub default_port: Option<i64>,
    pub default_identity_id: Option<String>,
    pub default_jump_host_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Group {
    pub const COLUMNS: &'static str =
        "id, parent_id, name, sort, default_port, default_identity_id, default_jump_host_id, \
         icon, color, created_at, updated_at";

    pub fn from_row(row: &Row) -> Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            parent_id: row.get(1)?,
            name: row.get(2)?,
            sort: row.get(3)?,
            default_port: row.get(4)?,
            default_identity_id: row.get(5)?,
            default_jump_host_id: row.get(6)?,
            icon: row.get(7)?,
            color: row.get(8)?,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Host {
    pub id: String,
    pub group_id: Option<String>,
    pub label: String,
    pub hostname: String,
    /// `None` inherits the group chain, then the global default.
    pub port: Option<i64>,
    pub identity_id: Option<String>,
    pub jump_host_id: Option<String>,
    pub color: Option<String>,
    /// Opaque key resolved to an icon by the frontend.
    pub icon: Option<String>,
    pub tags: Vec<String>,
    pub sort: i64,
    /// Set when the host carries its own credentials instead of using an identity.
    pub username: Option<String>,
    /// `None` means "use the inherited identity"; `Some` means these credentials win.
    pub auth_kind: Option<AuthKind>,
    /// Whether a password is stored on the host itself. Never the password.
    pub has_password: bool,
    pub key_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Host {
    pub const COLUMNS: &'static str =
        "id, group_id, label, hostname, port, identity_id, jump_host_id, color, tags, sort, \
         username, auth_kind, password_ref, key_id, icon, created_at, updated_at";

    pub fn from_row(row: &Row) -> Result<Self> {
        let tags: String = row.get(8)?;
        let auth_kind: Option<String> = row.get(11)?;
        let password_ref: Option<String> = row.get(12)?;
        Ok(Self {
            id: row.get(0)?,
            group_id: row.get(1)?,
            label: row.get(2)?,
            hostname: row.get(3)?,
            port: row.get(4)?,
            identity_id: row.get(5)?,
            jump_host_id: row.get(6)?,
            color: row.get(7)?,
            tags: tags_from_json(&tags),
            sort: row.get(9)?,
            username: row.get(10)?,
            auth_kind: auth_kind.as_deref().map(AuthKind::parse),
            has_password: password_ref.is_some(),
            key_id: row.get(13)?,
            icon: row.get(14)?,
            created_at: row.get(15)?,
            updated_at: row.get(16)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthKind {
    Password,
    Key,
    Agent,
    Interactive,
}

impl AuthKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AuthKind::Password => "password",
            AuthKind::Key => "key",
            AuthKind::Agent => "agent",
            AuthKind::Interactive => "interactive",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw {
            "password" => AuthKind::Password,
            "key" => AuthKind::Key,
            "agent" => AuthKind::Agent,
            _ => AuthKind::Interactive,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub id: String,
    pub label: String,
    pub username: String,
    pub auth_kind: AuthKind,
    /// Whether a password is stored, never the password itself.
    pub has_password: bool,
    pub key_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Identity {
    pub const COLUMNS: &'static str =
        "id, label, username, auth_kind, password_ref, key_id, created_at, updated_at";

    pub fn from_row(row: &Row) -> Result<Self> {
        let auth_kind: String = row.get(3)?;
        let password_ref: Option<String> = row.get(4)?;
        Ok(Self {
            id: row.get(0)?,
            label: row.get(1)?,
            username: row.get(2)?,
            auth_kind: AuthKind::parse(&auth_kind),
            has_password: password_ref.is_some(),
            key_id: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeySource {
    /// Private key sealed in the vault.
    Managed,
    /// Left on disk where it already lives; we only remember the path.
    SystemPath,
    /// Held by an ssh-agent; only the public half is known.
    Agent,
}

impl KeySource {
    pub fn as_str(self) -> &'static str {
        match self {
            KeySource::Managed => "managed",
            KeySource::SystemPath => "system_path",
            KeySource::Agent => "agent",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw {
            "managed" => KeySource::Managed,
            "agent" => KeySource::Agent,
            _ => KeySource::SystemPath,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshKey {
    pub id: String,
    pub label: String,
    pub algorithm: String,
    pub source: KeySource,
    pub public_key: String,
    pub fingerprint: String,
    pub path: Option<String>,
    pub comment: Option<String>,
    pub has_passphrase: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

impl SshKey {
    pub const COLUMNS: &'static str =
        "id, label, algorithm, source, public_key, fingerprint, path, comment, passphrase_ref, created_at, updated_at";

    pub fn from_row(row: &Row) -> Result<Self> {
        let source: String = row.get(3)?;
        let passphrase_ref: Option<String> = row.get(8)?;
        Ok(Self {
            id: row.get(0)?,
            label: row.get(1)?,
            algorithm: row.get(2)?,
            source: KeySource::parse(&source),
            public_key: row.get(4)?,
            fingerprint: row.get(5)?,
            path: row.get(6)?,
            comment: row.get(7)?,
            has_passphrase: passphrase_ref.is_some(),
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VarScope {
    Group,
    Host,
}

impl VarScope {
    pub fn as_str(self) -> &'static str {
        match self {
            VarScope::Group => "group",
            VarScope::Host => "host",
        }
    }
}

/// A placeholder a group or host declares. Shared, and syncable.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VarDef {
    pub id: String,
    pub scope: VarScope,
    pub scope_id: String,
    pub name: String,
    pub label: Option<String>,
    pub default_value: Option<String>,
    pub required: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

impl VarDef {
    pub const COLUMNS: &'static str =
        "id, scope, scope_id, name, label, default_value, required, created_at, updated_at";

    pub fn from_row(row: &Row) -> Result<Self> {
        let scope: String = row.get(1)?;
        Ok(Self {
            id: row.get(0)?,
            scope: if scope == "group" { VarScope::Group } else { VarScope::Host },
            scope_id: row.get(2)?,
            name: row.get(3)?,
            label: row.get(4)?,
            default_value: row.get(5)?,
            required: row.get::<_, i64>(6)? != 0,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    }
}

/// A user's own answer to a `VarDef`. Local only - never leaves this machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VarValue {
    pub scope: VarScope,
    pub scope_id: String,
    pub name: String,
    pub value: String,
}
