//! Helpers shared by the command modules for putting secrets into, and taking them out
//! of, the sealed `secrets` table. Nothing here is exposed to the frontend directly:
//! plaintext secrets only ever travel inbound.

use rusqlite::{params, Transaction};

use crate::crypto::vault::Vault;
use crate::db::{new_id, now_ms};
use crate::error::Result;

/// Seal `plaintext` and store it, reusing `existing` if there already is a row.
/// Returns the ref to record on the owning row.
pub fn put(
    tx: &Transaction,
    vault: &Vault,
    existing: Option<&str>,
    plaintext: &str,
) -> Result<String> {
    let (nonce, ciphertext) = vault.seal_str(plaintext)?;
    let now = now_ms();

    match existing {
        Some(reference) => {
            tx.execute(
                "UPDATE secrets SET nonce = ?1, ciphertext = ?2, updated_at = ?3 WHERE ref = ?4",
                params![nonce, ciphertext, now, reference],
            )?;
            Ok(reference.to_string())
        }
        None => {
            let reference = new_id();
            tx.execute(
                "INSERT INTO secrets (ref, nonce, ciphertext, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                params![reference, nonce, ciphertext, now],
            )?;
            Ok(reference)
        }
    }
}

pub fn delete(tx: &Transaction, reference: Option<&str>) -> Result<()> {
    if let Some(reference) = reference {
        tx.execute("DELETE FROM secrets WHERE ref = ?1", params![reference])?;
    }
    Ok(())
}

/// Read a sealed secret back out. Callers keep the plaintext in memory only as long as
/// they need it - it is zeroized on drop.
pub fn get(
    conn: &rusqlite::Connection,
    vault: &Vault,
    reference: &str,
) -> Result<zeroize::Zeroizing<String>> {
    let (nonce, ciphertext): (Vec<u8>, Vec<u8>) = conn.query_row(
        "SELECT nonce, ciphertext FROM secrets WHERE ref = ?1",
        params![reference],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    vault.unseal_str(&nonce, &ciphertext)
}
