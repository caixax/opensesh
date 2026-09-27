//! SCP fallback spike (Sprint 8): copies files with the SCP protocol over an exec channel, for
//! servers that run `scp` but have no SFTP subsystem (OpenSSH without `Subsystem sftp`, Dropbear
//! without an sftp-server, small devices).
//!
//! The protocol is the old rcp one, as OpenSSH's `scp.c` speaks it. The remote side runs
//! `scp -f <path>` to send ("from") or `scp -t <folder>` to receive ("to"); `-r` copies folders,
//! `-p` keeps times and modes. Each record is a text line, and the other side answers each one
//! with a byte: 0 (fine), 1 (a warning line follows) or 2 (a fatal line follows):
//!
//! - `T<mtime> 0 <atime> 0`: the times of the next file or folder
//! - `C<mode> <size> <name>`: a file; then `<size>` bytes and a 0 byte
//! - `D<mode> 0 <name>`: a folder begins; `E`: it ends
//!
//! Usage (see README.md):
//!
//! ```sh
//! SPIKE_KEY=~/.ssh/id_ed25519 cargo run --release -- user@host:port probe
//! SPIKE_KEY=... cargo run --release -- user@host:port download <remote path> <local folder>
//! SPIKE_PASSWORD=... cargo run --release -- user@host:port upload <local path> <remote folder>
//! ```
//!
//! A spike: it trusts any host key (it prints the fingerprint) and unwraps freely.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use russh::client::{self, Handle, Handler, Msg};
use russh::keys::{PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{Channel, ChannelMsg};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Client;

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
}

/// `text` as one word for the remote shell (single quotes).
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// A name the server sent for a file or folder: one plain name, never a path. A server that
/// sends `../x` or `/etc/x` (CVE-2019-6111) or `.` (CVE-2018-20685) is refused.
fn safe_name(name: &str) -> bool {
    !(name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || (cfg!(windows) && (name.contains('\\') || name.contains(':'))))
}

/// `0644 1234 name with spaces` (a `C` or `D` record after its letter) -> mode, size, name.
fn parse_record(line: &str) -> Option<(u32, u64, String)> {
    let mut parts = line.splitn(3, ' ');
    let mode = u32::from_str_radix(parts.next()?, 8).ok()?;
    let size = parts.next()?.parse().ok()?;
    let name = parts.next()?.to_owned();
    Some((mode, size, name))
}

/// `1700000000 0 1700000001 0` (a `T` record after its letter) -> modified, accessed.
fn parse_times(line: &str) -> Option<(u64, u64)> {
    let parts: Vec<&str> = line.split(' ').collect();
    match parts.as_slice() {
        [modified, _, accessed, _] => Some((modified.parse().ok()?, accessed.parse().ok()?)),
        _ => None,
    }
}

fn unix_seconds(time: std::io::Result<SystemTime>) -> u64 {
    time.ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |elapsed| elapsed.as_secs())
}

#[cfg(unix)]
fn mode_of(meta: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o7777
}

#[cfg(not(unix))]
fn mode_of(meta: &std::fs::Metadata) -> u32 {
    match (meta.is_dir(), meta.permissions().readonly()) {
        (true, _) => 0o755,
        (false, true) => 0o444,
        (false, false) => 0o644,
    }
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode & 0o7777));
}

#[cfg(not(unix))]
fn set_mode(path: &Path, mode: u32) {
    if let Ok(meta) = std::fs::metadata(path) {
        let mut permissions = meta.permissions();
        permissions.set_readonly(mode & 0o200 == 0);
        let _ = std::fs::set_permissions(path, permissions);
    }
}

fn set_times(path: &Path, times: Option<(u64, u64)>) {
    let Some((modified, accessed)) = times else {
        return;
    };
    let at = |seconds| UNIX_EPOCH + Duration::from_secs(seconds);
    let times = std::fs::FileTimes::new()
        .set_modified(at(modified))
        .set_accessed(at(accessed));
    // Folders open for writing times only on some systems; a spike doesn't mind.
    if let Ok(file) = std::fs::File::options().write(true).open(path) {
        let _ = file.set_times(times);
    }
}

