//! Headless harness: a real program on a real PTY, driven through the production `Session`.
//!
//! Waits poll the engine's text dump with generous timeouts (scale them with
//! `OPENSESH_TEST_TIMEOUT_SCALE` on slow machines). Predicates should match program output, not
//! the echoed command line.
//!
//! Keys go through the production key encoder ([`Terminal::press`], [`Terminal::type_text`]):
//! a key is described the way Qt reports it (`QKeyEvent::key()`, `modifiers()`, `text()`) and
//! encoded with the program's current modes, as the terminal item does. [`Terminal::send`]
//! writes raw bytes and is for typed shell commands only.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use opensesh_term::backend::{BackendEvent, TermSize, TerminalBackend};
use opensesh_term::input::keys::{KeyInput, KeyOptions, encode_key, qt};
use opensesh_term::pty;
use opensesh_term::session::{Notice, Session, SessionConfig};
use opensesh_term::shell::ShellCommand;
use opensesh_term::snapshot::Frame;

/// A key press as Qt reports it in a `QKeyEvent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QtKey {
    /// `QKeyEvent::key()`.
    pub key: i32,
    /// `QKeyEvent::modifiers()`.
    pub modifiers: u32,
    /// `QKeyEvent::text()`.
    pub text: &'static str,
}

/// The main Enter key.
pub const ENTER: QtKey = QtKey {
    key: qt::KEY_RETURN,
    modifiers: 0,
    text: "\r",
};
/// Escape.
pub const ESCAPE: QtKey = QtKey {
    key: qt::KEY_ESCAPE,
    modifiers: 0,
    text: "\x1b",
};
/// Space.
pub const SPACE: QtKey = QtKey {
    key: qt::KEY_SPACE,
    modifiers: 0,
    text: " ",
};
/// F1 (Qt gives no text for function keys).
pub const F1: QtKey = QtKey {
    key: qt::KEY_F1,
    modifiers: 0,
    text: "",
};
/// F10.
pub const F10: QtKey = QtKey {
    key: qt::KEY_F1 + 9,
    modifiers: 0,
    text: "",
};
/// Ctrl+B, as Qt reports it on Linux (the text is the control character).
pub const CTRL_B: QtKey = QtKey {
    key: 0x42,
    modifiers: qt::CONTROL_MODIFIER,
    text: "\x02",
};
/// Ctrl+C.
pub const CTRL_C: QtKey = QtKey {
    key: 0x43,
    modifiers: qt::CONTROL_MODIFIER,
    text: "\x03",
};

/// The environment variables that make a program ignore the user's configuration: a clean home
/// directory, the C.UTF-8 locale and a short prompt.
pub fn hermetic(command: ShellCommand, home: &Path) -> ShellCommand {
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

/// The multiplier from `OPENSESH_TEST_TIMEOUT_SCALE` (at least 1).
pub fn timeout_scale() -> u32 {
    std::env::var("OPENSESH_TEST_TIMEOUT_SCALE")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1)
        .max(1)
}

/// A terminal session under test.
pub struct Terminal {
    pub session: Session,
    pub notices: Receiver<Notice>,
    seen: std::cell::RefCell<Vec<Notice>>,
}

/// The default wait, scaled by `OPENSESH_TEST_TIMEOUT_SCALE`.
pub fn timeout() -> Duration {
    Duration::from_secs(20) * timeout_scale()
}

/// Starts `command` on a PTY of `columns` x `lines` behind a `Session`.
pub fn start(command: ShellCommand, columns: u16, lines: u16) -> Terminal {
    let size = test_size(columns, lines);
    let (backend, events) = pty::spawn(command, size).expect("backend thread");
    attach(backend, events, size)
}

/// The size tests use: `columns` x `lines` cells of 8 x 16 pixels.
pub fn test_size(columns: u16, lines: u16) -> TermSize {
    TermSize {
        columns,
        lines,
        cell_width: 8,
        cell_height: 16,
    }
}

