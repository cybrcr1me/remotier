//! Turns a stored host into everything needed to dial it.
//!
//! This is where inheritance and placeholders are applied: a host inherits port,
//! identity and jump host from its group chain, and `{{placeholders}}` in the hostname
//! and username are filled from the variable scopes.

use std::collections::HashMap;

use rusqlite::params;
use serde::Serialize;
use zeroize::Zeroizing;

use crate::commands::secrets;
use crate::crypto::vault::Vault;
use crate::db::models::{AuthKind, Group, Host, KeySource};
use crate::db::{query_all, query_one, Db};
use crate::error::{Error, Result};
use crate::vars;

pub const DEFAULT_PORT: u16 = 22;
pub const DEFAULT_TERM: &str = "xterm-256color";

/// Credentials for one connection. Secrets are zeroized when this is dropped.
pub enum AuthMaterial {
    Password(Zeroizing<String>),
    /// A private key held in the vault.
    Key {
        pem: Zeroizing<String>,
        passphrase: Option<Zeroizing<String>>,
    },
    /// A key left on disk, read at connect time and never copied.
    KeyPath {
        path: String,
        passphrase: Option<Zeroizing<String>>,
    },
    Agent {
        /// Restrict to this public key when the identity names one.
        public_openssh: Option<String>,
    },
    Interactive(Option<Zeroizing<String>>),
}

pub struct Target {
    pub host_id: String,
    pub label: String,
    pub hostname: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMaterial,
}

/// The safe-to-show half of a resolution, for previews in the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetPreview {
    pub host_id: String,
    pub label: String,
    pub hostname: String,
    pub port: u16,
    pub username: String,
    pub auth_kind: AuthKind,
    pub identity_label: Option<String>,
    pub key_label: Option<String>,
    /// Placeholders that still have no value. Non-empty means connecting will fail.
    pub missing_variables: Vec<String>,
}

/// Walk from a host's group up to the root. Nearest first.
fn group_chain(db: &Db, group_id: Option<&str>) -> Result<Vec<Group>> {
    let mut chain = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut current = group_id.map(str::to_string);

    while let Some(id) = current {
        // Defends against a cycle introduced by a bad edit; without it this loops forever.
        if !seen.insert(id.clone()) {
            break;
        }
        let group = db.read(|conn| {
            let sql = format!("SELECT {} FROM groups WHERE id = ?1", Group::COLUMNS);
            query_one(conn, &sql, params![id], Group::from_row, "group", &id)
        })?;
        current = group.parent_id.clone();
        chain.push(group);
    }
    Ok(chain)
}

/// Collect variable values, weakest scope first so nearer scopes overwrite.
fn variable_values(db: &Db, host: &Host, chain: &[Group]) -> Result<HashMap<String, String>> {
    let mut builder = vars::ValueBuilder::new();

    // Declared defaults, root group first.
    for group in chain.iter().rev() {
        builder.layer(defaults_for(db, "group", &group.id)?);
    }
    builder.layer(defaults_for(db, "host", &host.id)?);

    // Local answers override declared defaults, same ordering.
    for group in chain.iter().rev() {
        builder.layer(values_for(db, "group", &group.id)?);
    }
    builder.layer(values_for(db, "host", &host.id)?);

    builder.with_builtins([
        ("host".to_string(), host.hostname.clone()),
        ("group".to_string(), chain.first().map(|g| g.name.clone()).unwrap_or_default()),
        ("user".to_string(), whoami()),
    ]);

    Ok(builder.build())
}

fn whoami() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default()
}

