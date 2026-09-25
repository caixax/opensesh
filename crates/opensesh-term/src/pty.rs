//! The local PTY backend: a shell on a pseudoterminal through `portable-pty` 0.9 (ConPTY on
//! Windows), shaped as a push [`TerminalBackend`] (ADR 0012).
//!
//! [`spawn`] returns at once; the work runs on the backend's own threads:
//!
//! - **control** starts the program (resolving the shell and reading the environment, which can
//!   block on NSS or the registry), then writes input, applies resizes and runs the teardown;
//! - **reader** does blocking reads and pushes [`BackendEvent::Output`] into a bounded channel
//!   (back-pressure for `cat` of huge files). It never touches the `Term`. On Unix it polls the
//!   PTY together with a wake pipe, so a shutdown can stop it; after the receiver is gone it keeps
//!   draining (on Windows an undrained pipe blocks `ClosePseudoConsole`);
//! - **waiter** sits in `Child::wait()` (on Windows the reader sees no EOF when the shell exits);
//! - on Windows, a short-lived **close** thread drops the pseudoconsole, since
//!   `ClosePseudoConsole` can block on hosts older than Windows 11 24H2.
//!
//! Teardown: on shutdown the shell is hung up (Unix: `SIGHUP` to its process group and to the
//! terminal's foreground group) or terminated (Windows), the reader is stopped or drained to EOF,
//! and the PTY is closed; the program's exit code, when known, is reported last. When the shell
//! exits on its own, its remaining output gets a short grace period first.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use portable_pty::{Child, MasterPty, PtySize};

use crate::backend::{BackendError, BackendEvent, TermSize, TerminalBackend};
use crate::shell::{self, ShellCommand};

/// Output chunks in flight between the reader and the engine (each at most [`READ_BUFFER`]).
const EVENT_QUEUE: usize = 16;

/// Bytes per read.
const READ_BUFFER: usize = 64 * 1024;

/// After the program exits, how long its remaining output may take to arrive before the PTY is
/// closed (disowned background jobs can keep a Unix PTY open indefinitely).
const EXIT_GRACE: Duration = Duration::from_millis(if cfg!(windows) { 200 } else { 250 });

/// How long the teardown waits for the reader to finish before giving up on it.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(3);

/// Starts `command` on a new pseudoterminal of `size`. Returns at once: the program is started on
/// the backend's own thread, and if that fails the channel carries a [`BackendEvent::Error`]
/// (then closes).
///
/// # Errors
/// [`BackendError::Thread`] if the backend's control thread can't be created.
pub fn spawn(
    command: ShellCommand,
    size: TermSize,
) -> Result<(Box<dyn TerminalBackend>, Receiver<BackendEvent>), BackendError> {
    let (events, event_rx) = crossbeam_channel::bounded(EVENT_QUEUE);
    let (control, control_rx) = crossbeam_channel::unbounded();
    let shared = Arc::new(PtyShared::default());
    let control_for_thread = control.clone();
    let shared_for_thread = Arc::clone(&shared);
    thread::Builder::new()
        .name("opensesh-pty-control".to_owned())
        .spawn(move || {
            control_thread(
                &command,
                size.clamped(),
                &control_rx,
                &control_for_thread,
                &events,
                &shared_for_thread,
            );
        })
        .map_err(BackendError::Thread)?;
    Ok((Box::new(LocalPty { control, shared }), event_rx))
}

/// The handle returned by [`spawn`]. Dropping it shuts the program down.
#[derive(Debug)]
struct LocalPty {
    control: Sender<Control>,
    shared: Arc<PtyShared>,
}

impl TerminalBackend for LocalPty {
    fn write(&self, bytes: &[u8]) -> Result<(), BackendError> {
        self.control
            .send(Control::Write(bytes.to_vec()))
            .map_err(|_| BackendError::Closed)
    }

    fn resize(&self, size: TermSize) -> Result<(), BackendError> {
        self.control
            .send(Control::Resize(size.clamped()))
            .map_err(|_| BackendError::Closed)
    }

    fn shutdown(&self) {
        if self.shared.stopping.swap(true, Ordering::AcqRel) {
            return;
        }
        // Unix: hang up right away, since the control thread may be stuck writing input to a
        // program that doesn't read it, and the hangup unblocks that write. Windows: the control
        // thread closes the pseudoconsole, which ends every program attached to it.
        #[cfg(unix)]
        self.shared.hang_up();
        let _ = self.control.send(Control::Shutdown);
    }
}

