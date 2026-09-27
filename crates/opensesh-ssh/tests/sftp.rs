//! SFTP against the in-process server (PLAN Sprint 8): every file operation, transfers both
//! ways with times and permissions, pause and resume, a partial file continued, the overwrite
//! questions, and a folder of 10,000 files.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers"
)]

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use opensesh_ssh::connect::{self, Connection};
use opensesh_ssh::prompt::{Answer, Asker, Request as Question};
use opensesh_ssh::sftp::entry::{SortKey, sort};
use opensesh_ssh::sftp::remote::Remote;
use opensesh_ssh::sftp::transfer::{
    Choice, Event, Options, Policy, Progress, Queue, Request, State,
};
use opensesh_ssh::sftp::{Fs, Kind};
use opensesh_ssh::spec::{AuthPlan, ConnectSpec, Hop, KnownHostsFiles};
use opensesh_ssh::testing::{PASSWORD, Rules, USER, serve};
use secrecy::SecretString;

/// A server over `root`, and a connection to it.
async fn remote(root: &Path, known: &Path) -> Arc<Remote> {
    let port = serve(Rules {
        password: true,
        sftp_root: Some(root.to_path_buf()),
        ..Rules::default()
    })
    .await
    .unwrap();
    let spec = ConnectSpec {
        hops: vec![Hop {
            host: "127.0.0.1".into(),
            port,
            user: USER.into(),
            auth: AuthPlan {
                password: Some(SecretString::from(PASSWORD)),
                ..AuthPlan::default()
            },
        }],
        proxy: None,
        legacy: false,
        compression: false,
        keepalive: None,
        connect_timeout: Duration::from_secs(10),
        known_hosts: KnownHostsFiles {
            own: known.join("known_hosts"),
            ..KnownHostsFiles::default()
        },
        agent_forwarding: false,
        agent_socket: None,
    };
    let trust: Asker = Arc::new(|question: Question| question.answer(Answer::TrustOnce));
    let connection: Connection = connect::connect(&spec, &trust, &connect::quiet())
        .await
        .unwrap();
    Arc::new(Remote::open(Arc::new(connection)).await.unwrap())
}

/// Pseudo-random bytes (a fixed sequence), so contents differ from file to file.
fn bytes(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
    (0..len)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 33) as u8
        })
        .collect()
}

/// The events of a queue, and a way to wait for one.
#[derive(Clone, Default)]
struct Events(Arc<Mutex<Vec<Event>>>);

impl Events {
    fn sink(&self) -> opensesh_ssh::sftp::transfer::EventSink {
        let events = Arc::clone(&self.0);
        Arc::new(move |event| events.lock().unwrap().push(event))
    }

    fn last(&self, id: u64) -> Option<Progress> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find_map(|event| match event {
                Event::Progress(progress) if progress.id == id => Some(progress.clone()),
                _ => None,
            })
    }

    async fn until(&self, what: &str, done: impl Fn(&[Event]) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(60);
        while Instant::now() < deadline {
            // A copy: `done` may look at the events again.
            let events = self.0.lock().unwrap().clone();
            if done(&events) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!(
            "timed out waiting for {what}: {:?}",
            self.0.lock().unwrap().last()
        );
    }

    async fn finished(&self, id: u64) -> Progress {
        self.until("the job to finish", |events| {
            events.iter().any(|event| {
                matches!(event, Event::Progress(progress) if progress.id == id && progress.state.is_finished())
            })
        })
        .await;
        self.last(id).unwrap()
    }
}

