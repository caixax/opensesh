//! Reaching the first hop through a proxy (PLAN Sprint 7): SOCKS5 (RFC 1928, with the user and
//! password method of RFC 1929), HTTP CONNECT (RFC 9110, with Basic authentication), or a
//! ProxyCommand whose standard input and output carry the connection.

use std::pin::Pin;
use std::process::Stdio;
use std::task::{Context, Poll};

use base64ct::{Base64, Encoding as _};
use secrecy::{ExposeSecret, SecretString};
use tokio::io::{
    AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, ReadBuf,
};
use tokio::net::TcpStream;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use crate::SshError;

/// Any stream a hop can run over.
pub trait Transport: AsyncRead + AsyncWrite + Unpin + Send + 'static {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send + 'static> Transport for T {}

fn network(target: &str, message: impl std::fmt::Display) -> SshError {
    SshError::Network {
        target: target.to_owned(),
        message: message.to_string(),
    }
}

/// `host:port`, with brackets around IPv6 addresses.
#[must_use]
pub fn address(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

/// A TCP connection to `host`:`port`, with Nagle off (interactive traffic).
///
/// # Errors
///
/// [`SshError::Network`] when the name doesn't resolve or nothing answers.
pub async fn tcp(host: &str, port: u16) -> Result<TcpStream, SshError> {
    let target = address(host, port);
    let stream = TcpStream::connect((host.trim_start_matches('[').trim_end_matches(']'), port))
        .await
        .map_err(|error| network(&target, error))?;
    let _ = stream.set_nodelay(true);
    Ok(stream)
}

/// Asks a SOCKS5 proxy on `stream` for a connection to `host`:`port`.
///
/// # Errors
///
/// [`SshError::Network`] when the proxy refuses or doesn't speak SOCKS5.
pub async fn socks5<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    host: &str,
    port: u16,
    login: Option<&(String, SecretString)>,
) -> Result<(), SshError> {
    let target = address(host, port);
    let fail = |message: &str| network(&target, format!("SOCKS5 proxy: {message}"));
    let io = |error: std::io::Error| network(&target, format!("SOCKS5 proxy: {error}"));
    // Greeting: version 5, the methods we offer.
    let methods: &[u8] = if login.is_some() {
        &[0x00, 0x02]
    } else {
        &[0x00]
    };
    let mut greeting = vec![0x05, u8::try_from(methods.len()).unwrap_or(1)];
    greeting.extend_from_slice(methods);
    stream.write_all(&greeting).await.map_err(io)?;
    let mut choice = [0_u8; 2];
    stream.read_exact(&mut choice).await.map_err(io)?;
    if choice[0] != 0x05 {
        return Err(fail("not a SOCKS5 proxy"));
    }
    match choice[1] {
        0x00 => {}
        0x02 => {
            let Some((user, password)) = login else {
                return Err(fail("it asks for a user name and password"));
            };
            let password = password.expose_secret().as_bytes();
            let (Ok(user_len), Ok(password_len)) =
                (u8::try_from(user.len()), u8::try_from(password.len()))
            else {
                return Err(fail("the user name or password is longer than 255 bytes"));
            };
            let mut request = zeroize::Zeroizing::new(vec![0x01, user_len]);
            request.extend_from_slice(user.as_bytes());
            request.push(password_len);
            request.extend_from_slice(password);
            stream.write_all(&request).await.map_err(io)?;
            let mut status = [0_u8; 2];
            stream.read_exact(&mut status).await.map_err(io)?;
            if status[1] != 0x00 {
                return Err(fail("the user name or password was refused"));
            }
        }
        0xff => return Err(fail("none of our authentication methods is accepted")),
        _ => return Err(fail("it chose an unknown authentication method")),
    }
    // CONNECT to a domain name (the proxy resolves it) or an address.
    let mut request = vec![0x05, 0x01, 0x00];
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    match bare.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => {
            request.push(0x01);
            request.extend_from_slice(&ip.octets());
        }
        Ok(std::net::IpAddr::V6(ip)) => {
            request.push(0x04);
            request.extend_from_slice(&ip.octets());
        }
        Err(_) => {
            let len = u8::try_from(bare.len()).map_err(|_| fail("the host name is too long"))?;
            request.push(0x03);
            request.push(len);
            request.extend_from_slice(bare.as_bytes());
        }
    }
    request.extend_from_slice(&port.to_be_bytes());
    stream.write_all(&request).await.map_err(io)?;
    let mut reply = [0_u8; 4];
    stream.read_exact(&mut reply).await.map_err(io)?;
    if reply[1] != 0x00 {
        return Err(fail(match reply[1] {
            0x02 => "the connection is not allowed by its rules",
            0x03 => "the network is unreachable",
            0x04 => "the host is unreachable",
            0x05 => "the connection was refused",
            0x06 => "the connection timed out",
            _ => "the connection failed",
        }));
    }
    // Skip the bound address.
    let skip = match reply[3] {
        0x01 => 4,
        0x04 => 16,
        0x03 => {
            let mut len = [0_u8; 1];
            stream.read_exact(&mut len).await.map_err(io)?;
            usize::from(len[0])
        }
        _ => return Err(fail("an unknown address type in its reply")),
    };
    let mut rest = vec![0_u8; skip + 2];
    stream.read_exact(&mut rest).await.map_err(io)?;
    Ok(())
}

