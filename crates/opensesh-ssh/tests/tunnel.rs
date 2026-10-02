//! Tunnels against the in-process server (PLAN Sprint 9): each kind with HTTP through it (our own
//! client, and `curl` where it is installed), the traffic counters, a connection replaced under
//! running forwards, a stopped forward giving its port back, and an independent tunnel's own
//! connection coming back by itself.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers"
)]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use opensesh_ssh::connect::{self, Connection};
use opensesh_ssh::prompt::{Answer, Asker, Prompt, Request as Question};
use opensesh_ssh::proxy;
use opensesh_ssh::spec::{AuthPlan, ConnectSpec, Hop, KnownHostsFiles};
use opensesh_ssh::testing::{PASSWORD, Rules, USER, serve};
use opensesh_ssh::tunnel::{self, Connector, Endpoint, Forward, Link, Report, ReportSink, Running};
use secrecy::SecretString;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

const BODY: &str = "hello through the tunnel";

fn spec(port: u16, password: &str, known: &std::path::Path) -> ConnectSpec {
    ConnectSpec {
        hops: vec![Hop {
            host: "127.0.0.1".into(),
            port,
            user: USER.into(),
            auth: AuthPlan {
                password: Some(SecretString::from(password.to_owned())),
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

fn trust() -> Asker {
    Arc::new(|question: Question| question.answer(Answer::TrustOnce))
}

async fn server(jump: bool) -> u16 {
    serve(Rules {
        password: true,
        jump,
        ..Rules::default()
    })
    .await
    .unwrap()
}

async fn connection(port: u16, known: &std::path::Path) -> Arc<Connection> {
    Arc::new(
        connect::connect(&spec(port, PASSWORD, known), &trust(), &connect::quiet())
            .await
            .unwrap(),
    )
}

/// A tiny HTTP server: every request gets [`BODY`].
async fn http() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut request = Vec::new();
                let mut buffer = [0_u8; 1024];
                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    match socket.read(&mut buffer).await {
                        Ok(0) | Err(_) => return,
                        Ok(read) => request.extend_from_slice(&buffer[..read]),
                    }
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{BODY}",
                    BODY.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            });
        }
    });
    port
}

/// GET / over `stream`; the response's body (empty when the connection is just closed or reset,
/// as a refused forward does).
async fn get(mut stream: TcpStream) -> String {
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut response = String::new();
    let read = tokio::time::timeout(
        Duration::from_secs(10),
        stream.read_to_string(&mut response),
    )
    .await
    .expect("an answer in time");
    if read.is_err() {
        return String::new();
    }
    response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_owned())
        .unwrap_or_default()
}

async fn get_at(port: u16) -> String {
    get(TcpStream::connect(("127.0.0.1", port)).await.unwrap()).await
}

