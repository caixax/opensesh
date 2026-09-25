//! vttest, the VT100/VT102 conformance program, on the real PTY and engine (opt-in, Linux).
//!
//! ```sh
//! cargo xtask vttest                                                   # builds the pinned vttest
//! cargo test -p opensesh-term --test vttest -- --include-ignored
//! OPENSESH_BLESS=1 cargo test -p opensesh-term --test vttest -- --include-ignored  # rewrite goldens
//! ```
//!
//! The "basic sections" of the Sprint 2 definition of done (decision D8; `docs/testing/vttest.md`
//! has the results and the deviation list) are the main menu items 1 (cursor movements), 2
//! (screen features), 3 (character sets), 6 (terminal reports: LNM, DSR and CPR, DA1) and 8
//! (VT102 insert and delete), at 80 x 24. Each test walks one item screen by screen and compares
//! every screen's text dump (and, when some cell is bold, underlined or inverse, an attribute
//! map) with a golden file in `tests/vttest/`. Screens with a known deviation (132 columns,
//! blink...) have goldens too, so any change shows up.
//!
//! Every key goes through the production key encoder with the program's current modes, which is
//! what the line feed / new line test (6.2) checks.
//!
//! The program is `<target>/vttest/vttest` from `cargo xtask vttest`, or the one named by
//! `OPENSESH_VTTEST`. It must be the release the goldens were recorded with ([`VTTEST_VERSION`]):
//! distributions ship other releases, whose screens differ. The tests are skipped (with a
//! message) on Windows and when the program is missing.

// A test crate: clippy.toml allows these in `#[test]` functions, and the shared helpers need
// them too.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr
)]

mod common;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use common::{ENTER, Terminal, hermetic, start, timeout_scale};
use opensesh_term::palette::Palette;
use opensesh_term::shell::ShellCommand;
use opensesh_term::snapshot::{Frame, flags};

/// The vttest release the goldens were recorded with; keep it equal to `VERSION` in
/// `xtask/src/vttest.rs`.
const VTTEST_VERSION: &str = "20251205";

/// How long the output must stay quiet before a screen counts as complete. vttest pauses about
/// 200 ms while it waits for a terminal report, so this must be well above that.
const QUIET: Duration = Duration::from_millis(500);

/// The most screens one menu item may show before the test gives up.
const MAX_SCREENS: usize = 40;

/// Text of the prompt at the end of a screen that waits for Enter.
const PUSH_RETURN: &str = "Push <RETURN>";

/// Text of every vttest menu.
const MENU_PROMPT: &str = "Enter choice number";

/// The vttest program to test, or `None` after printing why the test is skipped.
fn vttest_program() -> Option<PathBuf> {
    if cfg!(windows) {
        eprintln!("skipped: vttest is a Unix program; run this test on Linux or in WSL");
        return None;
    }
    let program = match std::env::var_os("OPENSESH_VTTEST") {
        Some(path) if !path.is_empty() => PathBuf::from(path),
        // CARGO_TARGET_TMPDIR is `<target>/tmp`; `cargo xtask vttest` builds `<target>/vttest`.
        _ => Path::new(env!("CARGO_TARGET_TMPDIR"))
            .parent()
            .unwrap()
            .join("vttest")
            .join("vttest"),
    };
    if !program.is_file() {
        eprintln!(
            "skipped: {} is missing; build it with `cargo xtask vttest` or set OPENSESH_VTTEST",
            program.display()
        );
        return None;
    }
    let output = std::process::Command::new(&program)
        .arg("-V")
        .output()
        .unwrap();
    let version = String::from_utf8_lossy(&output.stdout);
    assert!(
        version.contains(VTTEST_VERSION),
        "{} is `{}`, but the goldens were recorded with vttest {VTTEST_VERSION}: build it with \
         `cargo xtask vttest`",
        program.display(),
        version.trim()
    );
    Some(program)
}

/// One vttest run at 80 x 24, collecting screens.
struct Vttest {
    terminal: Terminal,
    _home: tempfile::TempDir,
    quiet: Duration,
    /// Golden file names and contents, in order.
    files: Vec<(String, String)>,
}

impl Vttest {
    /// Starts vttest and waits for its main menu.
    fn start(program: &Path) -> Self {
        let home = tempfile::tempdir().unwrap();
        let terminal = start(
            hermetic(ShellCommand::program(program), home.path()),
            80,
            24,
        );
        terminal.wait_text(MENU_PROMPT);
        let quiet = QUIET * timeout_scale();
        terminal.settle(quiet);
        Self {
            terminal,
            _home: home,
            quiet,
            files: Vec::new(),
        }
    }

    /// Chooses `choice` in the current menu: types it and presses Enter.
    fn choose(&self, choice: &str) -> String {
        self.terminal.step(self.quiet, |terminal| {
            terminal.type_text(choice);
            terminal.press(ENTER);
        })
    }

    /// Presses Enter and returns the next screen.
    fn enter(&self) -> String {
        self.terminal
            .step(self.quiet, |terminal| terminal.press(ENTER))
    }

