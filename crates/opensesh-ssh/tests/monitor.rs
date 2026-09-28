//! The remote monitor against the in-process server (PLAN Sprint 11): readings with rates from
//! the second one on, the monitor ending when it is dropped (the server's loop stops with the
//! channel), the host info, and a server without the command reported as unsupported.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers"
)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use opensesh_ssh::connect::{self, Connection};
use opensesh_ssh::monitor::{self, Monitoring};
use opensesh_ssh::prompt::{Answer, Asker, Request as Question};
use opensesh_ssh::spec::{AuthPlan, ConnectSpec, Hop, KnownHostsFiles};
use opensesh_ssh::testing::{PASSWORD, Rules, USER, serve};
use secrecy::SecretString;

fn spec(port: u16, known: &std::path::Path) -> ConnectSpec {
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
    }
}

async fn connection(known: &std::path::Path, monitor: bool) -> Arc<Connection> {
    let port = serve(Rules {
        password: true,
        monitor,
        ..Rules::default()
    })
    .await
    .unwrap();
    let trust: Asker = Arc::new(|question: Question| question.answer(Answer::TrustOnce));
    Arc::new(
        connect::connect(&spec(port, known), &trust, &connect::quiet())
            .await
            .unwrap(),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn readings_arrive_with_rates_from_the_second_on() {
    let known = tempfile::tempdir().unwrap();
    let connection = connection(known.path(), true).await;
    let events: Arc<Mutex<Vec<Monitoring>>> = Arc::default();
    let seen = Arc::clone(&events);
    let watching = {
        let connection = Arc::clone(&connection);
        tokio::spawn(async move {
            monitor::watch(&connection, Duration::from_secs(1), |event| {
                seen.lock().unwrap().push(event);
            })
            .await;
        })
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while events.lock().unwrap().len() < 2 {
        assert!(tokio::time::Instant::now() < deadline, "no second reading");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let readings: Vec<_> = events
        .lock()
        .unwrap()
        .iter()
        .map(|event| match event {
            Monitoring::Reading(snapshot) => snapshot.clone(),
            Monitoring::Unsupported(why) => panic!("unsupported: {why}"),
        })
        .collect();
    let (first, second) = (&readings[0], &readings[1]);
    assert_eq!(first.cpu_permille, None);
    let memory = first.memory.unwrap();
    assert_eq!(memory.total, 7_812_500 * 1024);
    assert_eq!(first.root_disk().unwrap().mount, "/");
    assert_eq!(first.uptime_secs, Some(1_036_800));
    assert_eq!(first.load_hundredths, Some([12, 25, 50]));
    assert_eq!(first.users[0].from, "127.0.0.1");
    // 12 busy ticks of 100 between readings, and bytes over about a second.
    assert_eq!(second.cpu_permille, Some(120));
    let received = second.received_per_sec.unwrap();
    assert!((600_000..=2_500_000).contains(&received), "{received}");
    // Dropping the monitor closes its channel, and the server's loop stops.
    watching.abort();
    let _ = watching.await;
    let count = events.lock().unwrap().len();
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(events.lock().unwrap().len(), count);
    // The connection is still usable.
    let info = monitor::host_info(&connection).await.unwrap();
    assert_eq!(info.os_name, "Debian GNU/Linux 13 (trixie)");
    assert_eq!(info.hostname, "test-server");
    assert_eq!(info.cpus, Some(4));
    assert_eq!(info.addresses.len(), 1);
    assert_eq!(info.addresses[0].address, "10.0.0.5/24");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_without_sh_is_unsupported_once() {
    let known = tempfile::tempdir().unwrap();
    // It answers every command with "command not found", as a server without `sh` would.
    let connection = connection(known.path(), false).await;
    let events: Arc<Mutex<Vec<Monitoring>>> = Arc::default();
    let seen = Arc::clone(&events);
    monitor::watch(&connection, Duration::from_secs(1), |event| {
        seen.lock().unwrap().push(event);
    })
    .await;
    assert_eq!(
        *events.lock().unwrap(),
        [Monitoring::Unsupported("sh: command not found".to_owned())]
    );
    assert_eq!(
        monitor::host_info(&connection).await.unwrap_err(),
        "sh: command not found"
    );
}

/// What one reading costs (docs/perf.md): the real command's loop run 100 times with no pause
/// by `$MONITOR_SHELL` (`sh` by default; the programs it runs are found on `PATH`, so a folder of
/// busybox's applets measures busybox), with the shell's `times` for the CPU used; then 10,000
/// parses of one of its readings.
#[cfg(unix)]
#[test]
#[ignore = "a measurement, run by hand"]
fn measure_the_monitor() {
    use std::time::Instant;
    let command = monitor::command(Duration::from_secs(1));
    let script = command
        .strip_prefix("sh -c '")
        .and_then(|rest| rest.strip_suffix('\''))
        .unwrap()
        .replace("while :; do", "i=0; while [ $i -lt 100 ]; do i=$((i+1));")
        .replace("sleep $n;", "")
        + "; times";
    let shell = std::env::var("MONITOR_SHELL").unwrap_or_else(|_| "sh".to_owned());
    let started = Instant::now();
    let output = std::process::Command::new(&shell)
        .arg("-c")
        .arg(&script)
        .output()
        .unwrap();
    let elapsed = started.elapsed();
    let text = String::from_utf8_lossy(&output.stdout);
    let times: Vec<&str> = text.lines().rev().take(2).collect();
    println!(
        "{shell}: 100 readings in {elapsed:?} wall; `times` (children, then the shell): {times:?}"
    );
    let end = text.find("@end\n").unwrap() + 5;
    let reading = &text[..end];
    assert!(!monitor::parse(reading).is_empty());
    let started = Instant::now();
    for _ in 0..10_000 {
        std::hint::black_box(monitor::parse(std::hint::black_box(reading)));
    }
    println!(
        "a parse of a {}-byte reading: {:?}",
        reading.len(),
        started.elapsed() / 10_000
    );
}
