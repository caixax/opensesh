//! Mosh (PLAN Sprint 12, ADR 0032): the built-in SSH client connects as for an SSH session
//! (the same questions in the pane), starts `mosh-server new` on the host and closes again; then
//! `mosh-client` runs here in a PTY, told the server's address and UDP port on its command line
//! and the session key in `MOSH_KEY` (never on a command line, never logged).
//!
//! Mosh needs its own two programs: `mosh-server` on the host and `mosh-client` here. When one is
//! missing the pane says so, with how to install it. Mosh reaches the server straight over UDP:
//! jump hosts and proxies carry only the SSH part.

use std::future::Future;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use opensesh_ssh::backend::{Status, StatusSink, progress_notes};
use opensesh_ssh::connect;
use opensesh_ssh::mosh::{self as server, MoshConnect, ServerError};
use opensesh_ssh::prompt::Asker;
use opensesh_ssh::spec::ConnectSpec;
use opensesh_term::backend::{BackendError, BackendEvent, TermSize, TerminalBackend};
use opensesh_term::shell::ShellCommand;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::output::{self, Command, Output};

/// A mosh session.
#[derive(Debug, Clone)]
pub struct MoshSpec {
    /// The SSH connection that starts the server (its last hop is the host).
    pub connect: ConnectSpec,
    /// `TERM` for the server and the client.
    pub term: String,
    /// The `mosh-client` to run; `None` looks for it ([`find_client`]).
    pub client: Option<PathBuf>,
    /// Start the server but not the client (test runs, which start no programs).
    pub dry_run: bool,
}

/// `mosh-client` on the `PATH` (and, on macOS, where Homebrew puts it: apps started from the
/// Finder don't get the shell's `PATH`).
#[must_use]
pub fn find_client() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "mosh-client.exe"
    } else {
        "mosh-client"
    };
    let mut folders: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    if cfg!(target_os = "macos") {
        folders.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    }
    folders
        .into_iter()
        .filter(|folder| folder.is_absolute())
        .map(|folder| folder.join(name))
        .find(|path| path.is_file())
}

/// How to get `mosh-client` on this system.
#[must_use]
pub fn client_hint() -> &'static str {
    if cfg!(windows) {
        "mosh-client isn't on this computer's PATH. Windows has no build of its own: install mosh \
         in Cygwin and add its bin folder to PATH, or run mosh in a WSL tab."
    } else if cfg!(target_os = "macos") {
        "mosh-client isn't installed here. Install mosh with Homebrew: brew install mosh."
    } else {
        "mosh-client isn't installed here. Install the mosh package: sudo apt install mosh \
         (Debian, Ubuntu), sudo dnf install mosh (Fedora), sudo pacman -S mosh (Arch)."
    }
}

/// How to get `mosh-server` on the host.
pub const SERVER_HINT: &str = "Install the mosh package on the server: apt install mosh, dnf \
     install mosh, pacman -S mosh or brew install mosh. Its UDP ports (60000 to 61000) must be \
     open.";

/// Starts a mosh session: the SSH part asks its questions through `asker` and shows its state
/// through `status`, like an SSH session's.
///
/// # Errors
///
/// [`BackendError::Thread`] when the connection runtime isn't available.
pub fn start(
    spec: MoshSpec,
    size: TermSize,
    asker: Asker,
    status: StatusSink,
) -> Result<
    (
        Box<dyn TerminalBackend>,
        crossbeam_channel::Receiver<BackendEvent>,
    ),
    BackendError,
> {
    let (backend, events, ends, runtime) = output::channels(None)?;
    runtime.spawn(run(spec, size, asker, status, ends.output, ends.commands));
    Ok((backend, events))
}

/// How starting the server went.
enum Started {
    /// Where `mosh-client` goes.
    Ready(IpAddr, MoshConnect),
    /// Worth another try (Enter).
    Retry(String),
    /// Not worth another try.
    Failed(String),
    /// The pane closed.
    Shutdown,
}

async fn run(
    spec: MoshSpec,
    size: TermSize,
    asker: Asker,
    status: StatusSink,
    output: Output,
    mut commands: UnboundedReceiver<Command>,
) {
    let mut size = size;
    // Before connecting: without a client the server would wait for nothing.
    let client = if spec.dry_run {
        None
    } else {
        match spec.client.clone().or_else(find_client) {
            Some(path) if path.is_file() => Some(path),
            _ => {
                status(Status::Ended);
                output.fail(client_hint()).await;
                return;
            }
        }
    };
    let (address, session) = loop {
        match start_server(&spec, &asker, &status, &output, &mut commands, &mut size).await {
            Started::Ready(address, session) => break (address, session),
            Started::Shutdown => return,
            Started::Failed(reason) => {
                status(Status::Ended);
                output.fail(&reason).await;
                return;
            }
            Started::Retry(reason) => {
                status(Status::Disconnected {
                    code: "mosh",
                    reason: reason.clone(),
                    retry_in: None,
                });
                output.caution(&reason).await;
                output.note("Press Enter to try again.").await;
                if !wait_for_enter(&mut commands, &mut size).await {
                    return;
                }
            }
        }
    };
    if spec.connect.hops.len() > 1 || spec.connect.proxy.is_some() {
        output
            .caution(&format!(
                "Jump hosts and proxies carry only the SSH part: mosh reaches {address} directly \
                 over UDP."
            ))
            .await;
    }
    let Some(client) = client else {
        output
            .note(&format!(
                "mosh-server is listening on UDP port {} of {address}. A test run doesn't start \
                 mosh-client.",
                session.port
            ))
            .await;
        output.ended(None).await;
        return;
    };
    output
        .note(&format!(
            "Starting mosh-client for {address}, UDP port {}...",
            session.port
        ))
        .await;
    run_client(
        &client, &spec.term, address, &session, size, &output, commands,
    )
    .await;
}

