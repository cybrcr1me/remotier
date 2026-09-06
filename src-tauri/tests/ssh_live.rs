//! Live SSH tests against a real sshd.
//!
//! These are `#[ignore]`d so `cargo test` stays hermetic. Start the server first:
//!
//! ```text
//! ssh-keygen -t ed25519 -f /tmp/remotier-testkey -N ""
//! docker run -d --name remotier-test -p 2222:2222 \
//!   -e PASSWORD_ACCESS=true -e USER_NAME=test -e USER_PASSWORD=testpass \
//!   -e PUBLIC_KEY="$(cat /tmp/remotier-testkey.pub)" \
//!   lscr.io/linuxserver/openssh-server:latest
//! cargo test --test ssh_live -- --ignored --test-threads=1
//! ```
//!
//! `REMOTIER_TEST_KEY` points at the private key; it defaults to the path above.

use std::time::Duration;

use remotier_lib::error::Error;
use remotier_lib::ssh::client::HostKeyPolicy;
use remotier_lib::ssh::connect;
use remotier_lib::ssh::resolve::{AuthMaterial, Target};
use russh::ChannelMsg;
use zeroize::Zeroizing;

const HOST: &str = "127.0.0.1";
const PORT: u16 = 2222;
const USER: &str = "test";
const PASSWORD: &str = "testpass";

fn key_path() -> String {
    std::env::var("REMOTIER_TEST_KEY").unwrap_or_else(|_| "/tmp/remotier-testkey".to_string())
}

fn target(auth: AuthMaterial) -> Target {
    Target {
        host_id: "test".into(),
        label: "test".into(),
        hostname: HOST.into(),
        port: PORT,
        username: USER.into(),
        auth,
    }
}

/// Run a command in the shell and return what came back.
async fn shell_roundtrip(mut connection: connect::Connection) -> String {
    connection
        .channel
        .data(&b"echo remotier-ok\nexit\n"[..])
        .await
        .expect("write to shell");

    let mut output = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, connection.channel.wait()).await {
            Ok(Some(ChannelMsg::Data { data })) => output.extend_from_slice(&data),
            Ok(Some(ChannelMsg::Eof | ChannelMsg::Close)) | Ok(None) => break,
            Ok(Some(_)) => {}
            Err(_) => break,
        }
    }

    String::from_utf8_lossy(&output).into_owned()
}

#[tokio::test]
#[ignore = "needs the dockerised sshd"]
async fn connects_with_a_password_and_runs_a_command() {
    let target = target(AuthMaterial::Password(Zeroizing::new(PASSWORD.into())));
    let connection = connect::open(&target, HostKeyPolicy::TrustOnce, "xterm-256color", 80, 24, &|_| {})
        .await
        .expect("connect with password");

    let output = shell_roundtrip(connection).await;
    assert!(output.contains("remotier-ok"), "shell output was: {output}");
}

#[tokio::test]
#[ignore = "needs the dockerised sshd"]
async fn connects_with_a_key_from_disk() {
    let target = target(AuthMaterial::KeyPath {
        path: key_path(),
        passphrase: None,
    });
    let connection = connect::open(&target, HostKeyPolicy::TrustOnce, "xterm-256color", 80, 24, &|_| {})
        .await
        .expect("connect with key");

    let output = shell_roundtrip(connection).await;
    assert!(output.contains("remotier-ok"), "shell output was: {output}");
}

#[tokio::test]
#[ignore = "needs the dockerised sshd"]
async fn connects_with_a_key_held_in_the_vault() {
    let pem = std::fs::read_to_string(key_path()).expect("read test key");
    let target = target(AuthMaterial::Key {
        pem: Zeroizing::new(pem),
        passphrase: None,
    });
    let connection = connect::open(&target, HostKeyPolicy::TrustOnce, "xterm-256color", 80, 24, &|_| {})
        .await
        .expect("connect with vaulted key");

    let output = shell_roundtrip(connection).await;
    assert!(output.contains("remotier-ok"), "shell output was: {output}");
}

#[tokio::test]
#[ignore = "needs the dockerised sshd"]
async fn a_wrong_password_is_reported_as_an_auth_failure() {
    let target = target(AuthMaterial::Password(Zeroizing::new("nope".into())));
    let Err(error) = connect::open(&target, HostKeyPolicy::TrustOnce, "xterm", 80, 24, &|_| {}).await
    else {
        panic!("a wrong password must not authenticate");
    };

    assert!(
        matches!(error, Error::Auth(_)),
        "expected an auth error, got {error:?}"
    );
}

#[tokio::test]
#[ignore = "needs the dockerised sshd"]
async fn strict_policy_refuses_a_host_it_has_never_seen() {
    // The container's key is not in known_hosts, so strict mode must refuse it and say
    // so precisely enough for the UI to show a fingerprint prompt.
    let target = target(AuthMaterial::Password(Zeroizing::new(PASSWORD.into())));
    let Err(error) = connect::open(&target, HostKeyPolicy::Strict, "xterm", 80, 24, &|_| {}).await else {
        panic!("strict mode must refuse a host that is not in known_hosts");
    };

    match error {
        Error::UnknownHostKey { fingerprint, .. } => {
            assert!(fingerprint.starts_with("SHA256:"), "got {fingerprint}");
        }
        other => panic!("expected UnknownHostKey, got {other:?}"),
    }
}

