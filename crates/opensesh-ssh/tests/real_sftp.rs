//! SFTP against a real server (PLAN Sprint 8): a 1 GiB file up and back down with the same
//! SHA-256 here, on the server and back; an upload cut off by a lost connection and resumed on a
//! new one; and a server without the SFTP subsystem told apart from other failures.
//!
//! The servers come from `scripts/ssh-test-servers.sh start` (OpenSSH on 127.0.0.1:2221, and
//! 2225 without SFTP), so these tests are ignored by default:
//!
//! ```sh
//! OPENSESH_SSH_SERVERS=/tmp/opensesh-ssh-servers cargo test -p opensesh-ssh --test real_sftp -- --ignored
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers"
)]

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use opensesh_ssh::SshError;
use opensesh_ssh::connect::{self, Connection};
use opensesh_ssh::prompt::{Answer, Asker, Request as Question};
use opensesh_ssh::sftp::Fs;
use opensesh_ssh::sftp::path::{self, Style};
use opensesh_ssh::sftp::remote::Remote;
use opensesh_ssh::sftp::transfer::{
    Event, EventSink, Options, Policy, Progress, Queue, Request, State,
};
use opensesh_ssh::spec::{AuthPlan, ConnectSpec, Hop, KnownHostsFiles};
use russh::keys::ssh_key::sha2::{Digest as _, Sha256};

/// What `scripts/ssh-test-servers.sh` sets up.
const USER: &str = "opensesh-test";
const OPENSSH: u16 = 2221;
const OPENSSH_WITHOUT_SFTP: u16 = 2225;
const GIB: u64 = 1 << 30;
const MIB: f64 = 1_048_576.0;

fn state() -> PathBuf {
    std::env::var_os("OPENSESH_SSH_SERVERS")
        .map_or_else(|| PathBuf::from("/tmp/opensesh-ssh-servers"), PathBuf::from)
}

