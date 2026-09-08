use std::path::Path;
use std::sync::Arc;

use serde::Serialize;

use crate::crypto::vault::Vault;
use crate::db::Db;
use crate::error::{Error, Result};
use crate::ssh::session::Sessions;
use crate::sync::engine::SyncEngine;

pub struct AppState {
    pub db: Arc<Db>,
    /// `None` when the key store could not be opened. The app still runs: hosts, groups
    /// and settings work, only operations that touch secrets are refused.
    vault: Option<Arc<Vault>>,
    vault_error: Option<String>,
    sessions: Arc<Sessions>,
    sync: Arc<SyncEngine>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    pub available: bool,
    pub error: Option<String>,
    /// True when the dev key file is in use instead of the OS keychain.
    pub development_key_store: bool,
}

impl AppState {
    pub fn new(data_dir: &Path) -> Result<Self> {
        // Debug builds keep their own database so experiments never land in real data,
        // and their own key store because the macOS keychain ACL does not survive a
        // rebuild. Release builds use the OS keychain.
        let (db_name, vault) = if cfg!(debug_assertions) {
            (
                "remotier-dev.db",
                Vault::open_dev_file(&data_dir.join("dev-dek.key")),
            )
        } else {
            ("remotier.db", Vault::open_keychain())
        };

        let db = Arc::new(Db::open(&data_dir.join(db_name))?);

        let (vault, vault_error) = match vault {
            Ok(vault) => (Some(Arc::new(vault)), None),
            Err(e) => {
                log::error!("key store unavailable, secrets are disabled: {e}");
                (None, Some(e.to_string()))
            }
        };

        if cfg!(debug_assertions) && vault.is_some() {
            log::warn!(
                "using the development key store (dev-dek.key) - secrets are protected by \
                 file permissions only"
            );
        }

        let sync = Arc::new(SyncEngine::new(Arc::clone(&db), vault.clone()));

        Ok(Self {
            db,
            vault,
            vault_error,
            sessions: Arc::new(Sessions::new()),
            sync,
        })
    }

    pub fn sessions(&self) -> Arc<Sessions> {
        Arc::clone(&self.sessions)
    }

    pub fn sync(&self) -> Arc<SyncEngine> {
        Arc::clone(&self.sync)
    }

    /// Access the vault, or explain why secrets are unavailable.
    pub fn vault(&self) -> Result<&Vault> {
        self.vault.as_deref().ok_or_else(|| {
            Error::Vault(
                self.vault_error
                    .clone()
                    .unwrap_or_else(|| "key store unavailable".into()),
            )
        })
    }

    pub fn vault_status(&self) -> VaultStatus {
        VaultStatus {
            available: self.vault.is_some(),
            error: self.vault_error.clone(),
            development_key_store: cfg!(debug_assertions),
        }
    }
}
