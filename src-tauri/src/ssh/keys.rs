//! Key generation, import, and discovery of keys that already exist on disk.
//!
//! Discovery is deliberately read-only: Remotier records the *path* of a `~/.ssh` key and
//! reads it at connect time. It never copies a private key it did not generate, which is
//! what keeps the key repository optional rather than mandatory.

use std::path::{Path, PathBuf};

use russh::keys::ssh_key::{Algorithm, HashAlg, LineEnding, PrivateKey, PublicKey};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::error::{Error, Result};

/// Algorithms offered when generating a key. RSA is 4096-bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyAlgorithm {
    Ed25519,
    Rsa,
}

impl KeyAlgorithm {
    fn to_ssh(self) -> Algorithm {
        match self {
            KeyAlgorithm::Ed25519 => Algorithm::Ed25519,
            KeyAlgorithm::Rsa => Algorithm::Rsa { hash: None },
        }
    }
}

pub struct GeneratedKey {
    pub private_openssh: Zeroizing<String>,
    pub public_openssh: String,
    pub fingerprint: String,
    pub algorithm: String,
}

/// Generate a new key pair. The private half is returned unencrypted - the vault, not a
/// passphrase, is what protects it at rest.
pub fn generate(algorithm: KeyAlgorithm, comment: &str) -> Result<GeneratedKey> {
    let mut key = PrivateKey::random(&mut rand::rng(), algorithm.to_ssh())
        .map_err(|e| Error::Ssh(format!("could not generate key: {e}")))?;
    if !comment.is_empty() {
        key.set_comment(comment);
    }

    let private_openssh = key
        .to_openssh(LineEnding::LF)
        .map_err(|e| Error::Ssh(format!("could not encode private key: {e}")))?;
    let public = key.public_key();

    Ok(GeneratedKey {
        private_openssh,
        public_openssh: public
            .to_openssh()
            .map_err(|e| Error::Ssh(format!("could not encode public key: {e}")))?,
        fingerprint: public.fingerprint(HashAlg::Sha256).to_string(),
        algorithm: public.algorithm().to_string(),
    })
}

/// Parse an OpenSSH private key, decrypting it if a passphrase is supplied.
///
/// # Errors
///
/// Returns [`Error::Invalid`] when the key is encrypted and no passphrase was given, so
/// the caller can prompt for one rather than reporting a parse failure.
pub fn parse_private(pem: &str, passphrase: Option<&str>) -> Result<PrivateKey> {
    let key = PrivateKey::from_openssh(pem)
        .map_err(|e| Error::Invalid(format!("not a valid OpenSSH private key: {e}")))?;

    if !key.is_encrypted() {
        return Ok(key);
    }

    let Some(passphrase) = passphrase.filter(|p| !p.is_empty()) else {
        return Err(Error::Invalid("this key is encrypted and needs a passphrase".into()));
    };

    key.decrypt(passphrase)
        .map_err(|_| Error::Invalid("wrong passphrase for this key".into()))
}

pub fn is_encrypted(pem: &str) -> bool {
    PrivateKey::from_openssh(pem).is_ok_and(|key| key.is_encrypted())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicKeyInfo {
    pub algorithm: String,
    pub fingerprint: String,
    pub comment: String,
    pub openssh: String,
}

pub fn describe_public(line: &str) -> Result<PublicKeyInfo> {
    let key = PublicKey::from_openssh(line.trim())
        .map_err(|e| Error::Invalid(format!("not a valid OpenSSH public key: {e}")))?;
    Ok(info_from_public(&key))
}

fn info_from_public(key: &PublicKey) -> PublicKeyInfo {
    PublicKeyInfo {
        algorithm: key.algorithm().to_string(),
        fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
        comment: key.comment().to_string(),
        openssh: key.to_openssh().unwrap_or_default(),
    }
}

/// A key pair found in a directory such as `~/.ssh`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredKey {
    pub path: PathBuf,
    pub public_path: PathBuf,
    #[serde(flatten)]
    pub info: PublicKeyInfo,
    /// False when only the `.pub` half is present, e.g. the private key lives on a token.
    pub private_key_present: bool,
    /// True when the private key is passphrase-protected, so the UI can say so upfront.
    pub encrypted: bool,
}

pub fn default_ssh_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ssh"))
}