impl Drop for LocalPty {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Messages to the control thread.
enum Control {
    Write(Vec<u8>),
    Resize(TermSize),
    Shutdown,
    ChildExited(Option<i32>),
    ReaderDone,
}

/// Process state shared by the handle, the control thread and the waiter.
#[derive(Debug, Default)]
struct PtyShared {
    stopping: AtomicBool,
    process: Mutex<Process>,
}

#[derive(Default)]
struct Process {
    /// The waiter reaped the process: never signal its (possibly reused) pid again.
    exited: bool,
    #[cfg(unix)]
    pid: Option<rustix::process::Pid>,
    /// Write end of the reader's wake pipe.
    #[cfg(unix)]
    wake: Option<std::os::fd::OwnedFd>,
    #[cfg(windows)]
    killer: Option<Box<dyn portable_pty::ChildKiller + Send + Sync>>,
}

impl std::fmt::Debug for Process {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Process")
            .field("exited", &self.exited)
            .finish_non_exhaustive()
    }
}

impl PtyShared {
    fn process(&self) -> MutexGuard<'_, Process> {
        self.process.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Unix: `SIGHUP` to the shell's process group and a wake-up for the reader. Never blocks.
    #[cfg(unix)]
    fn hang_up(&self) {
        let process = self.process();
        if !process.exited {
            if let Some(pid) = process.pid {
                // The shell is a session leader (portable-pty calls setsid): pgid == pid.
                let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::HUP);
            }
        }
        if let Some(wake) = &process.wake {
            let _ = rustix::io::write(wake, &[1]);
        }
    }

    /// Windows: `TerminateProcess` on the shell, the fallback when closing the pseudoconsole
    /// didn't end it. Never blocks.
    #[cfg(windows)]
    fn terminate(&self) {
        let mut process = self.process();
        if !process.exited {
            if let Some(killer) = process.killer.as_mut() {
                // portable-pty 0.9.0 returns Err when TerminateProcess *succeeds* (fixed on its
                // master branch, unreleased): the waiter confirms the exit instead.
                let _ = killer.kill();
            }
        }
    }
}

