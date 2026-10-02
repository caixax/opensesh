//! RFB's messages (RFC 6143 §7.3 to §7.6): the version, the server's init, and what the client
//! sends.

use tokio::io::{AsyncRead, AsyncReadExt as _};

use crate::VncError;

/// Names, reasons and clipboard texts are cut at this many bytes.
pub const MAX_TEXT: usize = 1 << 20;

/// An RFB version this client speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Version {
    /// 3.3: the server picks the security type.
    V3_3,
    /// 3.7: the client picks; no reason when authentication fails.
    V3_7,
    /// 3.8.
    V3_8,
}

impl Version {
    /// The server's version line (`RFB 003.008\n`), as the version both speak. Unknown minor
    /// versions count as 3.3 (RFC 6143 §7.1.1), newer ones as 3.8.
    ///
    /// # Errors
    ///
    /// When it isn't an RFB version line.
    pub fn from_server(line: &[u8; 12]) -> Result<Self, VncError> {
        let text = std::str::from_utf8(line).unwrap_or_default();
        let parse =
            |range: std::ops::Range<usize>| text.get(range).and_then(|n| n.parse::<u32>().ok());
        let (Some("RFB "), Some(major), Some(minor), Some("\n")) =
            (text.get(0..4), parse(4..7), parse(8..11), text.get(11..12))
        else {
            return Err(VncError::Protocol(
                "this isn't a VNC server (no RFB version)".into(),
            ));
        };
        Ok(match (major, minor) {
            (3, 7) => Self::V3_7,
            (3, 8..) | (4.., _) => Self::V3_8,
            _ => Self::V3_3,
        })
    }

    /// The line the client answers with.
    #[must_use]
    pub fn line(self) -> &'static [u8; 12] {
        match self {
            Self::V3_3 => b"RFB 003.003\n",
            Self::V3_7 => b"RFB 003.007\n",
            Self::V3_8 => b"RFB 003.008\n",
        }
    }
}

/// A string: its length (u32), then its bytes, at most [`MAX_TEXT`] kept.
///
/// # Errors
///
/// On a read error.
pub async fn read_string<R: AsyncRead + Unpin>(reader: &mut R) -> Result<String, VncError> {
    let length = usize::try_from(reader.read_u32().await?).unwrap_or(usize::MAX);
    let mut kept = vec![0; length.min(MAX_TEXT)];
    reader.read_exact(&mut kept).await?;
    // The rest is read and dropped.
    let mut rest = length - kept.len();
    let mut sink = [0_u8; 4096];
    while rest > 0 {
        let chunk = rest.min(sink.len());
        reader.read_exact(&mut sink[..chunk]).await?;
        rest -= chunk;
    }
    Ok(String::from_utf8_lossy(&kept).into_owned())
}

/// Latin-1 bytes (RFB's clipboard) as text.
#[must_use]
pub fn from_latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| char::from(*byte)).collect()
}

/// Text as Latin-1 (characters outside it become `?`), with line breaks as `\n`.
#[must_use]
pub fn to_latin1(text: &str) -> Vec<u8> {
    text.replace("\r\n", "\n")
        .chars()
        .map(|character| u8::try_from(u32::from(character)).unwrap_or(b'?'))
        .collect()
}

/// The server's init: the desktop's size and name (its pixel format is replaced at once).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInit {
    /// Width.
    pub width: u16,
    /// Height.
    pub height: u16,
    /// The desktop's name.
    pub name: String,
}

/// Reads the server's init.
///
/// # Errors
///
/// On a read error.
pub async fn read_server_init<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<ServerInit, VncError> {
    let width = reader.read_u16().await?;
    let height = reader.read_u16().await?;
    let mut format = [0; 16];
    reader.read_exact(&mut format).await?;
    let name = read_string(reader).await?;
    Ok(ServerInit {
        width,
        height,
        name,
    })
}

/// SetPixelFormat: 32 bits, little-endian, true colour, red in the low byte (`[r, g, b, x]`).
#[must_use]
pub fn set_pixel_format() -> Vec<u8> {
    vec![
        0, 0, 0, 0, // type, padding
        32, 24, 0, 1, // bits per pixel, depth, big-endian, true colour
        0, 255, 0, 255, 0, 255, // the maximums
        0, 8, 16, // the shifts
        0, 0, 0, // padding
    ]
}

/// SetEncodings with `encodings`, in order of preference.
#[must_use]
pub fn set_encodings(encodings: &[i32]) -> Vec<u8> {
    let mut message = vec![2, 0];
    message.extend_from_slice(&u16::try_from(encodings.len()).unwrap_or(0).to_be_bytes());
    for encoding in encodings {
        message.extend_from_slice(&encoding.to_be_bytes());
    }
    message
}

