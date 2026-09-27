//! A server's files over the SFTP subsystem, on a channel of an SSH connection. Every method takes
//! `&self`: one session serves a whole pane, and requests are pipelined by `russh-sftp`.

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use russh::ChannelMsg;
use russh_sftp::client::fs::{File, Metadata};
use russh_sftp::client::{Config, SftpSession};
use russh_sftp::protocol::{FileAttributes, OpenFlags};
use tokio::io::AsyncSeekExt;

use super::FsError;
use super::entry::{Entry, Kind};
use super::path::{self, Style};
use crate::SshError;
use crate::connect::Connection;

/// How long one SFTP request may take (a slow server listing a huge folder included).
const REQUEST_TIMEOUT_SECS: u64 = 60;

/// How many symbolic links of a listing are resolved at once.
const LINKS_AT_ONCE: usize = 16;

/// What a remote command printed, and how it ended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Output {
    /// Exit status (`None` when killed by a signal).
    pub status: Option<u32>,
    /// Standard output.
    pub stdout: Vec<u8>,
    /// Standard error.
    pub stderr: Vec<u8>,
}

/// Space on the server's file system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Space {
    /// Total bytes.
    pub total: u64,
    /// Bytes available to the user.
    pub available: u64,
}

/// A server's files.
pub struct Remote {
    session: SftpSession,
    connection: Arc<Connection>,
    home: String,
}

impl std::fmt::Debug for Remote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Remote")
            .field("home", &self.home)
            .finish_non_exhaustive()
    }
}

/// An [`Entry`] for `name` from SFTP attributes.
fn entry(name: &str, attrs: &FileAttributes) -> Entry {
    let mode = attrs.permissions.unwrap_or(0);
    let kind = if attrs.permissions.is_some() {
        Kind::from_mode(mode)
    } else {
        Kind::Other
    };
    Entry {
        name: name.to_owned(),
        kind,
        size: attrs.size.unwrap_or(0),
        modified: attrs.mtime.map(i64::from),
        mode: mode & 0o7777,
        uid: attrs.uid,
        gid: attrs.gid,
        link_target: None,
        target_kind: None,
    }
}

/// Seconds as SFTP version 3 carries them (until 2106).
fn secs(time: i64) -> u32 {
    u32::try_from(time.max(0)).unwrap_or(u32::MAX)
}