/// Errors while starting the program (reported as [`BackendEvent::Error`]).
#[derive(Debug, thiserror::Error)]
enum SpawnError {
    #[error(transparent)]
    Shell(#[from] shell::ShellError),
    #[error("could not open a pseudoterminal: {0}")]
    OpenPty(String),
    #[error("could not start `{program}`: {message}")]
    Spawn { program: String, message: String },
    #[error("could not set up the pseudoterminal: {0}")]
    Setup(String),
    #[error("could not start a terminal thread: {0}")]
    Thread(#[source] std::io::Error),
}

/// Everything the control thread owns once the program runs.
struct Io {
    master: Option<Box<dyn MasterPty + Send>>,
    #[cfg(unix)]
    writer: Option<std::fs::File>,
    #[cfg(windows)]
    writer: Option<Box<dyn std::io::Write + Send>>,
    #[cfg(unix)]
    shell_pid: Option<rustix::process::Pid>,
    write_failed: bool,
    /// Some input reached the program (on Windows, the engine's answer to the console host's
    /// opening cursor-position query comes first).
    wrote_input: bool,
    closing: bool,
}

fn control_thread(
    command: &ShellCommand,
    size: TermSize,
    messages: &Receiver<Control>,
    control: &Sender<Control>,
    events: &Sender<BackendEvent>,
    shared: &Arc<PtyShared>,
) {
    #[cfg(windows)]
    allow_ctrl_c();
    let mut io = match start(command, size, control, events, shared) {
        Ok(io) => io,
        Err(error) => {
            tracing::warn!(%error, "could not start the local terminal");
            let _ = events.send(BackendEvent::Error(error.to_string()));
            return;
        }
    };
    let mut exit: Option<Option<i32>> = None;
    let mut reader_done = false;
    // `None` while running; afterwards the deadline of the current teardown step.
    let mut deadline: Option<Instant> = None;
    // shutdown() may have run while the program was starting.
    if shared.stopping.load(Ordering::Acquire) {
        io.close(shared);
        deadline = Some(Instant::now() + CLOSE_TIMEOUT);
    }
    #[cfg(windows)]
    let mut terminated = false;
    loop {
        let message = match deadline {
            None => messages.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(deadline) => messages.recv_deadline(deadline),
        };
        match message {
            Ok(Control::Write(bytes)) => {
                if exit.is_none() {
                    io.write(&bytes);
                }
            }
            Ok(Control::Resize(size)) => io.resize(size),
            Ok(Control::Shutdown) | Err(RecvTimeoutError::Disconnected) => {
                if !io.closing {
                    io.close(shared);
                    deadline = Some(Instant::now() + CLOSE_TIMEOUT);
                }
            }
            Ok(Control::ChildExited(code)) => {
                exit = Some(code);
                if deadline.is_none() {
                    // Let the last output arrive, then close.
                    deadline = Some(Instant::now() + EXIT_GRACE);
                }
            }
            Ok(Control::ReaderDone) => reader_done = true,
            Err(RecvTimeoutError::Timeout) => {
                if !io.closing {
                    io.close(shared);
                    deadline = Some(Instant::now() + CLOSE_TIMEOUT);
                    continue;
                }
                #[cfg(windows)]
                if exit.is_none() && !terminated {
                    // Closing the pseudoconsole didn't end the shell.
                    shared.terminate();
                    terminated = true;
                    deadline = Some(Instant::now() + CLOSE_TIMEOUT);
                    continue;
                }
                tracing::warn!("the terminal program or reader did not finish in time");
                break;
            }
        }
        if reader_done && exit.is_some() {
            break;
        }
    }
    io.finish();
    // After the reader finished, so the program's last output comes first.
    let _ = events.send(BackendEvent::Exited(exit.flatten()));
}

/// Starts the program and its reader and waiter threads.
fn start(
    command: &ShellCommand,
    size: TermSize,
    control: &Sender<Control>,
    events: &Sender<BackendEvent>,
    shared: &Arc<PtyShared>,
) -> Result<Io, SpawnError> {
    let (builder, program) = shell::command_builder(command)?;
    let pair = portable_pty::native_pty_system()
        .openpty(pty_size(size))
        .map_err(|error| SpawnError::OpenPty(format!("{error:#}")))?;

    #[cfg(unix)]
    let (writer, read_fd) = unix::prepare(pair.master.as_ref())?;
    #[cfg(unix)]
    let (wake_read, wake_write) = unix::wake_pipe()?;
    #[cfg(windows)]
    let (writer, reader) = match windows::take_io(pair.master.as_ref()) {
        Ok(io) => io,
        Err(error) => {
            drop(pair.slave);
            windows::discard(pair.master);
            return Err(error);
        }
    };

    let child = match pair.slave.spawn_command(builder) {
        Ok(child) => child,
        Err(error) => {
            #[cfg(windows)]
            windows::abandon(pair.master, writer, reader);
            return Err(SpawnError::Spawn {
                program: program.display().to_string(),
                message: format!("{error:#}"),
            });
        }
    };
    // The parent's copy of the slave would keep the PTY open after the shell exits.
    drop(pair.slave);

    #[cfg(unix)]
    let shell_pid = child
        .process_id()
        .and_then(|pid| i32::try_from(pid).ok())
        .and_then(rustix::process::Pid::from_raw);
    #[cfg(unix)]
    {
        let mut process = shared.process();
        process.pid = shell_pid;
        process.wake = Some(wake_write);
    }
    #[cfg(windows)]
    {
        shared.process().killer = Some(child.clone_killer());
    }

    let mut io = Io {
        master: Some(pair.master),
        writer: Some(writer),
        #[cfg(unix)]
        shell_pid,
        write_failed: false,
        wrote_input: false,
        closing: false,
    };
    let reader_events = events.clone();
    let reader_control = control.clone();
    #[cfg(unix)]
    let reader = move || unix::read_loop(&read_fd, &wake_read, &reader_events, &reader_control);
    #[cfg(windows)]
    let reader = move || windows::read_loop(reader, &reader_events, &reader_control);
    let started = thread::Builder::new()
        .name("opensesh-pty-reader".to_owned())
        .spawn(reader)
        .map(drop)
        .and_then(|()| {
            let waiter_shared = Arc::clone(shared);
            let waiter_control = control.clone();
            thread::Builder::new()
                .name("opensesh-pty-waiter".to_owned())
                .spawn(move || wait_for_exit(child, &waiter_shared, &waiter_control))
                .map(drop)
        });
    if let Err(error) = started {
        io.close(shared);
        io.finish();
        return Err(SpawnError::Thread(error));
    }
    Ok(io)
}

impl Io {
    fn write(&mut self, bytes: &[u8]) {
        use std::io::Write;
        let Some(writer) = self.writer.as_mut() else {
            return;
        };
        match writer.write_all(bytes).and_then(|()| writer.flush()) {
            Ok(()) => self.wrote_input = true,
            Err(error) => {
                if !self.write_failed {
                    tracing::warn!(%error, "writing to the terminal failed");
                    self.write_failed = true;
                }
            }
        }
    }

    fn resize(&self, size: TermSize) {
        if let Some(master) = &self.master {
            if let Err(error) = master.resize(pty_size(size)) {
                tracing::warn!(error = %format!("{error:#}"), "resizing the terminal failed");
            }
        }
    }

    /// Starts closing. Unix: hang the programs up and stop the reader. Windows: close the
    /// pseudoconsole (the reader drains it until EOF).
    fn close(&mut self, shared: &PtyShared) {
        if self.closing {
            return;
        }
        self.closing = true;
        #[cfg(unix)]
        {
            shared.hang_up();
            // The foreground job may be in another process group (and may ignore the shell's
            // hangup); the kernel only hangs it up once every PTY descriptor is closed.
            if let Some(writer) = &self.writer {
                if let Ok(group) = rustix::termios::tcgetpgrp(writer) {
                    // Read from the terminal now, so not a stale (reusable) id.
                    if Some(group) != self.shell_pid {
                        let _ = rustix::process::kill_process_group(
                            group,
                            rustix::process::Signal::HUP,
                        );
                    }
                }
            }
        }
        #[cfg(windows)]
        {
            // Closing the pseudoconsole ends every program attached to it (the shell too, with
            // CTRL_CLOSE_EVENT), like closing a console window; the shell is terminated only if
            // that fails. Measured on the Windows 10 inbox host: an unsolicited cursor report
            // written just before the close made it exit without ending its programs (1 close
            // in 8 leaked a `ping ... >nul`), so the report is only sent when the host may still
            // be waiting for it.
            let _ = shared;
            if let (Some(master), Some(writer)) = (self.master.take(), self.writer.take()) {
                windows::close(master, writer, !self.wrote_input);
            }
        }
    }

    /// Releases the PTY. On Unix closing the last master descriptor hangs up whatever still
    /// runs on it.
    fn finish(&mut self) {
        self.writer = None;
        self.master = None;
    }
}

fn wait_for_exit(
    mut child: Box<dyn Child + Send + Sync>,
    shared: &PtyShared,
    control: &Sender<Control>,
) {
    let code = match child.wait() {
        Ok(status) if status.signal().is_some() => None,
        // Windows exit codes are u32 (NTSTATUS values included): keep their bits.
        Ok(status) => Some(i32::from_ne_bytes(status.exit_code().to_ne_bytes())),
        Err(error) => {
            tracing::warn!(%error, "waiting for the terminal program failed");
            None
        }
    };
    {
        let mut process = shared.process();
        process.exited = true;
        #[cfg(windows)]
        {
            process.killer = None;
        }
    }
    let _ = control.send(Control::ChildExited(code));
}

/// `TermSize` to portable-pty's size: ConPTY takes `i16` coordinates, and the pixel size is the
/// whole text area (what `TIOCSWINSZ` expects).
fn pty_size(size: TermSize) -> PtySize {
    const MAX: u16 = i16::MAX.unsigned_abs();
    let size = size.clamped();
    PtySize {
        rows: size.lines.min(MAX),
        cols: size.columns.min(MAX),
        pixel_width: size.pixel_width(),
        pixel_height: size.pixel_height(),
    }
}

/// Windows: children inherit the console's "ignore Ctrl+C" flag, which OpenSesh itself may have
/// inherited (for example when started from some launchers): clear it once, so Ctrl+C works in
/// local terminals. Side effect: a console that OpenSesh is attached to can now stop it with
/// Ctrl+C, as for any console program.
#[cfg(windows)]
fn allow_ctrl_c() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // SAFETY: a null handler with `add = FALSE` only clears the process's ignore-Ctrl+C
        // flag; no pointer is passed.
        let ok = unsafe { windows_sys::Win32::System::Console::SetConsoleCtrlHandler(None, 0) };
        if ok == 0 {
            tracing::debug!("SetConsoleCtrlHandler failed (no console?)");
        }
    });
}