/// Asks an HTTP proxy on `stream` for a tunnel to `host`:`port` with CONNECT.
///
/// # Errors
///
/// [`SshError::Network`] when the proxy answers anything but 2xx.
pub async fn http_connect<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    host: &str,
    port: u16,
    login: Option<&(String, SecretString)>,
) -> Result<(), SshError> {
    let target = address(host, port);
    let io = |error: std::io::Error| network(&target, format!("HTTP proxy: {error}"));
    let mut request =
        zeroize::Zeroizing::new(format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n"));
    if let Some((user, password)) = login {
        let credentials = zeroize::Zeroizing::new(format!("{user}:{}", password.expose_secret()));
        request.push_str("Proxy-Authorization: Basic ");
        request.push_str(&Base64::encode_string(credentials.as_bytes()));
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).await.map_err(io)?;
    // Read the status line and headers byte by byte: what follows belongs to SSH.
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() > 16 * 1024 {
            return Err(network(&target, "HTTP proxy: the reply is too long"));
        }
        let mut byte = [0_u8; 1];
        stream.read_exact(&mut byte).await.map_err(io)?;
        head.push(byte[0]);
    }
    let status_line = String::from_utf8_lossy(&head);
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0);
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(network(
            &target,
            format!(
                "HTTP proxy answered {}",
                status_line.lines().next().unwrap_or_default().trim()
            ),
        ))
    }
}

/// `command` with `%h`, `%p`, `%r` and `%%` replaced.
#[must_use]
pub fn expand_command(command: &str, host: &str, port: u16, user: &str) -> String {
    let mut out = String::new();
    let mut chars = command.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('h') => out.push_str(host),
            Some('p') => out.push_str(&port.to_string()),
            Some('r') => out.push_str(user),
            Some('%') => out.push('%'),
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

/// A ProxyCommand's standard output and input as one stream. The process ends with it.
#[derive(Debug)]
pub struct CommandStream {
    stdout: ChildStdout,
    stdin: ChildStdin,
    _child: Child,
}

impl AsyncRead for CommandStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stdout).poll_read(cx, buf)
    }
}

impl AsyncWrite for CommandStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.stdin).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stdin).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stdin).poll_shutdown(cx)
    }
}

/// Starts `command` (already expanded) through the system shell (`sh -c` or `cmd /C`).
///
/// # Errors
///
/// [`SshError::Local`] when it can't be started.
pub fn spawn_command(command: &str) -> Result<CommandStream, SshError> {
    let mut process = if cfg!(windows) {
        let mut process = Command::new("cmd");
        process.arg("/C").arg(command);
        process
    } else {
        let mut process = Command::new("sh");
        process.arg("-c").arg(command);
        process
    };
    let local = |message: String| SshError::Local {
        what: "the proxy command".to_owned(),
        message,
    };
    let mut child = process
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| local(error.to_string()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| local("no standard output".to_owned()))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| local("no standard input".to_owned()))?;
    Ok(CommandStream {
        stdout,
        stdin,
        _child: child,
    })
}

