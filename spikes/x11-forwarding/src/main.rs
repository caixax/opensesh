//! X11 forwarding spike (Sprint 7, Linux): runs a command on an SSH server with X11 forwarding,
//! and carries each X11 connection the server opens back to the local display.
//!
//! What the client does (RFC 4254 §6.3, and what OpenSSH does):
//!
//! 1. `x11-req` on the session channel with a *fake* MIT-MAGIC-COOKIE-1, random for each run.
//!    The server's xauth stores it for the remote `DISPLAY`; the real cookie never leaves this
//!    machine.
//! 2. For each `x11` channel the server opens, read the X11 connection setup the remote program
//!    sends, check that it carries the fake cookie (refuse the channel otherwise), and send the
//!    setup on to the local display with the *real* cookie from `xauth list $DISPLAY` (or none,
//!    when the display has no cookie, as WSLg's Xwayland). Everything after the setup is copied
//!    both ways unchanged.
//! 3. The local display is `$DISPLAY`: `:N` is the Unix socket `/tmp/.X11-unix/XN`, `host:N` is
//!    TCP port 6000 + N.
//!
//! Usage (see README.md):
//!
//! ```sh
//! SPIKE_KEY=~/.ssh/id_ed25519 cargo run -- user@host:port "xdpyinfo | head -n 5"
//! SPIKE_PASSWORD=... cargo run -- user@host:port xeyes
//! ```
//!
//! A spike: it trusts any host key (it prints the fingerprint) and unwraps freely.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use russh::client::{self, Handle, Handler, Msg, Session};
use russh::keys::{PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{Channel, ChannelMsg};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const PROTOCOL: &str = "MIT-MAGIC-COOKIE-1";

/// Where the local X server listens.
#[derive(Debug, Clone)]
enum Display {
    Unix(PathBuf),
    Tcp(String, u16),
}

/// `:0`, `:0.0`, `unix:0`, `localhost:10.0` -> the socket, and the screen number.
fn parse_display(text: &str) -> Option<(Display, u32)> {
    let (host, rest) = text.rsplit_once(':')?;
    let (number, screen) = match rest.split_once('.') {
        Some((number, screen)) => (number, screen.parse().ok()?),
        None => (rest, 0),
    };
    let number: u16 = number.parse().ok()?;
    let display = if host.is_empty() || host == "unix" {
        Display::Unix(PathBuf::from(format!("/tmp/.X11-unix/X{number}")))
    } else {
        Display::Tcp(host.to_owned(), 6000 + number)
    };
    Some((display, screen))
}

/// The display's real cookie, from `xauth list` (none when it has no entry).
async fn real_cookie(display: &str) -> Option<Vec<u8>> {
    let output = tokio::process::Command::new("xauth")
        .args(["list", display])
        .output()
        .await
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().find(|line| line.contains(PROTOCOL))?;
    let hex = line.split_whitespace().last()?;
    from_hex(hex)
}

fn from_hex(hex: &str) -> Option<Vec<u8>> {
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(hex.get(index..index + 2)?, 16).ok())
        .collect()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn pad(length: usize) -> usize {
    (4 - length % 4) % 4
}

/// The X11 connection setup (the first bytes a client sends): byte order, version, and the
/// authorization it carries.
struct Setup {
    big_endian: bool,
    major: u16,
    minor: u16,
    name: Vec<u8>,
    data: Vec<u8>,
}

impl Setup {
    async fn read(stream: &mut (impl AsyncReadExt + Unpin)) -> std::io::Result<Self> {
        let mut head = [0_u8; 12];
        stream.read_exact(&mut head).await?;
        let big_endian = head[0] == b'B';
        let u16_at = |at: usize| {
            let pair = [head[at], head[at + 1]];
            if big_endian {
                u16::from_be_bytes(pair)
            } else {
                u16::from_le_bytes(pair)
            }
        };
        let (major, minor) = (u16_at(2), u16_at(4));
        let (name_length, data_length) = (usize::from(u16_at(6)), usize::from(u16_at(8)));
        let mut name = vec![0; name_length + pad(name_length)];
        stream.read_exact(&mut name).await?;
        name.truncate(name_length);
        let mut data = vec![0; data_length + pad(data_length)];
        stream.read_exact(&mut data).await?;
        data.truncate(data_length);
        Ok(Self {
            big_endian,
            major,
            minor,
            name,
            data,
        })
    }

    fn bytes(&self) -> Vec<u8> {
        let u16_bytes = |value: u16| {
            if self.big_endian {
                value.to_be_bytes()
            } else {
                value.to_le_bytes()
            }
        };
        let mut out = vec![if self.big_endian { b'B' } else { b'l' }, 0];
        out.extend_from_slice(&u16_bytes(self.major));
        out.extend_from_slice(&u16_bytes(self.minor));
        out.extend_from_slice(&u16_bytes(u16::try_from(self.name.len()).unwrap()));
        out.extend_from_slice(&u16_bytes(u16::try_from(self.data.len()).unwrap()));
        out.extend_from_slice(&[0, 0]);
        out.extend_from_slice(&self.name);
        out.extend(std::iter::repeat_n(0, pad(self.name.len())));
        out.extend_from_slice(&self.data);
        out.extend(std::iter::repeat_n(0, pad(self.data.len())));
        out
    }
}

/// One forwarded X11 connection: check the fake cookie, swap in the real one, then copy.
async fn forward(channel: Channel<Msg>, display: Display, fake: Vec<u8>, real: Option<Vec<u8>>) {
    let mut remote = channel.into_stream();
    let mut setup = match Setup::read(&mut remote).await {
        Ok(setup) => setup,
        Err(error) => {
            eprintln!("x11: no connection setup: {error}");
            return;
        }
    };
    if setup.name != PROTOCOL.as_bytes() || setup.data != fake {
        eprintln!(
            "x11: refused a connection without the fake cookie (protocol {:?})",
            String::from_utf8_lossy(&setup.name)
        );
        return;
    }
    match real {
        Some(cookie) => setup.data = cookie,
        None => {
            setup.name.clear();
            setup.data.clear();
        }
    }
    let result = match &display {
        Display::Unix(path) => match tokio::net::UnixStream::connect(path).await {
            Ok(mut local) => {
                local.write_all(&setup.bytes()).await.unwrap();
                tokio::io::copy_bidirectional(&mut remote, &mut local).await
            }
            Err(error) => Err(error),
        },
        Display::Tcp(host, port) => {
            match tokio::net::TcpStream::connect((host.as_str(), *port)).await {
                Ok(mut local) => {
                    local.write_all(&setup.bytes()).await.unwrap();
                    tokio::io::copy_bidirectional(&mut remote, &mut local).await
                }
                Err(error) => Err(error),
            }
        }
    };
    match result {
        Ok((up, down)) => eprintln!("x11: connection closed ({up} bytes up, {down} down)"),
        Err(error) => eprintln!("x11: {display:?}: {error}"),
    }
}

struct Client {
    display: Display,
    fake: Vec<u8>,
    real: Option<Vec<u8>>,
}

impl Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let key = key.public_key();
        eprintln!(
            "host key {} {} (a spike: not checked)",
            key.algorithm(),
            key.fingerprint(Default::default())
        );
        Ok(true)
    }

    async fn server_channel_open_x11(
        &mut self,
        channel: Channel<Msg>,
        originator_address: &str,
        originator_port: u32,
        reply: client::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        eprintln!("x11: the server opened a channel ({originator_address}:{originator_port})");
        reply.accept().await;
        tokio::spawn(forward(
            channel,
            self.display.clone(),
            self.fake.clone(),
            self.real.clone(),
        ));
        Ok(())
    }
}

