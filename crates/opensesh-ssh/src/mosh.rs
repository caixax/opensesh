//! Starting a mosh server over SSH (PLAN Sprint 12, ADR 0032): `mosh-server new` runs on an exec
//! channel of its own with a PTY, as the `mosh` script runs it, and prints the UDP port and the
//! session key `mosh-client` needs. The key is a secret: it is never logged, and it leaves this
//! module only in [`MoshConnect`], whose `Debug` leaves it out.

use std::time::Duration;

use opensesh_term::backend::TermSize;
use russh::ChannelMsg;

use crate::SshError;
use crate::connect::Connection;

/// What runs on the server: a new session bound to the address SSH came in on, with 256 colors,
/// and a UTF-8 locale when the server's own isn't one (`mosh-server` refuses to run without).
pub const SERVER_COMMAND: &str = "mosh-server new -s -c 256 -l LANG=C.UTF-8";

/// How long the server may take to answer.
const TIMEOUT: Duration = Duration::from_secs(20);

/// How much of its output is kept.
const OUTPUT_LIMIT: usize = 64 * 1024;

/// Where `mosh-client` goes: the server's UDP port and the session key.
#[derive(Clone, PartialEq, Eq)]
pub struct MoshConnect {
    /// The UDP port.
    pub port: u16,
    /// The session key (22 base64 characters), for `MOSH_KEY`.
    pub key: String,
}

impl std::fmt::Debug for MoshConnect {
    /// The key is left out.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MoshConnect")
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

/// Why the mosh server didn't start.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// The server has no `mosh-server` (or not on the `PATH` a command gets).
    #[error(
        "mosh-server isn't installed on the server (or isn't on the PATH a command gets there)"
    )]
    Missing,
    /// It ran but printed no session: what it said.
    #[error("mosh-server didn't start: {0}")]
    Failed(String),
    /// The channel failed.
    #[error(transparent)]
    Ssh(#[from] SshError),
}

/// The `MOSH CONNECT <port> <key>` line of what the server printed.
#[must_use]
pub fn parse(output: &str) -> Option<MoshConnect> {
    output.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        if words.next()? != "MOSH" || words.next()? != "CONNECT" {
            return None;
        }
        let port = words.next()?.parse().ok().filter(|port| *port > 0)?;
        let key = words.next()?;
        let base64 = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'/';
        (key.len() == 22 && key.bytes().all(base64)).then(|| MoshConnect {
            port,
            key: key.to_owned(),
        })
    })
}

/// Whether what the server printed (and its exit status) says there is no `mosh-server`.
#[must_use]
pub fn is_missing(output: &str, status: Option<u32>) -> bool {
    status == Some(127)
        || output.lines().any(|line| {
            line.contains("mosh-server")
                && (line.contains("not found") || line.contains("No such file"))
        })
}

/// The last lines of what the server printed, for people: no escape codes, no session line.
fn tail(output: &str) -> String {
    let lines: Vec<String> = output
        .lines()
        .map(|line| plain(line).trim().to_owned())
        .filter(|line| !line.is_empty() && !line.starts_with("MOSH CONNECT"))
        .collect();
    let start = lines.len().saturating_sub(3);
    let text = lines[start..].join(" ");
    if text.is_empty() {
        "it printed nothing".to_owned()
    } else {
        text
    }
}

/// `line` without escape sequences and control characters.
fn plain(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // A CSI sequence ends with a letter (or `~`); other escapes are two characters.
            if chars.next() == Some('[') {
                for next in chars.by_ref() {
                    if next.is_ascii_alphabetic() || next == '~' {
                        break;
                    }
                }
            }
        } else if !c.is_control() {
            out.push(c);
        }
    }
    out
}

/// Runs [`SERVER_COMMAND`] on `connection` in a PTY of `term` and `size`, and returns where
/// `mosh-client` goes as soon as the server printed it.
///
/// # Errors
///
/// [`ServerError::Missing`] without `mosh-server`, [`ServerError::Failed`] with what it printed
/// when it didn't start (or said nothing in time), [`ServerError::Ssh`] when the channel failed.
pub async fn start_server(
    connection: &Connection,
    term: &str,
    size: TermSize,
) -> Result<MoshConnect, ServerError> {
    let target = connection.target()?;
    let mut channel = target
        .channel_open_session()
        .await
        .map_err(SshError::from)?;
    channel
        .request_pty(
            true,
            term,
            u32::from(size.columns),
            u32::from(size.lines),
            0,
            0,
            &[],
        )
        .await
        .map_err(|_| SshError::Refused {
            what: "a terminal (PTY) for mosh-server".to_owned(),
        })?;
    channel
        .exec(true, SERVER_COMMAND)
        .await
        .map_err(SshError::from)?;
    let reading = async {
        let mut output = Vec::new();
        let mut status = None;
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                    if output.len() < OUTPUT_LIMIT {
                        output.extend_from_slice(&data);
                    }
                    if let Some(connect) = parse(&String::from_utf8_lossy(&output)) {
                        return Ok(connect);
                    }
                }
                ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        let text = String::from_utf8_lossy(&output);
        if let Some(connect) = parse(&text) {
            Ok(connect)
        } else if is_missing(&text, status) {
            Err(ServerError::Missing)
        } else {
            Err(ServerError::Failed(tail(&text)))
        }
    };
    let result = tokio::time::timeout(TIMEOUT, reading)
        .await
        .unwrap_or_else(|_| {
            Err(ServerError::Failed(format!(
                "no answer in {} s",
                TIMEOUT.as_secs()
            )))
        });
    // The server goes on by itself (detached); this channel isn't needed any more.
    let _ = channel.close().await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "4NeCCgvZFe2RnPgrcU1PQw";

    #[test]
    fn the_session_line() {
        let output = format!(
            "\r\nMOSH CONNECT 60001 {KEY}\r\n\r\nmosh-server (mosh 1.4.0) [build mosh 1.4.0]\r\n"
        );
        let connect = parse(&output).unwrap_or_else(|| MoshConnect {
            port: 0,
            key: String::new(),
        });
        assert_eq!(connect.port, 60001);
        assert_eq!(connect.key, KEY);
        // The key stays out of debug output.
        assert!(!format!("{connect:?}").contains(KEY));
        assert_eq!(parse("MOSH CONNECT 0 4NeCCgvZFe2RnPgrcU1PQw"), None);
        assert_eq!(parse("MOSH CONNECT 60001 short"), None);
        assert_eq!(parse("MOSH CONNECT 60001 4NeCCgvZFe2RnPgrcU1P-w"), None);
        assert_eq!(parse("nothing here"), None);
    }

    #[test]
    fn a_missing_server() {
        assert!(is_missing(
            "bash: line 1: mosh-server: command not found",
            Some(127)
        ));
        assert!(is_missing("sh: 1: mosh-server: not found", None));
        assert!(!is_missing(
            "mosh-server needs a UTF-8 native locale to run.",
            Some(1)
        ));
    }

    #[test]
    fn what_a_failed_server_said() {
        assert_eq!(
            tail(
                "\u{1b}[0mline one\r\n\r\nThe locale requested by LANG=C.UTF-8 isn't available here.\r\nmosh-server needs a UTF-8 native locale to run.\r\n"
            ),
            "line one The locale requested by LANG=C.UTF-8 isn't available here. mosh-server needs a UTF-8 native locale to run."
        );
        assert_eq!(tail(""), "it printed nothing");
    }
}