/// List usable key pairs in `dir` by reading only the `.pub` halves.
///
/// Private keys are opened solely to test whether they are encrypted, and the contents
/// are dropped immediately; nothing is copied into the app.
pub fn scan(dir: &Path) -> Result<Vec<DiscoveredKey>> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        // No ~/.ssh at all is normal, not a failure.
        return Ok(Vec::new());
    };

    let mut found = Vec::new();
    for entry in entries.flatten() {
        let public_path = entry.path();
        if public_path.extension().is_none_or(|ext| ext != "pub") {
            continue;
        }

        let Ok(contents) = std::fs::read_to_string(&public_path) else {
            continue;
        };
        let Ok(key) = PublicKey::from_openssh(contents.trim()) else {
            continue;
        };

        let path = public_path.with_extension("");
        let private_key_present = path.is_file();
        let encrypted = private_key_present
            && std::fs::read_to_string(&path).is_ok_and(|pem| is_encrypted(&pem));

        found.push(DiscoveredKey {
            path,
            public_path,
            info: info_from_public(&key),
            private_key_present,
            encrypted,
        });
    }

    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_an_ed25519_key_that_round_trips() {
        let generated = generate(KeyAlgorithm::Ed25519, "test@remotier").unwrap();

        assert!(generated.public_openssh.starts_with("ssh-ed25519 "));
        assert!(generated.fingerprint.starts_with("SHA256:"));
        assert_eq!(generated.algorithm, "ssh-ed25519");

        let parsed = parse_private(&generated.private_openssh, None).unwrap();
        assert_eq!(
            parsed.public_key().fingerprint(HashAlg::Sha256).to_string(),
            generated.fingerprint
        );
    }

    #[test]
    fn generated_keys_are_not_encrypted() {
        let generated = generate(KeyAlgorithm::Ed25519, "").unwrap();
        assert!(!is_encrypted(&generated.private_openssh));
    }

    #[test]
    fn generated_keys_are_unique() {
        let first = generate(KeyAlgorithm::Ed25519, "").unwrap();
        let second = generate(KeyAlgorithm::Ed25519, "").unwrap();
        assert_ne!(first.fingerprint, second.fingerprint);
    }

    #[test]
    fn carries_the_comment_into_the_public_key() {
        let generated = generate(KeyAlgorithm::Ed25519, "flex@laptop").unwrap();
        assert_eq!(describe_public(&generated.public_openssh).unwrap().comment, "flex@laptop");
    }

    #[test]
    fn rejects_junk_private_keys() {
        assert!(parse_private("not a key", None).is_err());
    }

    #[test]
    fn rejects_junk_public_keys() {
        assert!(describe_public("ssh-ed25519 not-base64").is_err());
    }

    #[test]
    fn scanning_a_missing_directory_is_not_an_error() {
        let missing = std::env::temp_dir().join("remotier-no-such-dir-xyz");
        assert!(scan(&missing).unwrap().is_empty());
    }

    #[test]
    fn scan_finds_pub_files_and_notes_the_missing_private_half() {
        let dir = std::env::temp_dir().join(format!("remotier-scan-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let generated = generate(KeyAlgorithm::Ed25519, "scan@test").unwrap();
        std::fs::write(dir.join("id_ed25519.pub"), &generated.public_openssh).unwrap();
        // Files that are not keys must be skipped rather than blowing up the scan.
        std::fs::write(dir.join("config"), "Host example\n").unwrap();
        std::fs::write(dir.join("broken.pub"), "ssh-ed25519 AAAAnonsense").unwrap();

        let found = scan(&dir).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].info.fingerprint, generated.fingerprint);
        assert!(!found[0].private_key_present);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn scan_reports_an_unencrypted_private_half_as_present() {
        let dir = std::env::temp_dir().join(format!("remotier-scan2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let generated = generate(KeyAlgorithm::Ed25519, "").unwrap();
        std::fs::write(dir.join("id_ed25519.pub"), &generated.public_openssh).unwrap();
        std::fs::write(dir.join("id_ed25519"), generated.private_openssh.as_str()).unwrap();

        let found = scan(&dir).unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0].private_key_present);
        assert!(!found[0].encrypted);

        std::fs::remove_dir_all(&dir).ok();
    }
}
