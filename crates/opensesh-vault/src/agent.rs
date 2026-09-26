//! Keys held by SSH agents (Sprint 6, ADR 0025): listed only; signing through an agent comes
//! with the SSH client (Sprint 7).
//!
//! The agent protocol (draft-miller-ssh-agent): each message is a u32 big-endian length and a
//! payload whose first byte is its type. "Request identities" (11) is answered by "identities
//! answer" (12): a u32 count, then each key's public blob and comment as SSH strings.
//!
//! Agents asked: `SSH_AUTH_SOCK` (a Unix socket on Linux; a named pipe path on Windows), the
//! Windows OpenSSH agent's named pipe, and Pageant (Windows; WM_COPYDATA). Calls block up to
//! their timeout: keep them off the GUI thread.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use ssh_key::PublicKey;

use crate::keys::{self, KeyInfo};

/// Request identities.
pub const SSH_AGENTC_REQUEST_IDENTITIES: u8 = 11;
/// Identities answer.
pub const SSH_AGENT_IDENTITIES_ANSWER: u8 = 12;
/// Failure.
pub const SSH_AGENT_FAILURE: u8 = 5;

/// Largest answer accepted.
pub const MAX_MESSAGE: usize = 256 * 1024;

/// The named pipe of the Windows OpenSSH agent service.
pub const OPENSSH_PIPE: &str = r"\\.\pipe\openssh-ssh-agent";

/// An agent to ask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Agent {
    /// A Unix socket (`SSH_AUTH_SOCK`).
    Socket(PathBuf),
    /// A named pipe: the Windows OpenSSH agent, or `SSH_AUTH_SOCK` naming a pipe.
    Pipe(String),
    /// PuTTY's Pageant.
    Pageant,
}

impl Agent {
    /// A short code for the UI (`ssh-auth-sock`, `openssh`, `pageant`).
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Socket(_) => "ssh-auth-sock",
            Self::Pipe(pipe) if pipe.eq_ignore_ascii_case(OPENSSH_PIPE) => "openssh",
            Self::Pipe(_) => "ssh-auth-sock",
            Self::Pageant => "pageant",
        }
    }

    /// Where it listens, for people.
    #[must_use]
    pub fn location(&self) -> String {
        match self {
            Self::Socket(path) => path.display().to_string(),
            Self::Pipe(pipe) => pipe.clone(),
            Self::Pageant => "Pageant".to_owned(),
        }
    }
}

/// Why an agent couldn't be listed. Messages never contain key material.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentError {
    /// Nothing listens there.
    #[error("the agent is not running")]
    NotRunning,
    /// It didn't answer in time.
    #[error("the agent did not answer in time")]
    Timeout,
    /// It answered with a failure.
    #[error("the agent refused the request")]
    Refused,
    /// Reading or writing failed.
    #[error("could not talk to the agent: {0}")]
    Io(String),
    /// The answer doesn't follow the protocol.
    #[error("the agent's answer doesn't follow the protocol: {0}")]
    Protocol(&'static str),
}

/// The agent's answer about one agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    /// The agent.
    pub agent: Agent,
    /// Its keys, or why they couldn't be listed.
    pub keys: Result<Vec<KeyInfo>, AgentError>,
}

/// The framed "request identities" message.
#[must_use]
pub fn request_identities() -> [u8; 5] {
    [0, 0, 0, 1, SSH_AGENTC_REQUEST_IDENTITIES]
}

/// The keys in an answer payload (the message without its length). Keys of algorithms that
/// don't parse are skipped.
///
/// # Errors
///
/// [`AgentError::Refused`] for a failure message, [`AgentError::Protocol`] for anything else
/// that isn't an identities answer.
pub fn parse_answer(payload: &[u8]) -> Result<Vec<KeyInfo>, AgentError> {
    let (kind, mut rest) = payload
        .split_first()
        .ok_or(AgentError::Protocol("empty message"))?;
    match *kind {
        SSH_AGENT_IDENTITIES_ANSWER => {}
        SSH_AGENT_FAILURE => return Err(AgentError::Refused),
        _ => return Err(AgentError::Protocol("unexpected message")),
    }
    let count = read_u32(&mut rest)?;
    let mut keys = Vec::new();
    for _ in 0..count {
        let blob = read_string(&mut rest)?;
        let comment = read_string(&mut rest)?;
        match PublicKey::from_bytes(blob) {
            Ok(mut key) => {
                key.set_comment(String::from_utf8_lossy(comment));
                keys.push(keys::public_info(&key));
            }
            Err(error) => tracing::debug!("skipped an agent key: {error}"),
        }
    }
    Ok(keys)
}

