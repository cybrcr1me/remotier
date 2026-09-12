//! Secret sealing.
//!
//! A single random 256-bit data encryption key (DEK) lives in the OS keychain
//! (macOS Keychain / Windows Credential Manager / Secret Service). Every secret --
//! passwords, key passphrases, private keys -- is sealed with that DEK using
//! XChaCha20-Poly1305 and stored as ciphertext in SQLite.
//!
//! Consequences worth knowing:
//!
//! - The database file on its own is useless without the keychain entry.
//! - Later cloud sync can ship the sealed blobs as-is; only the DEK has to be
//!   re-wrapped (e.g. under a master password) to make it end-to-end encrypted.

use std::path::Path;

use chacha20poly1305::aead::{Aead, Generate, Key, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

use crate::error::{Error, Result};

const KEYCHAIN_SERVICE: &str = "app.remotier.app";
const KEYCHAIN_ACCOUNT: &str = "vault-dek";
const KEY_LEN: usize = 32;

pub struct Vault {
    cipher: XChaCha20Poly1305,
}

impl Vault {
    /// Load the DEK from the OS keychain, creating one on first run.
    ///
    /// Note for macOS: a keychain item's ACL is bound to the binary that created it, so an
    /// unsigned build loses access to its own entry as soon as it is recompiled. Release
    /// builds carry a stable signing identity and are fine; debug builds use
    /// [`Vault::open_dev_file`] instead.
    pub fn open_keychain() -> Result<Self> {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)?;

        let key_bytes: Zeroizing<Vec<u8>> = match entry.get_secret() {
            Ok(bytes) => Zeroizing::new(bytes),
            Err(keyring::Error::NoEntry) => {
                let key = Key::<XChaCha20Poly1305>::generate();
                entry.set_secret(key.as_slice())?;
                Zeroizing::new(key.to_vec())
            }
            Err(e) => return Err(Error::Keychain(e.to_string())),
        };

        if key_bytes.len() != KEY_LEN {
            return Err(Error::Vault(format!(
                "keychain entry is {} bytes, expected {KEY_LEN}",
                key_bytes.len()
            )));
        }
        Self::from_key(&key_bytes)
    }

    /// Development-only key store: a 0600 file next to the dev database.
    ///
    /// This exists because the macOS keychain ACL is invalidated on every rebuild, which
    /// would make the app refuse to start after each code change. It is deliberately NOT
    /// used in release builds - the key sits on disk unprotected, and its only defence is
    /// file permissions.
    pub fn open_dev_file(path: &Path) -> Result<Self> {
        let key_bytes: Zeroizing<Vec<u8>> = if path.exists() {
            Zeroizing::new(std::fs::read(path)?)
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let key = Key::<XChaCha20Poly1305>::generate();
            std::fs::write(path, key.as_slice())?;
            restrict_permissions(path)?;
            Zeroizing::new(key.to_vec())
        };

        if key_bytes.len() != KEY_LEN {
            return Err(Error::Vault(format!(
                "dev key file is {} bytes, expected {KEY_LEN}",
                key_bytes.len()
            )));
        }
        Self::from_key(&key_bytes)
    }

    /// Build a vault from raw key material. Used by `open` and by tests, which must not
    /// touch the real keychain.
    pub fn from_key(key: &[u8]) -> Result<Self> {
        let cipher = XChaCha20Poly1305::new_from_slice(key)
            .map_err(|_| Error::Vault("invalid key length".into()))?;
        Ok(Self { cipher })
    }

    /// Seal a secret. Returns `(nonce, ciphertext)`; both are stored alongside each other.
    pub fn seal(&self, plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
        let nonce = XNonce::generate();
        let ciphertext = self
            .cipher
            .encrypt(&nonce, plaintext)
            .map_err(|_| Error::Vault("encryption failed".into()))?;
        Ok((nonce.to_vec(), ciphertext))
    }

    pub fn unseal(&self, nonce: &[u8], ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
        let nonce = XNonce::try_from(nonce)
            .map_err(|_| Error::Vault("stored nonce has the wrong length".into()))?;
        let plaintext = self
            .cipher
            .decrypt(&nonce, ciphertext)
            .map_err(|_| Error::Vault("decryption failed - wrong key or tampered data".into()))?;
        Ok(Zeroizing::new(plaintext))
    }

    pub fn seal_str(&self, plaintext: &str) -> Result<(Vec<u8>, Vec<u8>)> {
        self.seal(plaintext.as_bytes())
    }

    pub fn unseal_str(&self, nonce: &[u8], ciphertext: &[u8]) -> Result<Zeroizing<String>> {
        let bytes = self.unseal(nonce, ciphertext)?;
        let s = String::from_utf8(bytes.to_vec())
            .map_err(|_| Error::Vault("sealed value is not valid UTF-8".into()))?;
        Ok(Zeroizing::new(s))
    }
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_vault() -> Vault {
        Vault::from_key(&[7u8; KEY_LEN]).unwrap()
    }

    #[test]
    fn seals_and_unseals() {
        let vault = test_vault();
        let (nonce, ct) = vault.seal_str("hunter2").unwrap();
        assert_ne!(ct.as_slice(), b"hunter2");
        assert_eq!(*vault.unseal_str(&nonce, &ct).unwrap(), "hunter2");
    }

    #[test]
    fn nonce_is_unique_per_seal() {
        let vault = test_vault();
        let (n1, c1) = vault.seal_str("same").unwrap();
        let (n2, c2) = vault.seal_str("same").unwrap();
        assert_ne!(n1, n2);
        assert_ne!(c1, c2);
    }

    #[test]
    fn rejects_tampered_ciphertext() {
        let vault = test_vault();
        let (nonce, mut ct) = vault.seal_str("hunter2").unwrap();
        ct[0] ^= 0xff;
        assert!(vault.unseal(&nonce, &ct).is_err());
    }

    #[test]
    fn rejects_wrong_key() {
        let (nonce, ct) = test_vault().seal_str("hunter2").unwrap();
        let other = Vault::from_key(&[9u8; KEY_LEN]).unwrap();
        assert!(other.unseal(&nonce, &ct).is_err());
    }

    #[test]
    fn rejects_bad_key_length() {
        assert!(Vault::from_key(&[0u8; 16]).is_err());
    }

    #[test]
    fn dev_key_file_is_created_once_and_reused() {
        let mut path = std::env::temp_dir();
        path.push(format!("remotier-dev-key-{}.key", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let first = Vault::open_dev_file(&path).unwrap();
        let (nonce, ct) = first.seal_str("secret").unwrap();

        // A second open must reuse the same key, otherwise every restart would orphan
        // every stored secret.
        let second = Vault::open_dev_file(&path).unwrap();
        assert_eq!(*second.unseal_str(&nonce, &ct).unwrap(), "secret");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "dev key file must not be readable by others");
        }

        std::fs::remove_file(&path).ok();
    }
}
