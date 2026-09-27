//! The SSH client against an in-process `russh` server (PLAN Sprint 7): every authentication
//! method, host key decisions, a chain of two jump hosts, the agent, and the terminal backend
//! with reconnection.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers"
)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use opensesh_ssh::backend::{self, Options, Status};
use opensesh_ssh::connect::{self, quiet};
use opensesh_ssh::prompt::{Answer, Asker, HostKeyKind, Prompt, Request};
use opensesh_ssh::spec::{
    AuthMethod, AuthPlan, ConnectSpec, Hop, KnownHostsFiles, Reconnect, SessionSpec,
};
use opensesh_ssh::{SshError, osdetect};
use opensesh_term::backend::BackendEvent;
use russh::keys::{PrivateKey, PublicKey};
use russh::server::{Auth, Handler, Msg, Session};
use russh::{Channel, ChannelId, MethodKind, MethodSet};
use secrecy::SecretString;
use tokio::net::TcpListener;

const USER: &str = "tester";
const PASSWORD: &str = "right password";
const CODE: &str = "123456";
const PASSPHRASE: &str = "fixture passphrase";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../opensesh-vault/tests/fixtures/keys")
        .join(name)
}

fn key(name: &str, passphrase: Option<&str>) -> PrivateKey {
    let text = std::fs::read_to_string(fixture(name)).unwrap();
    russh::keys::decode_secret_key(&text, passphrase).unwrap()
}

/// What a test server accepts.
#[derive(Clone, Default)]
struct Rules {
    /// Accept the password.
    password: bool,
    /// Accept these public keys.
    keys: Vec<PublicKey>,
    /// After a key, ask for a one-time code (keyboard-interactive).
    mfa: bool,
    /// Allow `direct-tcpip` (jump host).
    jump: bool,
    /// Drop the connection when "drop" is typed.
    droppable: bool,
}

#[derive(Clone)]
struct Server {
    rules: Rules,
    key_accepted: bool,
    code_asked: bool,
    typed: Vec<u8>,
    /// Session channels (the tiny shell only answers on these, not on jump tunnels).
    sessions: Vec<ChannelId>,
}

fn methods(rules: &Rules, key_accepted: bool) -> MethodSet {
    let mut kinds = Vec::new();
    if rules.mfa && key_accepted {
        kinds.push(MethodKind::KeyboardInteractive);
    } else {
        if !rules.keys.is_empty() {
            kinds.push(MethodKind::PublicKey);
        }
        if rules.password {
            kinds.push(MethodKind::Password);
        }
    }
    MethodSet::from(kinds.as_slice())
}

impl Handler for Server {
    type Error = russh::Error;

    async fn auth_none(&mut self, _user: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Reject {
            proceed_with_methods: Some(methods(&self.rules, false)),
            partial_success: false,
        })
    }

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        if self.rules.password && user == USER && password == PASSWORD {
            return Ok(Auth::Accept);
        }
        Ok(Auth::Reject {
            proceed_with_methods: Some(methods(&self.rules, false)),
            partial_success: false,
        })
    }

    async fn auth_publickey_offered(
        &mut self,
        _user: &str,
        key: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        if self
            .rules
            .keys
            .iter()
            .any(|allowed| allowed.key_data() == key.key_data())
        {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::reject())
        }
    }

    async fn auth_publickey(&mut self, user: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        let allowed = user == USER
            && self
                .rules
                .keys
                .iter()
                .any(|allowed| allowed.key_data() == key.key_data());
        if !allowed {
            return Ok(Auth::Reject {
                proceed_with_methods: Some(methods(&self.rules, false)),
                partial_success: false,
            });
        }
        if self.rules.mfa {
            self.key_accepted = true;
            return Ok(Auth::Reject {
                proceed_with_methods: Some(methods(&self.rules, true)),
                partial_success: true,
            });
        }
        Ok(Auth::Accept)
    }

    async fn auth_keyboard_interactive<'a>(
        &'a mut self,
        _user: &str,
        _submethods: &str,
        response: Option<russh::server::Response<'a>>,
    ) -> Result<Auth, Self::Error> {
        if !(self.rules.mfa && self.key_accepted) {
            return Ok(Auth::reject());
        }
        if !self.code_asked {
            self.code_asked = true;
            return Ok(Auth::Partial {
                name: "Two-factor".into(),
                instructions: "Enter the code from your app.".into(),
                prompts: vec![("Verification code: ".into(), false)].into(),
            });
        }
        let answer = response.and_then(|mut response| response.next());
        if answer.as_deref() == Some(CODE.as_bytes()) {
            Ok(Auth::Accept)
        } else {
            self.code_asked = false;
            Ok(Auth::Reject {
                proceed_with_methods: Some(methods(&self.rules, true)),
                partial_success: false,
            })
        }
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: russh::server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.sessions.push(channel.id());
        reply.accept().await;
        Ok(())
    }

    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<Msg>,
        host: &str,
        port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: russh::server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if !self.rules.jump {
            reply
                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                .await;
            return Ok(());
        }
        let target = format!("{host}:{port}");
        reply.accept().await;
        tokio::spawn(async move {
            if let Ok(mut tcp) = tokio::net::TcpStream::connect(target).await {
                let mut stream = channel.into_stream();
                let _ = tokio::io::copy_bidirectional(&mut stream, &mut tcp).await;
            }
        });
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        session.data(channel, b"test$ ".to_vec())?;
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        command: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        if command == osdetect::COMMAND.as_bytes() {
            session.data(channel, b"ID=debian\n__uname__\nLinux\n".to_vec())?;
            session.exit_status_request(channel, 0)?;
            session.eof(channel)?;
            session.close(channel)?;
        }
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        // A tiny shell: echo, "exit" ends it, "drop" drops the connection.
        if !self.sessions.contains(&channel) {
            return Ok(());
        }
        session.data(channel, data.to_vec())?;
        self.typed.extend_from_slice(data);
        if self.typed.ends_with(b"exit\r") {
            session.exit_status_request(channel, 3)?;
            session.eof(channel)?;
            session.close(channel)?;
        }
        if self.rules.droppable && self.typed.ends_with(b"drop\r") {
            self.typed.clear();
            return Err(russh::Error::Disconnect);
        }
        Ok(())
    }
}