async fn authenticate(handle: &mut Handle<Client>, user: &str) {
    if let Ok(password) = std::env::var("SPIKE_PASSWORD") {
        assert!(
            handle
                .authenticate_password(user, password)
                .await
                .unwrap()
                .success(),
            "password refused"
        );
        return;
    }
    let path = std::env::var("SPIKE_KEY").expect("SPIKE_KEY (a key file) or SPIKE_PASSWORD");
    let key = russh::keys::load_secret_key(&path, None).unwrap();
    let hash = handle.best_supported_rsa_hash().await.unwrap().flatten();
    assert!(
        handle
            .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
            .await
            .unwrap()
            .success(),
        "key refused"
    );
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let target = args.next().expect("user@host[:port]");
    let command = args
        .next()
        .unwrap_or_else(|| "xdpyinfo | head -n 5".to_owned());
    let (user, address) = target.split_once('@').expect("user@host[:port]");
    let (host, port) = match address.rsplit_once(':') {
        Some((host, port)) => (host.to_owned(), port.parse::<u16>().unwrap()),
        None => (address.to_owned(), 22),
    };

    let display_name = std::env::var("DISPLAY").expect("DISPLAY: no local X display");
    let (display, screen) = parse_display(&display_name).expect("a DISPLAY like :0 or host:10.0");
    let real = real_cookie(&display_name).await;
    let mut fake = vec![0_u8; 16];
    getrandom::fill(&mut fake).unwrap();
    eprintln!(
        "local display {display_name} ({display:?}), real cookie: {}",
        if real.is_some() { "from xauth" } else { "none" }
    );

    let config = Arc::new(client::Config::default());
    let client = Client {
        display,
        fake: fake.clone(),
        real,
    };
    let mut handle = client::connect(config, (host.as_str(), port), client)
        .await
        .unwrap();
    authenticate(&mut handle, user).await;

    let mut channel = handle.channel_open_session().await.unwrap();
    channel
        .request_x11(true, false, PROTOCOL, to_hex(&fake), screen)
        .await
        .unwrap();
    channel.exec(true, command.as_bytes()).await.unwrap();
    let mut code = None;
    while let Some(message) = channel.wait().await {
        match message {
            ChannelMsg::Data { data } => print!("{}", String::from_utf8_lossy(&data)),
            ChannelMsg::ExtendedData { data, .. } => eprint!("{}", String::from_utf8_lossy(&data)),
            ChannelMsg::Success => eprintln!("(the server accepted a request)"),
            ChannelMsg::Failure => eprintln!("(the server refused a request: x11-req?)"),
            ChannelMsg::ExitStatus { exit_status } => code = Some(exit_status),
            ChannelMsg::Close => break,
            _ => {}
        }
    }
    eprintln!("exit status {code:?}");
    handle
        .disconnect(russh::Disconnect::ByApplication, "", "")
        .await
        .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displays() {
        assert!(
            matches!(parse_display(":0"), Some((Display::Unix(path), 0)) if path.ends_with("X0"))
        );
        assert!(matches!(
            parse_display("unix:1.2"),
            Some((Display::Unix(_), 2))
        ));
        assert!(matches!(
            parse_display("localhost:10.0"),
            Some((Display::Tcp(host, 6010), 0)) if host == "localhost"
        ));
        assert!(parse_display("nothing").is_none());
    }

    #[tokio::test]
    async fn the_setup_round_trips() {
        let setup = Setup {
            big_endian: false,
            major: 11,
            minor: 0,
            name: PROTOCOL.as_bytes().to_vec(),
            data: vec![7; 16],
        };
        let bytes = setup.bytes();
        assert_eq!(bytes.len() % 4, 0);
        let read = Setup::read(&mut bytes.as_slice()).await.unwrap();
        assert_eq!(read.name, setup.name);
        assert_eq!(read.data, setup.data);
        assert_eq!((read.major, read.minor), (11, 0));
        assert_eq!(from_hex(&to_hex(&[0, 255, 16])), Some(vec![0, 255, 16]));
    }
}
