//! A terminal backend over an SSH session channel (PLAN §3.4, Sprint 7): it connects, opens a
//! PTY with the pane's size, sends the environment, starts the shell (or a command), and carries
//! bytes both ways. Keystrokes, resizes and shutdown arrive through [`TerminalBackend`]; output,
//! the exit and errors leave through the engine's [`BackendEvent`] channel.
//!
//! **Reconnection.** When the connection is lost (not when the remote shell exits), the pane
//! shows "Connection lost" and waits for Enter; with automatic reconnection it also retries by
//! itself with backoff (1, 2, 4... up to 30 seconds). The terminal keeps its scrollback. A failure
//! that retrying won't fix (a refused host key, no working authentication, a cancelled prompt)
//! never retries by itself.
//!
//! Progress and state also go to a [`StatusSink`] for the pane's overlays.
//!
//! Once connected, the OS detection and "install my key" ([`Options`]) run on channels of their
//! own while the shell starts.

use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, TrySendError};
use opensesh_term::backend::{BackendError, BackendEvent, TermSize, TerminalBackend};
use russh::ChannelMsg;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::SshError;
use crate::connect::{self, Connection, Note, Notes};
use crate::copy_id::{self, Installed};
use crate::log::SessionLog;
use crate::osdetect;
use crate::prompt::Asker;
use crate::spec::{ConnectSpec, SessionSpec};

/// What the pane shows about the connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Reaching a hop.
    Connecting {
        /// Which hop (0-based).
        index: usize,
        /// How many.
        count: usize,
        /// `user@host:port`.
        label: String,
    },
    /// Authenticating.
    Authenticating {
        /// `user@host:port`.
        label: String,
    },
    /// The session is up.
    Connected,
    /// Not connected: why, and in how many seconds it retries by itself (none: waits for Enter).
    Disconnected {
        /// A short code (`network`, `auth`, `host-key`, `lost`...).
        code: &'static str,
        /// Why, for people.
        reason: String,
        /// Seconds until the automatic retry.
        retry_in: Option<u64>,
    },
    /// The remote shell exited.
    Ended,
    /// The remote OS, as a host icon name.
    OsDetected(&'static str),
    /// How installing the public key went ([`Options::install_key`]).
    KeyInstall(KeyInstall),
}

/// How installing a public key went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyInstall {
    /// It was added to `~/.ssh/authorized_keys`.
    Added,
    /// It was there already.
    AlreadyThere,
    /// Why it wasn't installed.
    Failed(String),
}

/// Where the backend reports its status. Must return at once.
pub type StatusSink = Arc<dyn Fn(Status) + Send + Sync>;

/// Extra behavior of a session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// Detect the remote OS once connected.
    pub detect_os: bool,
    /// A public key line to add to the server's `authorized_keys` once connected (the first
    /// connection only).
    pub install_key: Option<String>,
}

enum Command {
    Input(Vec<u8>),
    Resize(TermSize),
    Shutdown,
}

/// The handle the terminal engine writes to.
struct SshBackend {
    commands: UnboundedSender<Command>,
}

