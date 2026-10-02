//! Telnet (RFC 854) as a terminal backend (PLAN Sprint 12, ADR 0032).
//!
//! [`Nvt`] is the protocol: it takes what the server sends apart into text for the terminal and
//! replies, and prepares what the user types. It negotiates only what a terminal needs:
//! - **the server's** `ECHO`, `SGA` and `BINARY`;
//! - **ours:** `SGA`, `BINARY`, `NAWS` (the window size, sent again on every resize) and
//!   `TTYPE` (the `TERM` of the profile).
//!
//! Everything else is refused, and an option is answered only when its state changes, so two
//! ends can't loop. Like PuTTY, it waits for the server to start negotiating (a raw TCP service
//! sees only what the user types) and echoes typed text itself until the server says it echoes
//! (`WILL ECHO`).
//!
//! Enter is sent as CR LF (the telnet end of line), a 255 byte as `IAC IAC`; the server's CR NUL
//! is a CR. Telnet sends everything in clear, passwords too: the backend says so before
//! connecting.

use std::time::Duration;

use opensesh_term::backend::{BackendError, BackendEvent, TermSize, TerminalBackend};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::output::{self, Command, Output};

/// Option codes.
pub mod option {
    /// Binary transmission (RFC 856).
    pub const BINARY: u8 = 0;
    /// Echo (RFC 857).
    pub const ECHO: u8 = 1;
    /// Suppress go-ahead (RFC 858).
    pub const SGA: u8 = 3;
    /// Terminal type (RFC 1091).
    pub const TTYPE: u8 = 24;
    /// Negotiate about window size (RFC 1073).
    pub const NAWS: u8 = 31;
}

const IAC: u8 = 255;
const DONT: u8 = 254;
const DO: u8 = 253;
const WONT: u8 = 252;
const WILL: u8 = 251;
const SB: u8 = 250;
const SE: u8 = 240;
const CR: u8 = b'\r';
const LF: u8 = b'\n';
const NUL: u8 = 0;
const TTYPE_IS: u8 = 0;
const TTYPE_SEND: u8 = 1;

/// The longest subnegotiation kept (a server can't make the client buffer more).
const MAX_SUBNEGOTIATION: usize = 4096;

/// Options the server may turn on for itself.
const THEIRS: [u8; 3] = [option::ECHO, option::SGA, option::BINARY];
/// Options the client turns on when asked.
const OURS: [u8; 4] = [option::SGA, option::BINARY, option::NAWS, option::TTYPE];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Data,
    /// After a CR in text (NVT: CR NUL is a CR).
    Cr,
    Iac,
    Negotiate(u8),
    Sub,
    SubIac,
}

/// What the server's bytes come to.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Received {
    /// Text for the terminal.
    pub data: Vec<u8>,
    /// What to send back.
    pub reply: Vec<u8>,
}

/// One telnet connection's protocol state.
#[derive(Debug, Clone)]
pub struct Nvt {
    state: State,
    sub: Vec<u8>,
    ours: [bool; 256],
    theirs: [bool; 256],
    term: String,
    size: TermSize,
    /// The last byte typed was a CR (a LF right after it is already the end of line).
    typed_cr: bool,
}

impl Nvt {
    /// A connection with nothing negotiated yet, for a terminal of type `term` and size `size`.
    #[must_use]
    pub fn new(term: &str, size: TermSize) -> Self {
        Self {
            state: State::Data,
            sub: Vec::new(),
            ours: [false; 256],
            theirs: [false; 256],
            term: term.to_owned(),
            size,
            typed_cr: false,
        }
    }

    /// Whether the server echoes what is typed (else the client shows it).
    #[must_use]
    pub fn server_echoes(&self) -> bool {
        self.theirs[usize::from(option::ECHO)]
    }

    /// Whether option `code` is on for the client.
    #[must_use]
    pub fn ours(&self, code: u8) -> bool {
        self.ours[usize::from(code)]
    }

    /// Whether option `code` is on for the server.
    #[must_use]
    pub fn theirs(&self, code: u8) -> bool {
        self.theirs[usize::from(code)]
    }

