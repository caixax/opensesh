//! Full-screen programs on the real PTY and engine (opt-in, Unix).
//!
//! Run with `cargo test -p opensesh-term --test tui -- --include-ignored`. Each test skips (with
//! a message) when its program is not installed. Every key goes through the production key
//! encoder with the program's current modes ([`Terminal::press`], [`Terminal::type_text`]).

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
use common::{CTRL_B, ENTER, ESCAPE, F1, F10, SPACE, Terminal, hermetic, require, start};
use opensesh_term::shell::ShellCommand;

fn in_alt_screen(terminal: &Terminal) -> bool {
    terminal.session.modes().term.contains(TermMode::ALT_SCREEN)
}

/// Types a command line and presses Enter.
fn run(terminal: &Terminal, command: &str) {
    terminal.type_text(command);
    terminal.press(ENTER);
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
    run(&terminal, "echo tmux-$((6*7))");
    terminal.wait_line("tmux-42");
    // Ctrl+B % splits the window vertically.
    terminal.press(CTRL_B);
    terminal.type_text("%");
    terminal.wait_text("│");
    terminal.resize(120, 32);
    run(&terminal, "echo right-$((2*21))");
    terminal.wait_text("right-42");
    run(&terminal, "exit");
    terminal.settle(Duration::from_millis(200));
    run(&terminal, "exit");
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
    terminal.press(F1);
    terminal.wait_for("the help screen", |screen| {
        screen.contains("CPU usage bar") || screen.contains("htop ")
    });
    terminal.type_text("q");
    terminal.wait_text("PID");
    terminal.type_text("q");
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
    terminal.press(SPACE);
    terminal.wait_for("the second page", |screen| {
        !screen.lines().any(|line| line.trim() == "line number 1")
    });
    run(&terminal, "/number 250");
    terminal.wait_text("line number 250");
    terminal.type_text("G");
    terminal.wait_text("(END)");
    terminal.wait_line("line number 500");
    terminal.type_text("q");
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
    terminal.type_text("iHello from OpenSesh");
    terminal.wait_text("Hello from OpenSesh");
    terminal.press(ESCAPE);
    terminal.settle(Duration::from_millis(150));
    run(&terminal, ":wq");
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
    terminal.press(F10);
    // Depending on its settings mc may ask for confirmation.
    let confirm = terminal.wait_for_within("the exit", Duration::from_secs(3), |screen| {
        screen.contains("really want to quit")
    });
    if confirm.is_ok() {
        terminal.press(ENTER);
    }
    assert_eq!(terminal.wait_exit(), Some(0));
}

#[test]
#[ignore = "needs fzf; run with --include-ignored"]
fn fzf_filters_and_prints_the_choice() {
    let Some(fzf) = require("fzf") else { return };
    let home = tempfile::tempdir().unwrap();
    let script = format!(
        "printf 'alpha\nbeta\ngamma\n' | {} --height=40%; echo \"picked-$?\"",
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
    terminal.type_text("bet");
    terminal.wait_text("1/3");
    terminal.press(ENTER);
    terminal.wait_line("beta");
    terminal.wait_line("picked-0");
    assert_eq!(terminal.wait_exit(), Some(0));
}