/// Starts a server with `rules` on a free port of 127.0.0.1; returns the port. It runs until the
/// test process ends.
async fn serve(rules: Rules) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let config = Arc::new(russh::server::Config {
        keys: vec![key("ed25519", None)],
        auth_rejection_time: Duration::from_millis(1),
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..russh::server::Config::default()
    });
    tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                return;
            };
            let handler = Server {
                rules: rules.clone(),
                key_accepted: false,
                code_asked: false,
                typed: Vec::new(),
                sessions: Vec::new(),
            };
            let config = Arc::clone(&config);
            tokio::spawn(async move {
                if let Ok(session) = russh::server::run_stream(config, socket, handler).await {
                    let _ = session.await;
                }
            });
        }
    });
    port
}

/// An asker that answers from a script and records every question.
struct Script {
    asked: Arc<Mutex<Vec<Prompt>>>,
    asker: Asker,
}

fn script(answer: impl Fn(&Prompt) -> Answer + Send + Sync + 'static) -> Script {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&asked);
    let asker: Asker = Arc::new(move |request: Request| {
        log.lock().unwrap().push(request.prompt.clone());
        let reply = answer(&request.prompt);
        request.answer(reply);
    });
    Script { asked, asker }
}

fn secret(text: &str) -> Answer {
    Answer::Secrets(vec![SecretString::from(text)])
}

fn hop(port: u16, auth: AuthPlan) -> Hop {
    Hop {
        host: "127.0.0.1".into(),
        port,
        user: USER.into(),
        auth,
    }
}

fn spec(dir: &Path, hops: Vec<Hop>) -> ConnectSpec {
    ConnectSpec {
        hops,
        proxy: None,
        legacy: false,
        compression: false,
        keepalive: None,
        connect_timeout: Duration::from_secs(10),
        known_hosts: KnownHostsFiles {
            own: dir.join("known_hosts"),
            user: None,
        },
        agent_forwarding: false,
        agent_socket: None,
    }
}