impl TerminalBackend for SshBackend {
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

/// Starts a session: returns the input side and the event channel for the terminal engine.
///
/// # Errors
///
/// [`BackendError::Thread`] when the SSH runtime isn't available.
pub fn start(
    spec: ConnectSpec,
    session: SessionSpec,
    options: Options,
    asker: Asker,
    status: StatusSink,
) -> Result<(Box<dyn TerminalBackend>, Receiver<BackendEvent>), BackendError> {
    let runtime = crate::runtime().ok_or_else(|| {
        BackendError::Thread(std::io::Error::other("the SSH runtime is not running"))
    })?;
    // Bounded: a flood of output waits here instead of filling memory (the engine reads at its
    // own pace).
    let (events, receiver) = crossbeam_channel::bounded(512);
    let (commands, command_receiver) = unbounded_channel();
    let log = session.log.as_ref().and_then(|log| {
        SessionLog::open(&log.path, log.raw)
            .map_err(|error| {
                tracing::warn!(path = %log.path.display(), "session log not written: {error}");
            })
            .ok()
    });
    let output = Output { events, log };
    runtime.spawn(run(
        spec,
        session,
        options,
        asker,
        status,
        output,
        command_receiver,
    ));
    Ok((Box::new(SshBackend { commands }), receiver))
}

/// The engine's event channel, and the session log.
struct Output {
    events: Sender<BackendEvent>,
    log: Option<SessionLog>,
}

impl Output {
    /// Sends an event, waiting (without blocking the runtime) while the channel is full. `false`
    /// when the engine is gone.
    async fn send(&self, event: BackendEvent) -> bool {
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

    /// What the server printed.
    async fn data(&self, bytes: &[u8]) -> bool {
        if let Some(log) = &self.log {
            log.write(bytes);
        }
        self.send(BackendEvent::Output(bytes.to_vec())).await
    }

    /// A line of our own (dim), not logged. Dropped if the channel is full.
    fn note(&self, text: &str) {
        let line = format!("\x1b[2m{text}\x1b[0m\r\n");
        let _ = self
            .events
            .try_send(BackendEvent::Output(line.into_bytes()));
    }

    /// A line of our own in red, not logged.
    fn warn(&self, text: &str) {
        let line = format!("\x1b[31m{text}\x1b[0m\r\n");
        let _ = self
            .events
            .try_send(BackendEvent::Output(line.into_bytes()));
    }
}

/// How one connection ended.
enum Ended {
    /// The remote shell exited (its code, if it gave one).
    Exited(Option<i32>),
    /// The pane closed.
    Shutdown,
    /// Couldn't connect, or the connection was lost.
    Lost {
        code: &'static str,
        reason: String,
        /// Retrying won't help (host key, authentication, cancelled).
        final_error: bool,
    },
}

/// The wait before automatic attempt `attempt` (1-based): 1, 2, 4, 8, 16, then 30 seconds.
#[must_use]
pub fn backoff(attempt: u32) -> Duration {
    let seconds = 1_u64 << attempt.saturating_sub(1).min(5);
    Duration::from_secs(seconds.min(30))
}

async fn run(
    spec: ConnectSpec,
    session: SessionSpec,
    options: Options,
    asker: Asker,
    status: StatusSink,
    output: Output,
    mut commands: UnboundedReceiver<Command>,
) {
    let mut size = session.size;
    let mut failures: u32 = 0;
    // Installed on the first connection that gets that far.
    let mut install_key = options.install_key.clone();
    loop {
        let ended = once(
            &spec,
            &session,
            options.detect_os,
            &mut install_key,
            &asker,
            &status,
            &output,
            &mut commands,
            &mut size,
        )
        .await;
        let (code, reason, final_error) = match ended {
            Ended::Exited(code) => {
                status(Status::Ended);
                output.send(BackendEvent::Exited(code)).await;
                return;
            }
            Ended::Shutdown => return,
            Ended::Lost {
                code,
                reason,
                final_error,
            } => (code, reason, final_error),
        };
        failures = failures.saturating_add(1);
        let automatic = session.reconnect.automatic
            && !final_error
            && failures <= session.reconnect.max_attempts;
        let delay = automatic.then(|| backoff(failures));
        status(Status::Disconnected {
            code,
            reason: reason.clone(),
            retry_in: delay.map(|delay| delay.as_secs()),
        });
        output.warn(&format!("\r\n{reason}"));
        match delay {
            Some(delay) => output.note(&format!(
                "Reconnecting in {} s. Press Enter to reconnect now.",
                delay.as_secs()
            )),
            None => output.note("Press Enter to reconnect."),
        }
        match wait_for_enter(&mut commands, delay, &mut size).await {
            Wait::Reconnect => {}
            Wait::Shutdown => return,
        }
    }
}

enum Wait {
    Reconnect,
    Shutdown,
}

/// Waits for Enter (or the timer), keeping the size up to date; other keys are ignored.
async fn wait_for_enter(
    commands: &mut UnboundedReceiver<Command>,
    delay: Option<Duration>,
    size: &mut TermSize,
) -> Wait {
    let timer = async {
        match delay {
            Some(delay) => tokio::time::sleep(delay).await,
            None => std::future::pending::<()>().await,
        }
    };
    tokio::pin!(timer);
    loop {
        tokio::select! {
            () = &mut timer => return Wait::Reconnect,
            command = commands.recv() => match command {
                Some(Command::Input(bytes)) if bytes.contains(&b'\r') || bytes.contains(&b'\n') => {
                    return Wait::Reconnect;
                }
                Some(Command::Input(_)) => {}
                Some(Command::Resize(next)) => *size = next,
                Some(Command::Shutdown) | None => return Wait::Shutdown,
            },
        }
    }
}

fn lost(error: &SshError) -> Ended {
    Ended::Lost {
        code: error.code(),
        reason: error.to_string(),
        final_error: matches!(
            error,
            SshError::HostKey { .. }
                | SshError::Auth { .. }
                | SshError::Cancelled
                | SshError::SecretsLocked
        ),
    }
}

/// One connection: connect, run the session, return how it ended.
#[allow(clippy::too_many_arguments)] // The state of `run`, lent for one connection.
async fn once(
    spec: &ConnectSpec,
    session: &SessionSpec,
    detect_os: bool,
    install_key: &mut Option<String>,
    asker: &Asker,
    status: &StatusSink,
    output: &Output,
    commands: &mut UnboundedReceiver<Command>,
    size: &mut TermSize,
) -> Ended {
    let notes: Notes = {
        let status = Arc::clone(status);
        let events = output.events.clone();
        Arc::new(move |note: Note| {
            let (text, next) = match note {
                Note::Connecting {
                    index,
                    count,
                    label,
                } => (
                    if count > 1 {
                        format!("Connecting to {label} ({} of {count})...", index + 1)
                    } else {
                        format!("Connecting to {label}...")
                    },
                    Some(Status::Connecting {
                        index,
                        count,
                        label,
                    }),
                ),
                Note::Authenticating { label } => (
                    format!("Authenticating as {label}..."),
                    Some(Status::Authenticating { label }),
                ),
                Note::Banner(banner) => (banner.replace('\n', "\r\n"), None),
            };
            let line = format!("\x1b[2m{text}\x1b[0m\r\n");
            let _ = events.try_send(BackendEvent::Output(line.into_bytes()));
            if let Some(next) = next {
                status(next);
            }
        })
    };
    // Connect, while still following resizes and shutdown.
    let connecting = connect::connect(spec, asker, &notes);
    tokio::pin!(connecting);
    let connection = loop {
        tokio::select! {
            result = &mut connecting => match result {
                Ok(connection) => break connection,
                Err(error) => return lost(&error),
            },
            command = commands.recv() => match command {
                Some(Command::Resize(next)) => *size = next,
                Some(Command::Input(_)) => {}
                Some(Command::Shutdown) | None => return Ended::Shutdown,
            },
        }
    };
    status(Status::Connected);
    // The OS is detected, and the key installed, on channels of their own while the shell runs.
    let key = install_key.take();
    let detection = async {
        if let Some(line) = key {
            let result = match copy_id::install(&connection, &line).await {
                Ok(Installed::Added) => KeyInstall::Added,
                Ok(Installed::AlreadyThere) => KeyInstall::AlreadyThere,
                Err(error) => KeyInstall::Failed(error.to_string()),
            };
            status(Status::KeyInstall(result));
        }
        if detect_os && let Some(icon) = osdetect::detect(&connection).await {
            status(Status::OsDetected(icon));
        }
    };
    let (result, ()) = tokio::join!(
        shell(&connection, session, output, commands, size),
        detection
    );
    let ended = result.unwrap_or_else(|error| lost(&error));
    connection.close().await;
    ended
}

/// Opens the session channel with a PTY and runs it until it ends.
async fn shell(
    connection: &Connection,
    session: &SessionSpec,
    output: &Output,
    commands: &mut UnboundedReceiver<Command>,
    size: &mut TermSize,
) -> Result<Ended, SshError> {
    let target = connection.target()?;
    let mut channel = target.channel_open_session().await?;
    let refused = |what: &str| SshError::Refused {
        what: what.to_owned(),
    };
    channel
        .request_pty(
            true,
            &session.term,
            u32::from(size.columns),
            u32::from(size.lines),
            u32::from(size.pixel_width()),
            u32::from(size.pixel_height()),
            &[],
        )
        .await
        .map_err(|_| refused("a terminal (PTY)"))?;
    for (name, value) in &session.env {
        // Servers refuse variables they don't accept (AcceptEnv); that isn't an error.
        let _ = channel.set_env(false, name.as_str(), value.as_str()).await;
    }
    match &session.command {
        Some(command) => channel.exec(true, command.as_str()).await,
        None => channel.request_shell(true).await,
    }
    .map_err(|_| refused("a shell"))?;
    let mut startup = session
        .startup
        .as_deref()
        .filter(|text| !text.trim().is_empty())
        .map(|text| {
            let mut typed = text.replace("\r\n", "\r").replace('\n', "\r");
            if !typed.ends_with('\r') {
                typed.push('\r');
            }
            typed.into_bytes()
        });
    let mut exit_code: Option<i32> = None;
    let mut exited = false;
    loop {
        tokio::select! {
            message = channel.wait() => match message {
                Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                    if !output.data(&data).await {
                        let _ = channel.close().await;
                        return Ok(Ended::Shutdown);
                    }
                    // The startup snippet goes once the shell printed something (its prompt).
                    if let Some(text) = startup.take() {
                        channel.data(text.as_slice()).await?;
                    }
                }
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    exited = true;
                    exit_code = i32::try_from(exit_status).ok();
                }
                Some(ChannelMsg::ExitSignal { .. }) => {
                    exited = true;
                    exit_code = None;
                }
                Some(ChannelMsg::Eof) => exited = true,
                Some(ChannelMsg::Close) | None => {
                    let ended = if exited {
                        Ended::Exited(exit_code)
                    } else if connection.is_closed() {
                        Ended::Lost {
                            code: "lost",
                            reason: "Connection lost.".to_owned(),
                            final_error: false,
                        }
                    } else {
                        Ended::Exited(exit_code)
                    };
                    return Ok(ended);
                }
                Some(_) => {}
            },
            command = commands.recv() => match command {
                Some(Command::Input(bytes)) => {
                    if channel.data(bytes.as_slice()).await.is_err() {
                        return Ok(Ended::Lost {
                            code: "lost",
                            reason: "Connection lost.".to_owned(),
                            final_error: false,
                        });
                    }
                }
                Some(Command::Resize(next)) => {
                    *size = next;
                    let _ = channel
                        .window_change(
                            u32::from(next.columns),
                            u32::from(next.lines),
                            u32::from(next.pixel_width()),
                            u32::from(next.pixel_height()),
                        )
                        .await;
                }
                Some(Command::Shutdown) | None => {
                    let _ = channel.close().await;
                    return Ok(Ended::Shutdown);
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_to_thirty_seconds() {
        let delays: Vec<u64> = (1..=8).map(|attempt| backoff(attempt).as_secs()).collect();
        assert_eq!(delays, [1, 2, 4, 8, 16, 30, 30, 30]);
    }

    #[test]
    fn detection_command_is_constant() {
        assert!(osdetect::COMMAND.contains("os-release"));
    }
}
