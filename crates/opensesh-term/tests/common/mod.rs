//! Headless harness: a real program on a real PTY, driven through the production `Session`.
//!
//! Waits poll the engine's text dump with generous timeouts (scale them with
//! `OPENSESH_TEST_TIMEOUT_SCALE` on slow machines). Predicates should match program output, not
//! the echoed command line.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use opensesh_term::backend::TermSize;
use opensesh_term::pty;
use opensesh_term::session::{Notice, Session, SessionConfig};
use opensesh_term::shell::ShellCommand;
use opensesh_term::snapshot::Frame;

/// A terminal session under test.
pub struct Terminal {
    pub session: Session,
    pub notices: Receiver<Notice>,
    seen: std::cell::RefCell<Vec<Notice>>,
}

/// The default wait, scaled by `OPENSESH_TEST_TIMEOUT_SCALE`.
pub fn timeout() -> Duration {
    let scale = std::env::var("OPENSESH_TEST_TIMEOUT_SCALE")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1)
        .max(1);
    Duration::from_secs(20) * scale
}

/// Starts `command` on a PTY of `columns` x `lines` behind a `Session`.
pub fn start(command: ShellCommand, columns: u16, lines: u16) -> Terminal {
    let size = TermSize {
        columns,
        lines,
        cell_width: 8,
        cell_height: 16,
    };
    let (backend, events) = pty::spawn(command, size).expect("backend thread");
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
    /// Sends text as typed input.
    pub fn send(&self, text: &str) {
        self.session.write(text.as_bytes());
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
        let deadline = Instant::now() + timeout();
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
        self.session.resize(TermSize {
            columns,
            lines,
            cell_width: 8,
            cell_height: 16,
        });
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
