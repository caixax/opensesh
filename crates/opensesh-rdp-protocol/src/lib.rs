//! What OpenSesh and its RDP helper say to each other (ADR 0034). The helper (`rdp/`, a program
//! with its own lock file) runs one RDP session; the app starts it for a pane and talks to it over
//! its standard input and output, in length-prefixed messages. A VNC session (ADR 0035) is a task
//! in the app that speaks the same messages over channels, so one pane runs either:
//!
//! - **to the helper** ([`ToHelper`]): control in JSON (connect, input, resize, the certificate's
//!   answer, disconnect), and the password and clipboard text raw, so secrets never sit in JSON;
//! - **from the helper** ([`FromHelper`]): events in JSON (the state, the certificate to decide
//!   on, the pointer, the desktop's size), and raw rectangles of RGBA pixels, the pointer's
//!   picture and clipboard text.
//!
//! A message is a kind byte, a little-endian `u32` length and the payload. Also here, as neither
//! side's code: [`keys`] (Qt key events to scancodes) and [`frame`] (the app's copy of the
//! remote screen). This crate depends on nothing but `serde`.

pub mod frame;
pub mod keys;

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};

/// The largest message accepted (a 4K desktop's full frame is 32 MiB).
pub const MAX_MESSAGE: usize = 64 * 1024 * 1024;

/// What to connect to, and how.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connect {
    /// The address to connect to (127.0.0.1 through a jump host's tunnel).
    pub address: String,
    /// Its port.
    pub port: u16,
    /// The server's own name, for TLS and NLA.
    pub server_name: String,
    /// The user name (`DOMAIN\user` and `user@domain` are split).
    pub user: String,
    /// The domain, if any.
    pub domain: Option<String>,
    /// The desktop's width in pixels.
    pub width: u16,
    /// Its height.
    pub height: u16,
    /// The scale the server should use, in percent (100: none).
    pub scale_factor: u32,
    /// The keyboard layout id (see [`keys::layout_id`]).
    pub keyboard_layout: u32,
    /// Share the text clipboard.
    pub clipboard: bool,
    /// How long connecting may take, in seconds.
    pub timeout_secs: u64,
    /// This computer's name, as the server shows it.
    pub client_name: String,
    /// VNC: send no input and no clipboard.
    #[serde(default)]
    pub read_only: bool,
    /// VNC: Tight's JPEG quality, 0 to 9 (none: lossless).
    #[serde(default)]
    pub quality: Option<u8>,
    /// VNC: let other viewers stay connected.
    #[serde(default)]
    pub shared: bool,
}

/// A mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Button {
    /// Left.
    Left,
    /// Middle.
    Middle,
    /// Right.
    Right,
    /// Back.
    Back,
    /// Forward.
    Forward,
}

/// Control from the app, in JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "kebab-case")]
pub enum Control {
    /// Connect (the password follows as [`ToHelper::Password`] first).
    Connect(Connect),
    /// The answer to [`Event::Certificate`]: go on, or not.
    Certificate {
        /// Accepted.
        accept: bool,
    },
    /// A key by its scancode.
    Key {
        /// The set 1 code.
        code: u8,
        /// The `E0` prefix.
        extended: bool,
        /// Pressed or released.
        pressed: bool,
    },
    /// A key by its X keysym (VNC).
    Keysym {
        /// The keysym.
        keysym: u32,
        /// Pressed or released.
        pressed: bool,
    },
    /// A character without a scancode.
    Unicode {
        /// The character.
        character: char,
        /// Pressed or released.
        pressed: bool,
    },
    /// The mouse moved, in desktop pixels.
    Move {
        /// X.
        x: u16,
        /// Y.
        y: u16,
    },
    /// A mouse button.
    Button {
        /// Which.
        button: Button,
        /// Pressed or released.
        pressed: bool,
    },
    /// The wheel, in rotation units (120 a notch; positive is up and right).
    Wheel {
        /// Up and down.
        vertical: i16,
        /// Left and right.
        horizontal: i16,
    },
    /// Every key and button up.
    ReleaseAll,
    /// The lock keys' state.
    Locks {
        /// Caps Lock.
        caps: bool,
        /// Num Lock.
        num: bool,
        /// Scroll Lock.
        scroll: bool,
    },
    /// Ctrl+Alt+Del.
    CtrlAltDel,
    /// A new desktop size.
    Resize {
        /// Width.
        width: u16,
        /// Height.
        height: u16,
    },
    /// Connect again after a disconnection (a new password may come first).
    Reconnect,
    /// End the session and the helper.
    Disconnect,
}