    /// Takes the server's `bytes` apart.
    pub fn receive(&mut self, bytes: &[u8]) -> Received {
        let mut out = Received::default();
        for &byte in bytes {
            match self.state {
                State::Data | State::Cr => {
                    let after_cr = self.state == State::Cr;
                    self.state = State::Data;
                    match byte {
                        IAC => self.state = State::Iac,
                        NUL if after_cr => {}
                        CR if !self.theirs(option::BINARY) => {
                            out.data.push(CR);
                            self.state = State::Cr;
                        }
                        _ => out.data.push(byte),
                    }
                }
                State::Iac => {
                    self.state = State::Data;
                    match byte {
                        IAC => out.data.push(IAC),
                        WILL | WONT | DO | DONT => self.state = State::Negotiate(byte),
                        SB => {
                            self.sub.clear();
                            self.state = State::Sub;
                        }
                        // NOP, data mark, break, go ahead... nothing to do.
                        _ => {}
                    }
                }
                State::Negotiate(verb) => {
                    self.state = State::Data;
                    self.negotiate(verb, byte, &mut out.reply);
                }
                State::Sub => match byte {
                    IAC => self.state = State::SubIac,
                    _ if self.sub.len() < MAX_SUBNEGOTIATION => self.sub.push(byte),
                    _ => {}
                },
                State::SubIac => match byte {
                    SE => {
                        self.state = State::Data;
                        self.subnegotiation(&mut out.reply);
                    }
                    IAC => {
                        self.state = State::Sub;
                        if self.sub.len() < MAX_SUBNEGOTIATION {
                            self.sub.push(IAC);
                        }
                    }
                    // A broken subnegotiation: drop it.
                    _ => self.state = State::Data,
                },
            }
        }
        out
    }

    fn negotiate(&mut self, verb: u8, code: u8, reply: &mut Vec<u8>) {
        let index = usize::from(code);
        match verb {
            WILL => {
                if !THEIRS.contains(&code) {
                    reply.extend([IAC, DONT, code]);
                } else if !self.theirs[index] {
                    self.theirs[index] = true;
                    reply.extend([IAC, DO, code]);
                }
            }
            WONT => {
                if self.theirs[index] {
                    self.theirs[index] = false;
                    reply.extend([IAC, DONT, code]);
                }
            }
            DO => {
                if !OURS.contains(&code) {
                    reply.extend([IAC, WONT, code]);
                } else if !self.ours[index] {
                    self.ours[index] = true;
                    reply.extend([IAC, WILL, code]);
                    if code == option::NAWS {
                        reply.extend(self.window_size());
                    }
                }
            }
            DONT => {
                if self.ours[index] {
                    self.ours[index] = false;
                    reply.extend([IAC, WONT, code]);
                }
            }
            _ => {}
        }
    }

    fn subnegotiation(&mut self, reply: &mut Vec<u8>) {
        if self.sub.as_slice() == [option::TTYPE, TTYPE_SEND] && self.ours(option::TTYPE) {
            reply.extend([IAC, SB, option::TTYPE, TTYPE_IS]);
            reply.extend(
                self.term
                    .to_ascii_uppercase()
                    .bytes()
                    .filter(|byte| *byte != IAC),
            );
            reply.extend([IAC, SE]);
        }
        self.sub.clear();
    }

    /// The window size subnegotiation (a 255 in the numbers is doubled, as in any data).
    fn window_size(&self) -> Vec<u8> {
        let mut out = vec![IAC, SB, option::NAWS];
        for byte in self
            .size
            .columns
            .to_be_bytes()
            .into_iter()
            .chain(self.size.lines.to_be_bytes())
        {
            out.push(byte);
            if byte == IAC {
                out.push(IAC);
            }
        }
        out.extend([IAC, SE]);
        out
    }

    /// The terminal's new size: what to tell the server (nothing unless it asked with
    /// `DO NAWS`).
    pub fn resize(&mut self, size: TermSize) -> Vec<u8> {
        self.size = size;
        if self.ours(option::NAWS) {
            self.window_size()
        } else {
            Vec::new()
        }
    }

