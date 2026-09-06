//! Resolution: group inheritance, placeholder substitution, and credential loading.
//!
//! These run against a temporary database and need no network.

use remotier_lib::commands::secrets;
use remotier_lib::crypto::vault::Vault;
use remotier_lib::db::{new_id, now_ms, Db};
use remotier_lib::error::Error;
use remotier_lib::ssh::resolve::{self, AuthMaterial};

struct Fixture {
    db: Db,
    vault: Vault,
    dir: std::path::PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "remotier-resolve-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self {
            db: Db::open(&dir.join("test.db")).unwrap(),
            vault: Vault::from_key(&[5u8; 32]).unwrap(),
            dir,
        }
    }

    fn group(&self, name: &str, parent: Option<&str>, port: Option<i64>, identity: Option<&str>) -> String {
        let id = new_id();
        self.db
            .write(|tx| {
                tx.execute(
                    "INSERT INTO groups (id, parent_id, name, sort, default_port,
                                         default_identity_id, created_at, updated_at)
                     VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6, ?6)",
                    rusqlite::params![id, parent, name, port, identity, now_ms()],
                )?;
                Ok(())
            })
            .unwrap();
        id
    }

    fn identity(&self, username: &str, password: Option<&str>) -> String {
        let id = new_id();
        self.db
            .write(|tx| {
                let password_ref = match password {
                    Some(password) => Some(secrets::put(tx, &self.vault, None, password)?),
                    None => None,
                };
                tx.execute(
                    "INSERT INTO identities (id, label, username, auth_kind, password_ref,
                                             created_at, updated_at)
                     VALUES (?1, 'test', ?2, ?3, ?4, ?5, ?5)",
                    rusqlite::params![
                        id,
                        username,
                        if password.is_some() { "password" } else { "agent" },
                        password_ref,
                        now_ms()
                    ],
                )?;
                Ok(())
            })
            .unwrap();
        id
    }

    fn host(&self, hostname: &str, group: Option<&str>, port: Option<i64>, identity: Option<&str>) -> String {
        let id = new_id();
        self.db
            .write(|tx| {
                tx.execute(
                    "INSERT INTO hosts (id, group_id, label, hostname, port, identity_id,
                                        tags, sort, created_at, updated_at)
                     VALUES (?1, ?2, 'host', ?3, ?4, ?5, '[]', 0, ?6, ?6)",
                    rusqlite::params![id, group, hostname, port, identity, now_ms()],
                )?;
                Ok(())
            })
            .unwrap();
        id
    }

    fn set_var(&self, scope: &str, scope_id: &str, name: &str, value: &str) {
        self.db
            .write(|tx| {
                tx.execute(
                    "INSERT INTO var_values (scope, scope_id, name, value, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![scope, scope_id, name, value, now_ms()],
                )?;
                Ok(())
            })
            .unwrap();
    }

    fn declare_var(&self, scope: &str, scope_id: &str, name: &str, default: Option<&str>, required: bool) {
        self.db
            .write(|tx| {
                tx.execute(
                    "INSERT INTO var_defs (id, scope, scope_id, name, default_value, required,
                                           created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                    rusqlite::params![new_id(), scope, scope_id, name, default, i64::from(required), now_ms()],
                )?;
                Ok(())
            })
            .unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

#[test]
fn a_host_without_a_port_falls_back_to_22() {
    let f = Fixture::new("default-port");
    let host = f.host("plain.example.com", None, None, None);

    assert_eq!(resolve::preview(&f.db, &host).unwrap().port, 22);
}

#[test]
fn a_host_inherits_the_port_from_its_group() {
    let f = Fixture::new("group-port");
    let group = f.group("edge", None, Some(2222), None);
    let host = f.host("edge.example.com", Some(&group), None, None);

    assert_eq!(resolve::preview(&f.db, &host).unwrap().port, 2222);
}

#[test]
fn a_port_set_on_the_host_beats_the_group() {
    let f = Fixture::new("host-port");
    let group = f.group("edge", None, Some(2222), None);
    let host = f.host("edge.example.com", Some(&group), Some(2022), None);

    assert_eq!(resolve::preview(&f.db, &host).unwrap().port, 2022);
}

#[test]
fn inheritance_walks_up_to_the_nearest_group_that_sets_a_value() {
    let f = Fixture::new("nested-port");
    let root = f.group("root", None, Some(2222), None);
    let middle = f.group("middle", Some(&root), None, None);
    let host = f.host("deep.example.com", Some(&middle), None, None);

    // `middle` sets nothing, so the value comes from `root` rather than the 22 default.
    assert_eq!(resolve::preview(&f.db, &host).unwrap().port, 2222);
}

#[test]
fn a_host_inherits_the_identity_from_its_group() {
    let f = Fixture::new("group-identity");
    let identity = f.identity("deploy", Some("hunter2"));
    let group = f.group("edge", None, None, Some(&identity));
    let host = f.host("edge.example.com", Some(&group), None, None);

    assert_eq!(resolve::preview(&f.db, &host).unwrap().username, "deploy");
}

#[test]
fn placeholders_in_the_username_come_from_local_values() {
    let f = Fixture::new("warpgate");
    let identity = f.identity("{{wg_user}}:{{target}}", Some("hunter2"));
    let group = f.group("warpgate", None, Some(2222), Some(&identity));
    let host = f.host("bastion.example.com", Some(&group), None, None);

    // The shared group declares the variables; the values are this machine's own.
    f.declare_var("group", &group, "wg_user", None, true);
    f.declare_var("group", &group, "target", None, true);
    f.set_var("group", &group, "wg_user", "flex");
    f.set_var("group", &group, "target", "web-01");

    let preview = resolve::preview(&f.db, &host).unwrap();

    assert_eq!(preview.username, "flex:web-01");
    assert!(preview.missing_variables.is_empty());
}

#[test]
fn placeholders_in_the_hostname_are_resolved_too() {
    let f = Fixture::new("hostname-var");
    let host = f.host("{{env}}.example.com", None, None, None);
    f.declare_var("host", &host, "env", Some("staging"), false);

    assert_eq!(resolve::preview(&f.db, &host).unwrap().hostname, "staging.example.com");
}

#[test]
fn a_value_on_the_host_overrides_the_group_default() {
    let f = Fixture::new("override");
    let group = f.group("g", None, None, None);
    let host = f.host("{{env}}.example.com", Some(&group), None, None);

    f.declare_var("group", &group, "env", Some("staging"), false);
    f.set_var("host", &host, "env", "production");

    assert_eq!(resolve::preview(&f.db, &host).unwrap().hostname, "production.example.com");
}

#[test]
fn preview_lists_missing_variables_instead_of_failing() {
    let f = Fixture::new("missing-preview");
    let host = f.host("{{env}}.example.com", None, None, None);
    f.declare_var("host", &host, "env", None, true);

    let preview = resolve::preview(&f.db, &host).unwrap();

    // The UI needs the list to ask for them, so a preview must not error here.
    assert_eq!(preview.missing_variables, vec!["env"]);
}

#[test]
fn connecting_is_refused_while_a_variable_is_unresolved() {
    let f = Fixture::new("missing-target");
    let host = f.host("{{env}}.example.com", None, None, None);
    f.declare_var("host", &host, "env", None, true);

    match resolve::target(&f.db, &f.vault, &host, None) {
        Err(Error::UnresolvedVariables(names)) => assert_eq!(names, vec!["env"]),
        other => panic!("expected UnresolvedVariables, got {other:?}", other = other.map(|_| "a target")),
    }
}

#[test]
fn the_stored_password_is_decrypted_for_the_connection() {
    let f = Fixture::new("password");
    let identity = f.identity("deploy", Some("hunter2"));
    let host = f.host("example.com", None, None, Some(&identity));

    let target = resolve::target(&f.db, &f.vault, &host, None).unwrap();

    assert_eq!(target.username, "deploy");
    match target.auth {
        AuthMaterial::Password(Some(password)) => assert_eq!(*password, "hunter2"),
        _ => panic!("expected password auth"),
    }
}

#[test]
fn a_host_with_no_identity_falls_back_to_the_agent() {
    let f = Fixture::new("no-identity");
    let host = f.host("example.com", None, None, None);

    let target = resolve::target(&f.db, &f.vault, &host, None).unwrap();

    assert!(matches!(target.auth, AuthMaterial::Agent { .. }));
}

#[test]
fn a_cycle_in_the_group_chain_does_not_hang() {
    let f = Fixture::new("cycle");
    let a = f.group("a", None, None, None);
    let b = f.group("b", Some(&a), Some(2222), None);

    // Force a cycle the UI would normally prevent.
    f.db.write(|tx| {
        tx.execute(
            "UPDATE groups SET parent_id = ?2 WHERE id = ?1",
            rusqlite::params![a, b],
        )?;
        Ok(())
    })
    .unwrap();

    let host = f.host("example.com", Some(&a), None, None);

    assert_eq!(resolve::preview(&f.db, &host).unwrap().port, 2222);
}

// --- Credentials set directly on a host, instead of an identity ---

impl Fixture {
    fn host_with_credentials(
        &self,
        hostname: &str,
        username: &str,
        auth_kind: &str,
        password: Option<&str>,
    ) -> String {
        let id = new_id();
        self.db
            .write(|tx| {
                let password_ref = match password {
                    Some(password) => Some(secrets::put(tx, &self.vault, None, password)?),
                    None => None,
                };
                tx.execute(
                    "INSERT INTO hosts (id, label, hostname, tags, sort, username, auth_kind,
                                        password_ref, created_at, updated_at)
                     VALUES (?1, 'host', ?2, '[]', 0, ?3, ?4, ?5, ?6, ?6)",
                    rusqlite::params![id, hostname, username, auth_kind, password_ref, now_ms()],
                )?;
                Ok(())
            })
            .unwrap();
        id
    }
}

