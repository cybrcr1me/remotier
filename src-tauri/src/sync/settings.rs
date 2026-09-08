//! Which settings sync, and which are this machine's business alone.
//!
//! An allow-list rather than a deny-list. A setting added later and not thought about
//! stays local, which is the safe direction to be wrong in: a preference that should have
//! synced is an annoyance, one that should not have is a laptop overwriting a desktop's
//! agent socket with a path that does not exist there.

/// Settings that travel with the account.
pub const SYNCABLE: &[&str] = &[
    "terminal.fontSize",
    "terminal.term",
    "ssh.defaultPort",
    "appearance.theme",
];

pub fn is_syncable(key: &str) -> bool {
    SYNCABLE.contains(&key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agent_socket_never_syncs() {
        // macOS hands GUI apps launchd's agent and a Homebrew path is machine-specific;
        // syncing this would point one machine at a socket that does not exist on it.
        assert!(!is_syncable("ssh.agentSocket"));
    }

    #[test]
    fn sync_own_settings_never_sync() {
        assert!(!is_syncable("sync.instanceUrl"));
        assert!(!is_syncable("sync.autoSync"));
    }

    #[test]
    fn an_unknown_setting_stays_local() {
        assert!(!is_syncable("something.addedLater"));
    }

    #[test]
    fn the_ordinary_preferences_do_sync() {
        assert!(is_syncable("terminal.fontSize"));
        assert!(is_syncable("ssh.defaultPort"));
    }
}