/// Connects over SSH, starts `mosh-server` and finds the address to reach it on.
async fn start_server(
    spec: &MoshSpec,
    asker: &Asker,
    status: &StatusSink,
    output: &Output,
    commands: &mut UnboundedReceiver<Command>,
    size: &mut TermSize,
) -> Started {
    let Some(hop) = spec.connect.hops.last() else {
        return Started::Failed("no host to connect to".to_owned());
    };
    let notes = progress_notes(output.sender(), Arc::clone(status));
    let connection = match follow(
        connect::connect(&spec.connect, asker, &notes),
        commands,
        size,
    )
    .await
    {
        None => return Started::Shutdown,
        Some(Err(error)) => return Started::Retry(error.to_string()),
        Some(Ok(connection)) => connection,
    };
    status(Status::Connected);
    output
        .note(&format!("Starting mosh-server on {}...", hop.host))
        .await;
    let started = follow(
        server::start_server(&connection, &spec.term, *size),
        commands,
        size,
    )
    .await;
    connection.close().await;
    let session = match started {
        None => return Started::Shutdown,
        Some(Ok(session)) => session,
        Some(Err(ServerError::Missing)) => {
            return Started::Failed(format!(
                "mosh-server isn't installed on {} (or isn't on the PATH a command gets there). \
                 {SERVER_HINT}",
                hop.host
            ));
        }
        Some(Err(error @ ServerError::Failed(_))) => return Started::Failed(error.to_string()),
        Some(Err(ServerError::Ssh(error))) => return Started::Retry(error.to_string()),
    };
    // The address SSH reached (mosh-client wants a numeric one).
    let resolving = tokio::net::lookup_host((hop.host.as_str(), hop.port));
    match follow(resolving, commands, size).await {
        None => Started::Shutdown,
        Some(Ok(mut addresses)) => match addresses.next() {
            Some(address) => Started::Ready(address.ip(), session),
            None => Started::Retry(format!("{} has no address", hop.host)),
        },
        Some(Err(error)) => Started::Retry(format!("could not find {}: {error}", hop.host)),
    }
}

/// Runs `mosh-client` in a PTY and carries the pane's input, sizes and output until it ends.
async fn run_client(
    client: &Path,
    term: &str,
    address: IpAddr,
    session: &MoshConnect,
    size: TermSize,
    output: &Output,
    mut commands: UnboundedReceiver<Command>,
) {
    let command = ShellCommand::program(client)
        .arg(address.to_string())
        .arg(session.port.to_string())
        .env("MOSH_KEY", session.key.as_str())
        .env("TERM", term);
    let (pty, events) = match opensesh_term::pty::spawn(command, size) {
        Ok(started) => started,
        Err(error) => {
            output
                .fail(&format!("could not start mosh-client: {error}"))
                .await;
            return;
        }
    };
    // The client's output, its end and its errors go to the pane as they are.
    let sender = output.sender();
    let forwarding = std::thread::Builder::new()
        .name("opensesh-mosh".to_owned())
        .spawn(move || {
            for event in &events {
                if sender.send(event).is_err() {
                    break;
                }
            }
        });
    if let Err(error) = forwarding {
        pty.shutdown();
        output
            .fail(&format!("could not start a thread: {error}"))
            .await;
        return;
    }
    while let Some(command) = commands.recv().await {
        match command {
            Command::Input(bytes) => {
                let _ = pty.write(&bytes);
            }
            Command::Resize(next) => {
                let _ = pty.resize(next);
            }
            Command::Shutdown => break,
        }
    }
    pty.shutdown();
}

/// Runs `future` while following the pane: sizes are kept, typing is dropped, and closing the
/// pane gives `None`.
async fn follow<T>(
    future: impl Future<Output = T>,
    commands: &mut UnboundedReceiver<Command>,
    size: &mut TermSize,
) -> Option<T> {
    tokio::pin!(future);
    loop {
        tokio::select! {
            result = &mut future => return Some(result),
            command = commands.recv() => match command {
                Some(Command::Resize(next)) => *size = next,
                Some(Command::Input(_)) => {}
                Some(Command::Shutdown) | None => return None,
            },
        }
    }
}

/// Waits for Enter; `false` when the pane closed first.
async fn wait_for_enter(commands: &mut UnboundedReceiver<Command>, size: &mut TermSize) -> bool {
    loop {
        match commands.recv().await {
            Some(Command::Input(bytes)) if bytes.contains(&b'\r') || bytes.contains(&b'\n') => {
                return true;
            }
            Some(Command::Input(_)) => {}
            Some(Command::Resize(next)) => *size = next,
            Some(Command::Shutdown) | None => return false,
        }
    }
}

#[cfg(test)]
mod tests;