#[test]
fn a_host_can_carry_its_own_username_and_password() {
    let f = Fixture::new("host-creds");
    let host = f.host_with_credentials("example.com", "root", "password", Some("hunter2"));

    let target = resolve::target(&f.db, &f.vault, &host, None).unwrap();

    assert_eq!(target.username, "root");
    match target.auth {
        AuthMaterial::Password(Some(password)) => assert_eq!(*password, "hunter2"),
        _ => panic!("expected password auth from the host's own credentials"),
    }
}

#[test]
fn host_credentials_beat_an_inherited_identity() {
    let f = Fixture::new("host-beats-identity");
    let identity = f.identity("from-identity", Some("identity-password"));
    let group = f.group("g", None, None, Some(&identity));

    let host = f.host_with_credentials("example.com", "from-host", "password", Some("host-password"));
    f.db.write(|tx| {
        tx.execute(
            "UPDATE hosts SET group_id = ?2 WHERE id = ?1",
            rusqlite::params![host, group],
        )?;
        Ok(())
    })
    .unwrap();

    let target = resolve::target(&f.db, &f.vault, &host, None).unwrap();

    // The host is explicit; the inherited identity must not override it.
    assert_eq!(target.username, "from-host");
    match target.auth {
        AuthMaterial::Password(Some(password)) => assert_eq!(*password, "host-password"),
        _ => panic!("expected the host's own password"),
    }
}

