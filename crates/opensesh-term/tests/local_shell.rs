//! A real shell on a real PTY through the production `Session` (always run).
//!
//! Windows: `cmd.exe` and `powershell.exe` on ConPTY. Unix: `/bin/sh`.

// A test crate: clippy.toml allows these in `#[test]` functions, and the shared helpers need
// them too.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr
)]

mod common;

use std::time::{Duration, Instant};

use common::{Terminal, eventually, start};
use opensesh_term::session::Notice;
use opensesh_term::shell::ShellCommand;

/// A shell with a short, predictable prompt.
fn shell() -> ShellCommand {
    if cfg!(windows) {
        ShellCommand::program("cmd.exe")
            .arg("/q")
            .env("PROMPT", "$G ")
    } else {
        ShellCommand::program("/bin/sh")
            .env("PS1", "$ ")
            .env("ENV", "")
    }
}

fn prompt() -> &'static str {
    if cfg!(windows) { ">" } else { "$" }
}

fn wait_prompt(terminal: &Terminal) {
    terminal.wait_for("the prompt", |screen| {
        screen.lines().any(|line| line.trim() == prompt())
    });
}

#[test]
fn shell_sees_the_terminal_environment() {
    let terminal = start(shell(), 80, 24);
    wait_prompt(&terminal);
    if cfg!(windows) {
        terminal.send("echo [%TERM_PROGRAM%] [%COLORTERM%] [%TERM%]\r");
    } else {
        terminal.send("echo \"[$TERM_PROGRAM] [$COLORTERM] [$TERM]\"\r");
    }
    terminal.wait_line("[OpenSesh] [truecolor] [xterm-256color]");
    if cfg!(windows) {
        terminal.send("echo [%OPENSESH_NO_CRASH_DIALOG%] [%LINES%]\r");
        // cmd prints unset variables as written.
        terminal.wait_line("[%OPENSESH_NO_CRASH_DIALOG%] [%LINES%]");
    } else {
        // `printenv`, not `$LINES`: bash (Arch's /bin/sh) keeps LINES as a shell variable.
        terminal.send(
            "echo \"[$(printenv OPENSESH_NO_CRASH_DIALOG || echo unset)] [$(printenv LINES || echo unset)]\"\r",
        );
        terminal.wait_line("[unset] [unset]");
    }
}

#[test]
fn marker_output_and_exit_code() {
    let terminal = start(shell(), 80, 24);
    wait_prompt(&terminal);
    // The command line shows the expression, the output shows its value.
    if cfg!(windows) {
        terminal.send("set /a 6*7\r");
    } else {
        terminal.send("echo marker-$((6*7))\r");
    }
    let expected = if cfg!(windows) { "42" } else { "marker-42" };
    terminal.wait_line(expected);
    terminal.send("exit 3\r");
    assert_eq!(terminal.wait_exit(), Some(3));
    // The last screen stays readable after the exit.
    assert!(terminal.screen().contains(expected));
}

#[test]
fn resize_reaches_the_program() {
    let terminal = start(shell(), 80, 24);
    wait_prompt(&terminal);
    terminal.resize(100, 30);
    if cfg!(windows) {
        terminal.send("mode con\r");
        // Localized output: match the values at the end of the lines.
        let ends_with = |screen: &str, value: &str| {
            screen
                .lines()
                .any(|line| line.split_whitespace().last() == Some(value))
        };
        terminal.wait_for("the new size", |screen| {
            ends_with(screen, "100") && ends_with(screen, "30")
        });
    } else {
        terminal.send("stty size\r");
        terminal.wait_line("30 100");
    }
    let frame = terminal.frame();
    assert_eq!((frame.columns, frame.lines), (100, 30));
}

#[test]
fn working_directory_is_used() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("opensesh-cwd-marker");
    std::fs::write(&marker, b"").unwrap();
    let terminal = start(shell().cwd(dir.path()), 120, 24);
    wait_prompt(&terminal);
    if cfg!(windows) {
        terminal.send("dir /b\r");
    } else {
        terminal.send("ls\r");
    }
    terminal.wait_line("opensesh-cwd-marker");
}

#[test]
fn default_shell_starts_and_shuts_down() {
    let terminal = start(ShellCommand::user_shell(), 80, 24);
    terminal.wait_for("any prompt", |screen| !screen.trim().is_empty());
    let frame = terminal.frame();
    assert!(
        frame
            .rows
            .iter()
            .flat_map(|row| &row.cells)
            .any(|cell| cell.ch != ' ')
    );
    terminal.session.shutdown();
}

#[test]
fn a_missing_program_reports_an_error_and_exits() {
    let terminal = start(ShellCommand::program("opensesh-no-such-program"), 80, 24);
    assert_eq!(terminal.wait_exit(), None);
    assert!(
        terminal
            .screen()
            .contains("could not find the program `opensesh-no-such-program`"),
        "{}",
        terminal.screen()
    );
}

#[test]
fn title_notice_from_the_program() {
    let terminal = start(shell(), 80, 24);
    wait_prompt(&terminal);
    if cfg!(windows) {
        terminal.send("title opensesh-title-test\r");
    } else {
        terminal.send("printf '\\033]2;opensesh-title-test\\007'\r");
    }
    eventually("the title notice", || {
        terminal
            .notices()
            .iter()
            .any(|notice| matches!(notice, Notice::Title(title) if title.contains("opensesh-title-test")))
    });
}

