//! A reader for `~/.ssh/config`, used by the one-shot importer.
//!
//! This is deliberately not a full OpenSSH config implementation - it extracts the
//! handful of directives that map onto a Remotier host and ignores the rest. Nothing
//! here changes the file; it is read-only.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::Result;

/// One `Host` block, reduced to the fields Remotier stores.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigHost {
    /// The alias as written in the config, used as the host label.
    pub alias: String,
    pub hostname: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
    pub proxy_jump: Option<String>,
}

pub fn default_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ssh").join("config"))
}

/// Split a config line into its keyword and value.
///
/// OpenSSH accepts both `Key value` and `Key=value`, and keywords are case insensitive.
fn split_directive(line: &str) -> Option<(String, &str)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    let index = line.find(['=', ' ', '\t'])?;
    let keyword = &line[..index];
    let value = line[index + 1..].trim_start_matches(['=', ' ', '\t']);

    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    Some((keyword.to_ascii_lowercase(), value))
}

/// Parse config text into importable hosts.
///
/// Blocks whose alias contains a wildcard (`Host *`) are skipped: they are defaults
/// applied to other hosts, not hosts themselves. `Match` blocks are skipped for the same
/// reason. `Include` is not followed.
pub fn parse(contents: &str) -> Vec<ConfigHost> {
    let mut hosts = Vec::new();
    let mut current: Option<ConfigHost> = None;
    let mut skipping = false;

    for line in contents.lines() {
        let Some((keyword, value)) = split_directive(line) else {
            continue;
        };

        match keyword.as_str() {
            "host" => {
                if let Some(host) = current.take() {
                    hosts.push(host);
                }
                // `Host a b` declares several aliases; the first is the useful label.
                let alias = value.split_whitespace().next().unwrap_or_default().to_string();
                skipping = alias.contains('*') || alias.contains('?') || alias.is_empty();
                current = if skipping {
                    None
                } else {
                    Some(ConfigHost {
                        hostname: alias.clone(),
                        alias,
                        user: None,
                        port: None,
                        identity_file: None,
                        proxy_jump: None,
                    })
                };
            }
            "match" => {
                if let Some(host) = current.take() {
                    hosts.push(host);
                }
                skipping = true;
            }
            _ => {
                if skipping {
                    continue;
                }
                let Some(host) = current.as_mut() else { continue };
                match keyword.as_str() {
                    "hostname" => host.hostname = value.to_string(),
                    "user" => host.user = Some(value.to_string()),
                    "port" => host.port = value.parse().ok(),
                    // Only the first IdentityFile is kept; OpenSSH tries each in turn.
                    "identityfile" => {
                        if host.identity_file.is_none() {
                            host.identity_file = Some(expand_tilde(value));
                        }
                    }
                    "proxyjump" => host.proxy_jump = Some(value.to_string()),
                    _ => {}
                }
            }
        }
    }

    if let Some(host) = current {
        hosts.push(host);
    }
    hosts
}

fn expand_tilde(path: &str) -> String {
    let Some(rest) = path.strip_prefix("~/") else {
        return path.to_string();
    };
    match dirs::home_dir() {
        Some(home) => home.join(rest).to_string_lossy().into_owned(),
        None => path.to_string(),
    }
}

/// Read and parse the config at `path`. A missing file yields an empty list.
pub fn read(path: &Path) -> Result<Vec<ConfigHost>> {
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(parse(&contents)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_simple_block() {
        let hosts = parse(
            "Host web\n  HostName web.example.com\n  User deploy\n  Port 2222\n",
        );

        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].alias, "web");
        assert_eq!(hosts[0].hostname, "web.example.com");
        assert_eq!(hosts[0].user.as_deref(), Some("deploy"));
        assert_eq!(hosts[0].port, Some(2222));
    }

    #[test]
    fn falls_back_to_the_alias_when_no_hostname_is_given() {
        let hosts = parse("Host bare.example.com\n  User deploy\n");
        assert_eq!(hosts[0].hostname, "bare.example.com");
    }

    #[test]
    fn accepts_equals_separated_directives() {
        let hosts = parse("Host=web\nHostName=web.example.com\nPort=2022\n");

        assert_eq!(hosts[0].hostname, "web.example.com");
        assert_eq!(hosts[0].port, Some(2022));
    }

    #[test]
    fn keywords_are_case_insensitive() {
        let hosts = parse("HOST web\n  hostname web.example.com\n  USER deploy\n");

        assert_eq!(hosts[0].hostname, "web.example.com");
        assert_eq!(hosts[0].user.as_deref(), Some("deploy"));
    }

    #[test]
    fn skips_comments_and_blank_lines() {
        let hosts = parse("# a comment\n\nHost web\n\n  # another\n  HostName web.example.com\n");

        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].hostname, "web.example.com");
    }

    #[test]
    fn parses_several_blocks() {
        let hosts = parse(
            "Host a\n  HostName a.example.com\nHost b\n  HostName b.example.com\n  Port 2222\n",
        );

        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[0].hostname, "a.example.com");
        assert_eq!(hosts[1].port, Some(2222));
    }

    #[test]
    fn skips_wildcard_blocks() {
        // `Host *` sets defaults for other hosts; importing it as a host is meaningless.
        let hosts = parse(
            "Host *\n  User default\n  ServerAliveInterval 60\nHost web\n  HostName web.example.com\n",
        );

        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].alias, "web");
        assert_eq!(hosts[0].user, None, "wildcard defaults must not leak into a real host");
    }

    #[test]
    fn skips_match_blocks() {
        let hosts = parse(
            "Host web\n  HostName web.example.com\nMatch host *.internal\n  User internal\n",
        );

        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].alias, "web");
    }

    #[test]
    fn takes_the_first_alias_when_several_are_listed() {
        let hosts = parse("Host web web1 web-primary\n  HostName web.example.com\n");

        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].alias, "web");
    }

    #[test]
    fn keeps_only_the_first_identity_file() {
        let hosts = parse(
            "Host web\n  HostName web.example.com\n  IdentityFile /keys/first\n  IdentityFile /keys/second\n",
        );

        assert_eq!(hosts[0].identity_file.as_deref(), Some("/keys/first"));
    }

    #[test]
    fn records_proxy_jump() {
        let hosts = parse("Host web\n  HostName web.example.com\n  ProxyJump bastion\n");
        assert_eq!(hosts[0].proxy_jump.as_deref(), Some("bastion"));
    }

    #[test]
    fn ignores_an_unparseable_port_rather_than_failing() {
        let hosts = parse("Host web\n  HostName web.example.com\n  Port not-a-number\n");

        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].port, None);
    }

    #[test]
    fn ignores_directives_it_does_not_understand() {
        let hosts = parse(
            "Host web\n  HostName web.example.com\n  ServerAliveInterval 60\n  Compression yes\n",
        );

        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].hostname, "web.example.com");
    }

    #[test]
    fn handles_an_empty_config() {
        assert!(parse("").is_empty());
    }

    #[test]
    fn reading_a_missing_file_is_not_an_error() {
        let missing = std::env::temp_dir().join("remotier-no-such-config");
        assert!(read(&missing).unwrap().is_empty());
    }
}