/// A message to the helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToHelper {
    /// Control.
    Control(Control),
    /// The password (UTF-8), before [`Control::Connect`] or [`Control::Reconnect`].
    Password(String),
    /// This computer's clipboard text.
    Clipboard(String),
}

/// The connection's state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum Status {
    /// Reaching the server.
    Connecting {
        /// `host:port`.
        label: String,
    },
    /// Authenticating.
    Authenticating {
        /// `user@host`.
        label: String,
    },
    /// The desktop shows.
    Connected {
        /// Width.
        width: u16,
        /// Height.
        height: u16,
    },
    /// Not connected: a short code (`network`, `timeout`, `auth`, `certificate`, `protocol`,
    /// `lost`, `ended`...) and why. `auth` means a new password may help.
    Disconnected {
        /// The code.
        code: String,
        /// Why, for people.
        reason: String,
    },
}

/// An event from the helper, in JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum Event {
    /// The state.
    Status(Status),
    /// The server's certificate, to accept or not ([`Control::Certificate`]) before any
    /// credential is sent.
    Certificate {
        /// `SHA256:` and the unpadded base64 of its DER's digest.
        fingerprint: String,
        /// Its subject.
        subject: String,
        /// Its key's kind.
        key_type: String,
    },
    /// The desktop's size changed: rectangles for the whole of it follow.
    Size {
        /// Width.
        width: u16,
        /// Height.
        height: u16,
    },
    /// The system's arrow.
    PointerDefault,
    /// No pointer.
    PointerHidden,
    /// The session isn't encrypted (VNC without VeNCrypt): what is typed and shown crosses the
    /// network as it is. Sent after [`Status::Connected`].
    Unencrypted,
}

/// A rectangle of the desktop and its RGBA pixels (`width * 4` bytes a row).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pixels {
    /// Where.
    pub rect: frame::Rect,
    /// RGBA.
    pub rgba: Vec<u8>,
}

/// The pointer's picture: RGBA (not premultiplied) and its hot spot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PointerPicture {
    /// Width.
    pub width: u16,
    /// Height.
    pub height: u16,
    /// Hot spot X.
    pub hot_x: u16,
    /// Hot spot Y.
    pub hot_y: u16,
    /// RGBA.
    pub rgba: Vec<u8>,
}

/// A message from the helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FromHelper {
    /// An event.
    Event(Event),
    /// Pixels that changed.
    Pixels(Pixels),
    /// The pointer's picture.
    Pointer(PointerPicture),
    /// The server's clipboard text.
    Clipboard(String),
}

fn invalid(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_owned())
}

fn write_raw(out: &mut impl Write, kind: u8, payload: &[u8]) -> io::Result<()> {
    let length = u32::try_from(payload.len()).map_err(|_| invalid("message too big"))?;
    out.write_all(&[kind])?;
    out.write_all(&length.to_le_bytes())?;
    out.write_all(payload)?;
    out.flush()
}

/// A message: its kind and payload, or `None` at the end of the stream.
///
/// # Errors
///
/// When reading fails, or a message is over [`MAX_MESSAGE`].
pub fn read_raw(input: &mut impl Read) -> io::Result<Option<(u8, Vec<u8>)>> {
    let mut header = [0_u8; 5];
    match input.read_exact(&mut header) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let [kind, a, b, c, d] = header;
    let length =
        usize::try_from(u32::from_le_bytes([a, b, c, d])).map_err(|_| invalid("length"))?;
    if length > MAX_MESSAGE {
        return Err(invalid("message too big"));
    }
    let mut payload = vec![0; length];
    input.read_exact(&mut payload)?;
    Ok(Some((kind, payload)))
}