/// Reads one line (for tests of line-based fake servers).
#[doc(hidden)]
pub async fn read_line<S: AsyncRead + Unpin>(stream: &mut BufReader<S>) -> std::io::Result<String> {
    let mut line = String::new();
    stream.read_line(&mut line).await?;
    Ok(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[test]
    fn commands_expand() {
        assert_eq!(
            expand_command("nc -X 5 -x proxy:1080 %h %p # %r %% %x", "web", 22, "me"),
            "nc -X 5 -x proxy:1080 web 22 # me % %x"
        );
        assert_eq!(address("::1", 22), "[::1]:22");
        assert_eq!(address("web", 2222), "web:2222");
    }

    #[tokio::test]
    async fn socks5_with_login() {
        let (mut client, mut server) = duplex(1024);
        let proxy = tokio::spawn(async move {
            let mut greeting = [0_u8; 4];
            server.read_exact(&mut greeting).await.unwrap();
            assert_eq!(greeting, [5, 2, 0, 2]);
            server.write_all(&[5, 2]).await.unwrap();
            let mut login = [0_u8; 1 + 1 + 2 + 1 + 6];
            server.read_exact(&mut login).await.unwrap();
            assert_eq!(&login, b"\x01\x02me\x06secret");
            server.write_all(&[1, 0]).await.unwrap();
            let mut connect = [0_u8; 4 + 1 + 3 + 2];
            server.read_exact(&mut connect).await.unwrap();
            assert_eq!(&connect[..5], &[5, 1, 0, 3, 3]);
            assert_eq!(&connect[5..8], b"web");
            assert_eq!(u16::from_be_bytes([connect[8], connect[9]]), 2222);
            // Success, bound to 0.0.0.0:0, then SSH bytes follow.
            server
                .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
                .await
                .unwrap();
            server.write_all(b"SSH-2.0-test\r\n").await.unwrap();
        });
        let login = ("me".to_owned(), SecretString::from("secret"));
        socks5(&mut client, "web", 2222, Some(&login))
            .await
            .unwrap();
        let mut banner = [0_u8; 14];
        client.read_exact(&mut banner).await.unwrap();
        assert_eq!(&banner, b"SSH-2.0-test\r\n");
        proxy.await.unwrap();
    }

    #[tokio::test]
    async fn socks5_refusals() {
        let (mut client, mut server) = duplex(1024);
        tokio::spawn(async move {
            let mut greeting = [0_u8; 3];
            server.read_exact(&mut greeting).await.unwrap();
            server.write_all(&[5, 0]).await.unwrap();
            let mut connect = [0_u8; 4 + 4 + 2];
            server.read_exact(&mut connect).await.unwrap();
            assert_eq!(connect[3], 1);
            server
                .write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0])
                .await
                .unwrap();
        });
        let error = socks5(&mut client, "10.0.0.1", 22, None).await.unwrap_err();
        assert!(error.to_string().contains("refused"), "{error}");
    }

    #[tokio::test]
    async fn http_connect_with_basic_auth() {
        let (client, server) = duplex(4096);
        let proxy = tokio::spawn(async move {
            let mut reader = BufReader::new(server);
            let request = read_line(&mut reader).await.unwrap();
            assert_eq!(request, "CONNECT web:22 HTTP/1.1\r\n");
            let mut auth = String::new();
            loop {
                let line = read_line(&mut reader).await.unwrap();
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.strip_prefix("Proxy-Authorization: Basic ") {
                    auth = value.trim().to_owned();
                }
            }
            assert_eq!(auth, Base64::encode_string(b"me:pw"));
            let mut server = reader.into_inner();
            server
                .write_all(b"HTTP/1.1 200 Connection established\r\n\r\nSSH-2.0-x\r\n")
                .await
                .unwrap();
        });
        let mut client = client;
        let login = ("me".to_owned(), SecretString::from("pw"));
        http_connect(&mut client, "web", 22, Some(&login))
            .await
            .unwrap();
        let mut banner = [0_u8; 11];
        client.read_exact(&mut banner).await.unwrap();
        assert_eq!(&banner, b"SSH-2.0-x\r\n");
        proxy.await.unwrap();
    }

    #[tokio::test]
    async fn http_connect_refused() {
        let (mut client, mut server) = duplex(4096);
        tokio::spawn(async move {
            let mut buffer = [0_u8; 256];
            let _ = server.read(&mut buffer).await.unwrap();
            server
                .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n")
                .await
                .unwrap();
        });
        let error = http_connect(&mut client, "web", 22, None)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("407"), "{error}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_command_carries_the_connection() {
        // `cat` echoes what it gets: the stream reads back its input.
        let mut stream = spawn_command("cat").unwrap();
        stream.write_all(b"xyz\n").await.unwrap();
        stream.flush().await.unwrap();
        let mut back = [0_u8; 3];
        stream.read_exact(&mut back).await.unwrap();
        assert_eq!(&back, b"xyz");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn a_command_carries_the_connection() {
        // Windows has no `cat` that answers before its input closes: check the output side.
        let mut stream = spawn_command("echo xyz").unwrap();
        let mut back = [0_u8; 3];
        stream.read_exact(&mut back).await.unwrap();
        assert_eq!(&back, b"xyz");
    }
}
