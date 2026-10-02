//! X11 forwarding in the built-in client (Sprint 15, ADR 0036), as OpenSSH does it:
//!
//! - the session asks for `x11-req` with a **made-up cookie** ([`Forwarding::new`]), so the
//!   server never sees the real one;
//! - each `x11` channel the server opens is connected to the **local display** ([`Display`]),
//!   and the client's first message, which carries the made-up cookie, gets the **real one**
//!   instead ([`substitute`]); a channel with any other cookie is refused;
//! - the real cookie comes from `xauth`: one made for this connection with
//!   `xauth generate ... untrusted` (untrusted: the X SECURITY extension limits what remote
//!   programs may do to the others), or the display's own from `xauth list` (trusted).
//!
//! No cookie is logged.

use std::path::PathBuf;
use std::sync::Arc;

use russh::Channel;
use russh::client::Msg;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use zeroize::Zeroizing;

use crate::spec::X11Spec;

/// The only authorization protocol forwarded.
pub const MIT_MAGIC_COOKIE: &str = "MIT-MAGIC-COOKIE-1";

/// The largest first message accepted (its names and data are short).
const MAX_SETUP: usize = 4096;

/// Where the local X server listens, from `DISPLAY`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Display {
    /// A Unix socket (`:0` is `/tmp/.X11-unix/X0`; XQuartz gives a path).
    Unix {
        /// The socket.
        path: PathBuf,
        /// The display number.
        number: u16,
        /// The screen.
        screen: u16,
    },
    /// TCP: port 6000 + the display number.
    Tcp {
        /// The host.
        host: String,
        /// The display number.
        number: u16,
        /// The screen.
        screen: u16,
    },
}

impl Display {
    /// Reads `DISPLAY` (`:0`, `:0.1`, `unix:0`, `localhost:10.0`, `[::1]:0`,
    /// `/private/tmp/launch-x/org.xquartz:0`).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let (host, rest) = text.rsplit_once(':')?;
        let (number, screen) = match rest.split_once('.') {
            Some((number, screen)) => (number.parse().ok()?, screen.parse().ok()?),
            None => (rest.parse().ok()?, 0),
        };
        Some(match host {
            "" | "unix" => Self::Unix {
                path: PathBuf::from(format!("/tmp/.X11-unix/X{number}")),
                number,
                screen,
            },
            path if path.starts_with('/') => Self::Unix {
                path: PathBuf::from(text.rsplit_once(':')?.0),
                number,
                screen,
            },
            host => Self::Tcp {
                host: host.trim_matches(['[', ']']).to_owned(),
                number,
                screen,
            },
        })
    }

    /// The screen, for `x11-req`.
    #[must_use]
    pub fn screen(&self) -> u16 {
        match self {
            Self::Unix { screen, .. } | Self::Tcp { screen, .. } => *screen,
        }
    }

    /// The TCP port (6000 + the display number), for a TCP display.
    #[must_use]
    pub fn port(&self) -> Option<u16> {
        match self {
            Self::Tcp { number, .. } => 6000_u16.checked_add(*number),
            Self::Unix { .. } => None,
        }
    }
}

/// A forwarding's cookies: the made-up one the server gets, and the real one the local display
/// takes (none when it takes no cookie, as an X server started with `-ac`).
pub struct Forwarding {
    fake: Zeroizing<[u8; 16]>,
    real: Option<Zeroizing<Vec<u8>>>,
}

impl std::fmt::Debug for Forwarding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Forwarding").finish_non_exhaustive()
    }
}