impl Remote {
    /// Opens an SFTP session on a new channel of `connection`.
    ///
    /// # Errors
    ///
    /// [`SshError::Refused`] when the server has no SFTP subsystem (it may still have SCP), and
    /// the connection's errors.
    pub async fn open(connection: Arc<Connection>) -> Result<Self, SshError> {
        let target = connection.target()?;
        let mut channel = target.channel_open_session().await?;
        channel.request_subsystem(true, "sftp").await?;
        // The answer to the request comes first: a refusal is quick, not a timeout later.
        let answer = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                match channel.wait().await {
                    Some(ChannelMsg::Success) => return true,
                    Some(ChannelMsg::Failure) | None => return false,
                    Some(ChannelMsg::Close | ChannelMsg::Eof) => return false,
                    Some(_) => {}
                }
            }
        })
        .await
        .map_err(|_| SshError::Timeout {
            what: "starting SFTP".to_owned(),
        })?;
        if !answer {
            return Err(SshError::Refused {
                what: "the SFTP subsystem".to_owned(),
            });
        }
        let config = Config {
            request_timeout_secs: REQUEST_TIMEOUT_SECS,
            ..Config::default()
        };
        let session = SftpSession::new_with_config(channel.into_stream(), config)
            .await
            .map_err(|error| SshError::Protocol(format!("SFTP: {error}")))?;
        let home = session
            .canonicalize(".")
            .await
            .unwrap_or_else(|_| "/".to_owned());
        Ok(Self {
            session,
            connection,
            home,
        })
    }

    /// The folder the session started in (the user's home).
    #[must_use]
    pub fn home(&self) -> &str {
        &self.home
    }

    /// The connection it runs on.
    #[must_use]
    pub fn connection(&self) -> &Arc<Connection> {
        &self.connection
    }

    /// The files of folder `dir` (without `.` and `..`), links resolved.
    ///
    /// # Errors
    ///
    /// [`FsError`] about `dir`.
    pub async fn list(&self, dir: &str) -> Result<Vec<Entry>, FsError> {
        let listing = self
            .session
            .read_dir(dir)
            .await
            .map_err(|error| FsError::sftp(dir, error))?;
        let mut entries: Vec<Entry> = listing
            .filter_map(|item| {
                let name = item.file_name();
                // `.`, `..`, and names with a `/` a server shouldn't send (path::is_plain_name).
                path::is_plain_name(Style::Posix, &name).then(|| entry(&name, &item.metadata()))
            })
            .collect();
        let links: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.kind == Kind::Symlink)
            .map(|(index, _)| index)
            .collect();
        let resolved: Vec<(usize, Option<String>, Option<Kind>)> =
            futures::stream::iter(links.into_iter().map(|index| {
                let full = path::join(Style::Posix, dir, &entries[index].name);
                async move {
                    let target = self.session.read_link(full.clone()).await.ok();
                    let kind = self
                        .session
                        .metadata(full)
                        .await
                        .ok()
                        .and_then(|attrs| attrs.permissions)
                        .map(Kind::from_mode);
                    (index, target, kind)
                }
            }))
            .buffer_unordered(LINKS_AT_ONCE)
            .collect()
            .await;
        for (index, target, kind) in resolved {
            if let Some(entry) = entries.get_mut(index) {
                entry.link_target = target;
                entry.target_kind = kind;
            }
        }
        Ok(entries)
    }

    /// `path`, following links.
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn stat(&self, path: &str) -> Result<Entry, FsError> {
        let attrs = self
            .session
            .metadata(path)
            .await
            .map_err(|error| FsError::sftp(path, error))?;
        Ok(entry(&path::file_name(Style::Posix, path), &attrs))
    }

    /// `path` itself (a link is a link).
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn lstat(&self, path: &str) -> Result<Entry, FsError> {
        let attrs = self
            .session
            .symlink_metadata(path)
            .await
            .map_err(|error| FsError::sftp(path, error))?;
        let mut entry = entry(&path::file_name(Style::Posix, path), &attrs);
        if entry.kind == Kind::Symlink {
            entry.link_target = self.session.read_link(path).await.ok();
            entry.target_kind = self
                .session
                .metadata(path)
                .await
                .ok()
                .and_then(|attrs| attrs.permissions)
                .map(Kind::from_mode);
        }
        Ok(entry)
    }

    /// Whether something is at `path` (a broken link counts).
    ///
    /// # Errors
    ///
    /// [`FsError`] other than "not found".
    pub async fn exists(&self, path: &str) -> Result<bool, FsError> {
        match self.session.symlink_metadata(path).await {
            Ok(_) => Ok(true),
            Err(error) => match FsError::sftp(path, error) {
                FsError::NotFound { .. } => Ok(false),
                other => Err(other),
            },
        }
    }

    /// A new folder.
    ///
    /// # Errors
    ///
    /// [`FsError::Exists`] when something is there.
    pub async fn mkdir(&self, path: &str) -> Result<(), FsError> {
        if self.exists(path).await? {
            return Err(FsError::Exists {
                path: path.to_owned(),
            });
        }
        self.session
            .create_dir(path)
            .await
            .map_err(|error| FsError::sftp(path, error))
    }

    /// A new empty file.
    ///
    /// # Errors
    ///
    /// [`FsError::Exists`] when something is there.
    pub async fn create_file(&self, path: &str) -> Result<(), FsError> {
        if self.exists(path).await? {
            return Err(FsError::Exists {
                path: path.to_owned(),
            });
        }
        let file = self
            .session
            .open_with_flags(
                path,
                OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE,
            )
            .await
            .map_err(|error| FsError::sftp(path, error))?;
        file.close()
            .await
            .map_err(|error| FsError::io(path, &error))
    }

    /// Renames or moves `from` to `to`, which must not exist.
    ///
    /// # Errors
    ///
    /// [`FsError::Exists`] when `to` exists.
    pub async fn rename(&self, from: &str, to: &str) -> Result<(), FsError> {
        if self.exists(to).await? {
            return Err(FsError::Exists {
                path: to.to_owned(),
            });
        }
        self.session
            .rename(from, to)
            .await
            .map_err(|error| FsError::sftp(from, error))
    }

    /// Deletes `path`; a folder with its contents when `recursive`.
    ///
    /// # Errors
    ///
    /// The first [`FsError`] met; what was deleted before stays deleted.
    pub async fn remove(&self, path: &str, recursive: bool) -> Result<(), FsError> {
        let entry = self.lstat(path).await?;
        if entry.kind != Kind::Dir {
            return self
                .session
                .remove_file(path)
                .await
                .map_err(|error| FsError::sftp(path, error));
        }
        if recursive {
            // Depth first, without recursion: folders are removed once emptied.
            let mut stack = vec![(path.to_owned(), false)];
            while let Some((dir, listed)) = stack.pop() {
                if listed {
                    self.session
                        .remove_dir(dir.as_str())
                        .await
                        .map_err(|error| FsError::sftp(&dir, error))?;
                    continue;
                }
                stack.push((dir.clone(), true));
                for child in self.list(&dir).await? {
                    let full = path::join(Style::Posix, &dir, &child.name);
                    if child.kind == Kind::Dir {
                        stack.push((full, false));
                    } else {
                        self.session
                            .remove_file(full.as_str())
                            .await
                            .map_err(|error| FsError::sftp(&full, error))?;
                    }
                }
            }
            return Ok(());
        }
        self.session
            .remove_dir(path)
            .await
            .map_err(|error| FsError::sftp(path, error))
    }

    /// Sets the permission bits of `path`.
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn chmod(&self, path: &str, mode: u32) -> Result<(), FsError> {
        let attrs = Metadata {
            permissions: Some(mode & 0o7777),
            ..Metadata::empty()
        };
        self.session
            .set_metadata(path, attrs)
            .await
            .map_err(|error| FsError::sftp(path, error))
    }

    /// Sets the access and modification times of `path` (seconds since the Unix epoch).
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn set_times(&self, path: &str, accessed: i64, modified: i64) -> Result<(), FsError> {
        let attrs = Metadata {
            atime: Some(secs(accessed)),
            mtime: Some(secs(modified)),
            ..Metadata::empty()
        };
        self.session
            .set_metadata(path, attrs)
            .await
            .map_err(|error| FsError::sftp(path, error))
    }

    /// A symbolic link at `link` pointing to `target`.
    ///
    /// # Errors
    ///
    /// [`FsError`] about `link`.
    pub async fn symlink(&self, link: &str, target: &str) -> Result<(), FsError> {
        if self.exists(link).await? {
            return Err(FsError::Exists {
                path: link.to_owned(),
            });
        }
        // OpenSSH's server takes the target first, against the draft; other servers followed it.
        self.session
            .symlink(target, link)
            .await
            .map_err(|error| FsError::sftp(link, error))
    }

    /// Where the link at `path` points.
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn read_link(&self, path: &str) -> Result<String, FsError> {
        self.session
            .read_link(path)
            .await
            .map_err(|error| FsError::sftp(path, error))
    }

    /// The absolute form of `path`.
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn canonicalize(&self, path: &str) -> Result<String, FsError> {
        let path = if path == "~" || path.is_empty() {
            self.home.clone()
        } else if let Some(rest) = path.strip_prefix("~/") {
            path::join(Style::Posix, &self.home, rest)
        } else {
            path.to_owned()
        };
        self.session
            .canonicalize(path.as_str())
            .await
            .map_err(|error| FsError::sftp(&path, error))
    }

    /// Space on the file system of `path`, when the server tells (`statvfs@openssh.com`).
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn space(&self, path: &str) -> Result<Option<Space>, FsError> {
        let info = self
            .session
            .fs_info(path)
            .await
            .map_err(|error| FsError::sftp(path, error))?;
        Ok(info.map(|info| Space {
            total: info.blocks.saturating_mul(info.fragment_size),
            available: info.blocks_avail.saturating_mul(info.fragment_size),
        }))
    }

    /// `path` opened for reading at `offset`.
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn open_read(&self, path: &str, offset: u64) -> Result<File, FsError> {
        let mut file = self
            .session
            .open_with_flags(path, OpenFlags::READ)
            .await
            .map_err(|error| FsError::sftp(path, error))?;
        if offset > 0 {
            file.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(|error| FsError::io(path, &error))?;
        }
        Ok(file)
    }

    /// `path` opened for writing: from the start, truncated (`offset` `None`), or at `offset` to
    /// resume.
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn open_write(&self, path: &str, offset: Option<u64>) -> Result<File, FsError> {
        let flags = match offset {
            None => OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
            Some(_) => OpenFlags::WRITE | OpenFlags::CREATE,
        };
        let mut file = self
            .session
            .open_with_flags(path, flags)
            .await
            .map_err(|error| FsError::sftp(path, error))?;
        if let Some(offset) = offset.filter(|offset| *offset > 0) {
            file.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(|error| FsError::io(path, &error))?;
        }
        Ok(file)
    }

    /// Runs `command` in the user's shell on the server, with `input` on its standard input.
    /// Output is capped at 1 MiB per stream.
    ///
    /// # Errors
    ///
    /// The connection's errors.
    pub async fn run(&self, command: &str, input: &[u8]) -> Result<Output, SshError> {
        const CAP: usize = 1 << 20;
        let target = self.connection.target()?;
        let mut channel = target.channel_open_session().await?;
        channel.exec(true, command).await?;
        if !input.is_empty() {
            channel.data(input).await?;
        }
        channel.eof().await?;
        let mut output = Output::default();
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { data } if output.stdout.len() < CAP => {
                    output.stdout.extend_from_slice(&data);
                }
                ChannelMsg::ExtendedData { data, .. } if output.stderr.len() < CAP => {
                    output.stderr.extend_from_slice(&data);
                }
                ChannelMsg::ExitStatus { exit_status } => output.status = Some(exit_status),
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        Ok(output)
    }

    /// Copies `from` to `to` on the server with `cp -R -p`, when it has a POSIX shell.
    ///
    /// # Errors
    ///
    /// [`FsError::Unsupported`] when there is no `cp` (the caller copies through the client), and
    /// [`FsError::Failed`] with what `cp` said.
    pub async fn copy_within(&self, from: &str, to: &str) -> Result<(), FsError> {
        if self.exists(to).await? {
            return Err(FsError::Exists {
                path: to.to_owned(),
            });
        }
        let command = format!(
            "cp -R -p -- {} {}",
            path::shell_quote(from),
            path::shell_quote(to)
        );
        let output = self.run(&command, &[]).await.map_err(|error| match error {
            SshError::Refused { .. } => FsError::Unsupported {
                what: "commands".to_owned(),
            },
            _ => FsError::Disconnected,
        })?;
        match output.status {
            Some(0) => Ok(()),
            Some(127) => Err(FsError::Unsupported {
                what: "cp".to_owned(),
            }),
            _ => Err(FsError::Failed {
                path: from.to_owned(),
                message: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            }),
        }
    }

    /// Ends the SFTP session (the connection stays).
    pub async fn close(&self) {
        let _ = self.session.close().await;
    }
}