/// FramebufferUpdateRequest for the whole desktop.
#[must_use]
pub fn update_request(incremental: bool, width: u16, height: u16) -> Vec<u8> {
    let mut message = vec![3, u8::from(incremental), 0, 0, 0, 0];
    message.extend_from_slice(&width.to_be_bytes());
    message.extend_from_slice(&height.to_be_bytes());
    message
}

/// KeyEvent.
#[must_use]
pub fn key_event(keysym: u32, down: bool) -> Vec<u8> {
    let mut message = vec![4, u8::from(down), 0, 0];
    message.extend_from_slice(&keysym.to_be_bytes());
    message
}

/// PointerEvent: the buttons down (bit 0 left, 1 middle, 2 right, 3 and 4 the wheel up and down,
/// 5 and 6 left and right) and the position.
#[must_use]
pub fn pointer_event(buttons: u8, x: u16, y: u16) -> Vec<u8> {
    let mut message = vec![5, buttons];
    message.extend_from_slice(&x.to_be_bytes());
    message.extend_from_slice(&y.to_be_bytes());
    message
}

/// ClientCutText with `text` as Latin-1.
#[must_use]
pub fn client_cut_text(text: &str) -> Vec<u8> {
    let bytes = to_latin1(text);
    let mut message = vec![6, 0, 0, 0];
    message.extend_from_slice(&u32::try_from(bytes.len()).unwrap_or(0).to_be_bytes());
    message.extend_from_slice(&bytes);
    message
}

/// SetDesktopSize: one screen of `width` by `height` (`screen` is the server's id for it).
#[must_use]
pub fn set_desktop_size(width: u16, height: u16, screen: u32) -> Vec<u8> {
    let mut message = vec![251, 0];
    message.extend_from_slice(&width.to_be_bytes());
    message.extend_from_slice(&height.to_be_bytes());
    message.extend_from_slice(&[1, 0]);
    message.extend_from_slice(&screen.to_be_bytes());
    message.extend_from_slice(&[0, 0, 0, 0]);
    message.extend_from_slice(&width.to_be_bytes());
    message.extend_from_slice(&height.to_be_bytes());
    message.extend_from_slice(&0_u32.to_be_bytes());
    message
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests")]

    use super::*;

    #[test]
    fn versions() {
        assert_eq!(
            Version::from_server(b"RFB 003.008\n").unwrap(),
            Version::V3_8
        );
        assert_eq!(
            Version::from_server(b"RFB 003.007\n").unwrap(),
            Version::V3_7
        );
        assert_eq!(
            Version::from_server(b"RFB 003.003\n").unwrap(),
            Version::V3_3
        );
        // Apple's 3.889 and others above 3.8 speak 3.8; unknown ones below, 3.3.
        assert_eq!(
            Version::from_server(b"RFB 003.889\n").unwrap(),
            Version::V3_8
        );
        assert_eq!(
            Version::from_server(b"RFB 003.005\n").unwrap(),
            Version::V3_3
        );
        assert!(Version::from_server(b"SSH-2.0-Open").is_err());
        assert_eq!(Version::V3_8.line(), b"RFB 003.008\n");
    }

    #[tokio::test]
    async fn strings_and_init() {
        let mut data = vec![0, 0, 0, 3];
        data.extend(b"abc");
        assert_eq!(read_string(&mut data.as_slice()).await.unwrap(), "abc");
        let mut init = vec![4, 0, 3, 0];
        init.extend([0; 16]);
        init.extend([0, 0, 0, 4]);
        init.extend(b"desk");
        let init = read_server_init(&mut init.as_slice()).await.unwrap();
        assert_eq!(
            (init.width, init.height, init.name.as_str()),
            (1024, 768, "desk")
        );
    }

    #[test]
    fn messages() {
        assert_eq!(set_pixel_format().len(), 20);
        assert_eq!(
            set_encodings(&[1, -239]),
            vec![2, 0, 0, 2, 0, 0, 0, 1, 255, 255, 255, 17]
        );
        assert_eq!(
            update_request(true, 2, 3),
            vec![3, 1, 0, 0, 0, 0, 0, 2, 0, 3]
        );
        assert_eq!(key_event(0xff1b, true), vec![4, 1, 0, 0, 0, 0, 0xff, 0x1b]);
        assert_eq!(pointer_event(1, 2, 3), vec![5, 1, 0, 2, 0, 3]);
        assert_eq!(
            client_cut_text("a€\r\nb"),
            vec![6, 0, 0, 0, 0, 0, 0, 4, b'a', b'?', b'\n', b'b']
        );
        assert_eq!(from_latin1(&[0x61, 0xf1]), "añ");
        assert_eq!(set_desktop_size(800, 600, 7).len(), 24);
    }
}