fn read_u32(input: &mut &[u8]) -> Result<u32, AgentError> {
    let (value, rest) = input
        .split_first_chunk::<4>()
        .ok_or(AgentError::Protocol("message cut short"))?;
    *input = rest;
    Ok(u32::from_be_bytes(*value))
}

fn read_string<'a>(input: &mut &'a [u8]) -> Result<&'a [u8], AgentError> {
    let len =
        usize::try_from(read_u32(input)?).map_err(|_| AgentError::Protocol("field too long"))?;
    let value = input
        .get(..len)
        .ok_or(AgentError::Protocol("message cut short"))?;
    *input = input.get(len..).unwrap_or_default();
    Ok(value)
}

/// Sends "request identities" on `stream` and reads the answer.
///
/// # Errors
///
/// As [`parse_answer`], or I/O errors.
pub fn exchange(stream: &mut (impl Read + Write)) -> Result<Vec<KeyInfo>, AgentError> {
    let io = |error: std::io::Error| match error.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => AgentError::Timeout,
        _ => AgentError::Io(error.to_string()),
    };
    stream.write_all(&request_identities()).map_err(io)?;
    stream.flush().map_err(io)?;
    let mut len = [0_u8; 4];
    stream.read_exact(&mut len).map_err(io)?;
    let len = usize::try_from(u32::from_be_bytes(len)).unwrap_or(usize::MAX);
    if len > MAX_MESSAGE {
        return Err(AgentError::Protocol("answer too large"));
    }
    let mut payload = vec![0; len];
    stream.read_exact(&mut payload).map_err(io)?;
    parse_answer(&payload)
}

