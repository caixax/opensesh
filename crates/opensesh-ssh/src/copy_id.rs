//! "Install my key on the server" (PLAN Sprint 7), like `ssh-copy-id`: the public key is added
//! to `~/.ssh/authorized_keys` unless it is there already, with the permissions sshd expects.
//! The key travels on the command's standard input, not on its command line.

use russh::ChannelMsg;

use crate::SshError;
use crate::connect::Connection;

/// The remote script: creates `~/.ssh` (0700) and `authorized_keys` (0600), then appends the line
/// read from standard input unless the same key (type and base64) is already there.
pub const SCRIPT: &str = "umask 077; mkdir -p ~/.ssh && touch ~/.ssh/authorized_keys && \
     chmod 700 ~/.ssh && chmod 600 ~/.ssh/authorized_keys && \
     read -r line && set -- $line && \
     if grep -q -F -- \"$1 $2\" ~/.ssh/authorized_keys; then echo __present__; \
     else printf '%s\\n' \"$line\" >> ~/.ssh/authorized_keys && echo __added__; fi";

/// What happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Installed {
    /// The key was added.
    Added,
    /// The key was already there.
    AlreadyThere,
}

/// Installs `public_line` (an OpenSSH public key line) for the connected user.
///
/// # Errors
///
/// When the channel fails or the script doesn't report success (for example a server without a
/// POSIX shell).
pub async fn install(connection: &Connection, public_line: &str) -> Result<Installed, SshError> {
    let line = public_line
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned();
    if line.split_whitespace().count() < 2 {
        return Err(SshError::Local {
            what: "installing the key".to_owned(),
            message: "not a public key line".to_owned(),
        });
    }
    let target = connection.target()?;
    let mut channel = target.channel_open_session().await?;
    channel.exec(true, SCRIPT).await?;
    channel.data(format!("{line}\n").as_bytes()).await?;
    channel.eof().await?;
    let mut output = Vec::new();
    while let Some(message) = channel.wait().await {
        match message {
            ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                if output.len() < 64 * 1024 {
                    output.extend_from_slice(&data);
                }
            }
            ChannelMsg::Close => break,
            _ => {}
        }
    }
    let text = String::from_utf8_lossy(&output);
    if text.contains("__added__") {
        Ok(Installed::Added)
    } else if text.contains("__present__") {
        Ok(Installed::AlreadyThere)
    } else {
        Err(SshError::Refused {
            what: format!(
                "installing the key ({})",
                text.lines().last().unwrap_or("no answer").trim()
            ),
        })
    }
}