fn text(payload: Vec<u8>) -> io::Result<String> {
    String::from_utf8(payload).map_err(|_| invalid("not UTF-8"))
}

fn u16_at(bytes: &[u8], at: usize) -> io::Result<u16> {
    bytes
        .get(at..at + 2)
        .and_then(|two| <[u8; 2]>::try_from(two).ok())
        .map(u16::from_le_bytes)
        .ok_or_else(|| invalid("short message"))
}

impl ToHelper {
    /// Writes it.
    ///
    /// # Errors
    ///
    /// When writing fails.
    pub fn write(&self, out: &mut impl Write) -> io::Result<()> {
        match self {
            Self::Control(control) => {
                let json = serde_json::to_vec(control).map_err(io::Error::other)?;
                write_raw(out, 1, &json)
            }
            Self::Password(password) => write_raw(out, 2, password.as_bytes()),
            Self::Clipboard(text) => write_raw(out, 3, text.as_bytes()),
        }
    }

    /// Reads one, `None` at the end of the stream.
    ///
    /// # Errors
    ///
    /// When reading fails or the message isn't valid.
    pub fn read(input: &mut impl Read) -> io::Result<Option<Self>> {
        let Some((kind, payload)) = read_raw(input)? else {
            return Ok(None);
        };
        Ok(Some(match kind {
            1 => Self::Control(serde_json::from_slice(&payload).map_err(|_| invalid("control"))?),
            2 => Self::Password(text(payload)?),
            3 => Self::Clipboard(text(payload)?),
            _ => return Err(invalid("unknown message")),
        }))
    }
}

impl FromHelper {
    /// Writes it.
    ///
    /// # Errors
    ///
    /// When writing fails.
    pub fn write(&self, out: &mut impl Write) -> io::Result<()> {
        match self {
            Self::Event(event) => {
                let json = serde_json::to_vec(event).map_err(io::Error::other)?;
                write_raw(out, 1, &json)
            }
            Self::Pixels(pixels) => {
                let mut payload = Vec::with_capacity(8 + pixels.rgba.len());
                for value in [
                    pixels.rect.x,
                    pixels.rect.y,
                    pixels.rect.width,
                    pixels.rect.height,
                ] {
                    payload.extend_from_slice(&value.to_le_bytes());
                }
                payload.extend_from_slice(&pixels.rgba);
                write_raw(out, 2, &payload)
            }
            Self::Pointer(pointer) => {
                let mut payload = Vec::with_capacity(8 + pointer.rgba.len());
                for value in [pointer.width, pointer.height, pointer.hot_x, pointer.hot_y] {
                    payload.extend_from_slice(&value.to_le_bytes());
                }
                payload.extend_from_slice(&pointer.rgba);
                write_raw(out, 3, &payload)
            }
            Self::Clipboard(text) => write_raw(out, 4, text.as_bytes()),
        }
    }