/// Starts a shell running a long program identified by `marker`, shuts the session down once
/// the program runs, and returns whether it was gone within `limit`.
fn shutdown_round(marker: u32, limit: Duration) -> bool {
    let terminal = start(shell(), 80, 24);
    wait_prompt(&terminal);
    // A program that never touches the terminal (output to the null device), so only the
    // shutdown can end it.
    if cfg!(windows) {
        terminal.send(&format!("ping -n {marker} 127.0.0.1 >nul\r"));
    } else {
        terminal.send(&format!("sleep {marker} >/dev/null\r"));
    }
    eventually("the program to start", || count_processes(marker) == 1);
    let started = Instant::now();
    terminal.session.shutdown();
    while started.elapsed() < limit {
        if count_processes(marker) == 0 {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// Shutting a session down ends the program running in it (no leaked child).
#[test]
fn shutdown_ends_the_running_program() {
    let marker = 30_000 + std::process::id() % 10_000;
    assert!(
        shutdown_round(marker, Duration::from_secs(10)),
        "the program outlived its session"
    );
}

/// Same, many times and in parallel (catches races in the console host's teardown; one such
/// race leaked 1 program in 8 before it was fixed).
#[test]
#[ignore = "slow stress test; run with --include-ignored"]
fn shutdown_stress() {
    let base = 20_000 + (std::process::id() % 1_000) * 10;
    let workers: Vec<_> = (0..4_u32)
        .map(|worker| {
            std::thread::spawn(move || {
                (0..10_u32)
                    .map(|round| base + worker * 1_000_000 + round)
                    .filter(|&marker| !shutdown_round(marker, Duration::from_secs(8)))
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    let leaked: Vec<u32> = workers
        .into_iter()
        .flat_map(|worker| worker.join().unwrap())
        .collect();
    assert!(leaked.is_empty(), "leaked programs: {leaked:?}");
}

/// Shutting down right after the start (before the shell even ran) leaves nothing behind.
#[test]
fn immediate_shutdown_leaves_nothing_behind() {
    let marker = 50_000 + std::process::id() % 10_000;
    let command = if cfg!(windows) {
        ShellCommand::program("cmd.exe")
            .arg("/c")
            .arg(format!("ping -n {marker} 127.0.0.1 >nul"))
    } else {
        ShellCommand::program("/bin/sh")
            .arg("-c")
            .arg(format!("sleep {marker} >/dev/null"))
    };
    let terminal = start(command, 80, 24);
    terminal.session.shutdown();
    // Give a late start the chance to show up, then require that nothing runs.
    std::thread::sleep(Duration::from_millis(500));
    eventually("nothing left running", || count_processes(marker) == 0);
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(count_processes(marker), 0);
}

/// Processes whose command line is `sleep MARKER` (Unix) or pings `-n MARKER` (Windows).
fn count_processes(marker: u32) -> usize {
    if cfg!(windows) {
        let filter = format!("Name = 'PING.EXE' AND CommandLine like '%-n {marker} %'");
        let output = std::process::Command::new("powershell.exe")
            .args([
                "-NoLogo",
                "-NoProfile",
                "-Command",
                &format!("@(Get-CimInstance Win32_Process -Filter \"{filter}\").Count"),
            ])
            .output()
            .expect("powershell");
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .unwrap_or(usize::MAX)
    } else {
        let wanted = format!("sleep\0{marker}\0");
        std::fs::read_dir("/proc")
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| {
                        std::fs::read(entry.path().join("cmdline"))
                            .is_ok_and(|cmdline| cmdline.ends_with(wanted.as_bytes()))
                    })
                    .count()
            })
            .unwrap_or(0)
    }
}

#[cfg(windows)]
#[test]
fn powershell_output_colors_and_exit() {
    let terminal = start(
        ShellCommand::program("powershell.exe")
            .arg("-NoLogo")
            .arg("-NoProfile"),
        100,
        30,
    );
    terminal.wait_text("PS ");
    terminal.send("Write-Output ('X' + 'Y' * 3)\r");
    terminal.wait_line("XYYY");
    terminal.send("Write-Host -ForegroundColor Red ('re' + 'd')\r");
    let screen = terminal.wait_line("red");
    let row = screen
        .lines()
        .position(|line| line.trim() == "red")
        .expect("red line");
    let frame = terminal.frame();
    let cell = frame
        .rows
        .iter()
        .find(|candidate| usize::from(candidate.index) == row)
        .map(|found| found.cells[0])
        .expect("row in a full frame");
    let bright_red = opensesh_term::palette::Palette::OPENSESH_DARK.bright[1];
    let red = opensesh_term::palette::Palette::OPENSESH_DARK.normal[1];
    assert!(
        cell.fg == bright_red.to_argb() || cell.fg == red.to_argb(),
        "fg {:08x}",
        cell.fg
    );
    terminal.send("$Host.UI.RawUI.WindowSize.Width\r");
    terminal.wait_line("100");
    terminal.resize(120, 30);
    terminal.send("$Host.UI.RawUI.WindowSize.Width\r");
    terminal.wait_line("120");
    terminal.send("exit\r");
    assert_eq!(terminal.wait_exit(), Some(0));
}

#[cfg(unix)]
#[test]
fn utf8_line_editing_is_enabled() {
    let terminal = start(shell(), 120, 24);
    wait_prompt(&terminal);
    terminal.send("stty -a | tr ' ' '\\n' | grep -x -- '-*iutf8'\r");
    terminal.wait_line("iutf8");
}

#[cfg(unix)]
#[test]
fn exit_after_output_keeps_the_output() {
    let terminal = start(
        ShellCommand::program("/bin/sh")
            .arg("-c")
            .arg("i=0; while [ $i -lt 200 ]; do echo line-$i; i=$((i+1)); done; exit 7"),
        80,
        24,
    );
    assert_eq!(terminal.wait_exit(), Some(7));
    terminal.settle(Duration::from_millis(50));
    assert!(
        terminal.screen().contains("line-199"),
        "{}",
        terminal.screen()
    );
}
