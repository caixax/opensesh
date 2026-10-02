//! The security types (RFC 6143 §7.2 and VeNCrypt): what the client offers, picks and answers.
//!
//! - **None** (1).
//! - **VNC authentication** (2): the server's 16-byte challenge encrypted with DES, the password
//!   (its first 8 bytes) as the key with each byte's bits reversed. It only proves the password:
//!   the session that follows isn't encrypted.
//! - **VeNCrypt** (19), version 0.2, with the X509 subtypes: TLS with the server's certificate
//!   (decided by the app, as for RDP), then nothing (X509None), VNC authentication (X509Vnc) or a
//!   user name and password (X509Plain). The anonymous TLS subtypes need anonymous
//!   Diffie-Hellman, which rustls doesn't have (and which can't tell the server from someone in
//!   the middle), so they aren't offered.

use des::Des;
use des::cipher::{BlockCipherEncrypt as _, KeyInit as _};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use zeroize::Zeroizing;

use crate::VncError;

/// No authentication.
pub const NONE: u8 = 1;
/// VNC authentication.
pub const VNC_AUTH: u8 = 2;
/// VeNCrypt.
pub const VENCRYPT: u8 = 19;

/// VeNCrypt's subtypes this client takes.
pub mod vencrypt {
    /// Plain: a user name and password, without TLS. Never chosen.
    pub const PLAIN: u32 = 256;
    /// TLS, then no authentication.
    pub const X509_NONE: u32 = 260;
    /// TLS, then VNC authentication.
    pub const X509_VNC: u32 = 261;
    /// TLS, then a user name and password.
    pub const X509_PLAIN: u32 = 262;
}

/// What to do after the security type is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// Nothing more.
    None,
    /// VNC authentication.
    VncAuth,
    /// VeNCrypt: TLS, then this subtype.
    VeNCrypt,
}

/// Picks one of the server's security types: VeNCrypt first (it encrypts), then VNC
/// authentication, then none.
///
/// # Errors
///
/// When the server offers none of them.
pub fn choose(offered: &[u8]) -> Result<(u8, Choice), VncError> {
    for (kind, choice) in [
        (VENCRYPT, Choice::VeNCrypt),
        (VNC_AUTH, Choice::VncAuth),
        (NONE, Choice::None),
    ] {
        if offered.contains(&kind) {
            return Ok((kind, choice));
        }
    }
    Err(VncError::Unsupported(format!(
        "the server offers only security types this client doesn't have ({})",
        offered
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    )))
}

/// Picks a VeNCrypt subtype, the X509 ones only: with a password first.
///
/// # Errors
///
/// When the server offers none of them (anonymous TLS only, say).
pub fn choose_vencrypt(offered: &[u32]) -> Result<u32, VncError> {
    [
        vencrypt::X509_PLAIN,
        vencrypt::X509_VNC,
        vencrypt::X509_NONE,
    ]
    .into_iter()
    .find(|subtype| offered.contains(subtype))
    .ok_or_else(|| {
        VncError::Unsupported(
            "the server offers VeNCrypt without certificates (anonymous TLS), which this client \
             doesn't do: give the server a certificate (TigerVNC: X509Cert and X509Key), or allow \
             VNC authentication"
                .into(),
        )
    })
}

/// The answer to VNC authentication's `challenge`.
#[must_use]
pub fn vnc_response(password: &str, challenge: &[u8; 16]) -> [u8; 16] {
    let mut key = Zeroizing::new([0_u8; 8]);
    for (target, byte) in key.iter_mut().zip(password.bytes()) {
        *target = byte.reverse_bits();
    }
    let mut response = *challenge;
    // A key of 8 bytes is always the right length.
    if let Ok(cipher) = Des::new_from_slice(key.as_slice()) {
        for chunk in response.chunks_exact_mut(8) {
            if let Ok(mut block) = des::cipher::Block::<Des>::try_from(&*chunk) {
                cipher.encrypt_block(&mut block);
                chunk.copy_from_slice(&block);
            }
        }
    }
    response
}

/// VNC authentication: reads the challenge and answers it.
///
/// # Errors
///
/// On a read or write error.
pub async fn vnc_auth<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    password: &str,
) -> Result<(), VncError> {
    let mut challenge = [0_u8; 16];
    stream.read_exact(&mut challenge).await?;
    stream
        .write_all(&vnc_response(password, &challenge))
        .await?;
    stream.flush().await?;
    Ok(())
}

