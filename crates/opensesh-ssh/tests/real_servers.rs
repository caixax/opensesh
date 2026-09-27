//! The SSH client against real servers (PLAN Sprint 7): OpenSSH and Dropbear, a chain of two jump
//! hosts authenticated by an agent, a one-time code (TOTP through PAM) after a key, a user
//! certificate, agent forwarding, and the terminal backend reconnecting after the server side of
//! the session is killed.
//!
//! The servers come from `scripts/ssh-test-servers.sh start` (127.0.0.1:2221-2224), so these
//! tests are ignored by default. With the servers up, and an agent that holds
//! `$OPENSESH_SSH_SERVERS/client_ed25519`:
//!
//! ```sh
//! OPENSESH_SSH_SERVERS=/tmp/opensesh-ssh-servers cargo test -p opensesh-ssh --test real_servers -- --ignored
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers"
)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use opensesh_ssh::backend::{self, Options, Status};
use opensesh_ssh::connect;
use opensesh_ssh::osdetect;
use opensesh_ssh::prompt::{Answer, Asker, Prompt, Request};
use opensesh_ssh::spec::{AuthPlan, ConnectSpec, Hop, KnownHostsFiles, SessionSpec};
use opensesh_term::backend::BackendEvent;
use secrecy::SecretString;

/// What `scripts/ssh-test-servers.sh` sets up.
const USER: &str = "opensesh-test";
const PASSWORD: &str = "opensesh test password";
const TOTP_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
const OPENSSH_KEY_ONLY: u16 = 2221;
const DROPBEAR_JUMP: u16 = 2222;
const OPENSSH_MFA: u16 = 2223;
const DROPBEAR_PASSWORD: u16 = 2224;

fn state() -> PathBuf {
    std::env::var_os("OPENSESH_SSH_SERVERS")
        .map_or_else(|| PathBuf::from("/tmp/opensesh-ssh-servers"), PathBuf::from)
}

/// The current one-time code.
fn totp() -> String {
    let output = std::process::Command::new("oathtool")
        .args(["--totp", "-b", TOTP_SECRET])
        .output()
        .expect("oathtool (from the servers script's packages)");
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// Answers every question like a user who trusts new host keys and knows the password and the
/// code; records the questions.
fn user() -> (Asker, Arc<Mutex<Vec<Prompt>>>) {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&asked);
    let asker: Asker = Arc::new(move |request: Request| {
        log.lock().unwrap().push(request.prompt.clone());
        let answer = match &request.prompt {
            Prompt::HostKey(_) => Answer::TrustAndRemember,
            Prompt::Password { .. } => Answer::Secrets(vec![SecretString::from(PASSWORD)]),
            Prompt::KeyboardInteractive { fields, .. } => {
                Answer::Secrets(fields.iter().map(|_| SecretString::from(totp())).collect())
            }
            Prompt::Passphrase { .. } => Answer::Cancel,
        };
        request.answer(answer);
    });
    (asker, asked)
}

fn hop(port: u16, auth: AuthPlan) -> Hop {
    Hop {
        host: "127.0.0.1".into(),
        port,
        user: USER.into(),
        auth,
    }
}

/// The running agent (SSH_AUTH_SOCK) only.
fn agent() -> AuthPlan {
    AuthPlan {
        agent: true,
        ..AuthPlan::default()
    }
}