    /// Records `screen` as the golden `name.txt`, and its attributes as `name.attrs.txt` when
    /// some cell has one (see [`attributes`]).
    fn record(&mut self, name: &str, screen: String) {
        self.files.push((format!("{name}.txt"), screen));
        if let Some(attributes) = attributes(&self.terminal) {
            self.files.push((format!("{name}.attrs.txt"), attributes));
        }
    }

    /// Chooses `choice`, records every screen until vttest shows a menu again (`prefix-01`,
    /// `prefix-02`...), and returns that menu.
    fn walk(&mut self, choice: &str, prefix: &str) -> String {
        let mut screen = self.choose(choice);
        let mut count = 0;
        while !is_menu(&screen) {
            count += 1;
            assert!(count <= MAX_SCREENS, "too many screens in {prefix}");
            self.record(&format!("{prefix}-{count:02}"), screen);
            screen = self.enter();
        }
        screen
    }

    /// Leaves vttest from its main menu and checks that it exited cleanly.
    fn quit(&self) {
        self.terminal.type_text("0");
        self.terminal.press(ENTER);
        assert_eq!(self.terminal.wait_exit(), Some(0));
    }

    /// Compares the recorded screens with the goldens (or rewrites them with `OPENSESH_BLESS`).
    fn check(&self, item: &str) {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("vttest");
        let prefix = format!("{item}-");
        let goldens: Vec<String> = std::fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .filter(|name| name.starts_with(&prefix) && name.ends_with(".txt"))
                    .collect()
            })
            .unwrap_or_default();
        if std::env::var_os("OPENSESH_BLESS").is_some() {
            std::fs::create_dir_all(&dir).unwrap();
            for stale in &goldens {
                std::fs::remove_file(dir.join(stale)).unwrap();
            }
            for (file, contents) in &self.files {
                std::fs::write(dir.join(file), contents).unwrap();
            }
            eprintln!("blessed {} golden files of item {item}", self.files.len());
            return;
        }

        let actual_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("vttest-actual");
        std::fs::create_dir_all(&actual_dir).unwrap();
        let mut failures = String::new();
        for (file, actual) in &self.files {
            std::fs::write(actual_dir.join(file), actual).unwrap();
            match std::fs::read_to_string(dir.join(file)) {
                Ok(expected) if expected == *actual => {}
                Ok(expected) => {
                    let _ = writeln!(failures, "{file} differs:\n{}", diff(&expected, actual));
                }
                Err(_) => {
                    let _ = writeln!(failures, "{file} has no golden; actual:\n{actual}");
                }
            }
        }
        for golden in &goldens {
            if !self.files.iter().any(|(file, _)| file == golden) {
                let _ = writeln!(
                    failures,
                    "{golden}: not produced (a screen, or its attributes, went away)"
                );
            }
        }
        assert!(
            failures.is_empty(),
            "vttest item {item} does not match its goldens (the actual screens are in {}):\n{failures}",
            actual_dir.display()
        );
    }
}

/// The attributes the text dump can't show, from a full snapshot: one character per cell, `.`
/// for none, otherwise a hex digit adding 1 for bold, 2 for any underline and 4 for inverse
/// (a background other than the default one). Trailing `.` are trimmed. `None` when no cell
/// has an attribute.
fn attributes(terminal: &Terminal) -> Option<String> {
    // A palette change forces a full frame. Its redraw sends one `Dirty`, consumed here so the
    // next step doesn't take it for new output.
    terminal.session.set_palette(Palette::default());
    let mut frame = Frame::default();
    terminal.session.snapshot(&mut frame);
    assert!(terminal.wait_dirty(common::timeout()), "no redraw notice");
    assert_eq!(frame.rows.len(), usize::from(frame.lines), "a full frame");
    let mut out = String::new();
    let mut any = false;
    for row in &frame.rows {
        let mut line: String = row
            .cells
            .iter()
            .map(|cell| {
                let mut value = 0;
                if cell.flags & flags::BOLD != 0 {
                    value += 1;
                }
                if cell.flags & flags::ANY_UNDERLINE != 0 {
                    value += 2;
                }
                if cell.bg != frame.background {
                    value += 4;
                }
                char::from_digit(value, 16)
                    .filter(|_| value != 0)
                    .unwrap_or('.')
            })
            .collect();
        line.truncate(line.trim_end_matches('.').len());
        any |= !line.is_empty();
        out.push_str(&line);
        out.push('\n');
    }
    any.then_some(out)
}

/// Whether `screen` is a vttest menu waiting for a choice.
fn is_menu(screen: &str) -> bool {
    screen.contains(MENU_PROMPT) && !screen.contains(PUSH_RETURN)
}