#[test]
fn a_host_username_without_its_own_auth_still_overrides_the_identity_username() {
    let f = Fixture::new("username-only");
    let identity = f.identity("identity-user", Some("pw"));
    let host = f.host("example.com", None, None, Some(&identity));

    f.db.write(|tx| {
        tx.execute(
            "UPDATE hosts SET username = 'override' WHERE id = ?1",
            rusqlite::params![host],
        )?;
        Ok(())
    })
    .unwrap();

    let target = resolve::target(&f.db, &f.vault, &host, None).unwrap();

    assert_eq!(target.username, "override");
    // Authentication still comes from the identity, only the username was overridden.
    assert!(matches!(target.auth, AuthMaterial::Password(_)));
}

#[test]
fn a_host_set_to_agent_auth_needs_no_identity() {
    let f = Fixture::new("host-agent");
    let host = f.host_with_credentials("example.com", "deploy", "agent", None);

    let target = resolve::target(&f.db, &f.vault, &host, None).unwrap();

    assert_eq!(target.username, "deploy");
    assert!(matches!(target.auth, AuthMaterial::Agent { .. }));
}

#[test]
fn placeholders_work_in_a_host_username() {
    let f = Fixture::new("host-placeholder");
    let host = f.host_with_credentials("example.com", "{{wg_user}}", "agent", None);
    f.declare_var("host", &host, "wg_user", None, true);
    f.set_var("host", &host, "wg_user", "flex");

    assert_eq!(resolve::preview(&f.db, &host).unwrap().username, "flex");
}