fn spec(dir: &Path, hops: Vec<Hop>) -> ConnectSpec {
    ConnectSpec {
        hops,
        proxy: None,
        legacy: false,
        compression: true,
        keepalive: Some(Duration::from_secs(15)),
        connect_timeout: Duration::from_secs(10),
        known_hosts: KnownHostsFiles {
            own: dir.join("known_hosts"),
            ..KnownHostsFiles::default()
        },
        agent_forwarding: false,
        agent_socket: None,
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs scripts/ssh-test-servers.sh start"]
async fn openssh_with_a_key_file_and_dropbear_with_a_password() {
    let dir = tempfile::tempdir().unwrap();
    let (asker, asked) = user();
    let keyed = AuthPlan {
        key_files: vec![state().join("client_ed25519")],
        ..AuthPlan::default()
    };
    let openssh = connect::connect(
        &spec(dir.path(), vec![hop(OPENSSH_KEY_ONLY, keyed)]),
        &asker,
        &connect::quiet(),
    )
    .await
    .unwrap();
    assert!(osdetect::detect(&openssh).await.is_some());
    openssh.close().await;
    let password = AuthPlan {
        password: Some(SecretString::from(PASSWORD)),
        ..AuthPlan::default()
    };
    let dropbear = connect::connect(
        &spec(dir.path(), vec![hop(DROPBEAR_PASSWORD, password)]),
        &asker,
        &connect::quiet(),
    )
    .await
    .unwrap();
    assert!(osdetect::detect(&dropbear).await.is_some());
    dropbear.close().await;
    // One host key each time (both servers share 127.0.0.1 but not their port), no password
    // asked: the plan had it.
    {
        let asked = asked.lock().unwrap();
        assert_eq!(asked.len(), 2, "{asked:?}");
        assert!(
            asked
                .iter()
                .all(|prompt| matches!(prompt, Prompt::HostKey(_)))
        );
    }
    // A wrong password is refused (and asking again is cancelled).
    let wrong = AuthPlan {
        password: Some(SecretString::from("not the password")),
        ..AuthPlan::default()
    };
    let never: Asker = Arc::new(|request: Request| request.answer(Answer::Cancel));
    assert!(
        connect::connect(
            &spec(dir.path(), vec![hop(DROPBEAR_PASSWORD, wrong)]),
            &never,
            &connect::quiet(),
        )
        .await
        .is_err()
    );
}

#[test]
#[ignore = "needs scripts/ssh-test-servers.sh start"]
fn a_user_certificate_then_agent_forwarding() {
    let dir = tempfile::tempdir().unwrap();
    let (asker, _) = user();
    // The certified key isn't in authorized_keys: only its certificate gets it in.
    let certified = AuthPlan {
        key_files: vec![state().join("client_cert_ed25519")],
        ..AuthPlan::default()
    };
    let runtime = opensesh_ssh::runtime().unwrap();
    let connection = runtime
        .block_on(connect::connect(
            &spec(dir.path(), vec![hop(OPENSSH_KEY_ONLY, certified)]),
            &asker,
            &connect::quiet(),
        ))
        .unwrap();
    runtime.block_on(connection.close());
    // The agent reaches the server's `ssh-add -l` through the forwarded channel.
    let forwarding = ConnectSpec {
        agent_forwarding: true,
        ..spec(dir.path(), vec![hop(OPENSSH_KEY_ONLY, agent())])
    };
    let session = SessionSpec {
        command: Some("ssh-add -l".to_owned()),
        ..SessionSpec::default()
    };
    let sink: backend::StatusSink = Arc::new(|_| {});
    let (_terminal, events) =
        backend::start(forwarding, session, Options::default(), asker, sink).unwrap();
    let (text, all) = read_until(&events, |_, all| {
        all.iter()
            .any(|event| matches!(event, BackendEvent::Exited(_)))
    });
    assert!(text.contains("opensesh-test-client"), "{text:?}");
    assert!(all.contains(&BackendEvent::Exited(Some(0))), "{all:?}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs scripts/ssh-test-servers.sh start"]
async fn two_jump_hosts_with_the_agent_then_a_one_time_code() {
    let dir = tempfile::tempdir().unwrap();
    let (asker, asked) = user();
    let hops = vec![
        hop(OPENSSH_KEY_ONLY, agent()),
        hop(DROPBEAR_JUMP, agent()),
        hop(OPENSSH_MFA, agent()),
    ];
    let connection = connect::connect(&spec(dir.path(), hops), &asker, &connect::quiet())
        .await
        .unwrap();
    assert!(osdetect::detect(&connection).await.is_some());
    connection.close().await;
    let asked = asked.lock().unwrap();
    // Three host keys (the same key on two OpenSSH ports is still two hosts), then the code.
    assert_eq!(
        asked
            .iter()
            .filter(|prompt| matches!(prompt, Prompt::HostKey(_)))
            .count(),
        3,
        "{asked:?}"
    );
    assert!(
        asked.iter().any(|prompt| matches!(
            prompt,
            Prompt::KeyboardInteractive { fields, .. } if fields.len() == 1 && !fields[0].echo
        )),
        "{asked:?}"
    );
}

#[test]
#[ignore = "needs scripts/ssh-test-servers.sh start"]
fn the_terminal_reconnects_after_the_server_side_dies() {
    let dir = tempfile::tempdir().unwrap();
    let (asker, asked) = user();
    let statuses = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&statuses);
    let sink: backend::StatusSink = Arc::new(move |status| log.lock().unwrap().push(status));
    let hops = vec![hop(DROPBEAR_JUMP, agent()), hop(OPENSSH_MFA, agent())];
    let (terminal, events) = backend::start(
        spec(dir.path(), hops),
        SessionSpec::default(),
        Options {
            detect_os: true,
            ..Options::default()
        },
        asker,
        sink,
    )
    .unwrap();
    // Typing only reaches the shell once connected.
    wait_for(&statuses, 1, |status| *status == Status::Connected);
    terminal.write(b"echo ready-$((40 + 2))\r").unwrap();
    read_until(&events, |text, _| text.contains("ready-42"));
    // The session's sshd dies: the connection is lost, not ended.
    terminal.write(b"kill -9 $PPID\r").unwrap();
    read_until(&events, |text, _| {
        text.contains("Press Enter to reconnect.")
    });
    let codes_before = asked
        .lock()
        .unwrap()
        .iter()
        .filter(|prompt| matches!(prompt, Prompt::KeyboardInteractive { .. }))
        .count();
    terminal.write(b"\r").unwrap();
    wait_for(&statuses, 2, |status| *status == Status::Connected);
    terminal.write(b"echo again-$((40 + 3))\r").unwrap();
    read_until(&events, |text, _| text.contains("again-43"));
    terminal.write(b"exit 0\r").unwrap();
    let (_, all) = read_until(&events, |_, all| {
        all.iter()
            .any(|event| matches!(event, BackendEvent::Exited(_)))
    });
    assert!(all.contains(&BackendEvent::Exited(Some(0))), "{all:?}");
    // A new code for the new connection.
    let codes_after = asked
        .lock()
        .unwrap()
        .iter()
        .filter(|prompt| matches!(prompt, Prompt::KeyboardInteractive { .. }))
        .count();
    assert_eq!(codes_after, codes_before + 1);
    let statuses = statuses.lock().unwrap();
    assert!(
        statuses
            .iter()
            .any(|status| matches!(status, Status::Disconnected { code: "lost", .. })),
        "{statuses:?}"
    );
    assert!(
        statuses
            .iter()
            .any(|status| matches!(status, Status::OsDetected(_)))
    );
}

/// Waits until `statuses` holds `count` entries that match `what`.
fn wait_for(statuses: &Mutex<Vec<Status>>, count: usize, what: impl Fn(&Status) -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        if statuses
            .lock()
            .unwrap()
            .iter()
            .filter(|status| what(status))
            .count()
            >= count
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("timed out; statuses {:?}", statuses.lock().unwrap());
}

fn read_until(
    events: &crossbeam_channel::Receiver<BackendEvent>,
    mut until: impl FnMut(&str, &[BackendEvent]) -> bool,
) -> (String, Vec<BackendEvent>) {
    let mut text = String::new();
    let mut all = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        match events.recv_timeout(Duration::from_millis(200)) {
            Ok(event) => {
                if let BackendEvent::Output(bytes) = &event {
                    text.push_str(&String::from_utf8_lossy(bytes));
                }
                all.push(event);
                if until(&text, &all) {
                    return (text, all);
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }
    panic!("timed out; got {text:?}");
}