fn defaults_for(db: &Db, scope: &str, scope_id: &str) -> Result<Vec<(String, String)>> {
    db.read(|conn| {
        query_all(
            conn,
            "SELECT name, default_value FROM var_defs
             WHERE scope = ?1 AND scope_id = ?2 AND default_value IS NOT NULL",
            params![scope, scope_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
    })
}

fn values_for(db: &Db, scope: &str, scope_id: &str) -> Result<Vec<(String, String)>> {
    db.read(|conn| {
        query_all(
            conn,
            "SELECT name, value FROM var_values WHERE scope = ?1 AND scope_id = ?2",
            params![scope, scope_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
    })
}

struct Resolution {
    host: Host,
    port: u16,
    username: String,
    hostname: String,
    identity: Option<IdentityRow>,
    missing: Vec<String>,
}

struct IdentityRow {
    label: String,
    auth_kind: AuthKind,
    password_ref: Option<String>,
    key_id: Option<String>,
}

fn resolve_common(db: &Db, host_id: &str) -> Result<Resolution> {
    let host = db.read(|conn| {
        let sql = format!("SELECT {} FROM hosts WHERE id = ?1", Host::COLUMNS);
        query_one(conn, &sql, params![host_id], Host::from_row, "host", host_id)
    })?;

    let chain = group_chain(db, host.group_id.as_deref())?;

    // NULL on the host means inherit, so fall through the chain before the default.
    let port = host
        .port
        .or_else(|| chain.iter().find_map(|g| g.default_port))
        .unwrap_or(i64::from(DEFAULT_PORT));
    let port = u16::try_from(port)
        .map_err(|_| Error::Invalid(format!("port {port} is out of range")))?;

    let identity_id = host
        .identity_id
        .clone()
        .or_else(|| chain.iter().find_map(|g| g.default_identity_id.clone()));

    let identity = match identity_id.clone() {
        Some(id) => db.read(|conn| {
            query_one(
                conn,
                "SELECT label, auth_kind, password_ref, key_id FROM identities WHERE id = ?1",
                params![id],
                |row| {
                    let auth_kind: String = row.get(1)?;
                    Ok(IdentityRow {
                        label: row.get(0)?,
                        auth_kind: AuthKind::parse(&auth_kind),
                        password_ref: row.get(2)?,
                        key_id: row.get(3)?,
                    })
                },
                "identity",
                &id,
            )
        })
        .map(Some)?,
        None => None,
    };

    let values = variable_values(db, &host, &chain)?;

    // The username template lives on the identity and may itself contain placeholders.
    let raw_username = db.read(|conn| match identity_id.as_deref() {
        Some(id) => query_one(
            conn,
            "SELECT username FROM identities WHERE id = ?1",
            params![id],
            |row| Ok(row.get::<_, String>(0)?),
            "identity",
            id,
        ),
        None => Ok(whoami()),
    })?;

    let mut missing = vars::missing(&host.hostname, &values);
    missing.extend(vars::missing(&raw_username, &values));
    missing.sort();
    missing.dedup();

    let hostname = vars::render(&host.hostname, &values).unwrap_or_else(|_| host.hostname.clone());
    let username = vars::render(&raw_username, &values).unwrap_or(raw_username);

    Ok(Resolution {
        host,
        port,
        username,
        hostname,
        identity,
        missing,
    })
}

/// Resolve without touching the vault - safe to call for UI previews.
pub fn preview(db: &Db, host_id: &str) -> Result<TargetPreview> {
    let resolution = resolve_common(db, host_id)?;

    let key_label = match resolution.identity.as_ref().and_then(|i| i.key_id.clone()) {
        Some(key_id) => db
            .read(|conn| {
                query_one(
                    conn,
                    "SELECT label FROM keys WHERE id = ?1",
                    params![key_id],
                    |row| Ok(row.get::<_, String>(0)?),
                    "key",
                    &key_id,
                )
            })
            .ok(),
        None => None,
    };

    Ok(TargetPreview {
        host_id: resolution.host.id,
        label: resolution.host.label,
        hostname: resolution.hostname,
        port: resolution.port,
        username: resolution.username,
        auth_kind: resolution
            .identity
            .as_ref()
            .map_or(AuthKind::Agent, |i| i.auth_kind),
        identity_label: resolution.identity.map(|i| i.label),
        key_label,
        missing_variables: resolution.missing,
    })
}

/// Full resolution including credentials.
///
/// # Errors
///
/// [`Error::UnresolvedVariables`] when a placeholder has no value, so the UI can collect
/// them before a connection is attempted rather than after it fails to authenticate.
pub fn target(db: &Db, vault: &Vault, host_id: &str) -> Result<Target> {
    let resolution = resolve_common(db, host_id)?;

    if !resolution.missing.is_empty() {
        return Err(Error::UnresolvedVariables(resolution.missing));
    }

    let auth = match &resolution.identity {
        // No identity configured: the agent is the only thing we can try.
        None => AuthMaterial::Agent { public_openssh: None },
        Some(identity) => {
            let password = match &identity.password_ref {
                Some(reference) => Some(db.read(|conn| secrets::get(conn, vault, reference))?),
                None => None,
            };

            match identity.auth_kind {
                AuthKind::Password => AuthMaterial::Password(
                    password.ok_or_else(|| Error::Auth("no password stored for this identity".into()))?,
                ),
                AuthKind::Interactive => AuthMaterial::Interactive(password),
                AuthKind::Agent => AuthMaterial::Agent {
                    public_openssh: key_public(db, identity.key_id.as_deref())?,
                },
                AuthKind::Key => key_material(db, vault, identity.key_id.as_deref())?,
            }
        }
    };

    Ok(Target {
        host_id: resolution.host.id,
        label: resolution.host.label,
        hostname: resolution.hostname,
        port: resolution.port,
        username: resolution.username,
        auth,
    })
}

fn key_public(db: &Db, key_id: Option<&str>) -> Result<Option<String>> {
    let Some(key_id) = key_id else { return Ok(None) };
    db.read(|conn| {
        query_one(
            conn,
            "SELECT public_key FROM keys WHERE id = ?1",
            params![key_id],
            |row| Ok(row.get::<_, String>(0)?),
            "key",
            key_id,
        )
    })
    .map(Some)
}

fn key_material(db: &Db, vault: &Vault, key_id: Option<&str>) -> Result<AuthMaterial> {
    let key_id = key_id
        .ok_or_else(|| Error::Auth("this identity is set to key auth but has no key".into()))?;

    let (source, private_key_ref, path, passphrase_ref) = db.read(|conn| {
        query_one(
            conn,
            "SELECT source, private_key_ref, path, passphrase_ref FROM keys WHERE id = ?1",
            params![key_id],
            |row| {
                let source: String = row.get(0)?;
                Ok((
                    KeySource::parse(&source),
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
            "key",
            key_id,
        )
    })?;

    let passphrase = match &passphrase_ref {
        Some(reference) => Some(db.read(|conn| secrets::get(conn, vault, reference))?),
        None => None,
    };

    match source {
        KeySource::Managed => {
            let reference = private_key_ref
                .ok_or_else(|| Error::Auth("this key has no stored private half".into()))?;
            Ok(AuthMaterial::Key {
                pem: db.read(|conn| secrets::get(conn, vault, &reference))?,
                passphrase,
            })
        }
        KeySource::SystemPath => Ok(AuthMaterial::KeyPath {
            path: path.ok_or_else(|| Error::Auth("this key has no path on disk".into()))?,
            passphrase,
        }),
        KeySource::Agent => Ok(AuthMaterial::Agent {
            public_openssh: key_public(db, Some(key_id))?,
        }),
    }
}
