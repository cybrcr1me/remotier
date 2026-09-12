//! Host key trust, backed by the user's real `~/.ssh/known_hosts`.
//!
//! Sharing OpenSSH's file is deliberate: a host you already trust in your terminal is
//! trusted here too, and a host you trust here is trusted by `ssh` afterwards.

use std::path::{Path, PathBuf};

use russh::keys::ssh_key::{HashAlg, PublicKey};
use serde::Serialize;

use crate::error::Result;

/// What `~/.ssh/known_hosts` says about a key the server just presented.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    /// Recorded, and it matches.
    Known,
    /// Never seen. The user has to decide.
    Unknown,
    /// Recorded, and it does NOT match. Treated as hostile, never auto-accepted.
    Changed { line: usize },
}

/// Where host keys are read from and written to.
///
/// `REMOTIER_KNOWN_HOSTS` overrides the location. Tests set it so they never read or
/// write the developer's real file, and it gives users with a non-standard layout a way
/// to point at theirs.
pub fn default_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("REMOTIER_KNOWN_HOSTS") {
        return Some(PathBuf::from(path));
    }
    dirs::home_dir().map(|home| home.join(".ssh").join("known_hosts"))
}

pub fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

/// Check `key` against the known_hosts file at `path`.
///
/// A missing file is not an error - it means nothing is known yet, so every host is
/// [`Verdict::Unknown`].
pub fn verify(host: &str, port: u16, key: &PublicKey, path: &Path) -> Result<Verdict> {
    if !path.exists() {
        return Ok(Verdict::Unknown);
    }

    match russh::keys::check_known_hosts_path(host, port, key, path) {
        Ok(true) => Ok(Verdict::Known),
        Ok(false) => Ok(Verdict::Unknown),
        Err(russh::keys::Error::KeyChanged { line }) => Ok(Verdict::Changed { line }),
        Err(e) => Err(e.into()),
    }
}

/// Append a host key, so the next connection verifies without prompting.
pub fn learn(host: &str, port: u16, key: &PublicKey, path: &Path) -> Result<()> {
    russh::keys::known_hosts::learn_known_hosts_path(host, port, key, path)?;
    Ok(())
}

/// One entry in `known_hosts`, for the management screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// 1-based, matching how the file is usually referred to.
    pub line: usize,
    /// The host pattern as written, or `None` when the entry is hashed.
    pub host: Option<String>,
    pub algorithm: String,
    pub fingerprint: String,
    /// True for `|1|...` hashed entries, whose hostnames cannot be recovered.
    pub hashed: bool,
}

/// List every entry in the file. Unparseable lines are skipped rather than failing the
/// whole listing - a known_hosts file may contain entries from tools we do not model.
pub fn list(path: &Path) -> Result<Vec<Entry>> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Ok(Vec::new());
    };

    let mut entries = Vec::new();
    for (index, line) in contents.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let mut fields = trimmed.split_whitespace();
        let Some(host_field) = fields.next() else { continue };
        // `@revoked` / `@cert-authority` markers push the fields along by one.
        let (host_field, algorithm) = if host_field.starts_with('@') {
            match (fields.next(), fields.next()) {
                (Some(host), Some(algorithm)) => (host, algorithm),
                _ => continue,
            }
        } else {
            match fields.next() {
                Some(algorithm) => (host_field, algorithm),
                None => continue,
            }
        };
        let Some(encoded) = fields.next() else { continue };

        let Ok(key) = PublicKey::from_openssh(&format!("{algorithm} {encoded}")) else {
            continue;
        };

        let hashed = host_field.starts_with("|1|");
        entries.push(Entry {
            line: index + 1,
            host: if hashed { None } else { Some(host_field.to_string()) },
            algorithm: key.algorithm().to_string(),
            fingerprint: fingerprint(&key),
            hashed,
        });
    }

    Ok(entries)
}