/// The exec channel that runs `scp` on the server: what it sent that isn't read yet, what it
/// wrote to stderr, and its exit status.
struct Scp {
    channel: Channel<Msg>,
    buffer: Vec<u8>,
    at: usize,
    stderr: Vec<u8>,
    exit: Option<u32>,
    ended: bool,
}

impl Scp {
    async fn start(handle: &Handle<Client>, command: &str) -> Self {
        eprintln!("exec: {command}");
        let channel = handle.channel_open_session().await.unwrap();
        channel.exec(true, command.as_bytes()).await.unwrap();
        Self {
            channel,
            buffer: Vec::new(),
            at: 0,
            stderr: Vec::new(),
            exit: None,
            ended: false,
        }
    }

    fn pending(&self) -> usize {
        self.buffer.len() - self.at
    }

    /// Waits for more of what scp sends; false when it ended.
    async fn fill(&mut self) -> bool {
        while !self.ended {
            match self.channel.wait().await {
                Some(ChannelMsg::Data { data }) => {
                    if self.at == self.buffer.len() {
                        self.buffer.clear();
                        self.at = 0;
                    }
                    self.buffer.extend_from_slice(&data);
                    return true;
                }
                Some(ChannelMsg::ExtendedData { data, .. }) => self.stderr.extend_from_slice(&data),
                Some(ChannelMsg::ExitStatus { exit_status }) => self.exit = Some(exit_status),
                Some(ChannelMsg::Failure) => {
                    eprintln!("the server refused to run the command");
                    self.ended = true;
                }
                Some(ChannelMsg::Eof | ChannelMsg::Close) | None => self.ended = true,
                Some(_) => {}
            }
        }
        false
    }

    async fn byte(&mut self) -> Option<u8> {
        while self.pending() == 0 {
            if !self.fill().await {
                return None;
            }
        }
        let byte = self.buffer[self.at];
        self.at += 1;
        Some(byte)
    }

    /// The rest of a line, without its newline.
    async fn line(&mut self) -> String {
        let mut line = Vec::new();
        while let Some(byte) = self.byte().await {
            if byte == b'\n' {
                break;
            }
            line.push(byte);
        }
        String::from_utf8_lossy(&line).into_owned()
    }

    /// The other side's answer to a record: fine, or its message.
    async fn ack(&mut self) -> Result<(), String> {
        match self.byte().await {
            Some(0) => Ok(()),
            Some(1 | 2) => Err(self.line().await),
            Some(other) => Err(format!(
                "unexpected answer {other:#04x}{}",
                self.line().await
            )),
            None => Err(format!(
                "scp ended ({})",
                String::from_utf8_lossy(&self.stderr).trim()
            )),
        }
    }

    async fn send(&self, bytes: &[u8]) {
        self.channel.data_bytes(bytes.to_vec()).await.unwrap();
    }

    /// Writes the next `size` bytes scp sends to `file`; false if it ended before.
    async fn copy_to(&mut self, file: &mut tokio::fs::File, mut size: u64) -> bool {
        while size > 0 {
            if self.pending() == 0 && !self.fill().await {
                return false;
            }
            let take = usize::try_from(size)
                .unwrap_or(usize::MAX)
                .min(self.pending());
            file.write_all(&self.buffer[self.at..self.at + take])
                .await
                .unwrap();
            self.at += take;
            size -= take as u64;
        }
        true
    }