#[test]
fn preview_reports_host_credentials_rather_than_an_identity() {
    let f = Fixture::new("preview-host-creds");
    let identity = f.identity("unused", Some("pw"));
    let group = f.group("g", None, None, Some(&identity));
    let host = f.host_with_credentials("example.com", "root", "password", Some("hunter2"));

    f.db.write(|tx| {
        tx.execute(
            "UPDATE hosts SET group_id = ?2 WHERE id = ?1",
            rusqlite::params![host, group],
        )?;
        Ok(())
    })
    .unwrap();

    let preview = resolve::preview(&f.db, &host).unwrap();

    assert_eq!(preview.auth_kind, remotier_lib::db::models::AuthKind::Password);
    // The UI should not claim an identity is in use when it is being bypassed.
    assert_eq!(preview.identity_label, None);
}

#[test]
fn a_host_with_no_stored_password_still_resolves() {
    let f = Fixture::new("ask-host");
    let host = f.host_with_credentials("example.com", "root", "password", None);

    // Resolution must not refuse here: the connection is attempted first, and only a
    // refusal from the server prompts the user.
    let target = resolve::target(&f.db, &f.vault, &host, None).unwrap();

    assert_eq!(target.username, "root");
    assert!(matches!(target.auth, AuthMaterial::Password(None)));
}

#[test]
fn a_supplied_password_satisfies_a_host_that_stores_none() {
    let f = Fixture::new("supplied-host");
    let host = f.host_with_credentials("example.com", "root", "password", None);

    let target = resolve::target(&f.db, &f.vault, &host, Some("typed-in")).unwrap();

    match target.auth {
        AuthMaterial::Password(Some(password)) => assert_eq!(*password, "typed-in"),
        _ => panic!("expected password auth"),
    }
}

#[test]
fn an_identity_with_no_stored_password_still_resolves() {
    let f = Fixture::new("ask-identity");
    let identity = f.identity("deploy", None);
    // `identity()` only stores a password when given one; force password auth without it.
    f.db.write(|tx| {
        tx.execute(
            "UPDATE identities SET auth_kind = 'password' WHERE id = ?1",
            rusqlite::params![identity],
        )?;
        Ok(())
    })
    .unwrap();
    let host = f.host("example.com", None, None, Some(&identity));

    let target = resolve::target(&f.db, &f.vault, &host, None).unwrap();

    assert_eq!(target.username, "deploy");
    assert!(matches!(target.auth, AuthMaterial::Password(None)));
}

#[test]
fn a_supplied_password_satisfies_an_identity_that_stores_none() {
    let f = Fixture::new("supplied-identity");
    let identity = f.identity("deploy", None);
    f.db.write(|tx| {
        tx.execute(
            "UPDATE identities SET auth_kind = 'password' WHERE id = ?1",
            rusqlite::params![identity],
        )?;
        Ok(())
    })
    .unwrap();
    let host = f.host("example.com", None, None, Some(&identity));

    let target = resolve::target(&f.db, &f.vault, &host, Some("typed-in")).unwrap();

    match target.auth {
        AuthMaterial::Password(Some(password)) => assert_eq!(*password, "typed-in"),
        _ => panic!("expected password auth"),
    }
}

#[test]
fn a_stored_password_is_preferred_over_a_supplied_one() {
    let f = Fixture::new("stored-wins");
    let host = f.host_with_credentials("example.com", "root", "password", Some("stored"));

    let target = resolve::target(&f.db, &f.vault, &host, Some("typed-in")).unwrap();

    match target.auth {
        AuthMaterial::Password(Some(password)) => assert_eq!(*password, "stored"),
        _ => panic!("expected password auth"),
    }
}

#[test]
fn keyboard_interactive_never_demands_a_stored_password() {
    let f = Fixture::new("interactive-no-password");
    let host = f.host_with_credentials("example.com", "root", "interactive", None);

    // The server does the asking here, so an empty secret is a valid starting point.
    let target = resolve::target(&f.db, &f.vault, &host, None).unwrap();
    assert!(matches!(target.auth, AuthMaterial::Interactive(None)));
}