/// VeNCrypt's negotiation up to TLS: the version (0.2), then the subtype. Returns it.
///
/// # Errors
///
/// On a read or write error, a server that refuses version 0.2, or no X509 subtype.
pub async fn vencrypt_negotiate<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
) -> Result<u32, VncError> {
    let major = stream.read_u8().await?;
    let minor = stream.read_u8().await?;
    if (major, minor) < (0, 2) {
        return Err(VncError::Unsupported(format!(
            "VeNCrypt {major}.{minor} (this client needs 0.2)"
        )));
    }
    stream.write_all(&[0, 2]).await?;
    stream.flush().await?;
    if stream.read_u8().await? != 0 {
        return Err(VncError::Unsupported(
            "the server refused VeNCrypt 0.2".into(),
        ));
    }
    let count = stream.read_u8().await?;
    let mut offered = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        offered.push(stream.read_u32().await?);
    }
    let subtype = choose_vencrypt(&offered)?;
    stream.write_all(&subtype.to_be_bytes()).await?;
    stream.flush().await?;
    if stream.read_u8().await? != 1 {
        return Err(VncError::Unsupported(
            "the server refused the VeNCrypt subtype".into(),
        ));
    }
    Ok(subtype)
}

/// VeNCrypt's Plain authentication (inside TLS): the user name and the password.
///
/// # Errors
///
/// On a write error, or a name or password longer than 4 GiB.
pub async fn plain<S: AsyncWrite + Unpin>(
    stream: &mut S,
    user: &str,
    password: &str,
) -> Result<(), VncError> {
    let too_long = || VncError::Unsupported("a user name or password too long".into());
    let user_len = u32::try_from(user.len()).map_err(|_| too_long())?;
    let password_len = u32::try_from(password.len()).map_err(|_| too_long())?;
    let mut message = Zeroizing::new(Vec::with_capacity(8 + user.len() + password.len()));
    message.extend_from_slice(&user_len.to_be_bytes());
    message.extend_from_slice(&password_len.to_be_bytes());
    message.extend_from_slice(user.as_bytes());
    message.extend_from_slice(password.as_bytes());
    stream.write_all(&message).await?;
    stream.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests")]

    use super::*;

    #[test]
    fn choices() {
        assert_eq!(choose(&[1, 2, 19]).unwrap(), (19, Choice::VeNCrypt));
        assert_eq!(choose(&[2, 1]).unwrap(), (2, Choice::VncAuth));
        assert_eq!(choose(&[1]).unwrap(), (1, Choice::None));
        assert!(choose(&[5, 6]).is_err());
        assert_eq!(choose_vencrypt(&[257, 261, 262]).unwrap(), 262);
        assert_eq!(choose_vencrypt(&[260]).unwrap(), 260);
        // Anonymous TLS only.
        assert!(choose_vencrypt(&[257, 258, 259]).is_err());
    }

    #[test]
    fn vnc_authentication_matches_a_known_answer() {
        // The key for "password" with each byte's bits reversed is 0E86CECEEEF64E26; the answer
        // to the challenge 00..0F was computed with OpenSSL's DES-ECB (`openssl enc -des-ecb`).
        let challenge: [u8; 16] = std::array::from_fn(|i| u8::try_from(i).unwrap());
        let expected = [
            0xb8, 0x66, 0x92, 0x41, 0x25, 0xc8, 0xee, 0xbb, 0x9d, 0xeb, 0xc1, 0xdb, 0x61, 0xc5,
            0x38, 0xe2,
        ];
        assert_eq!(vnc_response("password", &challenge), expected);
        // Only the first 8 bytes of the password count.
        assert_eq!(vnc_response("password-longer", &challenge), expected);
        assert_ne!(vnc_response("passwore", &challenge), expected);
    }

    #[tokio::test]
    async fn vencrypt_picks_x509_and_plain_sends_both() {
        let (mut client, mut server) = tokio::io::duplex(256);
        let task = tokio::spawn(async move { vencrypt_negotiate(&mut client).await });
        server.write_all(&[0, 2]).await.unwrap();
        let mut version = [0; 2];
        server.read_exact(&mut version).await.unwrap();
        assert_eq!(version, [0, 2]);
        server.write_all(&[0, 2]).await.unwrap();
        server.write_all(&261_u32.to_be_bytes()).await.unwrap();
        server.write_all(&257_u32.to_be_bytes()).await.unwrap();
        let chosen = server.read_u32().await.unwrap();
        assert_eq!(chosen, 261);
        server.write_all(&[1]).await.unwrap();
        assert_eq!(task.await.unwrap().unwrap(), 261);

        let (mut client, mut server) = tokio::io::duplex(256);
        plain(&mut client, "me", "pw").await.unwrap();
        let mut message = [0; 12];
        server.read_exact(&mut message).await.unwrap();
        assert_eq!(&message, b"\0\0\0\x02\0\0\0\x02mepw");
    }
}
