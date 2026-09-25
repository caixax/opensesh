//! Engine performance probes (opt-in; numbers for `docs/perf.md`).
//!
//! `cargo test --release -p opensesh-term --test perf -- --ignored --nocapture --test-threads=1`
//!
//! Each probe prints throughput and how long a renderer-like thread waited for `snapshot`
//! (it snapshots every 16 ms, as a 60 Hz renderer would, while the engine parses).

// A test crate: clippy.toml allows these in `#[test]` functions, and the shared helpers need
// them too.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    clippy::cast_precision_loss
)]

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use opensesh_term::backend::{self, TermSize};
use opensesh_term::session::{Session, SessionConfig};
use opensesh_term::snapshot::Frame;

/// Mixed output: SGR colors, bold, 256 colors and truecolor, CJK, about 90 bytes per line.
fn corpus(bytes: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes + 128);
    let mut line = 0_u64;
    while out.len() < bytes {
        let text = format!(
            "\x1b[3{}m{line:>8}\x1b[0m \x1b[1mbold\x1b[0m \x1b[38;5;{}mindexed\x1b[0m \x1b[38;2;10;{};200mtrue\x1b[0m 界面 plain text here\r\n",
            line % 8,
            line % 256,
            line % 256
        );
        out.extend_from_slice(text.as_bytes());
        line += 1;
    }
    out
}

/// Snapshots every 16 ms until `stop`; returns the sorted snapshot durations.
fn render_probe(session: Session, stop: Arc<AtomicBool>) -> std::thread::JoinHandle<Vec<Duration>> {
    std::thread::spawn(move || {
        let mut frame = Frame::default();
        let mut waits = Vec::new();
        while !stop.load(Ordering::Acquire) {
            let started = Instant::now();
            session.snapshot(&mut frame);
            waits.push(started.elapsed());
            std::thread::sleep(Duration::from_millis(16));
        }
        waits.sort();
        waits
    })
}

fn report(what: &str, bytes: usize, elapsed: Duration, waits: &[Duration]) {
    let percentile = |p: f64| {
        let index = ((waits.len() as f64 - 1.0) * p).round() as usize;
        waits.get(index).copied().unwrap_or_default()
    };
    eprintln!(
        "{what}: {:.1} MB in {elapsed:.2?} = {:.1} MB/s; snapshot p50 {:?} p99 {:?} max {:?} ({} frames)",
        bytes as f64 / 1e6,
        bytes as f64 / 1e6 / elapsed.as_secs_f64(),
        percentile(0.5),
        percentile(0.99),
        waits.last().copied().unwrap_or_default(),
        waits.len()
    );
}

#[test]
#[ignore = "performance probe; run in release with --ignored --nocapture"]
fn engine_parse_throughput_with_a_renderer() {
    let mut data = corpus(64 * 1024 * 1024);
    data.extend_from_slice(b"\r\nEND-OF-PERF-CORPUS");
    let bytes = data.len();
    let (backend, events) = backend::replay(data);
    let session = Session::start(
        backend,
        events,
        SessionConfig {
            size: TermSize::new(200, 60),
            ..SessionConfig::default()
        },
        Arc::new(|_| {}),
    )
    .unwrap();
    let started = Instant::now();
    let stop = Arc::new(AtomicBool::new(false));
    let probe = render_probe(session.clone(), Arc::clone(&stop));
    while !session.text_dump().contains("END-OF-PERF-CORPUS") {
        std::thread::sleep(Duration::from_millis(5));
    }
    let elapsed = started.elapsed();
    stop.store(true, Ordering::Release);
    let waits = probe.join().unwrap();
    report("engine (replay, 200x60)", bytes, elapsed, &waits);
}