    /// What the user typed, ready for the server: Enter (CR) becomes CR LF outside binary mode,
    /// and a 255 byte is doubled.
    pub fn encode(&mut self, typed: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(typed.len() + 2);
        let binary = self.ours(option::BINARY);
        for &byte in typed {
            match byte {
                IAC => out.extend([IAC, IAC]),
                CR if !binary => out.extend([CR, LF]),
                LF if !binary && self.typed_cr => {}
                _ => out.push(byte),
            }
            self.typed_cr = byte == CR;
        }
        out
    }
}

/// What the client shows of `typed` while the server doesn't echo: Enter as a new line, Backspace
/// erasing the character before.
#[must_use]
pub fn local_echo(typed: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(typed.len());
    for &byte in typed {
        match byte {
            CR => out.extend(b"\r\n"),
            0x08 | 0x7f => out.extend(b"\x08 \x08"),
            // Other control characters (and escape sequences of keys) aren't shown.
            0x00..=0x1f => {}
            _ => out.push(byte),
        }
    }
    out
}

/// Where to connect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelnetSpec {
    /// Host name or address.
    pub host: String,
    /// Port (23 is telnet's).
    pub port: u16,
    /// `TERM` for the terminal type option.
    pub term: String,
    /// How long connecting may take.
    pub connect_timeout: Duration,
    /// The session log: its file and whether raw (escape codes kept).
    pub log: Option<(std::path::PathBuf, bool)>,
}

/// Starts a telnet session: the input side and the event channel for the terminal engine.
///
/// # Errors
///
/// [`BackendError::Thread`] when the connection runtime isn't available.
pub fn start(
    spec: TelnetSpec,
    size: TermSize,
) -> Result<
    (
        Box<dyn TerminalBackend>,
        crossbeam_channel::Receiver<BackendEvent>,
    ),
    BackendError,
> {
    let log = spec.log.as_ref().map(|(path, raw)| (path.as_path(), *raw));
    let (backend, events, ends, runtime) = output::channels(log)?;
    runtime.spawn(run(spec, size, ends.output, ends.commands));
    Ok((backend, events))
}

async fn run(
    spec: TelnetSpec,
    size: TermSize,
    output: Output,
    mut commands: tokio::sync::mpsc::UnboundedReceiver<Command>,
) {
    let label = if spec.port == 23 {
        spec.host.clone()
    } else {
        format!("{}:{}", spec.host, spec.port)
    };
    output
        .caution(
            "Telnet sends everything in clear, passwords too: use it only on networks you trust.",
        )
        .await;
    output.note(&format!("Connecting to {label}...")).await;
    // Connect, while following resizes (typed text is dropped) and shutdown.
    let mut size = size;
    let connecting = tokio::time::timeout(
        spec.connect_timeout,
        TcpStream::connect((spec.host.as_str(), spec.port)),
    );
    tokio::pin!(connecting);
    let stream = loop {
        tokio::select! {
            result = &mut connecting => match result {
                Ok(Ok(stream)) => break stream,
                Ok(Err(error)) => {
                    output.fail(&format!("could not connect to {label}: {error}")).await;
                    return;
                }
                Err(_) => {
                    let seconds = spec.connect_timeout.as_secs();
                    output.fail(&format!("could not connect to {label}: no answer in {seconds} s")).await;
                    return;
                }
            },
            command = commands.recv() => match command {
                Some(Command::Resize(next)) => size = next,
                Some(Command::Input(_)) => {}
                Some(Command::Shutdown) | None => return,
            },
        }
    };
    let _ = stream.set_nodelay(true);
    let mut nvt = Nvt::new(&spec.term, size);
    let (mut reader, mut writer) = stream.into_split();
    let mut buffer = vec![0_u8; 16 * 1024];
    loop {
        tokio::select! {
            read = reader.read(&mut buffer) => match read {
                Ok(0) => {
                    output.note(&format!("\r\nThe connection to {label} was closed.")).await;
                    output.ended(None).await;
                    return;
                }
                Ok(count) => {
                    let received = nvt.receive(&buffer[..count]);
                    if !received.reply.is_empty() && writer.write_all(&received.reply).await.is_err() {
                        output.fail(&format!("the connection to {label} was lost")).await;
                        return;
                    }
                    if !output.data(&received.data).await {
                        return;
                    }
                }
                Err(error) => {
                    output.fail(&format!("the connection to {label} was lost: {error}")).await;
                    return;
                }
            },
            command = commands.recv() => match command {
                Some(Command::Input(bytes)) => {
                    if !nvt.server_echoes() {
                        output.show(local_echo(&bytes)).await;
                    }
                    if writer.write_all(&nvt.encode(&bytes)).await.is_err() {
                        output.fail(&format!("the connection to {label} was lost")).await;
                        return;
                    }
                }
                Some(Command::Resize(next)) => {
                    let update = nvt.resize(next);
                    if !update.is_empty() && writer.write_all(&update).await.is_err() {
                        output.fail(&format!("the connection to {label} was lost")).await;
                        return;
                    }
                }
                Some(Command::Shutdown) | None => {
                    let _ = writer.shutdown().await;
                    return;
                }
            },
        }
    }
}