/// `curl` with `args`, when it is installed; its output.
async fn curl(args: &[String]) -> Option<String> {
    let mut command = tokio::process::Command::new("curl");
    command
        .args(["--silent", "--show-error", "--max-time", "10"])
        .args(args);
    let output = command.output().await.ok()?;
    assert!(
        output.status.success(),
        "curl {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Some(String::from_utf8(output.stdout).unwrap())
}

/// What a forward reported.
#[derive(Clone, Default)]
struct Reports(Arc<Mutex<Vec<Report>>>);

impl Reports {
    fn sink(&self) -> ReportSink {
        let reports = Arc::clone(&self.0);
        Arc::new(move |report| reports.lock().unwrap().push(report))
    }

    async fn until(&self, what: &str, wanted: impl Fn(&Report) -> bool) -> Report {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let last = self.0.lock().unwrap().last().cloned();
            if let Some(report) = last.filter(|report| wanted(report)) {
                return report;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("no {what}: {:?}", self.0.lock().unwrap());
    }

    async fn listening(&self) -> u16 {
        match self
            .until("listening", |report| matches!(report, Report::Listening(_)))
            .await
        {
            Report::Listening(port) => port,
            _ => unreachable!(),
        }
    }
}

fn run(forward: Forward, connections: tunnel::Connections) -> (Running, Reports) {
    let reports = Reports::default();
    let running = tunnel::start(
        &tokio::runtime::Handle::current(),
        forward,
        connections,
        reports.sink(),
    );
    (running, reports)
}

async fn until_closed(running: &Running) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while running.traffic().counts().open > 0 {
        assert!(Instant::now() < deadline, "connections stayed open");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn each_kind_carries_http() {
    let known = tempfile::tempdir().unwrap();
    let web = http().await;
    let ssh = server(true).await;
    let (_sender, connections) = watch::channel(Some(connection(ssh, known.path()).await));

    // Local: a port here reaches the web server from the server.
    let (local, reports) = run(
        Forward::Local {
            bind: Endpoint::new("127.0.0.1", 0),
            to: Endpoint::new("127.0.0.1", web),
        },
        connections.clone(),
    );
    let port = reports.listening().await;
    assert_eq!(get_at(port).await, BODY);
    if let Some(body) = curl(&[format!("http://127.0.0.1:{port}/")]).await {
        assert_eq!(body, BODY);
    }
    until_closed(&local).await;
    let counts = local.traffic().counts();
    assert!(
        counts.total >= 1 && counts.sent > 0 && counts.received > 0,
        "{counts:?}"
    );

    // Dynamic: a SOCKS5 proxy here.
    let (dynamic, reports) = run(
        Forward::Dynamic {
            bind: Endpoint::new("127.0.0.1", 0),
        },
        connections.clone(),
    );
    let socks = reports.listening().await;
    let mut stream = TcpStream::connect(("127.0.0.1", socks)).await.unwrap();
    proxy::socks5(&mut stream, "127.0.0.1", web, None)
        .await
        .unwrap();
    assert_eq!(get(stream).await, BODY);
    if let Some(body) = curl(&[
        "--socks5-hostname".to_owned(),
        format!("127.0.0.1:{socks}"),
        format!("http://localhost:{web}/"),
    ])
    .await
    {
        assert_eq!(body, BODY);
    }
    assert!(dynamic.traffic().counts().total >= 1);

    // Remote: the server listens (here, as the test server runs here) and comes back to us.
    let (remote, reports) = run(
        Forward::Remote {
            bind: Endpoint::new("127.0.0.1", 0),
            to: Endpoint::new("127.0.0.1", web),
        },
        connections.clone(),
    );
    let port = reports.listening().await;
    assert_eq!(get_at(port).await, BODY);
    if let Some(body) = curl(&[format!("http://127.0.0.1:{port}/")]).await {
        assert_eq!(body, BODY);
    }
    until_closed(&remote).await;
    assert!(remote.traffic().counts().received > 0);

    // Stopped: the ports are free again, and the server stops listening.
    drop(local);
    remote.stop();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(TcpStream::connect(("127.0.0.1", port)).await.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stopped_forward_frees_its_port() {
    let known = tempfile::tempdir().unwrap();
    let web = http().await;
    let ssh = server(true).await;
    let (_sender, connections) = watch::channel(Some(connection(ssh, known.path()).await));
    let (running, reports) = run(
        Forward::Local {
            bind: Endpoint::new("127.0.0.1", 0),
            to: Endpoint::new("127.0.0.1", web),
        },
        connections.clone(),
    );
    let port = reports.listening().await;
    running.stop();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let again = TcpListener::bind(("127.0.0.1", port)).await;
    assert!(again.is_ok(), "the port stayed taken");
    drop(again);

    // The same port twice: the second can't listen.
    let (_first, reports) = run(
        Forward::Local {
            bind: Endpoint::new("127.0.0.1", port),
            to: Endpoint::new("127.0.0.1", web),
        },
        connections.clone(),
    );
    reports.listening().await;
    let (_second, reports) = run(
        Forward::Dynamic {
            bind: Endpoint::new("127.0.0.1", port),
        },
        connections,
    );
    let failed = reports
        .until("a failure", |report| matches!(report, Report::Failed(_)))
        .await;
    assert!(matches!(failed, Report::Failed(message) if message.contains("can't listen")));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_that_forbids_forwarding() {
    let known = tempfile::tempdir().unwrap();
    let ssh = server(false).await;
    let (_sender, connections) = watch::channel(Some(connection(ssh, known.path()).await));
    let (_remote, reports) = run(
        Forward::Remote {
            bind: Endpoint::new("127.0.0.1", 0),
            to: Endpoint::new("127.0.0.1", 9),
        },
        connections.clone(),
    );
    let failed = reports
        .until("a refusal", |report| matches!(report, Report::Failed(_)))
        .await;
    assert!(matches!(failed, Report::Failed(message) if message.contains("refused")));

    // A local forward listens, but the server refuses each channel: the connection just ends.
    let (_local, reports) = run(
        Forward::Local {
            bind: Endpoint::new("127.0.0.1", 0),
            to: Endpoint::new("127.0.0.1", 9),
        },
        connections,
    );
    let port = reports.listening().await;
    assert_eq!(get_at(port).await, "");
}

#[tokio::test(flavor = "multi_thread")]
async fn forwards_carry_on_over_a_new_connection() {
    let known = tempfile::tempdir().unwrap();
    let web = http().await;
    let ssh = server(true).await;
    let first = connection(ssh, known.path()).await;
    let (sender, connections) = watch::channel(Some(Arc::clone(&first)));
    let (_local, local_reports) = run(
        Forward::Local {
            bind: Endpoint::new("127.0.0.1", 0),
            to: Endpoint::new("127.0.0.1", web),
        },
        connections.clone(),
    );
    let local_port = local_reports.listening().await;
    // A fixed port for the remote forward, so it comes back at the same place.
    let fixed = TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let (_remote, remote_reports) = run(
        Forward::Remote {
            bind: Endpoint::new("127.0.0.1", fixed),
            to: Endpoint::new("127.0.0.1", web),
        },
        connections,
    );
    assert_eq!(remote_reports.listening().await, fixed);
    assert_eq!(get_at(local_port).await, BODY);
    assert_eq!(get_at(fixed).await, BODY);

    // The connection goes away: the remote forward waits, the local one keeps its port.
    sender.send_replace(None);
    first.close().await;
    remote_reports
        .until("waiting", |report| *report == Report::Waiting)
        .await;
    // The server lets go of the port once it notices (in the background, so on a busy machine
    // it can take a moment).
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while TcpStream::connect(("127.0.0.1", fixed)).await.is_ok() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the server still listens on the remote forward's port"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // A request made meanwhile waits for the new connection.
    let pending = tokio::spawn(get_at(local_port));
    tokio::time::sleep(Duration::from_millis(300)).await;
    sender.send_replace(Some(connection(ssh, known.path()).await));
    assert_eq!(pending.await.unwrap(), BODY);
    assert_eq!(remote_reports.listening().await, fixed);
    assert_eq!(get_at(fixed).await, BODY);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_independent_tunnel_reconnects_by_itself() {
    let known = tempfile::tempdir().unwrap();
    let web = http().await;
    let ssh = server(true).await;
    let path = known.path().to_path_buf();
    let connector: Connector = Arc::new(move || {
        let spec = spec(ssh, PASSWORD, &path);
        Box::pin(async move { connect::connect(&spec, &trust(), &connect::quiet()).await })
    });
    let links = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&links);
    let kept = tunnel::keep_connected(
        &tokio::runtime::Handle::current(),
        connector,
        true,
        Arc::new(move |link| log.lock().unwrap().push(link)),
    );
    let (_local, reports) = run(
        Forward::Local {
            bind: Endpoint::new("127.0.0.1", 0),
            to: Endpoint::new("127.0.0.1", web),
        },
        kept.connections.clone(),
    );
    let port = reports.listening().await;
    assert_eq!(get_at(port).await, BODY);

    // The connection drops (here from this side; the real-server test kills the server's).
    let current = kept.connections.borrow().clone().unwrap();
    current.close().await;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let reconnected = {
            let links = links.lock().unwrap();
            links
                .iter()
                .skip_while(|link| !matches!(link, Link::Retrying { in_secs: 1, .. }))
                .any(|link| *link == Link::Connected)
        };
        if reconnected {
            break;
        }
        assert!(Instant::now() < deadline, "{:?}", links.lock().unwrap());
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(get_at(port).await, BODY);
    kept.stop();

    // A wrong password needs the user: no retry.
    let path = known.path().to_path_buf();
    let wrong: Connector = Arc::new(move || {
        let spec = spec(ssh, "not the password", &path);
        Box::pin(async move {
            // Trusts the key, but has no other password to give.
            let asker: Asker = Arc::new(|question: Question| {
                let answer = match question.prompt {
                    Prompt::HostKey(_) => Answer::TrustOnce,
                    _ => Answer::Cancel,
                };
                question.answer(answer);
            });
            connect::connect(&spec, &asker, &connect::quiet()).await
        })
    });
    let links = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&links);
    let _kept = tunnel::keep_connected(
        &tokio::runtime::Handle::current(),
        wrong,
        true,
        Arc::new(move |link| log.lock().unwrap().push(link)),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let ended = links
            .lock()
            .unwrap()
            .iter()
            .find(|link| matches!(link, Link::Ended { .. }))
            .cloned();
        if let Some(Link::Ended { code, .. }) = ended {
            assert!(matches!(code, "auth" | "cancelled"), "{code}");
            break;
        }
        assert!(Instant::now() < deadline, "{:?}", links.lock().unwrap());
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
