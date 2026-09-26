//! Engine performance probes (opt-in; the numbers in `docs/perf.md`).
//!
//! `cargo test --release -p opensesh-term --test perf -- --ignored --nocapture --test-threads=1`
//!
//! Run one probe with `--exact <name>`; `memory_idle_session_and_full_scrollback` must run alone
//! (in its own process), or earlier probes' allocations skew it.
//!
//! The PTY probes print, next to our numbers, a floor for the same program on the same PTY:
//! the local backend with no engine (output read and dropped), and on Linux `script(1)` (the
//! kernel PTY with no terminal at all). A renderer-like thread snapshots every 16 ms, as a
//! 60 Hz renderer would, and the probes print how long each snapshot took (lock wait included).

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

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use opensesh_term::backend::{self, BackendEvent, TermSize};
use opensesh_term::pty;
use opensesh_term::session::{Session, SessionConfig};
use opensesh_term::shell::ShellCommand;
use opensesh_term::snapshot::Frame;

use common::{CTRL_C, Terminal, attach, test_size};

/// The longest a PTY probe may take (the Windows 10 inbox console host needs about 80 s for the
/// 100 MB file).
const PTY_LIMIT: Duration = Duration::from_secs(600);

/// How long the `yes` probes run.
const YES_TIME: Duration = Duration::from_secs(10);

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

