//! Tunnels through a real OpenSSH server (PLAN Sprint 9, "done when"): `curl` through a local, a
//! remote and a dynamic (SOCKS5) forward to an HTTP server here, and an independent tunnel that
//! comes back by itself after the server kills its session.
//!
//! The servers come from `scripts/ssh-test-servers.sh start` (OpenSSH on 127.0.0.1:2221, with
//! `AllowTcpForwarding yes`); `curl` must be installed. Ignored by default:
//!
//! ```sh
//! OPENSESH_SSH_SERVERS=/tmp/opensesh-ssh-servers cargo test -p opensesh-ssh --test real_tunnels -- --ignored
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers"
)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use opensesh_ssh::connect::{self, Connection};
use opensesh_ssh::prompt::{Answer, Asker, Request as Question};
use opensesh_ssh::spec::{AuthPlan, ConnectSpec, Hop, KnownHostsFiles};
use opensesh_ssh::tunnel::{self, Connector, Endpoint, Forward, Link, Report, ReportSink, Running};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;

const USER: &str = "opensesh-test";
const OPENSSH: u16 = 2221;
const BODY: &str = "hello through a real tunnel";

fn state() -> PathBuf {
    std::env::var_os("OPENSESH_SSH_SERVERS")
        .map_or_else(|| PathBuf::from("/tmp/opensesh-ssh-servers"), PathBuf::from)
}

fn spec(known: &Path) -> ConnectSpec {
    ConnectSpec {
        hops: vec![Hop {
            host: "127.0.0.1".into(),
            port: OPENSSH,
            user: USER.into(),
            auth: AuthPlan {
                key_files: vec![state().join("client_ed25519")],
                ..AuthPlan::default()
            },
        }],
        proxy: None,
        legacy: false,
        compression: false,
        keepalive: Some(Duration::from_secs(5)),
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

/// A tiny HTTP server here: every request gets [`BODY`].
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

/// `curl` (which must be installed) with `args`; its output.
async fn curl(args: &[String]) -> String {
    let output = tokio::process::Command::new("curl")
        .args(["--silent", "--show-error", "--max-time", "15"])
        .args(args)
        .output()
        .await
        .expect("curl (the real-server tests need it)");
    assert!(
        output.status.success(),
        "curl {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[derive(Clone, Default)]
struct Reports(Arc<Mutex<Vec<Report>>>);

impl Reports {
    fn sink(&self) -> ReportSink {
        let reports = Arc::clone(&self.0);
        Arc::new(move |report| reports.lock().unwrap().push(report))
    }

    /// The port of the latest `Listening` after `skip` of them.
    async fn listening(&self, skip: usize) -> u16 {
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            let ports: Vec<u16> = self
                .0
                .lock()
                .unwrap()
                .iter()
                .filter_map(|report| match report {
                    Report::Listening(port) => Some(*port),
                    _ => None,
                })
                .collect();
            if let Some(port) = ports.get(skip) {
                return *port;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("not listening: {:?}", self.0.lock().unwrap());
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

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs scripts/ssh-test-servers.sh start"]
async fn curl_through_each_kind_of_tunnel() {
    let known = tempfile::tempdir().unwrap();
    let web = http().await;
    let connection: Arc<Connection> = Arc::new(
        connect::connect(&spec(known.path()), &trust(), &connect::quiet())
            .await
            .unwrap(),
    );
    let (_sender, connections) = watch::channel(Some(Arc::clone(&connection)));

    let (local, reports) = run(
        Forward::Local {
            bind: Endpoint::new("127.0.0.1", 0),
            to: Endpoint::new("127.0.0.1", web),
        },
        connections.clone(),
    );
    let port = reports.listening(0).await;
    assert_eq!(curl(&[format!("http://127.0.0.1:{port}/")]).await, BODY);

    let (dynamic, reports) = run(
        Forward::Dynamic {
            bind: Endpoint::new("127.0.0.1", 0),
        },
        connections.clone(),
    );
    let socks = reports.listening(0).await;
    // The name is resolved by the server (socks5h).
    assert_eq!(
        curl(&[
            "--socks5-hostname".to_owned(),
            format!("127.0.0.1:{socks}"),
            format!("http://localhost:{web}/"),
        ])
        .await,
        BODY
    );

    // sshd listens on its loopback (GatewayPorts no) and comes back here.
    let (remote, reports) = run(
        Forward::Remote {
            bind: Endpoint::new("127.0.0.1", 0),
            to: Endpoint::new("127.0.0.1", web),
        },
        connections,
    );
    let port = reports.listening(0).await;
    assert_eq!(curl(&[format!("http://127.0.0.1:{port}/")]).await, BODY);

    for running in [&local, &dynamic, &remote] {
        let counts = running.traffic().counts();
        assert!(counts.total >= 1 && counts.received > 0, "{counts:?}");
    }
    connection.close().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs scripts/ssh-test-servers.sh start"]
async fn a_tunnel_comes_back_after_its_session_is_killed() {
    let known = tempfile::tempdir().unwrap();
    let web = http().await;
    let path = known.path().to_path_buf();
    let connector: Connector = Arc::new(move || {
        let spec = spec(&path);
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
    let (_local, local_reports) = run(
        Forward::Local {
            bind: Endpoint::new("127.0.0.1", 0),
            to: Endpoint::new("127.0.0.1", web),
        },
        kept.connections.clone(),
    );
    let (_remote, remote_reports) = run(
        Forward::Remote {
            bind: Endpoint::new("127.0.0.1", 0),
            to: Endpoint::new("127.0.0.1", web),
        },
        kept.connections.clone(),
    );
    let local = local_reports.listening(0).await;
    let remote = remote_reports.listening(0).await;
    assert_eq!(curl(&[format!("http://127.0.0.1:{local}/")]).await, BODY);
    assert_eq!(curl(&[format!("http://127.0.0.1:{remote}/")]).await, BODY);

    // The server's side of the session dies (its sshd-session process).
    let connection = kept.connections.borrow().clone().unwrap();
    let channel = connection
        .target()
        .unwrap()
        .channel_open_session()
        .await
        .unwrap();
    channel.exec(false, "kill -9 $PPID").await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let back = {
            let links = links.lock().unwrap();
            links
                .iter()
                .skip_while(|link| !matches!(link, Link::Retrying { .. }))
                .any(|link| *link == Link::Connected)
        };
        if back {
            break;
        }
        assert!(Instant::now() < deadline, "{:?}", links.lock().unwrap());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // The local port stayed; the remote forward was asked for again (a new port).
    assert_eq!(curl(&[format!("http://127.0.0.1:{local}/")]).await, BODY);
    let remote = remote_reports.listening(1).await;
    assert_eq!(curl(&[format!("http://127.0.0.1:{remote}/")]).await, BODY);
    eprintln!("reconnected: {:?}", links.lock().unwrap());
    kept.stop();
}
