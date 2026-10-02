#![allow(clippy::unwrap_used, clippy::panic, reason = "tests")]

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use opensesh_ssh::prompt::{Answer, Request};
use opensesh_ssh::spec::{AuthPlan, Hop, KnownHostsFiles};
use opensesh_ssh::testing::{self, PASSWORD, Rules, USER};
use secrecy::SecretString;

use super::*;

/// The key the test server hands out.
const KEY: &str = "T3BlblNlc2ggdGVzdCBrZQ";

fn connect_spec(port: u16, known: &Path) -> ConnectSpec {
    ConnectSpec {
        hops: vec![Hop {
            host: "127.0.0.1".into(),
            port,
            user: USER.into(),
            auth: AuthPlan {
                password: Some(SecretString::from(PASSWORD.to_owned())),
                ..AuthPlan::default()
            },
        }],
        proxy: None,
        legacy: false,
        compression: false,
        keepalive: None,
        connect_timeout: Duration::from_secs(10),
        known_hosts: KnownHostsFiles {
            own: known.join("known_hosts"),
            ..KnownHostsFiles::default()
        },
        agent_forwarding: false,
        agent_socket: None,
        x11: None,
    }
}

fn server(mosh: bool) -> u16 {
    let runtime = opensesh_ssh::runtime().unwrap();
    runtime
        .block_on(testing::serve(Rules {
            password: true,
            mosh,
            ..Rules::default()
        }))
        .unwrap()
}

type Statuses = Arc<Mutex<Vec<Status>>>;

fn start_session(spec: MoshSpec) -> (Box<dyn TerminalBackend>, Receiver<BackendEvent>, Statuses) {
    let trust: Asker = Arc::new(|request: Request| request.answer(Answer::TrustOnce));
    let statuses: Statuses = Arc::default();
    let seen = Arc::clone(&statuses);
    let sink: StatusSink = Arc::new(move |status| seen.lock().unwrap().push(status));
    let (backend, events) = start(spec, TermSize::new(80, 24), trust, sink).unwrap();
    (backend, events, statuses)
}

/// Everything shown until the session ends, and the error it ended with.
fn until_the_end(events: &Receiver<BackendEvent>) -> (String, Option<String>) {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut text = String::new();
    let mut error = None;
    loop {
        match events.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(BackendEvent::Output(bytes)) => text.push_str(&String::from_utf8_lossy(&bytes)),
            Ok(BackendEvent::Error(reason)) => error = Some(reason),
            Ok(BackendEvent::Exited(_)) => return (text, error),
            Err(_) => panic!("no end in {text:?}"),
        }
    }
}

#[test]
fn the_server_starts_over_ssh() {
    let known = tempfile::tempdir().unwrap();
    let (_backend, events, statuses) = start_session(MoshSpec {
        connect: connect_spec(server(true), known.path()),
        term: "xterm-256color".into(),
        client: None,
        dry_run: true,
    });
    let (text, error) = until_the_end(&events);
    assert_eq!(error, None, "{text:?}");
    assert!(
        text.contains("Starting mosh-server on 127.0.0.1"),
        "{text:?}"
    );
    assert!(
        text.contains("listening on UDP port 60001 of 127.0.0.1"),
        "{text:?}"
    );
    assert!(!text.contains(KEY), "the key was shown: {text:?}");
    assert!(statuses.lock().unwrap().contains(&Status::Connected));
}

#[test]
fn a_host_without_mosh_server_says_how_to_install_it() {
    let known = tempfile::tempdir().unwrap();
    let (_backend, events, _) = start_session(MoshSpec {
        connect: connect_spec(server(false), known.path()),
        term: "xterm-256color".into(),
        client: None,
        dry_run: true,
    });
    let (text, error) = until_the_end(&events);
    let error = error.unwrap_or_else(|| panic!("no error in {text:?}"));
    assert!(
        error.contains("mosh-server isn't installed on 127.0.0.1")
            && error.contains("apt install mosh"),
        "{error}"
    );
}

#[test]
fn without_mosh_client_nothing_connects() {
    let (_backend, events, statuses) = start_session(MoshSpec {
        // Nothing listens there: it must not even try.
        connect: connect_spec(9, Path::new("unused")),
        term: "xterm-256color".into(),
        client: Some(PathBuf::from("/opensesh/no/mosh-client")),
        dry_run: false,
    });
    let (text, error) = until_the_end(&events);
    assert!(error.unwrap().contains("mosh-client"), "{text:?}");
    assert!(!statuses.lock().unwrap().contains(&Status::Connected));
}

/// A stand-in `mosh-client` (a shell script) gets the address and port as arguments and the
/// key in its environment.
#[cfg(unix)]
#[test]
fn the_client_gets_the_session() {
    use std::os::unix::fs::PermissionsExt;

    let known = tempfile::tempdir().unwrap();
    let client = known.path().join("mosh-client");
    std::fs::write(&client, "#!/bin/sh\necho \"client $1 $2 key=$MOSH_KEY\"\n").unwrap();
    std::fs::set_permissions(&client, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (_backend, events, _) = start_session(MoshSpec {
        connect: connect_spec(server(true), known.path()),
        term: "xterm-256color".into(),
        client: Some(client),
        dry_run: false,
    });
    let (text, error) = until_the_end(&events);
    assert_eq!(error, None, "{text:?}");
    assert!(
        text.contains(&format!("client 127.0.0.1 60001 key={KEY}")),
        "{text:?}"
    );
}
