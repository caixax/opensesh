//! What every backend here shares: the engine's event channel (with the session log), and the
//! handle the engine writes to.

use std::path::Path;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, TrySendError};
use opensesh_ssh::log::SessionLog;
use opensesh_term::backend::{BackendError, BackendEvent, TermSize, TerminalBackend};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

/// What the engine asks of a backend.
#[derive(Debug)]
pub(crate) enum Command {
    Input(Vec<u8>),
    Resize(TermSize),
    Shutdown,
}

/// The handle the terminal engine writes to.
struct Handle {
    commands: UnboundedSender<Command>,
}

impl TerminalBackend for Handle {
    fn write(&self, bytes: &[u8]) -> Result<(), BackendError> {
        self.commands
            .send(Command::Input(bytes.to_vec()))
            .map_err(|_| BackendError::Closed)
    }

    fn resize(&self, size: TermSize) -> Result<(), BackendError> {
        self.commands
            .send(Command::Resize(size))
            .map_err(|_| BackendError::Closed)
    }

    fn shutdown(&self) {
        let _ = self.commands.send(Command::Shutdown);
    }
}

/// A backend's two ends: what [`start`] gives the engine, and what the task keeps.
pub(crate) struct Ends {
    pub output: Output,
    pub commands: UnboundedReceiver<Command>,
}

/// What [`channels`] gives: the engine's two ends, the backend task's, and the runtime.
pub(crate) type Channels = (
    Box<dyn TerminalBackend>,
    Receiver<BackendEvent>,
    Ends,
    &'static tokio::runtime::Runtime,
);

/// What a backend starts with: the engine's handle and events, the task's ends, the session log
/// (`None` for none), and the runtime to spawn the task on.
///
/// # Errors
///
/// [`BackendError::Thread`] when the runtime isn't available.
pub(crate) fn channels(log: Option<(&Path, bool)>) -> Result<Channels, BackendError> {
    let runtime = opensesh_ssh::runtime().ok_or_else(|| {
        BackendError::Thread(std::io::Error::other(
            "the connection runtime is not running",
        ))
    })?;
    // Bounded: a flood of output waits here instead of filling memory.
    let (events, receiver) = crossbeam_channel::bounded(512);
    let (commands, command_receiver) = unbounded_channel();
    let log = log.and_then(|(path, raw)| {
        SessionLog::open(path, raw)
            .map_err(
                |error| tracing::warn!(path = %path.display(), "session log not written: {error}"),
            )
            .ok()
    });
    Ok((
        Box::new(Handle { commands }),
        receiver,
        Ends {
            output: Output { events, log },
            commands: command_receiver,
        },
        runtime,
    ))
}

/// The engine's event channel, and the session log.
pub(crate) struct Output {
    events: Sender<BackendEvent>,
    log: Option<SessionLog>,
}

impl Output {
    /// Sends an event, waiting (without blocking the runtime) while the channel is full. `false`
    /// when the engine is gone.
    pub async fn send(&self, event: BackendEvent) -> bool {
        let mut event = event;
        loop {
            match self.events.try_send(event) {
                Ok(()) => return true,
                Err(TrySendError::Full(back)) => {
                    event = back;
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
                Err(TrySendError::Disconnected(_)) => return false,
            }
        }
    }

    /// What the other end printed (logged).
    pub async fn data(&self, bytes: &[u8]) -> bool {
        if bytes.is_empty() {
            return true;
        }
        if let Some(log) = &self.log {
            log.write(bytes);
        }
        self.send(BackendEvent::Output(bytes.to_vec())).await
    }

    /// Bytes shown as if received (local echo, a hex view), not logged.
    pub async fn show(&self, bytes: Vec<u8>) -> bool {
        self.send(BackendEvent::Output(bytes)).await
    }

    /// A line of our own (dim), not logged.
    pub async fn note(&self, text: &str) {
        let line = format!("\x1b[2m{text}\x1b[0m\r\n");
        self.send(BackendEvent::Output(line.into_bytes())).await;
    }

    /// A line of our own in yellow, not logged.
    pub async fn caution(&self, text: &str) {
        let line = format!("\x1b[33m{text}\x1b[0m\r\n");
        self.send(BackendEvent::Output(line.into_bytes())).await;
    }

    /// Why the session ended (for people), then the end.
    pub async fn fail(&self, reason: &str) {
        self.send(BackendEvent::Error(reason.to_owned())).await;
        self.send(BackendEvent::Exited(None)).await;
    }

    /// The other end closed: the session is over.
    pub async fn ended(&self, code: Option<i32>) {
        self.send(BackendEvent::Exited(code)).await;
    }
}