impl Forwarding {
    /// A made-up cookie (random), and the display's `real` one.
    #[must_use]
    pub fn new(real: Option<Vec<u8>>) -> Self {
        let mut fake = Zeroizing::new([0_u8; 16]);
        rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, fake.as_mut_slice());
        Self {
            fake,
            real: real.map(Zeroizing::new),
        }
    }

    /// The made-up cookie in hexadecimal, for `x11-req`.
    #[must_use]
    pub fn fake_hex(&self) -> Zeroizing<String> {
        Zeroizing::new(self.fake.iter().map(|byte| format!("{byte:02x}")).collect())
    }

    /// Reads the X client's first message from `channel` and returns it with the real cookie in
    /// place of the made-up one, ready for the local display.
    ///
    /// # Errors
    ///
    /// When the message can't be read, is malformed, or carries another cookie.
    pub async fn first_message<R: AsyncRead + Unpin>(
        &self,
        channel: &mut R,
    ) -> std::io::Result<Zeroizing<Vec<u8>>> {
        let mut header = [0_u8; 12];
        channel.read_exact(&mut header).await?;
        let big_endian = match header[0] {
            b'B' => true,
            b'l' => false,
            _ => return Err(invalid("not an X11 connection")),
        };
        let read_u16 = |at: usize| {
            let bytes = [header[at], header[at + 1]];
            usize::from(if big_endian {
                u16::from_be_bytes(bytes)
            } else {
                u16::from_le_bytes(bytes)
            })
        };
        let (name_len, data_len) = (read_u16(6), read_u16(8));
        let rest_len = name_len.next_multiple_of(4) + data_len.next_multiple_of(4);
        if rest_len > MAX_SETUP {
            return Err(invalid("an X11 setup too long"));
        }
        let mut rest = Zeroizing::new(vec![0_u8; rest_len]);
        channel.read_exact(&mut rest).await?;
        let name = &rest[..name_len];
        let data_at = name_len.next_multiple_of(4);
        let data = &rest[data_at..data_at + data_len];
        substitute(
            &header,
            big_endian,
            name,
            data,
            self.fake.as_slice(),
            self.real.as_deref().map(Vec::as_slice),
        )
    }
}

fn invalid(what: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, what.to_owned())
}

/// The first message with `real` in place of `fake` (or no authorization at all when `real` is
/// `None`), as the local display takes it.
///
/// # Errors
///
/// When the client didn't send the made-up cookie.
pub fn substitute(
    header: &[u8; 12],
    big_endian: bool,
    name: &[u8],
    data: &[u8],
    fake: &[u8],
    real: Option<&[u8]>,
) -> std::io::Result<Zeroizing<Vec<u8>>> {
    if name != MIT_MAGIC_COOKIE.as_bytes() || data != fake {
        return Err(invalid("an X11 connection without this session's cookie"));
    }
    let (name, data): (&[u8], &[u8]) = match real {
        Some(real) => (MIT_MAGIC_COOKIE.as_bytes(), real),
        None => (&[], &[]),
    };
    let write_u16 = |value: usize| {
        let value = u16::try_from(value).unwrap_or(0);
        if big_endian {
            value.to_be_bytes()
        } else {
            value.to_le_bytes()
        }
    };
    let mut out = Zeroizing::new(Vec::with_capacity(12 + 64));
    out.extend_from_slice(&header[..6]);
    out.extend_from_slice(&write_u16(name.len()));
    out.extend_from_slice(&write_u16(data.len()));
    out.extend_from_slice(&header[10..12]);
    out.extend_from_slice(name);
    let padded = out.len().next_multiple_of(4);
    out.resize(padded, 0);
    out.extend_from_slice(data);
    let padded = out.len().next_multiple_of(4);
    out.resize(padded, 0);
    Ok(out)
}

/// X11 forwarding for one connection: its cookies and the local display.
#[derive(Debug)]
pub struct Live {
    forwarding: Forwarding,
    display: Display,
}

impl Live {
    /// The made-up cookie in hexadecimal, for `x11-req`.
    #[must_use]
    pub fn fake_hex(&self) -> Zeroizing<String> {
        self.forwarding.fake_hex()
    }

    /// The display's screen.
    #[must_use]
    pub fn screen(&self) -> u16 {
        self.display.screen()
    }
}

/// Gets X11 forwarding ready: the display from `spec`, and the real cookie from `xauth` (none
/// when `xauth` isn't installed: the display may take connections without one, as an X server
/// started with `-ac` does).
///
/// # Errors
///
/// Why it can't be: no display, or an untrusted cookie that couldn't be made (the X server has no
/// SECURITY extension).
pub async fn prepare(spec: &X11Spec) -> Result<Live, String> {
    if spec.display.trim().is_empty() {
        return Err("there is no X display here (DISPLAY isn't set)".to_owned());
    }
    let display = Display::parse(&spec.display)
        .ok_or_else(|| format!("{:?} isn't an X display (DISPLAY)", spec.display))?;
    if cfg!(not(unix)) && matches!(display, Display::Unix { .. }) {
        return Err(format!(
            "DISPLAY={} names a Unix socket, which this system doesn't have: set it to \
             localhost:0.0 for an X server such as VcXsrv",
            spec.display
        ));
    }
    let real = if spec.trusted {
        xauth(&["list", spec.display.as_str()]).await?
    } else {
        untrusted_cookie(&spec.display).await?
    };
    Ok(Live {
        forwarding: Forwarding::new(real),
        display,
    })
}