/// Starts a focused `Session` over any backend (spawned with `size`).
pub fn attach(
    backend: Box<dyn TerminalBackend>,
    events: Receiver<BackendEvent>,
    size: TermSize,
) -> Terminal {
    let (notice_tx, notices) = crossbeam_channel::unbounded();
    let session = Session::start(
        backend,
        events,
        SessionConfig {
            size,
            ..SessionConfig::default()
        },
        Arc::new(move |notice| {
            let _ = notice_tx.send(notice);
        }),
    )
    .expect("engine thread");
    session.focus_changed(true);
    Terminal {
        session,
        notices,
        seen: std::cell::RefCell::new(Vec::new()),
    }
}

impl Terminal {
    /// Writes `text` to the program as it is (no key encoding).
    pub fn send(&self, text: &str) {
        self.session.write(text.as_bytes());
    }

    /// Presses `key`: the production encoder turns it into bytes with the program's current
    /// modes (application cursor keys, line feed / new line mode...), as the terminal item does.
    pub fn press(&self, key: QtKey) {
        let input = KeyInput::from_qt(key.key, key.modifiers, key.text);
        self.press_input(&input);
    }

    /// Types `text` one key at a time through the encoder. Each character is reported as Qt does
    /// for a US layout: the key code is the upper-case character, and upper-case letters carry
    /// Shift.
    pub fn type_text(&self, text: &str) {
        for c in text.chars() {
            let mut buffer = [0; 4];
            let key = i32::try_from(u32::from(c.to_ascii_uppercase())).unwrap();
            let modifiers = if c.is_ascii_uppercase() {
                qt::SHIFT_MODIFIER
            } else {
                0
            };
            let input = KeyInput::from_qt(key, modifiers, c.encode_utf8(&mut buffer));
            self.press_input(&input);
        }
    }

    fn press_input(&self, input: &KeyInput) {
        let bytes = encode_key(input, &self.session.modes(), &KeyOptions::default())
            .unwrap_or_else(|| panic!("the key {input:?} sends nothing"));
        self.session.write(&bytes);
    }