#[cfg(unix)]
mod unix {
    use std::os::fd::{BorrowedFd, OwnedFd};

    use crossbeam_channel::Sender;
    use portable_pty::MasterPty;
    use rustix::event::{PollFd, PollFlags};
    use rustix::io::{Errno, FdFlags};
    use rustix::termios::{InputModes, OptionalActions};

    use super::{Control, READ_BUFFER, SpawnError};
    use crate::backend::BackendEvent;

    /// Our own descriptors for the PTY master: one to write to (unlike portable-pty's writer,
    /// dropping it doesn't type Enter and Ctrl+D into the program) and one for the reader to
    /// poll. Also sets IUTF8, so erasing in canonical mode removes whole UTF-8 characters.
    pub(super) fn prepare(
        master: &(dyn MasterPty + Send),
    ) -> Result<(std::fs::File, OwnedFd), SpawnError> {
        let setup = |error: Errno| SpawnError::Setup(error.to_string());
        let raw = master
            .as_raw_fd()
            .ok_or_else(|| SpawnError::Setup("no master descriptor".to_owned()))?;
        // SAFETY: `raw` is the master's open descriptor, and `master` outlives this borrow.
        let borrowed = unsafe { BorrowedFd::borrow_raw(raw) };
        let writer = rustix::io::fcntl_dupfd_cloexec(borrowed, 0).map_err(setup)?;
        let reader = rustix::io::fcntl_dupfd_cloexec(borrowed, 0).map_err(setup)?;
        match rustix::termios::tcgetattr(&writer) {
            Ok(mut termios) => {
                termios.input_modes.insert(InputModes::IUTF8);
                if let Err(error) =
                    rustix::termios::tcsetattr(&writer, OptionalActions::Now, &termios)
                {
                    tracing::debug!(%error, "could not set IUTF8");
                }
            }
            Err(error) => tracing::debug!(%error, "could not read the terminal modes"),
        }
        Ok((std::fs::File::from(writer), reader))
    }