#[cfg(test)]
mod tests;

/// A telnet server for tests and the app's smoke test (never started otherwise): on 127.0.0.1,
/// it offers to echo, asks for the window size and the terminal type, prints a banner and a
/// prompt, echoes what is typed and answers a line with `test> `; `size` prints the last window
/// size it was told, and `exit` closes the connection.
pub mod testing {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::{DO, IAC, Nvt, SB, SE, TTYPE_SEND, WILL, option};

    /// Starts the server on a free port of 127.0.0.1 and returns the port. It runs until the
    /// runtime ends, one task per connection.
    ///
    /// # Errors
    ///
    /// When no port can be bound.
    pub async fn serve() -> std::io::Result<u16> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                tokio::spawn(session(socket));
            }
        });
        Ok(port)
    }

    async fn session(mut socket: tokio::net::TcpStream) {
        let hello = [
            &[
                IAC,
                WILL,
                option::ECHO,
                IAC,
                WILL,
                option::SGA,
                IAC,
                DO,
                option::NAWS,
                IAC,
                DO,
                option::TTYPE,
            ][..],
            &[IAC, SB, option::TTYPE, TTYPE_SEND, IAC, SE],
            b"OpenSesh telnet test server\r\ntest> ",
        ]
        .concat();
        if socket.write_all(&hello).await.is_err() {
            return;
        }
        // The server's side of the protocol is the client's mirror: reuse the parser for the
        // subnegotiations the client sends (window size and terminal type).
        let mut size = (0_u16, 0_u16);
        let mut line = Vec::new();
        let mut buffer = [0_u8; 4096];
        let mut nvt = Nvt::new("", opensesh_term::backend::TermSize::new(80, 24));
        loop {
            let Ok(count) = socket.read(&mut buffer).await else {
                return;
            };
            if count == 0 {
                return;
            }
            let mut text = Vec::new();
            let mut index = 0;
            let bytes = &buffer[..count];
            while index < count {
                // NAWS from the client: IAC SB NAWS w w h h IAC SE (no 255 in the test sizes).
                if bytes[index..].starts_with(&[IAC, SB, option::NAWS]) && index + 9 <= count {
                    let at = &bytes[index + 3..index + 7];
                    size = (
                        u16::from_be_bytes([at[0], at[1]]),
                        u16::from_be_bytes([at[2], at[3]]),
                    );
                    index += 9;
                } else {
                    text.push(bytes[index]);
                    index += 1;
                }
            }
            let data = nvt.receive(&text).data;
            let mut reply = Vec::new();
            for byte in data {
                match byte {
                    b'\r' => {
                        reply.extend(b"\r\n");
                        let command = String::from_utf8_lossy(&line).trim().to_owned();
                        line.clear();
                        match command.as_str() {
                            "exit" => {
                                let _ = socket.write_all(&reply).await;
                                return;
                            }
                            "size" => reply.extend(format!("{}x{}\r\n", size.0, size.1).bytes()),
                            "" => {}
                            other => reply.extend(format!("you typed {other}\r\n").bytes()),
                        }
                        reply.extend(b"test> ");
                    }
                    b'\n' | 0 => {}
                    _ => {
                        line.push(byte);
                        reply.push(byte);
                    }
                }
            }
            if socket.write_all(&reply).await.is_err() {
                return;
            }
        }
    }
}