fn options(policy: Policy) -> Options {
    Options {
        policy,
        preserve_times: true,
        preserve_permissions: true,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn file_operations() {
    let root = tempfile::tempdir().unwrap();
    let known = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("hello.txt"), b"hello").unwrap();
    let remote = remote(root.path(), known.path()).await;
    let fs = Fs::Remote(Arc::clone(&remote));
    assert_eq!(remote.home(), "/");
    assert_eq!(
        fs.canonicalize("~/a/../hello.txt").await.unwrap(),
        "/hello.txt"
    );

    fs.mkdir("/docs").await.unwrap();
    assert_eq!(fs.mkdir("/docs").await.unwrap_err().code(), "exists");
    fs.create_file("/docs/empty.txt").await.unwrap();
    assert_eq!(
        fs.create_file("/docs/empty.txt").await.unwrap_err().code(),
        "exists"
    );
    fs.rename("/hello.txt", "/docs/hi.txt").await.unwrap();
    assert_eq!(
        fs.rename("/docs/hi.txt", "/docs/empty.txt")
            .await
            .unwrap_err()
            .code(),
        "exists"
    );
    fs.set_times("/docs/hi.txt", 1_500_000_000, 1_600_000_000)
        .await
        .unwrap();
    let hi = fs.stat("/docs/hi.txt").await.unwrap();
    assert_eq!(
        (hi.kind, hi.size, hi.modified),
        (Kind::File, 5, Some(1_600_000_000))
    );
    fs.chmod("/docs/hi.txt", 0o444).await.unwrap();
    assert_eq!(fs.stat("/docs/hi.txt").await.unwrap().mode & 0o222, 0);
    fs.chmod("/docs/hi.txt", 0o644).await.unwrap();

    let mut listing = fs.list("/docs").await.unwrap();
    sort(&mut listing, SortKey::Name, true);
    let names: Vec<&str> = listing.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, ["empty.txt", "hi.txt"]);
    assert!(
        fs.list("/")
            .await
            .unwrap()
            .iter()
            .any(|entry| entry.name == "docs" && entry.kind == Kind::Dir)
    );

    // Links (Windows needs Developer Mode for them; the server then refuses).
    if fs.symlink("/docs/link", "hi.txt").await.is_ok() {
        let link = fs.lstat("/docs/link").await.unwrap();
        assert_eq!(link.kind, Kind::Symlink);
        assert_eq!(link.link_target.as_deref(), Some("hi.txt"));
        assert_eq!(link.target_kind, Some(Kind::File));
        assert_eq!(fs.read_link("/docs/link").await.unwrap(), "hi.txt");
    }

    // This server has no shell: copying on it falls back to the client.
    assert_eq!(
        remote
            .copy_within("/docs", "/copy")
            .await
            .unwrap_err()
            .code(),
        "unsupported"
    );
    assert_eq!(
        fs.remove("/docs", false).await.unwrap_err().code(),
        "failed"
    );
    fs.remove("/docs", true).await.unwrap();
    assert!(fs.try_lstat("/docs").await.unwrap().is_none());
    assert_eq!(fs.list("/nowhere").await.unwrap_err().code(), "not-found");
}

/// A tree of folders and files in `dir`; returns (relative path, contents).
fn tree(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let files = [
        ("a.bin", 300_000),
        ("empty", 0),
        ("sub/b.bin", 1_000_000),
        ("sub/deeper/c.txt", 12),
        ("sub/deeper/d.bin", 70_000),
    ];
    std::fs::create_dir_all(dir.join("sub/deeper")).unwrap();
    std::fs::create_dir_all(dir.join("sub/empty-folder")).unwrap();
    files
        .iter()
        .enumerate()
        .map(|(seed, (name, len))| {
            let data = bytes(seed as u64, *len);
            std::fs::write(dir.join(name), &data).unwrap();
            let file = std::fs::File::options()
                .write(true)
                .open(dir.join(name))
                .unwrap();
            let when = std::time::UNIX_EPOCH + Duration::from_secs(1_600_000_000 + seed as u64);
            file.set_times(std::fs::FileTimes::new().set_modified(when))
                .unwrap();
            ((*name).to_owned(), data)
        })
        .collect()
}

fn check(dir: &Path, expected: &[(String, Vec<u8>)]) {
    for (seed, (name, data)) in expected.iter().enumerate() {
        let path = dir.join(name);
        assert_eq!(&std::fs::read(&path).unwrap(), data, "{name}");
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        let secs = modified
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(secs, 1_600_000_000 + seed as u64, "{name} kept its time");
    }
    assert!(dir.join("sub/empty-folder").is_dir());
}