    /// Runs `input`, waits until the program printed something, then until its output stayed
    /// quiet for `quiet` (the engine sent no `Dirty` notice), and returns the screen.
    ///
    /// Waiting for new output first matters: right after the input the previous screen is still
    /// there, and it may already contain the text the next screen will show.
    pub fn step(&self, quiet: Duration, input: impl FnOnce(&Self)) -> String {
        // Drain first, then clear the dirty flag: a `Dirty` sent after this is new output.
        self.drain_dirty();
        let mut frame = Frame::default();
        self.session.snapshot(&mut frame);
        input(self);
        assert!(
            self.wait_dirty(timeout()),
            "no output after the input; screen:\n{}",
            self.screen()
        );
        let deadline = Instant::now() + timeout();
        loop {
            self.session.snapshot(&mut frame);
            if !self.wait_dirty(quiet) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the output never went quiet; screen:\n{}",
                self.screen()
            );
        }
        self.screen()
    }

    /// Drops pending `Dirty` notices and keeps the others.
    fn drain_dirty(&self) {
        let mut seen = self.seen.borrow_mut();
        seen.extend(
            self.notices
                .try_iter()
                .filter(|notice| *notice != Notice::Dirty),
        );
    }

    /// Waits up to `limit` for a `Dirty` notice, keeping the others.
    pub fn wait_dirty(&self, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.notices.recv_timeout(left) {
                Ok(Notice::Dirty) => return true,
                Ok(other) => self.seen.borrow_mut().push(other),
                Err(_) => return false,
            }
        }
    }

    /// The visible screen.
    pub fn screen(&self) -> String {
        self.session.text_dump()
    }

    /// Waits until `condition(screen)` holds; panics with the screen on timeout.
    pub fn wait_for(&self, what: &str, condition: impl Fn(&str) -> bool) -> String {
        self.wait_for_within(what, timeout(), condition)
            .unwrap_or_else(|screen| panic!("timed out waiting for {what}; screen:\n{screen}"))
    }

    /// Like [`Terminal::wait_for`], returning the last screen as an error on timeout.
    pub fn wait_for_within(
        &self,
        _what: &str,
        limit: Duration,
        condition: impl Fn(&str) -> bool,
    ) -> Result<String, String> {
        let deadline = Instant::now() + limit;
        loop {
            let screen = self.screen();
            if condition(&screen) {
                return Ok(screen);
            }
            if Instant::now() >= deadline {
                return Err(screen);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Waits until some screen line, trimmed, equals `line`.
    pub fn wait_line(&self, line: &str) -> String {
        self.wait_for(&format!("the line {line:?}"), |screen| {
            screen.lines().any(|candidate| candidate.trim() == line)
        })
    }

    /// Waits until the screen contains `text`.
    pub fn wait_text(&self, text: &str) -> String {
        self.wait_for(&format!("{text:?}"), |screen| screen.contains(text))
    }

    /// Waits until the output stops changing for `quiet`.
    pub fn settle(&self, quiet: Duration) -> String {
        let deadline = Instant::now() + timeout();
        let mut last = self.screen();
        let mut since = Instant::now();
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(25));
            let screen = self.screen();
            if screen == last {
                if since.elapsed() >= quiet {
                    break;
                }
            } else {
                last = screen;
                since = Instant::now();
            }
        }
        last
    }

    /// Waits for `Notice::Exited` and returns its code.
    pub fn wait_exit(&self) -> Option<i32> {
        self.wait_exit_within(timeout())
    }

    /// Like [`Terminal::wait_exit`], waiting up to `limit`.
    pub fn wait_exit_within(&self, limit: Duration) -> Option<i32> {
        let deadline = Instant::now() + limit;
        if let Some(code) = self.recorded_exit() {
            return code;
        }
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.notices.recv_timeout(left) {
                Ok(Notice::Exited(code)) => {
                    self.seen.borrow_mut().push(Notice::Exited(code));
                    return code;
                }
                Ok(other) => self.seen.borrow_mut().push(other),
                Err(_) => panic!("the program did not exit; screen:\n{}", self.screen()),
            }
        }
    }

    fn recorded_exit(&self) -> Option<Option<i32>> {
        self.seen.borrow().iter().find_map(|notice| match notice {
            Notice::Exited(code) => Some(*code),
            _ => None,
        })
    }

    /// Every notice received so far (drains the channel).
    pub fn notices(&self) -> Vec<Notice> {
        let mut seen = self.seen.borrow_mut();
        seen.extend(self.notices.try_iter());
        seen.clone()
    }

    /// A snapshot of the whole screen.
    pub fn frame(&self) -> Frame {
        let mut frame = Frame::default();
        self.session.snapshot(&mut frame);
        frame
    }

    /// Resizes and waits until the engine applied it.
    pub fn resize(&self, columns: u16, lines: u16) {
        self.session.resize(test_size(columns, lines));
        let deadline = Instant::now() + timeout();
        loop {
            let frame = self.frame();
            if (frame.columns, frame.lines) == (columns, lines) {
                return;
            }
            assert!(Instant::now() < deadline, "resize not applied");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.session.shutdown();
    }
}

/// The absolute path of `program` if it is on `PATH`.
pub fn find_program(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        let candidate = dir.join(program);
        candidate.is_file().then_some(candidate)
    })
}

/// For opt-in tests: the program's path, or `None` after printing why the test is skipped.
pub fn require(program: &str) -> Option<PathBuf> {
    let found = find_program(program);
    if found.is_none() {
        eprintln!("skipped: `{program}` is not installed");
    }
    found
}

/// Polls `condition` until it holds or the default timeout passes.
pub fn eventually(what: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + timeout();
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}