#[tokio::test]
#[ignore = "needs the dockerised sshd"]
async fn resizing_a_live_pty_is_accepted() {
    let target = target(AuthMaterial::Password(Zeroizing::new(PASSWORD.into())));
    let connection = connect::open(&target, HostKeyPolicy::TrustOnce, "xterm-256color", 80, 24, &|_| {})
        .await
        .expect("connect");

    connection
        .channel
        .window_change(120, 40, 0, 0)
        .await
        .expect("resize must be accepted by the server");
}

/// Agent auth. Run against a throwaway agent rather than the developer's own:
///
/// ```text
/// eval "$(ssh-agent -s -a /tmp/remotier-agent.sock)"
/// SSH_AUTH_SOCK=/tmp/remotier-agent.sock ssh-add /tmp/remotier-testkey
/// SSH_AUTH_SOCK=/tmp/remotier-agent.sock cargo test --test ssh_live -- --ignored
/// ```
#[tokio::test]
#[ignore = "needs the dockerised sshd and an ssh-agent holding the test key"]
async fn connects_using_the_ssh_agent() {
    let keys = remotier_lib::ssh::agent::identities()
        .await
        .expect("an ssh-agent must be reachable");
    assert!(
        !keys.is_empty(),
        "the agent is not holding any keys - see the setup above; the ambient SSH_AUTH_SOCK \
         is not the throwaway agent this test needs"
    );

    let target = target(AuthMaterial::Agent {
        public_openssh: None,
    });
    let connection = connect::open(&target, HostKeyPolicy::TrustOnce, "xterm-256color", 80, 24, &|_| {})
        .await
        .expect("connect via agent");

    let output = shell_roundtrip(connection).await;
    assert!(output.contains("remotier-ok"), "shell output was: {output}");
}