    /// Reads one, `None` at the end of the stream.
    ///
    /// # Errors
    ///
    /// When reading fails or the message isn't valid (pixels that don't fill their rectangle).
    pub fn read(input: &mut impl Read) -> io::Result<Option<Self>> {
        let Some((kind, mut payload)) = read_raw(input)? else {
            return Ok(None);
        };
        Ok(Some(match kind {
            1 => Self::Event(serde_json::from_slice(&payload).map_err(|_| invalid("event"))?),
            2 => {
                let rect = frame::Rect {
                    x: u16_at(&payload, 0)?,
                    y: u16_at(&payload, 2)?,
                    width: u16_at(&payload, 4)?,
                    height: u16_at(&payload, 6)?,
                };
                let rgba = payload.split_off(8);
                if rgba.len() != usize::from(rect.width) * usize::from(rect.height) * 4 {
                    return Err(invalid("pixels"));
                }
                Self::Pixels(Pixels { rect, rgba })
            }
            3 => {
                let (width, height) = (u16_at(&payload, 0)?, u16_at(&payload, 2)?);
                let (hot_x, hot_y) = (u16_at(&payload, 4)?, u16_at(&payload, 6)?);
                let rgba = payload.split_off(8);
                if rgba.len() != usize::from(width) * usize::from(height) * 4 {
                    return Err(invalid("pointer"));
                }
                Self::Pointer(PointerPicture {
                    width,
                    height,
                    hot_x,
                    hot_y,
                    rgba,
                })
            }
            4 => Self::Clipboard(text(payload)?),
            _ => return Err(invalid("unknown message")),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_go_both_ways() {
        let mut wire = Vec::new();
        let to = [
            ToHelper::Password("pässword".to_owned()),
            ToHelper::Control(Control::Key {
                code: 0x1E,
                extended: false,
                pressed: true,
            }),
            ToHelper::Control(Control::Connect(Connect {
                address: "127.0.0.1".into(),
                port: 3389,
                server_name: "win11".into(),
                user: "ana".into(),
                domain: None,
                width: 1280,
                height: 800,
                scale_factor: 100,
                keyboard_layout: 0x040A,
                clipboard: true,
                timeout_secs: 20,
                client_name: "laptop".into(),
                read_only: false,
                quality: None,
                shared: true,
            })),
            ToHelper::Clipboard("copied".into()),
        ];
        for message in &to {
            message.write(&mut wire).ok();
        }
        let mut input = wire.as_slice();
        for message in &to {
            assert_eq!(
                ToHelper::read(&mut input).ok().flatten().as_ref(),
                Some(message)
            );
        }
        assert!(matches!(ToHelper::read(&mut input), Ok(None)));
        // The password never appears as JSON.
        assert!(!String::from_utf8_lossy(&wire).contains("\"pässword\""));

        let mut wire = Vec::new();
        let from = [
            FromHelper::Event(Event::Status(Status::Connected {
                width: 2,
                height: 1,
            })),
            FromHelper::Pixels(Pixels {
                rect: frame::Rect {
                    x: 1,
                    y: 0,
                    width: 2,
                    height: 1,
                },
                rgba: vec![1, 2, 3, 4, 5, 6, 7, 8],
            }),
            FromHelper::Pointer(PointerPicture {
                width: 1,
                height: 1,
                hot_x: 0,
                hot_y: 0,
                rgba: vec![9, 9, 9, 9],
            }),
            FromHelper::Clipboard("text".into()),
        ];
        for message in &from {
            message.write(&mut wire).ok();
        }
        let mut input = wire.as_slice();
        for message in &from {
            assert_eq!(
                FromHelper::read(&mut input).ok().flatten().as_ref(),
                Some(message)
            );
        }
    }

    #[test]
    fn bad_messages_are_refused() {
        // Too big.
        let mut big = vec![2_u8];
        big.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(FromHelper::read(&mut big.as_slice()).is_err());
        // Pixels that don't fill their rectangle.
        let mut wire = Vec::new();
        write_raw(&mut wire, 2, &[0, 0, 0, 0, 2, 0, 2, 0, 1, 2, 3]).ok();
        assert!(FromHelper::read(&mut wire.as_slice()).is_err());
        // Unknown kind.
        let mut wire = Vec::new();
        write_raw(&mut wire, 9, b"").ok();
        assert!(ToHelper::read(&mut wire.as_slice()).is_err());
    }

    #[test]
    fn control_json_reads_as_written() {
        let json = serde_json::to_string(&Control::Wheel {
            vertical: -120,
            horizontal: 0,
        })
        .unwrap_or_default();
        assert_eq!(json, r#"{"do":"wheel","vertical":-120,"horizontal":0}"#);
    }
}