/// Delete the entry at `line` (1-based).
///
/// Every other line is preserved byte for byte, including comments and entries this
/// module cannot parse.
///
/// # Errors
///
/// [`Error::NotFound`] when the line does not exist, so a stale UI cannot silently delete
/// the wrong host.
pub fn revoke(path: &Path, line: usize) -> Result<()> {
    let contents = std::fs::read_to_string(path)?;
    let lines: Vec<&str> = contents.lines().collect();

    if line == 0 || line > lines.len() {
        return Err(crate::error::Error::NotFound(
            "known_hosts entry",
            line.to_string(),
        ));
    }

    let mut kept: Vec<&str> = Vec::with_capacity(lines.len() - 1);
    kept.extend_from_slice(&lines[..line - 1]);
    kept.extend_from_slice(&lines[line..]);

    let mut out = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    std::fs::write(path, out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> PublicKey {
        let private =
            russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519)
                .unwrap();
        private.public_key().clone()
    }

    fn temp_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("remotier-kh-{name}-{}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn missing_file_means_unknown() {
        let path = temp_path("missing");
        assert_eq!(verify("example.com", 22, &test_key(), &path).unwrap(), Verdict::Unknown);
    }

    #[test]
    fn learned_key_verifies() {
        let path = temp_path("learn");
        let key = test_key();

        assert_eq!(verify("example.com", 22, &key, &path).unwrap(), Verdict::Unknown);
        learn("example.com", 22, &key, &path).unwrap();
        assert_eq!(verify("example.com", 22, &key, &path).unwrap(), Verdict::Known);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_different_key_for_a_known_host_is_reported_as_changed() {
        let path = temp_path("changed");
        learn("example.com", 22, &test_key(), &path).unwrap();

        // Same host, different key: this is the MITM signature and must never come back
        // as merely Unknown.
        let verdict = verify("example.com", 22, &test_key(), &path).unwrap();
        assert!(matches!(verdict, Verdict::Changed { .. }), "got {verdict:?}");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_key_learned_for_one_port_does_not_authorise_another() {
        let path = temp_path("port");
        let key = test_key();
        learn("example.com", 2222, &key, &path).unwrap();

        assert_eq!(verify("example.com", 2222, &key, &path).unwrap(), Verdict::Known);
        assert_eq!(verify("example.com", 22, &key, &path).unwrap(), Verdict::Unknown);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn lists_learned_entries() {
        let path = temp_path("list");
        learn("a.example.com", 22, &test_key(), &path).unwrap();
        learn("b.example.com", 2222, &test_key(), &path).unwrap();

        let entries = list(&path).unwrap();

        assert_eq!(entries.len(), 2);
        // Line numbers are real file lines, not entry indices: russh writes a leading
        // newline, so the first entry does not sit on line 1.
        assert!(entries[0].line < entries[1].line);
        assert!(entries.iter().all(|e| e.fingerprint.starts_with("SHA256:")));
        assert!(entries.iter().all(|e| e.algorithm == "ssh-ed25519"));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn listing_a_missing_file_is_empty() {
        assert!(list(&temp_path("list-missing")).unwrap().is_empty());
    }

    #[test]
    fn skips_comments_and_junk_lines() {
        let path = temp_path("junk");
        let key = test_key();
        learn("a.example.com", 22, &key, &path).unwrap();

        let mut contents = std::fs::read_to_string(&path).unwrap();
        contents.push_str("# a comment\n\nnot-a-valid-entry\n");
        std::fs::write(&path, contents).unwrap();

        // One real entry survives; the noise is ignored rather than failing the listing.
        assert_eq!(list(&path).unwrap().len(), 1);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn revoking_removes_only_that_entry() {
        let path = temp_path("revoke");
        let keep = test_key();
        learn("a.example.com", 22, &test_key(), &path).unwrap();
        learn("b.example.com", 22, &keep, &path).unwrap();

        let first = list(&path).unwrap()[0].line;
        revoke(&path, first).unwrap();

        let entries = list(&path).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].fingerprint, fingerprint(&keep));
        // The host that was left alone must still verify.
        assert_eq!(verify("b.example.com", 22, &keep, &path).unwrap(), Verdict::Known);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn revoking_makes_the_host_unknown_again() {
        let path = temp_path("revoke-verify");
        let key = test_key();
        learn("a.example.com", 22, &key, &path).unwrap();

        let line = list(&path).unwrap()[0].line;
        revoke(&path, line).unwrap();

        assert_eq!(verify("a.example.com", 22, &key, &path).unwrap(), Verdict::Unknown);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn revoking_preserves_surrounding_comments() {
        let path = temp_path("revoke-comments");
        learn("a.example.com", 22, &test_key(), &path).unwrap();
        let entry = std::fs::read_to_string(&path).unwrap();
        let entry = entry.trim();
        std::fs::write(&path, format!("# header\n{entry}\n# footer\n")).unwrap();

        let line = list(&path).unwrap()[0].line;
        revoke(&path, line).unwrap();

        // Comments either side survive untouched; only the key line goes.
        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "# header\n# footer\n");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn revoking_a_line_that_does_not_exist_is_refused() {
        let path = temp_path("revoke-oob");
        learn("a.example.com", 22, &test_key(), &path).unwrap();

        assert!(revoke(&path, 999).is_err());
        assert!(revoke(&path, 0).is_err());
        // Nothing was removed by the failed attempts.
        assert_eq!(list(&path).unwrap().len(), 1);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn fingerprints_are_sha256_formatted() {
        assert!(fingerprint(&test_key()).starts_with("SHA256:"));
    }
}