    /// A close-on-exec pipe whose write end wakes the reader up.
    pub(super) fn wake_pipe() -> Result<(OwnedFd, OwnedFd), SpawnError> {
        let setup = |error: Errno| SpawnError::Setup(error.to_string());
        let (read, write) = rustix::pipe::pipe().map_err(setup)?;
        rustix::io::fcntl_setfd(&read, FdFlags::CLOEXEC).map_err(setup)?;
        rustix::io::fcntl_setfd(&write, FdFlags::CLOEXEC).map_err(setup)?;
        Ok((read, write))
    }

    /// Reads until EOF (`EIO` once the slave side is closed) or until woken up.
    pub(super) fn read_loop(
        pty: &OwnedFd,
        wake: &OwnedFd,
        events: &Sender<BackendEvent>,
        control: &Sender<Control>,
    ) {
        let mut buffer = vec![0_u8; READ_BUFFER];
        let mut forward = true;
        loop {
            let mut fds = [
                PollFd::new(pty, PollFlags::IN),
                PollFd::new(wake, PollFlags::IN),
            ];
            match rustix::event::poll(&mut fds, None) {
                Ok(_) => {}
                Err(Errno::INTR) => continue,
                Err(error) => {
                    tracing::warn!(%error, "polling the terminal failed");
                    break;
                }
            }
            if !fds[1].revents().is_empty() {
                break;
            }
            if fds[0].revents().is_empty() {
                continue;
            }
            match rustix::io::read(pty, &mut buffer[..]) {
                Ok(0) | Err(Errno::IO) => break,
                Ok(count) => {
                    // Once the session is gone, keep reading (and dropping) until told to stop.
                    if forward {
                        let chunk = buffer[..count].to_vec();
                        forward = events.send(BackendEvent::Output(chunk)).is_ok();
                    }
                }
                Err(Errno::INTR | Errno::AGAIN) => {}
                Err(error) => {
                    tracing::warn!(%error, "reading from the terminal failed");
                    break;
                }
            }
        }
        let _ = control.send(Control::ReaderDone);
    }
}