    /// Says there's nothing more to send, and waits for scp to exit.
    async fn finish(mut self) -> (Option<u32>, String) {
        let _ = self.channel.eof().await;
        while let Some(message) = self.channel.wait().await {
            match message {
                ChannelMsg::ExtendedData { data, .. } => self.stderr.extend_from_slice(&data),
                ChannelMsg::ExitStatus { exit_status } => self.exit = Some(exit_status),
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        (
            self.exit,
            String::from_utf8_lossy(&self.stderr).trim().to_owned(),
        )
    }
}

#[derive(Debug, Default)]
struct Stats {
    files: u64,
    folders: u64,
    bytes: u64,
}

/// Receives what `scp -r -p -f <remote>` sends into `folder` (we are the sink).
async fn download(handle: &Handle<Client>, remote: &str, folder: &Path) -> Result<Stats, String> {
    let mut scp = Scp::start(handle, &format!("scp -r -p -f -- {}", quote(remote))).await;
    let mut stats = Stats::default();
    // The folders being received, with the times their `T` record gave.
    let mut folders: Vec<(PathBuf, Option<(u64, u64)>)> = vec![(folder.to_path_buf(), None)];
    let mut times = None;
    let mut outcome = Ok(());
    scp.send(&[0]).await;
    while let Some(kind) = scp.byte().await {
        let line = scp.line().await;
        let here = folders.last().map(|(path, _)| path.clone()).unwrap();
        match kind {
            // A file the source couldn't send: it goes on with the others.
            1 => eprintln!("scp: {line}"),
            2 => {
                outcome = Err(line);
                break;
            }
            b'T' => {
                times = parse_times(&line);
                scp.send(&[0]).await;
            }
            b'C' | b'D' => {
                let Some((mode, size, name)) = parse_record(&line) else {
                    outcome = Err(format!("a bad record: {line:?}"));
                    break;
                };
                if !safe_name(&name) {
                    scp.send(b"\x02refused a name that isn't a plain name\n")
                        .await;
                    outcome = Err(format!("the server sent the name {name:?}"));
                    break;
                }
                let path = here.join(&name);
                if kind == b'D' {
                    tokio::fs::create_dir_all(&path).await.unwrap();
                    set_mode(&path, mode);
                    folders.push((path, times.take()));
                    stats.folders += 1;
                    scp.send(&[0]).await;
                    continue;
                }
                let mut file = tokio::fs::File::create(&path).await.unwrap();
                scp.send(&[0]).await;
                if !scp.copy_to(&mut file, size).await {
                    outcome = Err(format!("the connection ended inside {name}"));
                    break;
                }
                file.flush().await.unwrap();
                drop(file);
                if let Err(message) = scp.ack().await {
                    outcome = Err(format!("{name}: {message}"));
                    break;
                }
                scp.send(&[0]).await;
                set_times(&path, times.take());
                set_mode(&path, mode);
                stats.files += 1;
                stats.bytes += size;
            }
            b'E' => {
                if let Some((path, folder_times)) = folders.pop() {
                    set_times(&path, folder_times);
                }
                scp.send(&[0]).await;
            }
            other => {
                outcome = Err(format!("an unknown record {other:#04x}{line}"));
                break;
            }
        }
    }
    let (status, stderr) = scp.finish().await;
    eprintln!("scp exited with {status:?} {stderr}");
    outcome.map(|()| stats)
}

/// Sends `path` (a file, or a folder and everything in it) to the sink.
async fn send_path(scp: &mut Scp, path: &Path, stats: &mut Stats) -> Result<(), String> {
    let meta = tokio::fs::metadata(path)
        .await
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    // The protocol ends a record with a newline: such a name can't be sent.
    if name.contains('\n') {
        eprintln!("skipped {name:?}: SCP can't carry a newline in a name");
        return Ok(());
    }
    let modified = unix_seconds(meta.modified());
    let accessed = unix_seconds(meta.accessed());
    scp.send(format!("T{modified} 0 {accessed} 0\n").as_bytes())
        .await;
    scp.ack().await?;
    if meta.is_dir() {
        scp.send(format!("D{:04o} 0 {name}\n", mode_of(&meta)).as_bytes())
            .await;
        scp.ack().await?;
        let mut entries = Vec::new();
        let mut listing = tokio::fs::read_dir(path).await.unwrap();
        while let Some(entry) = listing.next_entry().await.unwrap() {
            entries.push(entry.path());
        }
        entries.sort();
        for entry in entries {
            Box::pin(send_path(scp, &entry, stats)).await?;
        }
        scp.send(b"E\n").await;
        scp.ack().await?;
        stats.folders += 1;
    } else {
        let size = meta.len();
        scp.send(format!("C{:04o} {size} {name}\n", mode_of(&meta)).as_bytes())
            .await;
        scp.ack().await?;
        let file = tokio::fs::File::open(path).await.unwrap();
        scp.channel.data(file.take(size)).await.unwrap();
        scp.send(&[0]).await;
        scp.ack().await?;
        stats.files += 1;
        stats.bytes += size;
    }
    Ok(())
}

/// Sends `local` into `remote_folder` through `scp -r -p -t` (we are the source).
async fn upload(
    handle: &Handle<Client>,
    local: &Path,
    remote_folder: &str,
) -> Result<Stats, String> {
    let mut scp = Scp::start(handle, &format!("scp -r -p -t -- {}", quote(remote_folder))).await;
    let mut stats = Stats::default();
    // The sink says it is ready.
    let mut outcome = scp.ack().await;
    if outcome.is_ok() {
        outcome = send_path(&mut scp, local, &mut stats).await;
    }
    let (status, stderr) = scp.finish().await;
    eprintln!("scp exited with {status:?} {stderr}");
    outcome.map(|()| stats)
}

/// Is there an SFTP subsystem, and an `scp` program?
async fn probe(handle: &Handle<Client>) {
    let mut channel = handle.channel_open_session().await.unwrap();
    channel.request_subsystem(true, "sftp").await.unwrap();
    let sftp = loop {
        match channel.wait().await {
            Some(ChannelMsg::Success) => break true,
            Some(ChannelMsg::Failure | ChannelMsg::Close) | None => break false,
            Some(_) => {}
        }
    };
    eprintln!("sftp subsystem: {}", if sftp { "yes" } else { "no" });
    let _ = channel.close().await;
    let mut scp = Scp::start(handle, "command -v scp; scp 2>&1 | head -n 3").await;
    let mut output = String::new();
    while let Some(byte) = scp.byte().await {
        output.push(char::from(byte));
    }
    let (status, stderr) = scp.finish().await;
    eprintln!("scp: {} (exit {status:?}) {stderr}", output.trim());
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
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "user@host[:port] probe | download <remote> <local folder> | upload <local> <remote folder>";
    let target = args.first().expect(usage);
    let (user, address) = target.split_once('@').expect(usage);
    let (host, port) = match address.rsplit_once(':') {
        Some((host, port)) => (host.to_owned(), port.parse::<u16>().unwrap()),
        None => (address.to_owned(), 22),
    };
    let config = Arc::new(client::Config::default());
    let mut handle = client::connect(config, (host.as_str(), port), Client)
        .await
        .unwrap();
    authenticate(&mut handle, user).await;

    let started = Instant::now();
    let result = match args.get(1).map(String::as_str) {
        Some("probe") => {
            probe(&handle).await;
            Ok(Stats::default())
        }
        Some("download") => {
            let (remote, local) = (args.get(2).expect(usage), args.get(3).expect(usage));
            download(&handle, remote, Path::new(local)).await
        }
        Some("upload") => {
            let (local, remote) = (args.get(2).expect(usage), args.get(3).expect(usage));
            upload(&handle, Path::new(local), remote).await
        }
        _ => panic!("{usage}"),
    };
    let elapsed = started.elapsed().as_secs_f64();
    match result {
        Ok(stats) => {
            #[allow(clippy::cast_precision_loss)]
            let speed = stats.bytes as f64 / elapsed.max(0.001) / 1_048_576.0;
            eprintln!(
                "{} files, {} folders, {} bytes in {elapsed:.2} s ({speed:.1} MiB/s)",
                stats.files, stats.folders, stats.bytes
            );
        }
        Err(message) => eprintln!("failed: {message}"),
    }
    handle
        .disconnect(russh::Disconnect::ByApplication, "", "")
        .await
        .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records() {
        assert_eq!(
            parse_record("0644 1234 a name with spaces"),
            Some((0o644, 1234, "a name with spaces".to_owned()))
        );
        assert_eq!(
            parse_record("0755 0 dir"),
            Some((0o755, 0, "dir".to_owned()))
        );
        assert_eq!(parse_record("rw 1 x"), None);
        assert_eq!(
            parse_times("1700000000 0 1700000001 0"),
            Some((1_700_000_000, 1_700_000_001))
        );
        assert_eq!(parse_times("1 0"), None);
    }

    #[test]
    fn names() {
        assert!(safe_name("report.txt") && safe_name(".profile") && safe_name("a b"));
        for bad in ["", ".", "..", "../x", "/etc/passwd", "a/b"] {
            assert!(!safe_name(bad), "{bad}");
        }
        assert_eq!(quote("it's"), r"'it'\''s'");
    }
}