/// The full backend path: a host row with a vault-sealed password, resolved and dialled.
/// This is the join between `resolve.rs` and `connect.rs` that the other tests exercise
/// only one side of.
#[tokio::test]
#[ignore = "needs the dockerised sshd"]
async fn resolves_a_stored_host_and_connects_with_its_sealed_password() {
    use remotier_lib::commands::secrets;
    use remotier_lib::crypto::vault::Vault;
    use remotier_lib::db::{new_id, now_ms, Db};
    use remotier_lib::ssh::resolve;

    let dir = std::env::temp_dir().join(format!("remotier-live-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = Db::open(&dir.join("live.db")).unwrap();
    let vault = Vault::from_key(&[11u8; 32]).unwrap();

    let identity_id = new_id();
    let host_id = new_id();
    let group_id = new_id();

    db.write(|tx| {
        let now = now_ms();
        let password_ref = secrets::put(tx, &vault, None, PASSWORD)?;

        tx.execute(
            "INSERT INTO identities (id, label, username, auth_kind, password_ref, created_at, updated_at)
             VALUES (?1, 'live', ?2, 'password', ?3, ?4, ?4)",
            rusqlite::params![identity_id, USER, password_ref, now],
        )?;
        // The port comes from the group, so inheritance is part of what is being proved.
        tx.execute(
            "INSERT INTO groups (id, name, sort, default_port, default_identity_id, created_at, updated_at)
             VALUES (?1, 'docker', 0, ?2, ?3, ?4, ?4)",
            rusqlite::params![group_id, i64::from(PORT), identity_id, now],
        )?;
        tx.execute(
            "INSERT INTO hosts (id, group_id, label, hostname, tags, sort, created_at, updated_at)
             VALUES (?1, ?2, 'live', ?3, '[]', 0, ?4, ?4)",
            rusqlite::params![host_id, group_id, HOST, now],
        )?;
        Ok(())
    })
    .unwrap();

    let resolved = resolve::target(&db, &vault, &host_id).expect("resolve the stored host");
    assert_eq!(resolved.port, PORT, "the port must be inherited from the group");

    let connection = connect::open(&resolved, HostKeyPolicy::TrustOnce, "xterm-256color", 80, 24, &|_| {})
        .await
        .expect("connect using the resolved target");

    let output = shell_roundtrip(connection).await;
    assert!(output.contains("remotier-ok"), "shell output was: {output}");

    std::fs::remove_dir_all(&dir).ok();
}

/// The phase 5 gate: a shared group declaring a `{{placeholder}}` username, a per-user
/// value supplied locally, and a real connection through it.
///
/// This is the warpgate shape - the group can be shared with a team, and each member
/// fills in their own value without touching the shared definition.
#[tokio::test]
#[ignore = "needs the dockerised sshd"]
async fn connects_through_a_group_with_a_username_placeholder() {
    use remotier_lib::commands::secrets;
    use remotier_lib::crypto::vault::Vault;
    use remotier_lib::db::{new_id, now_ms, Db};
    use remotier_lib::ssh::resolve;

    let dir = std::env::temp_dir().join(format!("remotier-warpgate-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = Db::open(&dir.join("warpgate.db")).unwrap();
    let vault = Vault::from_key(&[13u8; 32]).unwrap();

    let identity_id = new_id();
    let group_id = new_id();
    let host_id = new_id();

    db.write(|tx| {
        let now = now_ms();
        let password_ref = secrets::put(tx, &vault, None, PASSWORD)?;

        // Shared: the username is a template, not a name.
        tx.execute(
            "INSERT INTO identities (id, label, username, auth_kind, password_ref, created_at, updated_at)
             VALUES (?1, 'warpgate', '{{wg_user}}', 'password', ?2, ?3, ?3)",
            rusqlite::params![identity_id, password_ref, now],
        )?;
        tx.execute(
            "INSERT INTO groups (id, name, sort, default_port, default_identity_id, created_at, updated_at)
             VALUES (?1, 'warpgate', 0, ?2, ?3, ?4, ?4)",
            rusqlite::params![group_id, i64::from(PORT), identity_id, now],
        )?;
        tx.execute(
            "INSERT INTO var_defs (id, scope, scope_id, name, required, created_at, updated_at)
             VALUES (?1, 'group', ?2, 'wg_user', 1, ?3, ?3)",
            rusqlite::params![new_id(), group_id, now],
        )?;
        tx.execute(
            "INSERT INTO hosts (id, group_id, label, hostname, tags, sort, created_at, updated_at)
             VALUES (?1, ?2, 'via warpgate', ?3, '[]', 0, ?4, ?4)",
            rusqlite::params![host_id, group_id, HOST, now],
        )?;
        Ok(())
    })
    .unwrap();

    // Without a value the connection must be refused, naming what is missing.
    let preview = resolve::preview(&db, &host_id).unwrap();
    assert_eq!(preview.missing_variables, vec!["wg_user"]);
    assert!(resolve::target(&db, &vault, &host_id).is_err());

    // This user's own answer, stored locally.
    db.write(|tx| {
        tx.execute(
            "INSERT INTO var_values (scope, scope_id, name, value, updated_at)
             VALUES ('group', ?1, 'wg_user', ?2, ?3)",
            rusqlite::params![group_id, USER, now_ms()],
        )?;
        Ok(())
    })
    .unwrap();

    let target = resolve::target(&db, &vault, &host_id).expect("resolve after filling the variable");
    assert_eq!(target.username, USER, "the placeholder must be substituted");
    assert_eq!(target.port, PORT);

    let connection = connect::open(&target, HostKeyPolicy::TrustOnce, "xterm-256color", 80, 24, &|_| {})
        .await
        .expect("connect through the warpgate-style group");

    let output = shell_roundtrip(connection).await;
    assert!(output.contains("remotier-ok"), "shell output was: {output}");

    std::fs::remove_dir_all(&dir).ok();
}

/// The connection panel is only useful if the stages actually arrive, and in order.
#[tokio::test]
#[ignore = "needs the dockerised sshd"]
async fn reports_each_connection_stage_in_order() {
    use remotier_lib::ssh::connect::Stage;
    use std::sync::{Arc, Mutex};

    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let recorder = {
        let seen = Arc::clone(&seen);
        move |stage: Stage| {
            let name = match stage {
                Stage::Connecting { .. } => "connecting",
                Stage::HostKeyAccepted { .. } => "hostKeyAccepted",
                Stage::Authenticating { .. } => "authenticating",
                Stage::Authenticated { .. } => "authenticated",
                Stage::OpeningShell { .. } => "openingShell",
                Stage::Ready => "ready",
            };
            seen.lock().unwrap().push(name.to_string());
        }
    };

    let target = target(AuthMaterial::Password(Zeroizing::new(PASSWORD.into())));
    connect::open(&target, HostKeyPolicy::TrustOnce, "xterm-256color", 80, 24, &recorder)
        .await
        .expect("connect");

    let stages = seen.lock().unwrap().clone();
    assert_eq!(
        stages,
        vec![
            "connecting",
            "hostKeyAccepted",
            "authenticating",
            "authenticated",
            "openingShell",
            "ready",
        ]
    );
}

/// A failure has to leave the stages it did reach, so the panel can show how far it got.
#[tokio::test]
#[ignore = "needs the dockerised sshd"]
async fn reports_stages_up_to_the_point_of_failure() {
    use remotier_lib::ssh::connect::Stage;
    use std::sync::{Arc, Mutex};

    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let recorder = {
        let seen = Arc::clone(&seen);
        move |stage: Stage| {
            seen.lock().unwrap().push(format!("{stage:?}"));
        }
    };

    let target = target(AuthMaterial::Password(Zeroizing::new("wrong".into())));
    let result = connect::open(&target, HostKeyPolicy::TrustOnce, "xterm", 80, 24, &recorder).await;
    assert!(result.is_err(), "a wrong password must not connect");

    let stages = seen.lock().unwrap().clone();
    assert!(stages.iter().any(|s| s.starts_with("Authenticating")), "got {stages:?}");
    assert!(
        !stages.iter().any(|s| s.starts_with("Authenticated")),
        "must not report success: {stages:?}"
    );
}