/// The agents to ask on this system.
#[must_use]
pub fn agents() -> Vec<Agent> {
    let mut out = Vec::new();
    let sock = std::env::var_os("SSH_AUTH_SOCK").filter(|value| !value.is_empty());
    if cfg!(windows) {
        if let Some(sock) = sock.and_then(|value| value.into_string().ok()) {
            if sock.starts_with(r"\\.\pipe\") || sock.starts_with("//./pipe/") {
                out.push(Agent::Pipe(sock.replace('/', r"\")));
            } else {
                tracing::debug!("SSH_AUTH_SOCK is not a named pipe; not asked");
            }
        }
        if !out.iter().any(
            |agent| matches!(agent, Agent::Pipe(pipe) if pipe.eq_ignore_ascii_case(OPENSSH_PIPE)),
        ) {
            out.push(Agent::Pipe(OPENSSH_PIPE.to_owned()));
        }
        out.push(Agent::Pageant);
    } else if let Some(sock) = sock {
        out.push(Agent::Socket(PathBuf::from(sock)));
    }
    out
}

/// The keys of `agent`, waiting at most `timeout`.
///
/// # Errors
///
/// [`AgentError::NotRunning`] when nothing listens, and the errors of [`exchange`].
pub fn list(agent: &Agent, timeout: Duration) -> Result<Vec<KeyInfo>, AgentError> {
    match agent {
        Agent::Socket(path) => list_socket(path, timeout),
        Agent::Pipe(pipe) => list_pipe(pipe, timeout),
        Agent::Pageant => pageant::list(timeout),
    }
}

/// Every agent of [`agents`] with its keys.
#[must_use]
pub fn list_all(timeout: Duration) -> Vec<Listing> {
    agents()
        .into_iter()
        .map(|agent| {
            let keys = list(&agent, timeout);
            Listing { agent, keys }
        })
        .collect()
}

fn not_running(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
    )
}

#[cfg(unix)]
fn list_socket(path: &std::path::Path, timeout: Duration) -> Result<Vec<KeyInfo>, AgentError> {
    let mut stream = std::os::unix::net::UnixStream::connect(path).map_err(|error| {
        if not_running(&error) {
            AgentError::NotRunning
        } else {
            AgentError::Io(error.to_string())
        }
    })?;
    let io = |error: std::io::Error| AgentError::Io(error.to_string());
    stream.set_read_timeout(Some(timeout)).map_err(io)?;
    stream.set_write_timeout(Some(timeout)).map_err(io)?;
    exchange(&mut stream)
}

#[cfg(not(unix))]
fn list_socket(_path: &std::path::Path, _timeout: Duration) -> Result<Vec<KeyInfo>, AgentError> {
    Err(AgentError::NotRunning)
}

/// A named pipe has no I/O timeouts: the exchange runs in a helper thread, which is left to
/// finish on its own if the agent never answers.
fn list_pipe(pipe: &str, timeout: Duration) -> Result<Vec<KeyInfo>, AgentError> {
    if !cfg!(windows) {
        return Err(AgentError::NotRunning);
    }
    let pipe = pipe.to_owned();
    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("agent-pipe".to_owned())
        .spawn(move || {
            let result = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&pipe)
                .map_err(|error| {
                    if not_running(&error) {
                        AgentError::NotRunning
                    } else {
                        AgentError::Io(error.to_string())
                    }
                })
                .and_then(|mut file| exchange(&mut file));
            let _ = sender.send(result);
        })
        .map_err(|error| AgentError::Io(error.to_string()))?;
    receiver
        .recv_timeout(timeout)
        .unwrap_or(Err(AgentError::Timeout))
}

#[cfg(windows)]
mod pageant {
    //! Pageant's WM_COPYDATA protocol (PuTTY's `windows/agent-client.c`): the request goes into a
    //! named file mapping, the mapping's name is sent to Pageant's window with WM_COPYDATA
    //! (`dwData` 0x804e50ba), and Pageant writes its answer over the request.

    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
    use windows_sys::Win32::System::Memory::{
        CreateFileMappingW, FILE_MAP_READ, FILE_MAP_WRITE, MEMORY_MAPPED_VIEW_ADDRESS,
        MapViewOfFile, PAGE_READWRITE, UnmapViewOfFile,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_COPYDATA,
    };

    use super::{AgentError, KeyInfo, MAX_MESSAGE, parse_answer, request_identities};

    const AGENT_COPYDATA_ID: usize = 0x804e_50ba;
    /// The size of the shared buffer, as in PuTTY.
    const MAX_MSG_LEN: usize = 8192;

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// A file mapping and its view, closed on drop.
    struct Mapping {
        handle: HANDLE,
        view: MEMORY_MAPPED_VIEW_ADDRESS,
    }

    impl Drop for Mapping {
        fn drop(&mut self) {
            // SAFETY: both come from successful CreateFileMappingW and MapViewOfFile calls in
            // `list` and are released exactly once, here.
            unsafe {
                UnmapViewOfFile(self.view);
                CloseHandle(self.handle);
            }
        }
    }

    pub(super) fn list(timeout: Duration) -> Result<Vec<KeyInfo>, AgentError> {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let class = wide("Pageant");
        // SAFETY: both arguments are valid NUL-terminated UTF-16 strings that outlive the call.
        let window = unsafe { FindWindowW(class.as_ptr(), class.as_ptr()) };
        if window.is_null() {
            return Err(AgentError::NotRunning);
        }
        let name = format!(
            "PageantRequest{:08x}{:08x}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let wide_name = wide(&name);
        let size = u32::try_from(MAX_MSG_LEN).unwrap_or(u32::MAX);
        // SAFETY: a pagefile-backed mapping (INVALID_HANDLE_VALUE) with the default security
        // (owned by this user, which Pageant checks) and a valid NUL-terminated name.
        let handle = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                std::ptr::null(),
                PAGE_READWRITE,
                0,
                size,
                wide_name.as_ptr(),
            )
        };
        if handle.is_null() {
            return Err(AgentError::Io(std::io::Error::last_os_error().to_string()));
        }
        // SAFETY: `handle` is the mapping just created; the whole mapping is viewed.
        let view = unsafe { MapViewOfFile(handle, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, 0) };
        if view.Value.is_null() {
            let error = std::io::Error::last_os_error();
            // SAFETY: `handle` is valid and not used again.
            unsafe { CloseHandle(handle) };
            return Err(AgentError::Io(error.to_string()));
        }
        let mapping = Mapping { handle, view };
        let buffer = mapping.view.Value.cast::<u8>();
        let request = request_identities();
        // SAFETY: the view is MAX_MSG_LEN bytes long and the request is 5.
        unsafe { std::ptr::copy_nonoverlapping(request.as_ptr(), buffer, request.len()) };

        // The name as ANSI bytes with its NUL, which is what Pageant reads.
        let mut ansi = name.into_bytes();
        ansi.push(0);
        let data = COPYDATASTRUCT {
            dwData: AGENT_COPYDATA_ID,
            cbData: u32::try_from(ansi.len()).unwrap_or(u32::MAX),
            lpData: ansi.as_mut_ptr().cast(),
        };
        let millis = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
        let mut answer: usize = 0;
        // SAFETY: `data` and `ansi` live until SendMessageTimeoutW returns; the window handle
        // came from FindWindowW (a stale handle only makes the call fail).
        let sent = unsafe {
            SendMessageTimeoutW(
                window,
                WM_COPYDATA,
                0,
                std::ptr::addr_of!(data) as isize,
                SMTO_ABORTIFHUNG,
                millis,
                &raw mut answer,
            )
        };
        if sent == 0 {
            return Err(AgentError::Timeout);
        }
        if answer == 0 {
            return Err(AgentError::Refused);
        }
        let mut len = [0_u8; 4];
        // SAFETY: the view holds at least 4 bytes.
        unsafe { std::ptr::copy_nonoverlapping(buffer, len.as_mut_ptr(), 4) };
        let len = usize::try_from(u32::from_be_bytes(len)).unwrap_or(usize::MAX);
        if len > MAX_MSG_LEN - 4 || len > MAX_MESSAGE {
            return Err(AgentError::Protocol("answer too large"));
        }
        let mut payload = vec![0_u8; len];
        // SAFETY: `len` was checked against the view's size just above.
        unsafe { std::ptr::copy_nonoverlapping(buffer.add(4), payload.as_mut_ptr(), len) };
        drop(mapping);
        parse_answer(&payload)
    }
}