/// The lines that differ, numbered from 1.
fn diff(expected: &str, actual: &str) -> String {
    let expected: Vec<&str> = expected.lines().collect();
    let actual: Vec<&str> = actual.lines().collect();
    let mut out = String::new();
    for row in 0..expected.len().max(actual.len()) {
        let (want, got) = (expected.get(row), actual.get(row));
        if want != got {
            let _ = writeln!(
                out,
                "  row {:>2} expected: {:?}\n  row {:>2}   actual: {:?}",
                row + 1,
                want.unwrap_or(&""),
                row + 1,
                got.unwrap_or(&"")
            );
        }
    }
    out
}

/// Runs a plain main-menu item (screens that each end with "Push <RETURN>").
fn plain_item(item: &str) {
    let Some(program) = vttest_program() else {
        return;
    };
    let mut vttest = Vttest::start(&program);
    let menu = vttest.walk(item, item);
    assert!(menu.contains("VT100 test program"), "{menu}");
    vttest.quit();
    vttest.check(item);
}

#[test]
#[ignore = "needs vttest (cargo xtask vttest); run with --include-ignored"]
fn vttest_1_cursor_movements() {
    plain_item("1");
}

#[test]
#[ignore = "needs vttest (cargo xtask vttest); run with --include-ignored"]
fn vttest_2_screen_features() {
    plain_item("2");
}

#[test]
#[ignore = "needs vttest (cargo xtask vttest); run with --include-ignored"]
fn vttest_3_character_sets() {
    plain_item("3");
}

#[test]
#[ignore = "needs vttest (cargo xtask vttest); run with --include-ignored"]
fn vttest_8_insert_delete() {
    plain_item("8");
}

/// Item 6, terminal reports: 6.2 line feed / new line mode (Enter must send CR LF while LNM is
/// set), 6.3 device status and cursor position reports, 6.4 primary device attributes.
#[test]
#[ignore = "needs vttest (cargo xtask vttest); run with --include-ignored"]
fn vttest_6_terminal_reports() {
    let Some(program) = vttest_program() else {
        return;
    };
    let mut vttest = Vttest::start(&program);
    let menu = vttest.choose("6");
    assert!(menu.contains("Terminal Reports/Responses"), "{menu}");
    vttest.record("6-00-menu", menu);

    // 6.2: two prompts that read one line each (without "Push <RETURN>"), then the results.
    let screen = vttest.choose("2");
    assert!(screen.contains("NewLine mode set"), "{screen}");
    vttest.record("6-2-01", screen);
    let screen = vttest.enter();
    assert!(screen.contains("NewLine mode reset"), "{screen}");
    vttest.record("6-2-02", screen);
    let screen = vttest.enter();
    // CR LF while LNM was set, CR after it was reset: the encoder follows the mode.
    assert!(screen.contains(" <13> <10>  -- OK"), "{screen}");
    assert!(screen.contains(" <13>  -- OK"), "{screen}");
    vttest.record("6-2-03", screen);
    let menu = vttest.enter();
    assert!(is_menu(&menu), "{menu}");

    for (choice, prefix, expected) in [
        ("3", "6-3", "Report is: <27> [ 5 ; 1 R  -- OK"),
        ("4", "6-4", "Report is: <27> [ ? 6 c  -- means VT102"),
    ] {
        let screen = vttest.choose(choice);
        assert!(screen.contains(expected), "{screen}");
        vttest.record(&format!("{prefix}-01"), screen);
        let menu = vttest.enter();
        assert!(menu.contains("Terminal Reports/Responses"), "{menu}");
    }
    let menu = vttest.choose("0");
    assert!(menu.contains("VT100 test program"), "{menu}");
    vttest.quit();
    vttest.check("6");
}

/// The items outside the basic sections, whose deviations `docs/testing/vttest.md` lists: 4
/// (double-size lines), 7 (VT52 mode) and the reports 6.1 (ENQ answerback), 6.5 (DA2), 6.6
/// (DA3) and 6.7 (DECREQTPARM). There are no goldens: the test checks that vttest runs through
/// them and back to its menu, and leaves the screens in `<target>/tmp/vttest-actual/other/`.
#[test]
#[ignore = "needs vttest (cargo xtask vttest); run with --include-ignored"]
fn vttest_other_items_run_to_the_end() {
    let Some(program) = vttest_program() else {
        return;
    };
    let mut vttest = Vttest::start(&program);
    for item in ["4", "7"] {
        let menu = vttest.walk(item, item);
        assert!(menu.contains("VT100 test program"), "{menu}");
    }
    let menu = vttest.choose("6");
    assert!(menu.contains("Terminal Reports/Responses"), "{menu}");
    for choice in ["1", "5", "6", "7"] {
        let menu = vttest.walk(choice, &format!("6-{choice}"));
        assert!(menu.contains("Terminal Reports/Responses"), "{menu}");
    }
    let menu = vttest.choose("0");
    assert!(menu.contains("VT100 test program"), "{menu}");
    vttest.quit();
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("vttest-actual")
        .join("other");
    std::fs::create_dir_all(&dir).unwrap();
    for (file, contents) in &vttest.files {
        std::fs::write(dir.join(file), contents).unwrap();
    }
    eprintln!("{} screens in {}", vttest.files.len(), dir.display());
}