#[test]
#[ignore = "performance probe; run in release with --ignored --nocapture"]
fn full_snapshot_cost() {
    let mut data = Vec::new();
    for row in 0..60 {
        for column in 0..200 {
            data.extend_from_slice(
                format!(
                    "\x1b[38;5;{}m\x1b[48;5;{}m{}",
                    (row + column) % 256,
                    column % 256,
                    char::from(b'a' + (column % 26) as u8)
                )
                .as_bytes(),
            );
        }
    }
    data.extend_from_slice(b"\x1b[0mEND");
    let (backend, events) = backend::replay(data);
    let session = Session::start(
        backend,
        events,
        SessionConfig {
            size: TermSize::new(200, 60),
            ..SessionConfig::default()
        },
        Arc::new(|_| {}),
    )
    .unwrap();
    while !session.text_dump().contains("END") {
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut frame = Frame::default();
    let rounds = 200;
    let mut total = Duration::ZERO;
    let mut worst = Duration::ZERO;
    for _ in 0..rounds {
        // A palette change forces a full frame.
        session.set_palette(opensesh_term::palette::Palette::OPENSESH_DARK);
        let started = Instant::now();
        session.snapshot(&mut frame);
        let took = started.elapsed();
        total += took;
        worst = worst.max(took);
        assert_eq!(frame.rows.len(), 60);
    }
    eprintln!(
        "full 200x60 snapshot: mean {:?}, max {:?}",
        total / rounds,
        worst
    );
    // With a search active every frame is full and highlights are computed.
    session.search("a", false).unwrap();
    let started = Instant::now();
    for _ in 0..rounds {
        session.snapshot(&mut frame);
    }
    eprintln!(
        "full 200x60 snapshot with a search matching 1 cell in 26: mean {:?}",
        started.elapsed() / rounds
    );
}

#[test]
#[ignore = "performance probe; run in release with --ignored --nocapture"]
fn search_step_cost() {
    let mut data = Vec::new();
    for line in 0..10_050 {
        data.extend_from_slice(format!("{line:>6} ").as_bytes());
        data.extend(std::iter::repeat_n(b'x', 190));
        data.extend_from_slice(b"\r\n");
    }
    data.extend_from_slice(b"END");
    let (backend, events) = backend::replay(data);
    let config = SessionConfig {
        size: TermSize::new(200, 50),
        ..SessionConfig::default()
    };
    let max_lines = config.search_max_lines;
    let session = Session::start(backend, events, config, Arc::new(|_| {})).unwrap();
    while !session.text_dump().contains("END") {
        std::thread::sleep(Duration::from_millis(5));
    }
    for pattern in ["no such text", "x{5}y", "[0-9]+ z"] {
        let started = Instant::now();
        assert_eq!(session.search(pattern, false).unwrap(), None);
        eprintln!(
            "search step without a match over {max_lines} lines of 200 columns, {pattern:?}: {:?}",
            started.elapsed()
        );
        session.search_clear();
    }
    let started = Instant::now();
    assert!(session.search(" 9042 x", false).unwrap().is_some());
    eprintln!(
        "search step finding a line 1000 lines up: {:?}",
        started.elapsed()
    );
    let started = Instant::now();
    assert!(session.search(" 100 x", false).unwrap().is_some());
    eprintln!(
        "search step finding a line 9950 lines up: {:?}",
        started.elapsed()
    );
}

#[cfg(unix)]
#[test]
#[ignore = "performance probe; run in release with --ignored --nocapture"]
fn pty_cat_throughput_with_a_renderer() {
    use opensesh_term::shell::ShellCommand;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("corpus.txt");
    let data = corpus(100 * 1024 * 1024);
    std::fs::write(&file, &data).unwrap();
    let terminal = common::start(ShellCommand::program("/bin/cat").arg(&file), 200, 60);
    let started = Instant::now();
    let stop = Arc::new(AtomicBool::new(false));
    let probe = render_probe(terminal.session.clone(), Arc::clone(&stop));
    let code = terminal.wait_exit();
    let elapsed = started.elapsed();
    stop.store(true, Ordering::Release);
    let waits = probe.join().unwrap();
    assert_eq!(code, Some(0));
    report("PTY cat (200x60)", data.len(), elapsed, &waits);
}
