//! Key repository metadata. Generating, importing and parsing real key material lands in
//! phase 3 with the SSH engine; this module owns storage and lifecycle only.

use rusqlite::params;
use serde::Deserialize;
use tauri::State;

use crate::commands::secrets;
use crate::db::models::{KeySource, SshKey};
use crate::db::{new_id, now_ms, query_all, query_one};
use crate::error::{Error, Result};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyMetaInput {
    pub label: String,
    pub passphrase: Option<String>,
}

#[tauri::command(async)]
pub fn list_keys(state: State<'_, AppState>) -> Result<Vec<SshKey>> {
    state.db.read(|conn| {
        let sql = format!("SELECT {} FROM keys ORDER BY label", SshKey::COLUMNS);
        query_all(conn, &sql, [], SshKey::from_row)
    })
}

/// Register a key that stays where it already lives on disk (typically `~/.ssh/id_*`).
/// The private key is never read or copied here - only the path is remembered.
#[tauri::command(async)]
pub fn register_system_key(
    state: State<'_, AppState>,
    label: String,
    path: String,
    public_key: String,
    algorithm: String,
    fingerprint: String,
    comment: Option<String>,
) -> Result<SshKey> {
    let id = new_id();
    let now = now_ms();

    state.db.write(|tx| {
        tx.execute(
            "INSERT INTO keys (id, label, algorithm, source, public_key, fingerprint,
                               private_key_ref, path, passphrase_ref, comment, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, NULL, ?8, ?9, ?9)",
            params![
                id,
                label.trim(),
                algorithm,
                KeySource::SystemPath.as_str(),
                public_key.trim(),
                fingerprint,
                path,
                comment,
                now,
            ],
        )?;
        Ok(())
    })?;

    get_key(state, id)
}

#[tauri::command(async)]
pub fn update_key(state: State<'_, AppState>, id: String, input: KeyMetaInput) -> Result<SshKey> {
    state.db.write(|tx| {
        let existing: Option<String> = tx
            .query_row(
                "SELECT passphrase_ref FROM keys WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .map_err(|_| Error::NotFound("key", id.clone()))?;

        let passphrase_ref = match input.passphrase.as_deref() {
            None => existing,
            Some("") => {
                secrets::delete(tx, existing.as_deref())?;
                None
            }
            Some(passphrase) => {
                Some(secrets::put(tx, state.vault()?, existing.as_deref(), passphrase)?)
            }
        };

        tx.execute(
            "UPDATE keys SET label = ?2, passphrase_ref = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, input.label.trim(), passphrase_ref, now_ms()],
        )?;
        Ok(())
    })?;

    get_key(state, id)
}

/// Removes the key from the repository. For `system_path` keys the file on disk is left
/// alone - Remotier never deletes something it did not create.
#[tauri::command(async)]
pub fn delete_key(state: State<'_, AppState>, id: String) -> Result<()> {
    state.db.write(|tx| {
        let (private_key_ref, passphrase_ref): (Option<String>, Option<String>) = tx
            .query_row(
                "SELECT private_key_ref, passphrase_ref FROM keys WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| Error::NotFound("key", id.clone()))?;

        secrets::delete(tx, private_key_ref.as_deref())?;
        secrets::delete(tx, passphrase_ref.as_deref())?;
        tx.execute("DELETE FROM keys WHERE id = ?1", params![id])?;
        Ok(())
    })
}

fn get_key(state: State<'_, AppState>, id: String) -> Result<SshKey> {
    state.db.read(|conn| {
        let sql = format!("SELECT {} FROM keys WHERE id = ?1", SshKey::COLUMNS);
        query_one(conn, &sql, params![id], SshKey::from_row, "key", &id)
    })
}

/// Generate a key pair. The private half is sealed in the vault; only the public half
/// and metadata are ever returned.
#[tauri::command(async)]
pub fn generate_key(
    state: State<'_, AppState>,
    label: String,
    algorithm: crate::ssh::keys::KeyAlgorithm,
    comment: Option<String>,
    passphrase: Option<String>,
) -> Result<SshKey> {
    let generated = crate::ssh::keys::generate(algorithm, comment.as_deref().unwrap_or_default())?;
    store_managed(
        state,
        label,
        &generated.private_openssh,
        &generated.public_openssh,
        &generated.algorithm,
        &generated.fingerprint,
        comment,
        passphrase,
    )
}

/// Import an existing OpenSSH private key into the vault.
#[tauri::command(async)]
pub fn import_key(
    state: State<'_, AppState>,
    label: String,
    pem: String,
    passphrase: Option<String>,
) -> Result<SshKey> {
    // Parsing first means a bad key or wrong passphrase is rejected before anything is
    // written, and it gives us the public half and fingerprint.
    let parsed = crate::ssh::keys::parse_private(&pem, passphrase.as_deref())?;
    let public = parsed.public_key();
    let public_openssh = public
        .to_openssh()
        .map_err(|e| Error::Invalid(format!("could not encode the public key: {e}")))?;
    let fingerprint = public
        .fingerprint(russh::keys::ssh_key::HashAlg::Sha256)
        .to_string();
    let algorithm = public.algorithm().to_string();
    let comment = Some(public.comment().to_string()).filter(|c| !c.is_empty());

    store_managed(
        state,
        label,
        &pem,
        &public_openssh,
        &algorithm,
        &fingerprint,
        comment,
        passphrase,
    )
}

#[allow(clippy::too_many_arguments)]
fn store_managed(
    state: State<'_, AppState>,
    label: String,
    private_pem: &str,
    public_openssh: &str,
    algorithm: &str,
    fingerprint: &str,
    comment: Option<String>,
    passphrase: Option<String>,
) -> Result<SshKey> {
    let id = new_id();
    let now = now_ms();
    let vault = state.vault()?;

    state.db.write(|tx| {
        let private_key_ref = secrets::put(tx, vault, None, private_pem)?;
        let passphrase_ref = match passphrase.as_deref().filter(|p| !p.is_empty()) {
            Some(passphrase) => Some(secrets::put(tx, vault, None, passphrase)?),
            None => None,
        };

        tx.execute(
            "INSERT INTO keys (id, label, algorithm, source, public_key, fingerprint,
                               private_key_ref, path, passphrase_ref, comment, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9, ?10, ?10)",
            params![
                id,
                label.trim(),
                algorithm,
                KeySource::Managed.as_str(),
                public_openssh.trim(),
                fingerprint,
                private_key_ref,
                passphrase_ref,
                comment,
                now,
            ],
        )?;
        Ok(())
    })?;

    get_key(state, id)
}
