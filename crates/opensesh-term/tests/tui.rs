//! Full-screen programs on the real PTY and engine (opt-in, Unix).
//!
//! Run with `cargo test -p opensesh-term --test tui -- --include-ignored`. Each test skips (with
//! a message) when its program is not installed. Keys are sent as the xterm sequences the key
//! encoder produces for them.

// A test crate: clippy.toml allows these in `#[test]` functions, and the shared helpers need
// them too.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr
)]
#![cfg(unix)]

mod common;

use std::time::Duration;

use alacritty_terminal::term::TermMode;
use common::{Terminal, require, start};
use opensesh_term::shell::ShellCommand;

const ENTER: &str = "\r";
const ESCAPE: &str = "\x1b";
const F1: &str = "\x1bOP";
const F10: &str = "\x1b[21~";
const CTRL_B: &str = "\x02";

/// A clean home directory and neutral settings, so user configuration can't change the result.
fn hermetic(command: ShellCommand, home: &std::path::Path) -> ShellCommand {
    command
        .cwd(home)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("LANG", "C.UTF-8")
        .env("LC_ALL", "C.UTF-8")
        .env("PS1", "$ ")
        .env("ENV", "")
        .env("TMUX", "")
        .env("LESSHISTFILE", "-")
        .env("FZF_DEFAULT_OPTS", "")
        .env("FZF_DEFAULT_COMMAND", "")
}

fn in_alt_screen(terminal: &Terminal) -> bool {
    terminal.session.modes().term.contains(TermMode::ALT_SCREEN)
}

#[test]
#[ignore = "needs tmux; run with --include-ignored"]
fn tmux_splits_runs_commands_and_exits() {
    let Some(tmux) = require("tmux") else { return };
    let home = tempfile::tempdir().unwrap();
    let socket = format!("opensesh-test-{}", std::process::id());
    let terminal = start(
        hermetic(
            ShellCommand::program(tmux)
                .arg("-L")
                .arg(&socket)
                .arg("-f")
                .arg("/dev/null")
                .arg("new-session")
                .arg("sh"),
            home.path(),
        ),
        100,
        30,
    );
    terminal.wait_text("[0]");
    terminal.send("echo tmux-$((6*7))\r");
    terminal.wait_line("tmux-42");
    // Ctrl+B % splits the window vertically.
    terminal.send(CTRL_B);
    terminal.send("%");
    terminal.wait_text("│");
    terminal.resize(120, 32);
    terminal.send("echo right-$((2*21))\r");
    terminal.wait_text("right-42");
    terminal.send("exit\r");
    terminal.settle(Duration::from_millis(200));
    terminal.send("exit\r");
    assert_eq!(terminal.wait_exit(), Some(0));
}

#[test]
#[ignore = "needs htop; run with --include-ignored"]
fn htop_shows_processes_help_and_quits() {
    let Some(htop) = require("htop") else { return };
    let home = tempfile::tempdir().unwrap();
    let terminal = start(hermetic(ShellCommand::program(htop), home.path()), 120, 40);
    terminal.wait_text("PID");
    assert!(in_alt_screen(&terminal));
    terminal.send(F1);
    terminal.wait_for("the help screen", |screen| {
        screen.contains("CPU usage bar") || screen.contains("htop ")
    });
    terminal.send("q");
    terminal.wait_text("PID");
    terminal.send("q");
    assert_eq!(terminal.wait_exit(), Some(0));
}

#[test]
#[ignore = "needs less; run with --include-ignored"]
fn less_pages_searches_and_restores_the_screen() {
    let Some(less) = require("less") else { return };
    let home = tempfile::tempdir().unwrap();
    let file = home.path().join("numbers.txt");
    let text: String = (1..=500)
        .map(|line| format!("line number {line}\n"))
        .collect();
    std::fs::write(&file, text).unwrap();
    let terminal = start(
        hermetic(ShellCommand::program(less).arg(&file), home.path()),
        80,
        24,
    );
    terminal.wait_line("line number 1");
    terminal.send(" ");
    terminal.wait_for("the second page", |screen| {
        !screen.lines().any(|line| line.trim() == "line number 1")
    });
    terminal.send("/number 250");
    terminal.send(ENTER);
    terminal.wait_text("line number 250");
    terminal.send("G");
    terminal.wait_text("(END)");
    terminal.wait_line("line number 500");
    terminal.send("q");
    assert_eq!(terminal.wait_exit(), Some(0));
    assert!(!in_alt_screen(&terminal), "the primary screen is back");
}

#[test]
#[ignore = "needs nvim; run with --include-ignored"]
fn nvim_edits_and_saves_a_file() {
    let Some(nvim) = require("nvim") else { return };
    let home = tempfile::tempdir().unwrap();
    let file = home.path().join("note.txt");
    let terminal = start(
        hermetic(
            ShellCommand::program(nvim)
                .arg("--clean")
                .arg("-n")
                .arg("-i")
                .arg("NONE")
                .arg(&file),
            home.path(),
        ),
        100,
        30,
    );
    terminal.wait_for("the empty buffer", |screen| {
        screen.lines().filter(|line| line.trim() == "~").count() > 5
    });
    terminal.send("iHello from OpenSesh");
    terminal.wait_text("Hello from OpenSesh");
    terminal.send(ESCAPE);
    terminal.settle(Duration::from_millis(150));
    terminal.send(":wq");
    terminal.send(ENTER);
    assert_eq!(terminal.wait_exit(), Some(0));
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "Hello from OpenSesh\n"
    );
    assert!(!in_alt_screen(&terminal));
}

#[test]
#[ignore = "needs mc; run with --include-ignored"]
fn mc_draws_its_panels_and_quits_with_f10() {
    let Some(mc) = require("mc") else { return };
    let home = tempfile::tempdir().unwrap();
    let terminal = start(
        hermetic(
            ShellCommand::program(mc).arg("-u").arg("-X").arg("-d"),
            home.path(),
        ),
        120,
        36,
    );
    terminal.wait_for("the panels and the key bar", |screen| {
        screen.contains("Left") && screen.contains("Quit")
    });
    assert!(in_alt_screen(&terminal));
    terminal.send(F10);
    // Depending on its settings mc may ask for confirmation.
    let confirm = terminal.wait_for_within("the exit", Duration::from_secs(3), |screen| {
        screen.contains("really want to quit")
    });
    if confirm.is_ok() {
        terminal.send(ENTER);
    }
    assert_eq!(terminal.wait_exit(), Some(0));
}

#[test]
#[ignore = "needs fzf; run with --include-ignored"]
fn fzf_filters_and_prints_the_choice() {
    let Some(fzf) = require("fzf") else { return };
    let home = tempfile::tempdir().unwrap();
    let script = format!(
        "printf 'alpha\\nbeta\\ngamma\\n' | {} --height=40%; echo \"picked-$?\"",
        fzf.display()
    );
    let terminal = start(
        hermetic(
            ShellCommand::program("/bin/sh").arg("-c").arg(script),
            home.path(),
        ),
        80,
        24,
    );
    terminal.wait_text("3/3");
    terminal.send("bet");
    terminal.wait_text("1/3");
    terminal.send(ENTER);
    terminal.wait_line("beta");
    terminal.wait_line("picked-0");
    assert_eq!(terminal.wait_exit(), Some(0));
}
