//! Seeds the development database with a host pointing at the dockerised test server.
//!
//! Development only - it writes to the debug database (`remotier-dev.db`) and uses the
//! dev key file, never the real one. Run it with the app closed:
//!
//! ```text
//! cargo run --example seed_dev
//! ```

use remotier_lib::commands::secrets;
use remotier_lib::crypto::vault::Vault;
use remotier_lib::db::{new_id, now_ms, Db};

fn data_dir() -> std::path::PathBuf {
    dirs::data_dir()
        .expect("a data directory")
        .join("de.flexusma.remotier")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = data_dir();
    let db = Db::open(&dir.join("remotier-dev.db"))?;
    let vault = Vault::open_dev_file(&dir.join("dev-dek.key"))?;

    let identity_id = new_id();
    let host_id = new_id();
    let now = now_ms();

    db.write(|tx| {
        // Re-running should replace the fixture rather than pile up duplicates.
        tx.execute("DELETE FROM hosts WHERE label = 'docker test server'", [])?;
        tx.execute("DELETE FROM identities WHERE label = 'docker test user'", [])?;

        let password_ref = secrets::put(tx, &vault, None, "testpass")?;

        tx.execute(
            "INSERT INTO identities (id, label, username, auth_kind, password_ref, created_at, updated_at)
             VALUES (?1, 'docker test user', 'test', 'password', ?2, ?3, ?3)",
            rusqlite::params![identity_id, password_ref, now],
        )?;

        tx.execute(
            "INSERT INTO hosts (id, label, hostname, port, identity_id, sort, created_at, updated_at)
             VALUES (?1, 'docker test server', '127.0.0.1', 2222, ?2, 0, ?3, ?3)",
            rusqlite::params![host_id, identity_id, now],
        )?;
        Ok(())
    })?;

    println!("seeded host 'docker test server' -> test@127.0.0.1:2222");
    Ok(())
}