#[cfg(not(windows))]
mod pageant {
    use std::time::Duration;

    use super::{AgentError, KeyInfo};

    pub(super) fn list(_timeout: Duration) -> Result<Vec<KeyInfo>, AgentError> {
        Err(AgentError::NotRunning)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string(out: &mut Vec<u8>, value: &[u8]) {
        out.extend_from_slice(&u32::try_from(value.len()).unwrap().to_be_bytes());
        out.extend_from_slice(value);
    }

    fn answer(keys: &[(&str, &str)]) -> Vec<u8> {
        let mut payload = vec![SSH_AGENT_IDENTITIES_ANSWER];
        payload.extend_from_slice(&u32::try_from(keys.len()).unwrap().to_be_bytes());
        for (public, comment) in keys {
            let key = PublicKey::from_openssh(public).unwrap();
            string(&mut payload, &key.to_bytes().unwrap());
            string(&mut payload, comment.as_bytes());
        }
        payload
    }

    const ED25519: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIC6tmVU1VE59P7TYx6UJYcZkhy7FRiLjhH6gdK9Sayyd";

    #[test]
    fn answers_parse() {
        let keys = parse_answer(&answer(&[(ED25519, "me@laptop")])).unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].comment, "me@laptop");
        assert_eq!(keys[0].algorithm, "ssh-ed25519");
        assert_eq!(parse_answer(&answer(&[])).unwrap(), Vec::new());
        assert_eq!(parse_answer(&[SSH_AGENT_FAILURE]), Err(AgentError::Refused));
        assert!(matches!(parse_answer(&[]), Err(AgentError::Protocol(_))));
        // Claims more keys than it has.
        let mut short = answer(&[(ED25519, "x")]);
        short[4] = 2;
        assert!(matches!(parse_answer(&short), Err(AgentError::Protocol(_))));
        // An unknown key type is skipped, not an error.
        let mut odd = vec![SSH_AGENT_IDENTITIES_ANSWER, 0, 0, 0, 1];
        let mut blob = Vec::new();
        string(&mut blob, b"ssh-future");
        string(&mut blob, b"xyz");
        string(&mut odd, &blob);
        string(&mut odd, b"c");
        assert!(parse_answer(&odd).unwrap().is_empty());
    }

    /// A stream that answers with `reply` and records what was written.
    struct Fake {
        reply: std::io::Cursor<Vec<u8>>,
        written: Vec<u8>,
    }

    impl Read for Fake {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.reply.read(buf)
        }
    }

    impl Write for Fake {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.written.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn exchange_frames_messages() {
        let payload = answer(&[(ED25519, "a")]);
        let mut reply = u32::try_from(payload.len()).unwrap().to_be_bytes().to_vec();
        reply.extend_from_slice(&payload);
        let mut fake = Fake {
            reply: std::io::Cursor::new(reply),
            written: Vec::new(),
        };
        assert_eq!(exchange(&mut fake).unwrap().len(), 1);
        assert_eq!(fake.written, request_identities());

        let mut huge = Fake {
            reply: std::io::Cursor::new(vec![0x7f, 0xff, 0xff, 0xff]),
            written: Vec::new(),
        };
        assert!(matches!(exchange(&mut huge), Err(AgentError::Protocol(_))));
    }

    /// A tiny agent on a Unix socket in a temporary folder.
    #[cfg(unix)]
    #[test]
    fn unix_socket_agent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let payload = answer(&[(ED25519, "from the socket")]);
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 5];
            stream.read_exact(&mut request).unwrap();
            assert_eq!(request, request_identities());
            let mut reply = u32::try_from(payload.len()).unwrap().to_be_bytes().to_vec();
            reply.extend_from_slice(&payload);
            stream.write_all(&reply).unwrap();
        });
        let keys = list(&Agent::Socket(path.clone()), Duration::from_secs(5)).unwrap();
        server.join().unwrap();
        assert_eq!(keys[0].comment, "from the socket");
        // Nobody listens any more.
        drop(dir);
        assert_eq!(
            list(&Agent::Socket(path), Duration::from_secs(1)),
            Err(AgentError::NotRunning)
        );
    }

    /// A tiny agent on a named pipe, like the Windows OpenSSH agent's.
    #[cfg(windows)]
    #[test]
    fn named_pipe_agent() {
        use interprocess::local_socket::prelude::*;
        use interprocess::local_socket::{GenericNamespaced, ListenerOptions};

        let name = format!("opensesh-test-agent-{}", std::process::id());
        let listener = ListenerOptions::new()
            .name(name.as_str().to_ns_name::<GenericNamespaced>().unwrap())
            .create_sync()
            .unwrap();
        let payload = answer(&[(ED25519, "from the pipe")]);
        let server = std::thread::spawn(move || {
            let mut stream = listener.accept().unwrap();
            let mut request = [0_u8; 5];
            stream.read_exact(&mut request).unwrap();
            assert_eq!(request, request_identities());
            let mut reply = u32::try_from(payload.len()).unwrap().to_be_bytes().to_vec();
            reply.extend_from_slice(&payload);
            stream.write_all(&reply).unwrap();
        });
        let pipe = format!(r"\\.\pipe\{name}");
        let keys = list(&Agent::Pipe(pipe), Duration::from_secs(5)).unwrap();
        server.join().unwrap();
        assert_eq!(keys[0].comment, "from the pipe");
    }

    #[cfg(windows)]
    #[test]
    fn a_missing_pipe_is_not_running() {
        assert_eq!(
            list(
                &Agent::Pipe(r"\\.\pipe\opensesh-test-no-such-agent".to_owned()),
                Duration::from_secs(2)
            ),
            Err(AgentError::NotRunning)
        );
    }

    /// The agents of this machine, printed (read-only). Run by hand with a running agent:
    /// `cargo test -p opensesh-vault --lib real_agents -- --ignored --nocapture`.
    #[test]
    #[ignore = "asks the agents of the machine it runs on"]
    fn real_agents() {
        for listing in list_all(Duration::from_secs(3)) {
            println!("{} ({}):", listing.agent.location(), listing.agent.code());
            match listing.keys {
                Ok(keys) => {
                    for key in keys {
                        println!("  {} {} {}", key.label, key.fingerprint, key.comment);
                    }
                }
                Err(error) => println!("  {error}"),
            }
        }
    }

    #[test]
    fn codes() {
        assert_eq!(Agent::Pipe(OPENSSH_PIPE.to_owned()).code(), "openssh");
        assert_eq!(Agent::Pageant.code(), "pageant");
        assert_eq!(Agent::Socket("/tmp/x".into()).code(), "ssh-auth-sock");
    }
}