/// Plain ASCII lines of 79 characters and LF, like a log file.
fn plain_corpus(bytes: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes + 128);
    let mut line = 0_u64;
    while out.len() < bytes {
        let text = format!(
            "{line:>10} the quick brown fox jumps over the lazy dog 0123456789 abcdefghijklm\n"
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

fn snapshot_summary(waits: &[Duration]) -> String {
    let percentile = |p: f64| {
        let index = ((waits.len() as f64 - 1.0) * p).round() as usize;
        waits.get(index).copied().unwrap_or_default()
    };
    format!(
        "snapshot p50 {:?} p99 {:?} max {:?} ({} frames)",
        percentile(0.5),
        percentile(0.99),
        waits.last().copied().unwrap_or_default(),
        waits.len()
    )
}

fn megabytes(bytes: u64) -> f64 {
    bytes as f64 / 1e6
}

fn report(what: &str, bytes: usize, elapsed: Duration, waits: &[Duration]) {
    eprintln!(
        "{what}: {:.1} MB in {elapsed:.2?} = {:.1} MB/s; {}",
        bytes as f64 / 1e6,
        bytes as f64 / 1e6 / elapsed.as_secs_f64(),
        snapshot_summary(waits)
    );
}

/// Forwards backend events through a bounded channel (like the PTY backend's own), counting the
/// output bytes the engine receives.
fn count_output(events: Receiver<BackendEvent>) -> (Receiver<BackendEvent>, Arc<AtomicU64>) {
    let (sender, receiver) = crossbeam_channel::bounded(16);
    let bytes = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&bytes);
    std::thread::spawn(move || {
        for event in events {
            if let BackendEvent::Output(chunk) = &event {
                counter.fetch_add(chunk.len() as u64, Ordering::Relaxed);
            }
            if sender.send(event).is_err() {
                break;
            }
        }
    });
    (receiver, bytes)
}

/// Starts `command` on the PTY behind a session, counting the bytes the engine receives.
fn start_counted(command: ShellCommand, columns: u16, lines: u16) -> (Terminal, Arc<AtomicU64>) {
    let size = test_size(columns, lines);
    let (backend, events) = pty::spawn(command, size).unwrap();
    let (events, bytes) = count_output(events);
    (attach(backend, events, size), bytes)
}

/// Runs `command` on the PTY with no engine: output is read and dropped. The console host's
/// opening queries are answered as the engine would (the cursor position; the bundled host also
/// asks for the device attributes and waits 3 s without an answer). Stops at the exit, or after
/// `limit` if given. Returns the output bytes and the time from the first byte.
fn backend_floor(command: ShellCommand, limit: Option<Duration>) -> (u64, Duration) {
    let (backend, events) = pty::spawn(command, TermSize::new(200, 60)).unwrap();
    let mut total = 0_u64;
    let mut first: Option<Instant> = None;
    let deadline = Instant::now() + PTY_LIMIT;
    loop {
        if let (Some(first), Some(limit)) = (first, limit) {
            if first.elapsed() >= limit {
                break;
            }
        }
        let left = deadline.saturating_duration_since(Instant::now());
        match events.recv_timeout(left.min(Duration::from_millis(100))) {
            Ok(BackendEvent::Output(chunk)) => {
                first.get_or_insert_with(Instant::now);
                // What the engine would answer, in the same order.
                if chunk.windows(4).any(|window| window == b"\x1b[6n") {
                    backend.write(b"\x1b[1;1R").unwrap();
                }
                if chunk.windows(3).any(|window| window == b"\x1b[c") {
                    backend.write(b"\x1b[?6c").unwrap();
                }
                total += chunk.len() as u64;
            }
            Ok(BackendEvent::Exited(_)) => break,
            Ok(BackendEvent::Error(error)) => panic!("{error}"),
            Err(_) => assert!(Instant::now() < deadline, "the program never ended"),
        }
    }
    let elapsed = first.map(|first| first.elapsed()).unwrap_or_default();
    backend.shutdown();
    (total, elapsed)
}

/// `cat` of `file`: `/bin/cat` on Unix. On Windows, `cmd /c type` (it writes line by line) and
/// a PowerShell stream copy (64 KiB block writes, like `cat`).
fn cat_commands(file: &Path) -> Vec<(&'static str, ShellCommand)> {
    if cfg!(windows) {
        let copy = format!(
            "$in = [IO.File]::OpenRead('{}'); $out = [Console]::OpenStandardOutput(); \
             $in.CopyTo($out, 65536); $out.Flush()",
            file.display()
        );
        vec![
            (
                "cmd /c type",
                ShellCommand::program("cmd.exe")
                    .arg("/c")
                    .arg("type")
                    .arg(file),
            ),
            ("PowerShell stream copy", powershell(&copy)),
        ]
    } else {
        vec![("cat", ShellCommand::program("/bin/cat").arg(file))]
    }
}

/// `yes`: the real one on Unix; on Windows a PowerShell loop writing 12 KiB blocks of `y` lines
/// (GNU `yes` writes 8 KiB blocks).
fn yes_command() -> ShellCommand {
    if cfg!(windows) {
        powershell(
            "$b = ('y' + [char]13 + [char]10) * 4096; while ($true) { [Console]::Out.Write($b) }",
        )
    } else {
        ShellCommand::program("yes")
    }
}

fn powershell(script: &str) -> ShellCommand {
    ShellCommand::program("powershell.exe")
        .arg("-NoLogo")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(script)
}

/// Whether `script(1)` from util-linux is installed (not by default on Fedora).
fn has_script() -> bool {
    cfg!(unix) && common::find_program("script").is_some()
}

#[test]
#[ignore = "performance probe; run in release with --ignored --nocapture"]
fn engine_parse_throughput_with_a_renderer() {
    for (name, mut data) in [
        (
            "mixed SGR, 256 colors, truecolor, CJK, CR LF",
            corpus(64 * 1024 * 1024),
        ),
        (
            "plain ASCII, 79 columns, LF",
            plain_corpus(64 * 1024 * 1024),
        ),
    ] {
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
        report(
            &format!("engine (replay, 200x60, {name})"),
            bytes,
            elapsed,
            &waits,
        );
    }
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
    // Only the cursor row is damaged.
    let started = Instant::now();
    for _ in 0..rounds {
        session.snapshot(&mut frame);
    }
    eprintln!(
        "200x60 snapshot with nothing damaged (the cursor row only): mean {:?}",
        started.elapsed() / rounds
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

/// Full 200x60 snapshots of log-like text, plain, with keyword highlighting (the built-in log,
/// network, status and path sets: 12 rules) and with a minimum contrast (PLAN §6.2, §6.5).
#[test]
#[ignore = "performance probe; run in release with --ignored --nocapture"]
fn highlighting_and_minimum_contrast_cost() {
    let mut data = Vec::new();
    for row in 0..60 {
        let level = ["INFO", "WARN", "ERROR", "DEBUG"][row % 4];
        let line = format!(
            "2026-09-26 12:{:02}:{:02} {level} worker-{row} connected from 10.0.{}.{} to \
             /var/lib/app/data/{row}.db status OK after {} ms; see https://example.com/runs/{row}",
            row % 60,
            (row * 7) % 60,
            row % 256,
            (row * 3) % 256,
            row * 13
        );
        data.extend_from_slice(
            format!("\x1b[2m{:<200}\x1b[0m", &line[..line.len().min(200)]).as_bytes(),
        );
    }
    data.extend_from_slice(b"\x1b[0mEND");
    let (backend, events) = backend::replay(data);
    let session = Session::start(
        backend,
        events,
        SessionConfig {
            size: TermSize::new(200, 61),
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
    let measure = |label: &str, palette: opensesh_term::palette::Palette, frame: &mut Frame| {
        let mut total = Duration::ZERO;
        let mut worst = Duration::ZERO;
        for _ in 0..rounds {
            // A palette change forces a full frame.
            session.set_palette(palette.clone());
            let started = Instant::now();
            session.snapshot(frame);
            let took = started.elapsed();
            total += took;
            worst = worst.max(took);
        }
        eprintln!(
            "full 200x61 snapshot of log lines, {label}: mean {:?}, max {:?}",
            total / rounds,
            worst
        );
    };
    let dark = opensesh_term::palette::Palette::OPENSESH_DARK;
    measure("plain", dark.clone(), &mut frame);
    let sets = opensesh_core::terminal::highlight::builtin_sets();
    let highlighter = Arc::new(opensesh_term::highlight::Highlighter::new(&sets));
    session.set_highlighter(Some(Arc::clone(&highlighter)));
    measure(
        "with the 4 built-in highlight sets",
        dark.clone(),
        &mut frame,
    );
    session.set_highlighter(None);
    let contrast = opensesh_term::palette::Palette {
        minimum_contrast: 4.5,
        ..dark.clone()
    };
    measure(
        "with a 4.5:1 minimum contrast (dim text)",
        contrast.clone(),
        &mut frame,
    );
    session.set_highlighter(Some(highlighter));
    measure("with both", contrast, &mut frame);
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

/// `cat` of a 100 MiB file through the real PTY into the engine, with a renderer probe, then
/// the floors: the backend with no engine, and `script(1)` on Linux.
#[test]
#[ignore = "performance probe; run in release with --ignored --nocapture"]
fn pty_cat_throughput_with_a_renderer() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("corpus.txt");
    let data = corpus(100 * 1024 * 1024);
    std::fs::write(&file, &data).unwrap();
    let size = data.len() as u64;
    drop(data);
    for (name, command) in cat_commands(&file) {
        let (terminal, bytes) = start_counted(command.clone(), 200, 60);
        let started = Instant::now();
        let stop = Arc::new(AtomicBool::new(false));
        let probe = render_probe(terminal.session.clone(), Arc::clone(&stop));
        let code = terminal.wait_exit_within(PTY_LIMIT);
        let elapsed = started.elapsed();
        stop.store(true, Ordering::Release);
        let waits = probe.join().unwrap();
        assert_eq!(code, Some(0), "{}", terminal.screen());
        eprintln!(
            "PTY {name} (200x60): {:.1} MB file, {:.1} MB from the PTY, in {elapsed:.2?} = {:.1} MB/s; {}",
            megabytes(size),
            megabytes(bytes.load(Ordering::Relaxed)),
            megabytes(size) / elapsed.as_secs_f64(),
            snapshot_summary(&waits)
        );
        drop(terminal);
        let (floor_bytes, floor_time) = backend_floor(command, None);
        eprintln!(
            "  floor, PTY backend with no engine: {:.1} MB from the PTY in {floor_time:.2?} = {:.1} MB/s of file",
            megabytes(floor_bytes),
            megabytes(size) / floor_time.as_secs_f64()
        );
    }
    if has_script() {
        let started = Instant::now();
        let status = std::process::Command::new("script")
            .arg("-qfec")
            .arg(format!("cat '{}'", file.display()))
            .arg("/dev/null")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        let elapsed = started.elapsed();
        assert!(status.success());
        eprintln!(
            "  floor, script(1) with output to /dev/null (kernel PTY, no terminal): {elapsed:.2?} = {:.1} MB/s",
            megabytes(size) / elapsed.as_secs_f64()
        );
    }
}

/// `yes` for 10 s through the real PTY into the engine, with a renderer probe; then Ctrl+C
/// (through the key encoder) and the time until the program is gone. Then the floors.
#[test]
#[ignore = "performance probe; run in release with --ignored --nocapture"]
fn pty_yes_for_ten_seconds() {
    let (terminal, bytes) = start_counted(yes_command(), 200, 60);
    common::eventually("the first output", || bytes.load(Ordering::Relaxed) > 0);
    let stop = Arc::new(AtomicBool::new(false));
    let probe = render_probe(terminal.session.clone(), Arc::clone(&stop));
    let started = Instant::now();
    std::thread::sleep(YES_TIME);
    let delivered = bytes.load(Ordering::Relaxed);
    let elapsed = started.elapsed();
    let pressed = Instant::now();
    terminal.press(CTRL_C);
    let code = terminal.wait_exit();
    let to_exit = pressed.elapsed();
    stop.store(true, Ordering::Release);
    let waits = probe.join().unwrap();
    eprintln!(
        "PTY yes (200x60): {:.1} MB in {elapsed:.2?} = {:.1} MB/s; Ctrl+C to exit {to_exit:.1?} (code {code:?}); {}",
        megabytes(delivered),
        megabytes(delivered) / elapsed.as_secs_f64(),
        snapshot_summary(&waits)
    );
    drop(terminal);
    let (floor_bytes, floor_time) = backend_floor(yes_command(), Some(YES_TIME));
    eprintln!(
        "  floor, PTY backend with no engine: {:.1} MB in {floor_time:.2?} = {:.1} MB/s",
        megabytes(floor_bytes),
        megabytes(floor_bytes) / floor_time.as_secs_f64()
    );
    if has_script() {
        use std::io::Read;
        let mut child = std::process::Command::new("script")
            .arg("-qfc")
            .arg("yes")
            .arg("/dev/null")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdout = child.stdout.take().unwrap();
        let mut buffer = vec![0; 64 * 1024];
        let mut total = 0_u64;
        let started = Instant::now();
        while started.elapsed() < YES_TIME {
            let read = stdout.read(&mut buffer).unwrap();
            if read == 0 {
                break;
            }
            total += read as u64;
        }
        let elapsed = started.elapsed();
        child.kill().unwrap();
        let _ = child.wait();
        eprintln!(
            "  floor, script(1) read by this process (kernel PTY, no terminal): {:.1} MB in {elapsed:.2?} = {:.1} MB/s",
            megabytes(total),
            megabytes(total) / elapsed.as_secs_f64()
        );
    }
}

/// This process's memory: Linux `VmRSS`, `VmHWM` and `RssAnon`; Windows working set, peak
/// working set and private bytes.
fn memory() -> String {
    if cfg!(windows) {
        let output = std::process::Command::new("powershell.exe")
            .args([
                "-NoLogo",
                "-NoProfile",
                "-Command",
                &format!(
                    "$p = Get-Process -Id {}; '{{0}} {{1}} {{2}}' -f $p.WorkingSet64, $p.PeakWorkingSet64, $p.PrivateMemorySize64",
                    std::process::id()
                ),
            ])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&output.stdout);
        let values: Vec<f64> = text
            .split_whitespace()
            .map(|value| value.parse::<f64>().unwrap() / 1e6)
            .collect();
        format!(
            "working set {:.1} MB, peak {:.1} MB, private {:.1} MB",
            values[0], values[1], values[2]
        )
    } else {
        let status = std::fs::read_to_string("/proc/self/status").unwrap();
        let field = |name: &str| {
            status
                .lines()
                .find_map(|line| line.strip_prefix(name))
                .and_then(|rest| rest.split_whitespace().next())
                .and_then(|kb| kb.parse::<f64>().ok())
                .map_or(f64::NAN, |kb| kb * 1024.0 / 1e6)
        };
        format!(
            "VmRSS {:.1} MB, VmHWM {:.1} MB, RssAnon {:.1} MB",
            field("VmRSS:"),
            field("VmHWM:"),
            field("RssAnon:")
        )
    }
}

/// Memory of one idle shell session and of full 10,000-line scrollbacks. Run it alone:
/// `... --test perf -- --ignored --nocapture --exact memory_idle_session_and_full_scrollback`.
#[test]
#[ignore = "performance probe; run in release with --ignored --nocapture"]
fn memory_idle_session_and_full_scrollback() {
    eprintln!(
        "size_of::<alacritty_terminal::term::cell::Cell>() = {} bytes",
        std::mem::size_of::<alacritty_terminal::term::cell::Cell>()
    );
    eprintln!("before any session: {}", memory());
    let shell = common::start(ShellCommand::user_shell(), 120, 40);
    shell.wait_for("the prompt", |screen| !screen.trim().is_empty());
    shell.settle(Duration::from_secs(3));
    eprintln!(
        "one idle session (user shell, 120x40; the shell's own memory is not counted): {}",
        memory()
    );
    let mut sessions = Vec::new();
    for (columns, lines) in [(120_u16, 40_u16), (200, 60)] {
        let mut data = Vec::new();
        for line in 0..10_100 {
            data.extend_from_slice(format!("{line:>8} ").as_bytes());
            data.extend(std::iter::repeat_n(b'x', usize::from(columns) - 9));
            data.extend_from_slice(b"\r\n");
        }
        data.extend_from_slice(b"END");
        let (backend, events) = backend::replay(data);
        let session = Session::start(
            backend,
            events,
            SessionConfig {
                size: TermSize::new(columns, lines),
                ..SessionConfig::default()
            },
            Arc::new(|_| {}),
        )
        .unwrap();
        while !session.text_dump().contains("END") {
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut frame = Frame::default();
        session.snapshot(&mut frame);
        assert_eq!(frame.history_size, 10_000);
        eprintln!(
            "plus a session with a full 10,000-line scrollback at {columns}x{lines}: {}",
            memory()
        );
        sessions.push(session);
    }
}
