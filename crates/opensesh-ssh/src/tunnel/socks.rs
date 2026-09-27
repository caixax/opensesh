//! The SOCKS5 server of dynamic forwarding (RFC 1928): no authentication, `CONNECT` only, to an
//! IPv4 or IPv6 address or a name (resolved by the SSH server, like `curl --socks5-hostname`).

use std::net::{Ipv4Addr, Ipv6Addr};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Why a client was turned away.
#[derive(Debug, thiserror::Error)]
pub enum SocksError {
    /// Not SOCKS5 (SOCKS4 included).
    #[error("not a SOCKS5 client (version {0})")]
    Version(u8),
    /// It offered no method without authentication.
    #[error("the client wants authentication, which isn't offered")]
    NoMethod,
    /// It asked for something else than `CONNECT` (`BIND`, `UDP ASSOCIATE`).
    #[error("command {0} is not supported (only CONNECT)")]
    Command(u8),
    /// An address type that doesn't exist.
    #[error("address type {0} is not supported")]
    AddressType(u8),
    /// The connection broke.
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// The answer to a `CONNECT` (RFC 1928 §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    /// Connected.
    Succeeded = 0,
    /// Something else went wrong.
    GeneralFailure = 1,
    /// The server refused to connect there.
    NotAllowed = 2,
    /// The host couldn't be reached.
    HostUnreachable = 4,
    /// The host refused the connection.
    ConnectionRefused = 5,
    /// Not `CONNECT`.
    CommandNotSupported = 7,
    /// Not an address type we know.
    AddressTypeNotSupported = 8,
}

/// Greets a client and reads where it wants to go: the host (an address or a name) and port.
/// Unsupported requests are answered before the error is returned.
///
/// # Errors
///
/// [`SocksError`] when the client isn't one we serve, or the connection breaks.
pub async fn accept<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
) -> Result<(String, u16), SocksError> {
    let mut head = [0_u8; 2];
    stream.read_exact(&mut head).await?;
    if head[0] != 5 {
        return Err(SocksError::Version(head[0]));
    }
    let mut methods = vec![0_u8; usize::from(head[1])];
    stream.read_exact(&mut methods).await?;
    if !methods.contains(&0) {
        stream.write_all(&[5, 0xff]).await?;
        return Err(SocksError::NoMethod);
    }
    stream.write_all(&[5, 0]).await?;

    let mut request = [0_u8; 4];
    stream.read_exact(&mut request).await?;
    if request[0] != 5 {
        return Err(SocksError::Version(request[0]));
    }
    let host = match request[3] {
        1 => {
            let mut octets = [0_u8; 4];
            stream.read_exact(&mut octets).await?;
            Ipv4Addr::from(octets).to_string()
        }
        3 => {
            let mut length = [0_u8; 1];
            stream.read_exact(&mut length).await?;
            let mut name = vec![0_u8; usize::from(length[0])];
            stream.read_exact(&mut name).await?;
            String::from_utf8_lossy(&name).into_owned()
        }
        4 => {
            let mut octets = [0_u8; 16];
            stream.read_exact(&mut octets).await?;
            Ipv6Addr::from(octets).to_string()
        }
        other => {
            reply(stream, Reply::AddressTypeNotSupported).await?;
            return Err(SocksError::AddressType(other));
        }
    };
    let mut port = [0_u8; 2];
    stream.read_exact(&mut port).await?;
    if request[1] != 1 {
        reply(stream, Reply::CommandNotSupported).await?;
        return Err(SocksError::Command(request[1]));
    }
    Ok((host, u16::from_be_bytes(port)))
}

/// Answers a `CONNECT` (the bound address is left as 0.0.0.0:0, as most servers do).
///
/// # Errors
///
/// When the connection breaks.
pub async fn reply<S: AsyncWrite + Unpin>(stream: &mut S, reply: Reply) -> std::io::Result<()> {
    stream
        .write_all(&[5, reply as u8, 0, 1, 0, 0, 0, 0, 0, 0])
        .await
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests")]

    use super::*;

    /// Runs `accept` against what a client sends; returns its result and what it answered.
    async fn serve(sent: &[u8]) -> (Result<(String, u16), SocksError>, Vec<u8>) {
        let (mut client, mut server) = tokio::io::duplex(1024);
        client.write_all(sent).await.unwrap();
        let result = accept(&mut server).await;
        drop(server);
        let mut answered = Vec::new();
        client.read_to_end(&mut answered).await.unwrap();
        (result, answered)
    }

    #[tokio::test]
    async fn requests() {
        let (result, answered) = serve(&[
            5, 1, 0, 5, 1, 0, 3, 11, b'e', b'x', b'a', b'm', b'p', b'l', b'e', b'.', b'o', b'r',
            b'g', 0, 80,
        ])
        .await;
        assert_eq!(result.unwrap(), ("example.org".to_owned(), 80));
        assert_eq!(answered, [5, 0]);

        let (result, _) = serve(&[5, 2, 2, 0, 5, 1, 0, 1, 10, 0, 0, 7, 0x1f, 0x90]).await;
        assert_eq!(result.unwrap(), ("10.0.0.7".to_owned(), 8080));

        let mut v6 = vec![5, 1, 0, 5, 1, 0, 4];
        v6.extend_from_slice(&"2001:db8::1".parse::<Ipv6Addr>().unwrap().octets());
        v6.extend_from_slice(&[0, 22]);
        let (result, _) = serve(&v6).await;
        assert_eq!(result.unwrap(), ("2001:db8::1".to_owned(), 22));
    }

    #[tokio::test]
    async fn refusals() {
        // SOCKS4.
        let (result, answered) = serve(&[4, 1, 0, 80, 127, 0, 0, 1, 0]).await;
        assert!(matches!(result, Err(SocksError::Version(4))));
        assert!(answered.is_empty());
        // Only username and password offered.
        let (result, answered) = serve(&[5, 1, 2]).await;
        assert!(matches!(result, Err(SocksError::NoMethod)));
        assert_eq!(answered, [5, 0xff]);
        // BIND.
        let (result, answered) = serve(&[5, 1, 0, 5, 2, 0, 1, 127, 0, 0, 1, 0, 80]).await;
        assert!(matches!(result, Err(SocksError::Command(2))));
        assert_eq!(answered, [5, 0, 5, 7, 0, 1, 0, 0, 0, 0, 0, 0]);
        // An unknown address type.
        let (result, answered) = serve(&[5, 1, 0, 5, 1, 0, 9]).await;
        assert!(matches!(result, Err(SocksError::AddressType(9))));
        assert_eq!(answered, [5, 0, 5, 8, 0, 1, 0, 0, 0, 0, 0, 0]);
    }
}