/// Runs `xauth` with `args`: the cookie in its output. `Ok(None)` when `xauth` isn't installed.
async fn xauth(args: &[&str]) -> Result<Option<Vec<u8>>, String> {
    let output = match tokio::process::Command::new("xauth")
        .args(args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .await
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("xauth didn't run: {error}")),
    };
    let text = Zeroizing::new(String::from_utf8_lossy(&output.stdout).into_owned());
    Ok(cookie_from_xauth(&text))
}

/// A cookie made for this connection, with the X SECURITY extension's limits, in a file of its
/// own (removed at once): `xauth -f <file> generate <display> . untrusted timeout 1200`.
async fn untrusted_cookie(display: &str) -> Result<Option<Vec<u8>>, String> {
    let folder = tempfile::tempdir().map_err(|error| error.to_string())?;
    let file = folder.path().join("xauthority");
    let file = file.to_string_lossy().into_owned();
    let generated = tokio::process::Command::new("xauth")
        .args([
            "-f",
            file.as_str(),
            "generate",
            display,
            MIT_MAGIC_COOKIE,
            "untrusted",
            "timeout",
            "1200",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;
    match generated {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("xauth didn't run: {error}")),
        Ok(status) if !status.success() => {
            return Err(
                "the display can't make an untrusted cookie (no X SECURITY extension): set the \
                 host's X11 forwarding to trusted"
                    .to_owned(),
            );
        }
        Ok(_) => {}
    }
    match xauth(&["-f", file.as_str(), "list", display]).await? {
        Some(cookie) => Ok(Some(cookie)),
        None => Err("xauth made no cookie".to_owned()),
    }
}

/// A byte stream to the local display.
trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

async fn connect_display(display: &Display) -> std::io::Result<Box<dyn Stream>> {
    match display {
        #[cfg(unix)]
        Display::Unix { path, .. } => Ok(Box::new(tokio::net::UnixStream::connect(path).await?)),
        #[cfg(not(unix))]
        Display::Unix { .. } => Err(std::io::Error::other(
            "a Unix socket display, which this system doesn't have",
        )),
        Display::Tcp { host, .. } => {
            let port = display
                .port()
                .ok_or_else(|| std::io::Error::other("a display number too high"))?;
            let stream = tokio::net::TcpStream::connect((host.as_str(), port)).await?;
            let _ = stream.set_nodelay(true);
            Ok(Box::new(stream))
        }
    }
}

/// Serves one `x11` channel: its first message with the real cookie, then both ways until either
/// side closes. A channel without this session's cookie is closed at once.
pub async fn serve(channel: Channel<Msg>, live: Arc<Live>) {
    let mut channel = channel.into_stream();
    let result = async {
        let first = live.forwarding.first_message(&mut channel).await?;
        let mut display = connect_display(&live.display).await?;
        display.write_all(&first).await?;
        tokio::io::copy_bidirectional(&mut channel, &mut display).await?;
        Ok::<(), std::io::Error>(())
    }
    .await;
    if let Err(error) = result {
        tracing::debug!("an X11 connection ended: {error}");
    }
}

/// A cookie from `xauth list` output's lines (`host/unix:0  MIT-MAGIC-COOKIE-1  <hex>`).
#[must_use]
pub fn cookie_from_xauth(output: &str) -> Option<Vec<u8>> {
    output.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        let (_display, protocol, hex) = (words.next()?, words.next()?, words.next()?);
        if protocol != MIT_MAGIC_COOKIE || hex.len() % 2 != 0 {
            return None;
        }
        (0..hex.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok())
            .collect()
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests")]

    use super::*;

    #[test]
    fn displays() {
        assert_eq!(
            Display::parse(":0"),
            Some(Display::Unix {
                path: "/tmp/.X11-unix/X0".into(),
                number: 0,
                screen: 0
            })
        );
        assert_eq!(Display::parse("unix:1.2").unwrap().screen(), 2);
        let tcp = Display::parse("localhost:10.0").unwrap();
        assert_eq!(tcp.port(), Some(6010));
        assert!(matches!(&tcp, Display::Tcp { host, .. } if host == "localhost"));
        assert!(
            matches!(Display::parse("[::1]:0").unwrap(), Display::Tcp { host, .. } if host == "::1")
        );
        assert_eq!(
            Display::parse("/private/tmp/com.apple.launchd.abc/org.xquartz:0"),
            Some(Display::Unix {
                path: "/private/tmp/com.apple.launchd.abc/org.xquartz".into(),
                number: 0,
                screen: 0
            })
        );
        assert_eq!(Display::parse("nonsense"), None);
        assert_eq!(Display::parse(":x"), None);
    }

    fn setup(big_endian: bool, name: &[u8], data: &[u8]) -> Vec<u8> {
        let u16b = |value: usize| {
            let value = u16::try_from(value).unwrap();
            if big_endian {
                value.to_be_bytes()
            } else {
                value.to_le_bytes()
            }
        };
        let mut message = vec![if big_endian { b'B' } else { b'l' }, 0];
        message.extend_from_slice(&u16b(11));
        message.extend_from_slice(&u16b(0));
        message.extend_from_slice(&u16b(name.len()));
        message.extend_from_slice(&u16b(data.len()));
        message.extend_from_slice(&[0, 0]);
        message.extend_from_slice(name);
        message.resize(message.len().next_multiple_of(4), 0);
        message.extend_from_slice(data);
        message.resize(message.len().next_multiple_of(4), 0);
        message
    }

    #[tokio::test]
    async fn the_real_cookie_replaces_the_made_up_one() {
        let real = vec![7_u8; 16];
        let forwarding = Forwarding::new(Some(real.clone()));
        assert_eq!(forwarding.fake_hex().len(), 32);
        for big_endian in [false, true] {
            let sent = setup(
                big_endian,
                MIT_MAGIC_COOKIE.as_bytes(),
                forwarding.fake.as_slice(),
            );
            let out = forwarding
                .first_message(&mut sent.as_slice())
                .await
                .unwrap();
            assert_eq!(
                out.as_slice(),
                setup(big_endian, MIT_MAGIC_COOKIE.as_bytes(), &real)
            );
        }
        // No real cookie: no authorization.
        let open = Forwarding::new(None);
        let sent = setup(false, MIT_MAGIC_COOKIE.as_bytes(), open.fake.as_slice());
        let out = open.first_message(&mut sent.as_slice()).await.unwrap();
        assert_eq!(out.as_slice(), setup(false, &[], &[]));
        // Another cookie, or none: refused.
        let wrong = setup(false, MIT_MAGIC_COOKIE.as_bytes(), &[1; 16]);
        assert!(
            forwarding
                .first_message(&mut wrong.as_slice())
                .await
                .is_err()
        );
        let none = setup(false, &[], &[]);
        assert!(
            forwarding
                .first_message(&mut none.as_slice())
                .await
                .is_err()
        );
        assert!(
            forwarding
                .first_message(&mut b"GET / HTTP/1.1".as_slice())
                .await
                .is_err()
        );
        // Two forwardings don't share a made-up cookie.
        assert_ne!(
            Forwarding::new(None).fake_hex(),
            Forwarding::new(None).fake_hex()
        );
    }

    #[test]
    fn cookies_from_xauth() {
        let output = "host/unix:0  MIT-MAGIC-COOKIE-1  00ff10ab\nother  XDM-AUTHORIZATION-1  aa\n";
        assert_eq!(
            cookie_from_xauth(output),
            Some(vec![0x00, 0xff, 0x10, 0xab])
        );
        assert_eq!(
            cookie_from_xauth("host/unix:0  XDM-AUTHORIZATION-1  aa"),
            None
        );
        assert_eq!(cookie_from_xauth(""), None);
    }
}