#[tokio::test(flavor = "multi_thread")]
async fn transfers_both_ways() {
    let root = tempfile::tempdir().unwrap();
    let known = tempfile::tempdir().unwrap();
    let here = tempfile::tempdir().unwrap();
    let back = tempfile::tempdir().unwrap();
    let source = here.path().join("project");
    let expected = tree(&source);
    let remote = Fs::Remote(remote(root.path(), known.path()).await);
    let events = Events::default();
    let queue = Queue::new(tokio::runtime::Handle::current(), 3, events.sink());

    let up = queue.add(Request {
        from: Fs::local(),
        sources: vec![source.display().to_string()],
        to: remote.clone(),
        destination: "/".into(),
        options: options(Policy::Ask),
        remove_sources: false,
    });
    let done = events.finished(up).await;
    assert_eq!(done.state, State::Done, "{done:?}");
    assert_eq!((done.files_total, done.files_done), (5, 5));
    assert_eq!(done.bytes_done, done.bytes_total);
    assert!(!done.from_remote && done.to_remote);
    check(&root.path().join("project"), &expected);

    let down = queue.add(Request {
        from: remote.clone(),
        sources: vec!["/project".into()],
        to: Fs::local(),
        destination: back.path().display().to_string(),
        options: options(Policy::Ask),
        remove_sources: false,
    });
    assert_eq!(events.finished(down).await.state, State::Done);
    check(&back.path().join("project"), &expected);

    // Moving: the sources go once copied.
    let moved = queue.add(Request {
        from: remote.clone(),
        sources: vec!["/project/sub/deeper".into()],
        to: Fs::local(),
        destination: back.path().display().to_string(),
        options: options(Policy::Overwrite),
        remove_sources: true,
    });
    assert_eq!(events.finished(moved).await.state, State::Done);
    assert!(!root.path().join("project/sub/deeper").exists());
    assert!(back.path().join("deeper/d.bin").is_file());

    queue.clear_finished();
    assert!(queue.snapshot().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn pause_resume_and_cancel() {
    let root = tempfile::tempdir().unwrap();
    let known = tempfile::tempdir().unwrap();
    let here = tempfile::tempdir().unwrap();
    let big = here.path().join("big.bin");
    let data = bytes(7, 48 * 1024 * 1024);
    std::fs::write(&big, &data).unwrap();
    let remote = Fs::Remote(remote(root.path(), known.path()).await);
    let events = Events::default();
    let queue = Queue::new(tokio::runtime::Handle::current(), 2, events.sink());
    let request = Request {
        from: Fs::local(),
        sources: vec![big.display().to_string()],
        to: remote.clone(),
        destination: "/".into(),
        options: options(Policy::Ask),
        remove_sources: false,
    };

    let id = queue.add(request.clone());
    events
        .until("some bytes", |_| events_bytes(&events, id) > 0)
        .await;
    queue.pause(id);
    events
        .until("the pause", |list| {
            list.iter().any(|event| matches!(event, Event::Progress(p) if p.id == id && p.state == State::Paused))
        })
        .await;
    let paused_at = events.last(id).unwrap().bytes_done;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        events.last(id).unwrap().bytes_done,
        paused_at,
        "nothing moves while paused"
    );
    assert!(paused_at < data.len() as u64);
    queue.resume(id);
    let done = events.finished(id).await;
    assert_eq!(done.state, State::Done, "{done:?}");
    assert_eq!(std::fs::read(root.path().join("big.bin")).unwrap(), data);

    // Cancelled while copying: the file it was writing from the start is gone.
    std::fs::remove_file(root.path().join("big.bin")).unwrap();
    let id = queue.add(request);
    events
        .until("some bytes", |_| events_bytes(&events, id) > 0)
        .await;
    queue.cancel(id);
    assert_eq!(events.finished(id).await.state, State::Cancelled);
    assert!(!root.path().join("big.bin").exists());
}

fn events_bytes(events: &Events, id: u64) -> u64 {
    events.last(id).map_or(0, |progress| progress.bytes_done)
}

#[tokio::test(flavor = "multi_thread")]
async fn partial_files_resume_and_questions() {
    let root = tempfile::tempdir().unwrap();
    let known = tempfile::tempdir().unwrap();
    let here = tempfile::tempdir().unwrap();
    let data = bytes(11, 3_000_000);
    let file = here.path().join("data.bin");
    std::fs::write(&file, &data).unwrap();
    let remote = Fs::Remote(remote(root.path(), known.path()).await);
    let events = Events::default();
    let queue = Queue::new(tokio::runtime::Handle::current(), 2, events.sink());
    let request = |policy| Request {
        from: Fs::local(),
        sources: vec![file.display().to_string()],
        to: remote.clone(),
        destination: "/".into(),
        options: options(policy),
        remove_sources: false,
    };

    // The first million bytes are there already (an interrupted upload): only the rest goes.
    std::fs::write(root.path().join("data.bin"), &data[..1_000_000]).unwrap();
    let id = queue.add(request(Policy::Resume));
    let done = events.finished(id).await;
    assert_eq!(done.state, State::Done);
    assert_eq!(std::fs::read(root.path().join("data.bin")).unwrap(), data);
    let first = events
        .0
        .lock()
        .unwrap()
        .iter()
        .find_map(|event| match event {
            Event::Progress(p) if p.id == id && p.state == State::Running => Some(p.bytes_done),
            _ => None,
        });
    assert!(first.is_some());

    // A partial file that doesn't match is copied again.
    let mut wrong = data[..1_000_000].to_vec();
    wrong[999_999] ^= 0xff;
    std::fs::write(root.path().join("data.bin"), &wrong).unwrap();
    let id = queue.add(request(Policy::Resume));
    assert_eq!(events.finished(id).await.state, State::Done);
    assert_eq!(std::fs::read(root.path().join("data.bin")).unwrap(), data);

    // Complete already: skipped.
    let id = queue.add(request(Policy::Resume));
    let done = events.finished(id).await;
    assert_eq!((done.files_skipped, done.state), (1, State::Done));

    // Asked: rename, then skip.
    let id = queue.add(request(Policy::Ask));
    events
        .until("the question", |list| {
            list.iter()
                .any(|event| matches!(event, Event::Conflict(c) if c.job == id))
        })
        .await;
    queue.answer(id, Choice::Rename, false);
    assert_eq!(events.finished(id).await.state, State::Done);
    assert_eq!(
        std::fs::read(root.path().join("data (1).bin")).unwrap(),
        data
    );
    let id = queue.add(request(Policy::Ask));
    events
        .until("the question", |list| {
            list.iter()
                .any(|event| matches!(event, Event::Conflict(c) if c.job == id))
        })
        .await;
    queue.answer(id, Choice::Cancel, false);
    assert_eq!(events.finished(id).await.state, State::Cancelled);

    // A failed job is retried with a new session: the file is complete, so it is skipped.
    events.0.lock().unwrap().clear();
    queue.retry(id, None, None);
    let done = events.finished(id).await;
    assert_eq!((done.state, done.files_skipped), (State::Done, 1));
}

#[tokio::test(flavor = "multi_thread")]
async fn ten_thousand_entries() {
    let root = tempfile::tempdir().unwrap();
    let known = tempfile::tempdir().unwrap();
    let big = root.path().join("big");
    std::fs::create_dir(&big).unwrap();
    for n in 0..10_000 {
        std::fs::write(big.join(format!("file-{n:05}.txt")), b"x").unwrap();
    }
    let remote = remote(root.path(), known.path()).await;
    let started = Instant::now();
    let mut listing = remote.list("/big").await.unwrap();
    let listed = started.elapsed();
    let started = Instant::now();
    sort(&mut listing, SortKey::Name, true);
    let sorted = started.elapsed();
    assert_eq!(listing.len(), 10_000);
    assert_eq!(listing.first().unwrap().name, "file-00000.txt");
    eprintln!("10,000 entries: listed in {listed:?}, sorted in {sorted:?}");
    assert!(
        sorted < Duration::from_millis(500),
        "sorting took {sorted:?}"
    );
}