/// A connection to `port` with the servers' client key (no compression: the data is random).
async fn connect(port: u16, known: &Path) -> Arc<Connection> {
    let spec = ConnectSpec {
        hops: vec![Hop {
            host: "127.0.0.1".into(),
            port,
            user: USER.into(),
            auth: AuthPlan {
                key_files: vec![state().join("client_ed25519")],
                ..AuthPlan::default()
            },
        }],
        proxy: None,
        legacy: false,
        compression: false,
        keepalive: Some(Duration::from_secs(15)),
        connect_timeout: Duration::from_secs(10),
        known_hosts: KnownHostsFiles {
            own: known.join("known_hosts"),
            ..KnownHostsFiles::default()
        },
        agent_forwarding: false,
        agent_socket: None,
    };
    let trust: Asker = Arc::new(|question: Question| question.answer(Answer::TrustOnce));
    Arc::new(
        connect::connect(&spec, &trust, &connect::quiet())
            .await
            .unwrap(),
    )
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Writes `len` pseudo-random bytes (xorshift) to `path` and returns their SHA-256.
fn write_random(path: &Path, len: u64) -> String {
    let mut file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut hash = Sha256::new();
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut chunk = vec![0_u8; 1 << 20];
    let mut left = len;
    while left > 0 {
        for word in chunk.chunks_exact_mut(8) {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            word.copy_from_slice(&state.to_le_bytes());
        }
        let take = usize::try_from(left.min(chunk.len() as u64)).unwrap();
        file.write_all(&chunk[..take]).unwrap();
        hash.update(&chunk[..take]);
        left -= take as u64;
    }
    file.flush().unwrap();
    hex(&hash.finalize())
}

fn sha256_of(path: &Path) -> String {
    let mut file = std::fs::File::open(path).unwrap();
    let mut hash = Sha256::new();
    let mut chunk = vec![0_u8; 1 << 20];
    loop {
        let read = file.read(&mut chunk).unwrap();
        if read == 0 {
            return hex(&hash.finalize());
        }
        hash.update(&chunk[..read]);
    }
}

/// The server's own `sha256sum` of `path`.
async fn remote_sha256(remote: &Remote, path: &str) -> String {
    let output = remote
        .run(&format!("sha256sum {}", path::shell_quote(path)), &[])
        .await
        .unwrap();
    assert_eq!(
        output.status,
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned()
}

/// The events of a queue, and a way to wait for one.
#[derive(Clone, Default)]
struct Events(Arc<Mutex<Vec<Event>>>);

impl Events {
    fn sink(&self) -> EventSink {
        let events = Arc::clone(&self.0);
        Arc::new(move |event| events.lock().unwrap().push(event))
    }

    fn progress(&self, id: u64) -> Vec<Progress> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|event| match event {
                Event::Progress(progress) if progress.id == id => Some(progress.clone()),
                _ => None,
            })
            .collect()
    }

    async fn until(
        &self,
        what: &str,
        within: Duration,
        done: impl Fn(&[Progress]) -> bool,
        id: u64,
    ) {
        let deadline = Instant::now() + within;
        while Instant::now() < deadline {
            if done(&self.progress(id)) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!(
            "timed out waiting for {what}: {:?}",
            self.progress(id).last()
        );
    }

    async fn finished(&self, id: u64, within: Duration) -> Progress {
        self.until(
            "the job to finish",
            within,
            |list| list.iter().any(|progress| progress.state.is_finished()),
            id,
        )
        .await;
        self.progress(id).pop().unwrap()
    }
}

fn request(from: Fs, source: &Path, to: Fs, destination: &str, policy: Policy) -> Request {
    Request {
        from,
        sources: vec![source.display().to_string()],
        to,
        destination: destination.to_owned(),
        options: Options {
            policy,
            preserve_times: true,
            preserve_permissions: false,
        },
        remove_sources: false,
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs scripts/ssh-test-servers.sh start"]
async fn a_gigabyte_up_and_down_with_the_same_checksum() {
    let known = tempfile::tempdir().unwrap();
    let here = tempfile::tempdir().unwrap();
    let back = tempfile::tempdir().unwrap();
    let name = format!("opensesh-gib-{}.bin", std::process::id());
    let source = here.path().join(&name);
    let expected = write_random(&source, GIB);

    let connection = connect(OPENSSH, known.path()).await;
    let remote = Arc::new(Remote::open(Arc::clone(&connection)).await.unwrap());
    let home = remote.home().to_owned();
    let target = path::join(Style::Posix, &home, &name);
    let fs = Fs::Remote(Arc::clone(&remote));
    let events = Events::default();
    let queue = Queue::new(tokio::runtime::Handle::current(), 3, events.sink());

    let started = Instant::now();
    let up = queue.add(request(
        Fs::local(),
        &source,
        fs.clone(),
        &home,
        Policy::Overwrite,
    ));
    let done = events.finished(up, Duration::from_secs(900)).await;
    let up_secs = started.elapsed().as_secs_f64();
    assert_eq!(done.state, State::Done, "{done:?}");
    assert_eq!(done.bytes_done, GIB);
    assert_eq!(remote_sha256(&remote, &target).await, expected);

    let started = Instant::now();
    let down = queue.add(Request {
        sources: vec![target.clone()],
        ..request(
            fs.clone(),
            &source,
            Fs::local(),
            &back.path().display().to_string(),
            Policy::Overwrite,
        )
    });
    let done = events.finished(down, Duration::from_secs(900)).await;
    let down_secs = started.elapsed().as_secs_f64();
    assert_eq!(done.state, State::Done, "{done:?}");
    assert_eq!(sha256_of(&back.path().join(&name)), expected);

    #[allow(clippy::cast_precision_loss, reason = "a speed to read")]
    let gib = GIB as f64;
    eprintln!(
        "1 GiB: up in {up_secs:.1} s ({:.0} MiB/s), down in {down_secs:.1} s ({:.0} MiB/s)",
        gib / MIB / up_secs,
        gib / MIB / down_secs
    );
    fs.remove(&target, false).await.unwrap();
    connection.close().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs scripts/ssh-test-servers.sh start"]
async fn an_upload_cut_off_resumes_on_a_new_connection() {
    const SIZE: u64 = 256 << 20;
    let known = tempfile::tempdir().unwrap();
    let here = tempfile::tempdir().unwrap();
    let name = format!("opensesh-cut-{}.bin", std::process::id());
    let source = here.path().join(&name);
    let expected = write_random(&source, SIZE);
    let events = Events::default();
    let queue = Queue::new(tokio::runtime::Handle::current(), 3, events.sink());

    // The connection goes away a quarter of the way in: the job fails, the server keeps the
    // part that arrived.
    let first = connect(OPENSSH, known.path()).await;
    let remote = Arc::new(Remote::open(Arc::clone(&first)).await.unwrap());
    let home = remote.home().to_owned();
    let target = path::join(Style::Posix, &home, &name);
    let id = queue.add(request(
        Fs::local(),
        &source,
        Fs::Remote(remote),
        &home,
        Policy::Overwrite,
    ));
    events
        .until(
            "a quarter of the upload",
            Duration::from_secs(300),
            |list| list.iter().any(|progress| progress.bytes_done >= SIZE / 4),
            id,
        )
        .await;
    first.close().await;
    let failed = events.finished(id, Duration::from_secs(120)).await;
    assert!(matches!(failed.state, State::Failed { .. }), "{failed:?}");

    let second = connect(OPENSSH, known.path()).await;
    let remote = Arc::new(Remote::open(Arc::clone(&second)).await.unwrap());
    let partial = remote.stat(&target).await.unwrap().size;
    assert!(
        partial > 0 && partial < SIZE,
        "{partial} of {SIZE} bytes arrived"
    );

    // Resumed: the first progress already counts the part that was there.
    let started = Instant::now();
    let id = queue.add(request(
        Fs::local(),
        &source,
        Fs::Remote(Arc::clone(&remote)),
        &home,
        Policy::Resume,
    ));
    let done = events.finished(id, Duration::from_secs(300)).await;
    assert_eq!(done.state, State::Done, "{done:?}");
    let first_counted = events
        .progress(id)
        .iter()
        .map(|progress| progress.bytes_done)
        .find(|bytes| *bytes > 0)
        .unwrap();
    assert!(
        first_counted >= partial,
        "resumed at {first_counted}, but {partial} bytes were there"
    );
    assert_eq!(remote_sha256(&remote, &target).await, expected);
    eprintln!(
        "cut off after {partial} of {SIZE} bytes; the rest went in {:.1} s",
        started.elapsed().as_secs_f64()
    );
    Fs::Remote(Arc::clone(&remote))
        .remove(&target, false)
        .await
        .unwrap();
    second.close().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs scripts/ssh-test-servers.sh start"]
async fn a_copy_within_the_server_uses_cp() {
    let known = tempfile::tempdir().unwrap();
    let here = tempfile::tempdir().unwrap();
    let name = format!("opensesh-copy-{}", std::process::id());
    let tree = here.path().join(&name);
    std::fs::create_dir_all(tree.join("sub")).unwrap();
    let small = write_random(&tree.join("small.bin"), 70_000);
    let deep = write_random(&tree.join("sub").join("deep.bin"), 3 << 20);

    let connection = connect(OPENSSH, known.path()).await;
    let remote = Arc::new(Remote::open(Arc::clone(&connection)).await.unwrap());
    let home = remote.home().to_owned();
    let fs = Fs::Remote(Arc::clone(&remote));
    let events = Events::default();
    let queue = Queue::new(tokio::runtime::Handle::current(), 3, events.sink());
    let up = queue.add(request(
        Fs::local(),
        &tree,
        fs.clone(),
        &home,
        Policy::Overwrite,
    ));
    assert_eq!(
        events.finished(up, Duration::from_secs(120)).await.state,
        State::Done
    );

    // Into a folder next to it: `cp -R -p` on the server, times kept.
    let source = path::join(Style::Posix, &home, &name);
    let folder = format!("{source}-copies");
    fs.mkdir(&folder).await.unwrap();
    let within = queue.add(Request {
        sources: vec![source.clone()],
        ..request(fs.clone(), &tree, fs.clone(), &folder, Policy::Ask)
    });
    let done = events.finished(within, Duration::from_secs(120)).await;
    assert_eq!(done.state, State::Done, "{done:?}");
    assert_eq!(done.files_done, 2);
    let copy = path::join(Style::Posix, &folder, &name);
    assert_eq!(
        remote_sha256(&remote, &path::join(Style::Posix, &copy, "small.bin")).await,
        small
    );
    assert_eq!(
        remote_sha256(&remote, &format!("{copy}/sub/deep.bin")).await,
        deep
    );
    assert_eq!(
        fs.stat(&format!("{copy}/sub/deep.bin"))
            .await
            .unwrap()
            .modified,
        fs.stat(&format!("{source}/sub/deep.bin"))
            .await
            .unwrap()
            .modified
    );

    fs.remove(&source, true).await.unwrap();
    fs.remove(&folder, true).await.unwrap();
    connection.close().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs scripts/ssh-test-servers.sh start"]
async fn a_server_without_sftp_says_so() {
    let known = tempfile::tempdir().unwrap();
    let connection = connect(OPENSSH_WITHOUT_SFTP, known.path()).await;
    let error = Remote::open(Arc::clone(&connection)).await.unwrap_err();
    assert!(matches!(error, SshError::Refused { .. }), "{error}");
    connection.close().await;
}