#[cfg(windows)]
mod windows {
    use std::io::{Read, Write};

    use crossbeam_channel::Sender;
    use portable_pty::MasterPty;

    use super::{Control, READ_BUFFER, SpawnError};
    use crate::backend::BackendEvent;

    type Writer = Box<dyn Write + Send>;
    type Reader = Box<dyn Read + Send>;

    /// The pseudoconsole's input writer and output reader.
    pub(super) fn take_io(master: &(dyn MasterPty + Send)) -> Result<(Writer, Reader), SpawnError> {
        fn setup<E: std::fmt::Display>(error: E) -> SpawnError {
            // `{:#}` prints the whole cause chain of portable-pty's errors.
            SpawnError::Setup(format!("{error:#}"))
        }
        let writer = master.take_writer().map_err(setup)?;
        let reader = master.try_clone_reader().map_err(setup)?;
        Ok((writer, reader))
    }

    /// Drops a pseudoconsole that never got a program, off the calling thread.
    pub(super) fn discard(master: Box<dyn MasterPty + Send>) {
        if std::thread::Builder::new()
            .name("opensesh-pty-close".to_owned())
            .spawn(move || drop(master))
            .is_err()
        {
            tracing::warn!("could not start the pseudoconsole close thread");
        }
    }

    /// A cursor position report. portable-pty creates the pseudoconsole with
    /// `PSEUDOCONSOLE_INHERIT_CURSOR`, so the console host asks for the cursor position first and
    /// does nothing, not even close, until it gets an answer. The engine answers it; when the
    /// pseudoconsole is closed before any input was written, this answer is sent first.
    const CURSOR_REPORT: &[u8] = b"\x1b[1;1R";

    /// Reads until EOF, which comes only after the pseudoconsole is closed.
    pub(super) fn read_loop(
        mut reader: Box<dyn Read + Send>,
        events: &Sender<BackendEvent>,
        control: &Sender<Control>,
    ) {
        let mut buffer = vec![0_u8; READ_BUFFER];
        let mut forward = true;
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    // Keep draining after the session is gone: ClosePseudoConsole waits for it.
                    if forward {
                        let chunk = buffer[..count].to_vec();
                        forward = events.send(BackendEvent::Output(chunk)).is_ok();
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        let _ = control.send(Control::ReaderDone);
    }

    /// Closes the pseudoconsole on its own thread (it can block until the output is drained).
    /// `answer_cursor`: the console host may still wait for its opening cursor query.
    pub(super) fn close(
        master: Box<dyn MasterPty + Send>,
        mut writer: Box<dyn Write + Send>,
        answer_cursor: bool,
    ) {
        let closing = move || {
            if answer_cursor {
                let _ = writer
                    .write_all(CURSOR_REPORT)
                    .and_then(|()| writer.flush());
            }
            drop(master);
            drop(writer);
        };
        if let Err(error) = std::thread::Builder::new()
            .name("opensesh-pty-close".to_owned())
            .spawn(closing)
        {
            // The closure (and the pseudoconsole) was dropped with the error, on this thread.
            tracing::warn!(%error, "could not start the pseudoconsole close thread");
        }
    }

    /// After a failed spawn: close the pseudoconsole without blocking the caller, draining its
    /// output meanwhile.
    pub(super) fn abandon(
        master: Box<dyn MasterPty + Send>,
        writer: Box<dyn Write + Send>,
        mut reader: Box<dyn Read + Send>,
    ) {
        let drain = move || {
            let mut buffer = [0_u8; 4096];
            while matches!(reader.read(&mut buffer), Ok(count) if count > 0) {}
        };
        if std::thread::Builder::new()
            .name("opensesh-pty-drain".to_owned())
            .spawn(drain)
            .is_err()
        {
            tracing::warn!("could not start the pseudoconsole drain thread");
        }
        close(master, writer, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pty_sizes_are_clamped_for_conpty() {
        let size = pty_size(TermSize {
            columns: u16::MAX,
            lines: 0,
            cell_width: 10,
            cell_height: 20,
        });
        assert_eq!((size.cols, size.rows), (32_767, 1));
        assert_eq!(size.pixel_height, 20);
        assert_eq!(size.pixel_width, u16::MAX);
    }
}