fn password_plan(password: Option<&str>) -> AuthPlan {
    AuthPlan {
        password: password.map(SecretString::from),
        ..AuthPlan::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn passwords_and_host_keys() {
    let port = serve(Rules {
        password: true,
        ..Rules::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    // First time: the key is new; trust and remember it. The password is asked, wrong first.
    let attempts = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&attempts);
    let first = script(move |prompt| match prompt {
        Prompt::HostKey(question) => {
            assert!(matches!(question.kind, HostKeyKind::New { .. }));
            assert!(question.fingerprint.starts_with("SHA256:"));
            Answer::TrustAndRemember
        }
        Prompt::Password { retry, .. } => {
            let attempt = counter.fetch_add(1, Ordering::SeqCst);
            assert_eq!(*retry, attempt > 0);
            secret(if attempt == 0 { "wrong" } else { PASSWORD })
        }
        other => panic!("unexpected {other:?}"),
    });
    let connection = connect::connect(
        &spec(dir.path(), vec![hop(port, password_plan(None))]),
        &first.asker,
        &quiet(),
    )
    .await
    .unwrap();
    connection.close().await;
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    let remembered = std::fs::read_to_string(dir.path().join("known_hosts")).unwrap();
    assert!(remembered.starts_with(&format!("[127.0.0.1]:{port} ssh-ed25519 ")));

    // Second time: the key is known, the identity's password works: nothing is asked.
    let second = script(|prompt| panic!("asked {prompt:?}"));
    let connection = connect::connect(
        &spec(dir.path(), vec![hop(port, password_plan(Some(PASSWORD)))]),
        &second.asker,
        &quiet(),
    )
    .await
    .unwrap();
    connection.close().await;
    assert!(second.asked.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn changed_and_revoked_host_keys() {
    let port = serve(Rules {
        password: true,
        ..Rules::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    // Another Ed25519 key than the server's, remembered for its address: the key "changed".
    let old = std::fs::read_to_string(fixture("ed25519-enc.pub")).unwrap();
    std::fs::write(
        dir.path().join("known_hosts"),
        format!("[127.0.0.1]:{port} {}", old.trim()),
    )
    .unwrap();
    let refuse = script(|prompt| match prompt {
        Prompt::HostKey(question) => {
            assert!(
                matches!(question.kind, HostKeyKind::Changed { line: 1, .. }),
                "{question:?}"
            );
            Answer::Cancel
        }
        other => panic!("unexpected {other:?}"),
    });
    let error = connect::connect(
        &spec(dir.path(), vec![hop(port, password_plan(Some(PASSWORD)))]),
        &refuse.asker,
        &quiet(),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, SshError::HostKey { .. }), "{error}");

    assert_eq!(refuse.asked.lock().unwrap().len(), 1);
    // Accepting the changed key replaces the old line.
    let accept = script(|_| Answer::TrustAndRemember);
    connect::connect(
        &spec(dir.path(), vec![hop(port, password_plan(Some(PASSWORD)))]),
        &accept.asker,
        &quiet(),
    )
    .await
    .unwrap()
    .close()
    .await;
    let server_key = key("ed25519", None).public_key().to_openssh().unwrap();
    let known = std::fs::read_to_string(dir.path().join("known_hosts")).unwrap();
    assert_eq!(known.lines().count(), 1);
    assert!(known.contains(server_key.split_whitespace().nth(1).unwrap()));

    // Revoked: refused without asking.
    std::fs::write(
        dir.path().join("known_hosts"),
        format!("@revoked * {server_key}\n"),
    )
    .unwrap();
    let never = script(|prompt| panic!("asked {prompt:?}"));
    let error = connect::connect(
        &spec(dir.path(), vec![hop(port, password_plan(Some(PASSWORD)))]),
        &never.asker,
        &quiet(),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("revoked"), "{error}");
}

#[tokio::test(flavor = "multi_thread")]
async fn keys_from_files_and_the_vault() {
    let client = key("p256", None);
    let encrypted = key("ed25519-enc", Some(PASSPHRASE));
    let port = serve(Rules {
        keys: vec![client.public_key().clone(), encrypted.public_key().clone()],
        ..Rules::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let trust = |prompt: &Prompt| match prompt {
        Prompt::HostKey(_) => Answer::TrustAndRemember,
        Prompt::Passphrase { retry, .. } => secret(if *retry { PASSPHRASE } else { "not it" }),
        other => panic!("unexpected {other:?}"),
    };
    // A vault key (already decrypted).
    let answers = script(trust);
    let plan = AuthPlan {
        keys: vec![Arc::new(client.clone())],
        order: vec![AuthMethod::PublicKey],
        ..AuthPlan::default()
    };
    connect::connect(
        &spec(dir.path(), vec![hop(port, plan)]),
        &answers.asker,
        &quiet(),
    )
    .await
    .unwrap()
    .close()
    .await;
    // A key file with a passphrase: asked, wrong once, then right.
    let answers = script(trust);
    let plan = AuthPlan {
        key_files: vec![fixture("ed25519-enc")],
        order: vec![AuthMethod::PublicKey],
        ..AuthPlan::default()
    };
    connect::connect(
        &spec(dir.path(), vec![hop(port, plan)]),
        &answers.asker,
        &quiet(),
    )
    .await
    .unwrap()
    .close()
    .await;
    let asked = answers.asked.lock().unwrap().clone();
    assert_eq!(
        asked
            .iter()
            .filter(|prompt| matches!(prompt, Prompt::Passphrase { .. }))
            .count(),
        2
    );
    // No usable key: a clear error naming what was tried.
    let answers = script(trust);
    let plan = AuthPlan {
        key_files: vec![fixture("p384-enc")],
        order: vec![AuthMethod::PublicKey],
        ..AuthPlan::default()
    };
    let error = connect::connect(
        &spec(dir.path(), vec![hop(port, plan)]),
        &answers.asker,
        &quiet(),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, SshError::Auth { .. }), "{error}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_key_then_a_one_time_code() {
    let client = key("p256", None);
    let port = serve(Rules {
        keys: vec![client.public_key().clone()],
        mfa: true,
        ..Rules::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let answers = script(|prompt| match prompt {
        Prompt::HostKey(_) => Answer::TrustOnce,
        Prompt::KeyboardInteractive {
            name,
            instructions,
            fields,
            ..
        } => {
            assert_eq!(name, "Two-factor");
            assert_eq!(instructions, "Enter the code from your app.");
            assert_eq!(fields.len(), 1);
            assert!(!fields[0].echo);
            secret(CODE)
        }
        other => panic!("unexpected {other:?}"),
    });
    let plan = AuthPlan {
        keys: vec![Arc::new(client)],
        ..AuthPlan::default()
    };
    connect::connect(
        &spec(dir.path(), vec![hop(port, plan)]),
        &answers.asker,
        &quiet(),
    )
    .await
    .unwrap()
    .close()
    .await;
    // "Trust once" leaves nothing behind.
    assert!(!dir.path().join("known_hosts").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn through_two_jump_hosts() {
    let jump_key = key("p256", None);
    let first = serve(Rules {
        password: true,
        jump: true,
        ..Rules::default()
    })
    .await;
    let second = serve(Rules {
        keys: vec![jump_key.public_key().clone()],
        jump: true,
        ..Rules::default()
    })
    .await;
    let target = serve(Rules {
        password: true,
        ..Rules::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let answers = script(|prompt| match prompt {
        Prompt::HostKey(_) => Answer::TrustAndRemember,
        other => panic!("unexpected {other:?}"),
    });
    let hops = vec![
        hop(first, password_plan(Some(PASSWORD))),
        hop(
            second,
            AuthPlan {
                keys: vec![Arc::new(jump_key)],
                ..AuthPlan::default()
            },
        ),
        hop(target, password_plan(Some(PASSWORD))),
    ];
    let notes = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&notes);
    let sink: connect::Notes = Arc::new(move |note| log.lock().unwrap().push(note));
    let connection = connect::connect(&spec(dir.path(), hops), &answers.asker, &sink)
        .await
        .unwrap();
    // The OS is detected on the target, through both jumps.
    assert_eq!(osdetect::detect(&connection).await, Some("os-debian"));
    connection.close().await;
    // Three host keys asked and remembered.
    assert_eq!(answers.asked.lock().unwrap().len(), 3);
    let known = std::fs::read_to_string(dir.path().join("known_hosts")).unwrap();
    assert_eq!(known.lines().count(), 3);
    let connecting = notes
        .lock()
        .unwrap()
        .iter()
        .filter(|note| matches!(note, connect::Note::Connecting { count: 3, .. }))
        .count();
    assert_eq!(connecting, 3);
}

/// Starts russh's agent server on a fresh Unix socket (or Windows named pipe) holding `key`;
/// returns where it listens.
async fn agent_with(key: &PrivateKey, dir: &Path) -> String {
    #[cfg(unix)]
    let (address, listener) = {
        let path = dir.join("agent.sock");
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let connections = futures::stream::unfold(listener, |listener| async move {
            let next = listener.accept().await.map(|(stream, _)| stream);
            Some((next, listener))
        });
        (path.display().to_string(), connections)
    };
    #[cfg(windows)]
    let (address, listener) = {
        let _ = dir;
        let pipe = format!(r"\\.\pipe\opensesh-test-agent-{}", std::process::id());
        let first = tokio::net::windows::named_pipe::ServerOptions::new()
            .first_pipe_instance(true)
            .create(&pipe)
            .unwrap();
        let name = pipe.clone();
        let connections = futures::stream::unfold((first, name), |(server, name)| async move {
            let next = server.connect().await.map(|()| server);
            let following = tokio::net::windows::named_pipe::ServerOptions::new()
                .create(&name)
                .ok()?;
            Some((next, (following, name)))
        });
        (pipe, connections)
    };
    tokio::spawn(russh::keys::agent::server::serve(Box::pin(listener), ()));
    // Load the key into it, as ssh-add would.
    #[cfg(unix)]
    let stream = tokio::net::UnixStream::connect(&address).await.unwrap();
    #[cfg(windows)]
    let stream = tokio::net::windows::named_pipe::ClientOptions::new()
        .open(&address)
        .unwrap();
    let mut client = russh::keys::agent::client::AgentClient::connect(stream);
    client.add_identity(key, &[]).await.unwrap();
    address
}

#[tokio::test(flavor = "multi_thread")]
async fn the_agent_signs() {
    let client = key("p256", None);
    let port = serve(Rules {
        keys: vec![client.public_key().clone()],
        ..Rules::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let socket = agent_with(&client, dir.path()).await;
    let answers = script(|prompt| match prompt {
        Prompt::HostKey(_) => Answer::TrustOnce,
        other => panic!("unexpected {other:?}"),
    });
    let plan = AuthPlan {
        agent: true,
        agent_socket: Some(socket),
        order: vec![AuthMethod::PublicKey],
        ..AuthPlan::default()
    };
    connect::connect(
        &spec(dir.path(), vec![hop(port, plan)]),
        &answers.asker,
        &quiet(),
    )
    .await
    .unwrap()
    .close()
    .await;
}

/// Waits for backend events until `until` returns true for the output so far.
fn read_until(
    events: &crossbeam_channel::Receiver<BackendEvent>,
    mut until: impl FnMut(&str, &[BackendEvent]) -> bool,
) -> (String, Vec<BackendEvent>) {
    let mut text = String::new();
    let mut all = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
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

#[test]
fn the_terminal_backend_reconnects() {
    let runtime = opensesh_ssh::runtime().unwrap();
    let port = runtime.block_on(serve(Rules {
        password: true,
        droppable: true,
        ..Rules::default()
    }));
    let dir = tempfile::tempdir().unwrap();
    let answers = script(|prompt| match prompt {
        Prompt::HostKey(_) => Answer::TrustAndRemember,
        other => panic!("unexpected {other:?}"),
    });
    let statuses = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&statuses);
    let os_seen = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&os_seen);
    let sink: backend::StatusSink = Arc::new(move |status| {
        if status == Status::OsDetected("os-debian") {
            seen.store(true, Ordering::SeqCst);
        }
        log.lock().unwrap().push(status);
    });
    let session = SessionSpec {
        startup: Some("echo started".into()),
        reconnect: Reconnect {
            automatic: false,
            max_attempts: 3,
        },
        ..SessionSpec::default()
    };
    let (backend, events) = backend::start(
        spec(dir.path(), vec![hop(port, password_plan(Some(PASSWORD)))]),
        session,
        Options { detect_os: true },
        answers.asker,
        sink,
    )
    .unwrap();
    // The prompt, then the startup snippet echoed by the server.
    let (text, _) = read_until(&events, |text, _| text.contains("echo started\r"));
    assert!(text.contains("Connecting to tester@127.0.0.1:"), "{text:?}");
    assert!(text.contains("test$ "), "{text:?}");
    // Typing reaches the server and comes back.
    backend.write(b"hello\r").unwrap();
    read_until(&events, |text, _| text.contains("hello\r"));
    // The connection drops: the pane says so and waits for Enter.
    backend.write(b"drop\r").unwrap();
    read_until(&events, |text, _| {
        text.contains("Press Enter to reconnect.")
    });
    backend.write(b"\r").unwrap();
    read_until(&events, |text, _| {
        text.matches("test$ ").count() >= 1 && text.contains("Connecting to")
    });
    // Exiting the remote shell ends the session with its code.
    backend.write(b"exit\r").unwrap();
    let (_, all) = read_until(&events, |_, all| {
        all.iter()
            .any(|event| matches!(event, BackendEvent::Exited(_)))
    });
    assert!(all.contains(&BackendEvent::Exited(Some(3))), "{all:?}");
    let statuses = statuses.lock().unwrap().clone();
    assert!(statuses.contains(&Status::Connected));
    assert!(
        statuses
            .iter()
            .any(|status| matches!(status, Status::Disconnected { retry_in: None, .. }))
    );
    assert!(statuses.contains(&Status::Ended));
    assert!(os_seen.load(Ordering::SeqCst), "{statuses:?}");
}
