//! The byte pipe between a terminal session and the program it shows (PLAN §3.4).
//!
//! A backend runs the program (a local shell through a PTY today, SSH and serial later) and
//! **pushes** what it prints into a channel of [`BackendEvent`]s. The session's engine thread
//! consumes that channel; the other direction (keystrokes, replies to terminal queries, window
//! size) goes through the object-safe [`TerminalBackend`] trait. Every trait method must return
//! at once: the engine thread calls them between parse chunks and must never wait for the
//! program. See `docs/design/terminal-engine.md` and ADR 0012.

use crossbeam_channel::Receiver;

/// Size of a terminal's text area, in cells and in pixels per cell.
///
/// The pixel size only feeds `TIOCSWINSZ` / `CSI 14 t` replies; `0` means unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TermSize {
    /// Width in cells.
    pub columns: u16,
    /// Height in cells.
    pub lines: u16,
    /// Width of one cell in device pixels (`0` if unknown).
    pub cell_width: u16,
    /// Height of one cell in device pixels (`0` if unknown).
    pub cell_height: u16,
}

impl TermSize {
    /// The smallest grid the engine accepts (`alacritty_terminal` misbehaves below it).
    pub const MIN_COLUMNS: u16 = 2;
    /// The smallest number of lines the engine accepts.
    pub const MIN_LINES: u16 = 1;

    /// A size in cells with an unknown pixel size.
    #[must_use]
    pub const fn new(columns: u16, lines: u16) -> Self {
        Self {
            columns,
            lines,
            cell_width: 0,
            cell_height: 0,
        }
    }

    /// The same size with at least [`Self::MIN_COLUMNS`] columns and [`Self::MIN_LINES`] lines.
    #[must_use]
    pub fn clamped(self) -> Self {
        Self {
            columns: self.columns.max(Self::MIN_COLUMNS),
            lines: self.lines.max(Self::MIN_LINES),
            ..self
        }
    }

    /// Width of the whole text area in pixels (saturating), as `TIOCSWINSZ` expects.
    #[must_use]
    pub fn pixel_width(self) -> u16 {
        self.columns.saturating_mul(self.cell_width)
    }

    /// Height of the whole text area in pixels (saturating).
    #[must_use]
    pub fn pixel_height(self) -> u16 {
        self.lines.saturating_mul(self.cell_height)
    }
}

impl Default for TermSize {
    /// 80 x 24, the classic VT100 size.
    fn default() -> Self {
        Self::new(80, 24)
    }
}

/// What a backend reports, in order, through its event channel.
///
/// Ordering contract: every `Output` the program produced before it exited is sent before
/// `Exited`. The channel disconnects when the backend is completely finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendEvent {
    /// Bytes printed by the program.
    Output(Vec<u8>),
    /// The program ended: its exit code, or `None` if it was killed by a signal or the code is
    /// unknown. Sent at most once.
    Exited(Option<i32>),
    /// Something went wrong (for example the shell could not be started). The text is meant for
    /// the user: it never contains secrets or environment values.
    Error(String),
}

/// Errors of [`TerminalBackend`] calls.
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    /// The backend has stopped (the program exited or [`TerminalBackend::shutdown`] ran).
    #[error("the terminal backend has stopped")]
    Closed,
    /// A backend thread could not be started.
    #[error("could not start a terminal thread")]
    Thread(#[source] std::io::Error),
}

/// The input side of a backend. Implementations are cheap handles: every method returns without
/// waiting for the program (they queue the work for the backend's own threads).
pub trait TerminalBackend: Send {
    /// Queues bytes for the program's input (keystrokes, paste, replies to terminal queries).
    ///
    /// # Errors
    /// [`BackendError::Closed`] once the backend has stopped.
    fn write(&self, bytes: &[u8]) -> Result<(), BackendError>;

    /// Queues a window size change (`TIOCSWINSZ` / `ResizePseudoConsole`).
    ///
    /// # Errors
    /// [`BackendError::Closed`] once the backend has stopped.
    fn resize(&self, size: TermSize) -> Result<(), BackendError>;

    /// Ends the program and releases the backend's resources in the background. Idempotent;
    /// never blocks.
    fn shutdown(&self);
}

/// A backend with no program: it emits `bytes` once and ignores input. Useful for demos,
/// screenshots and benchmarks that need terminal content without a PTY.
#[must_use]
pub fn replay(bytes: Vec<u8>) -> (Box<dyn TerminalBackend>, Receiver<BackendEvent>) {
    let (sender, receiver) = crossbeam_channel::unbounded();
    // The receiver is alive here, so this cannot fail.
    let _ = sender.send(BackendEvent::Output(bytes));
    (Box::new(Replay { _events: sender }), receiver)
}

/// See [`replay`]. Keeps the sender so the channel stays open until the backend is dropped.
#[derive(Debug)]
struct Replay {
    _events: crossbeam_channel::Sender<BackendEvent>,
}

impl TerminalBackend for Replay {
    fn write(&self, _bytes: &[u8]) -> Result<(), BackendError> {
        Ok(())
    }

    fn resize(&self, _size: TermSize) -> Result<(), BackendError> {
        Ok(())
    }

    fn shutdown(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_clamps_to_the_engine_minimum() {
        assert_eq!(TermSize::new(0, 0).clamped(), TermSize::new(2, 1));
        assert_eq!(TermSize::new(80, 24).clamped(), TermSize::new(80, 24));
    }

    #[test]
    fn pixel_size_is_the_whole_text_area_and_saturates() {
        let size = TermSize {
            columns: 80,
            lines: 24,
            cell_width: 9,
            cell_height: 18,
        };
        assert_eq!((size.pixel_width(), size.pixel_height()), (720, 432));
        let huge = TermSize {
            cell_width: u16::MAX,
            ..size
        };
        assert_eq!(huge.pixel_width(), u16::MAX);
    }

    #[test]
    fn replay_emits_its_bytes_once() {
        let (backend, events) = replay(b"hello".to_vec());
        assert_eq!(
            events.try_recv(),
            Ok(BackendEvent::Output(b"hello".to_vec()))
        );
        assert!(events.try_recv().is_err());
        assert!(backend.write(b"ignored").is_ok());
    }
}
