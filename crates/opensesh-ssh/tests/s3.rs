//! S3 storage in the file views against the in-process S3 server (PLAN Sprint 12): buckets as
//! folders, folders as prefixes, every file operation, transfers both ways and inside the
//! storage, and an upload that is paused (it starts over), resumed, cancelled (nothing stays) or
//! asked to resume a partial object (it starts over too).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers"
)]

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use opensesh_s3::S3;
use opensesh_s3::testing::{self, Rules, TestServer};
use opensesh_ssh::sftp::s3::S3Fs;
use opensesh_ssh::sftp::transfer::{Event, Options, Policy, Progress, Queue, Request, State};
use opensesh_ssh::sftp::{Fs, FsError, Kind};
use tokio::io::AsyncReadExt;

/// Small parts, so that test files go in several.
const PART: usize = 64 * 1024;

async fn storage(buckets: &[&str]) -> (TestServer, Fs) {
    let server = testing::serve(Rules {
        buckets: buckets.iter().map(|name| (*name).to_owned()).collect(),
        ..Rules::default()
    })
    .await
    .unwrap();
    let s3 = S3::new(&server.spec()).unwrap();
    (server, Fs::S3(Arc::new(S3Fs::with_part_size(s3, PART))))
}

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

fn names(entries: &[opensesh_ssh::sftp::Entry]) -> Vec<(String, Kind)> {
    let mut names: Vec<(String, Kind)> = entries
        .iter()
        .map(|entry| (entry.name.clone(), entry.kind))
        .collect();
    names.sort_by(|a, b| a.0.cmp(&b.0));
    names
}

#[tokio::test(flavor = "multi_thread")]
async fn file_operations() {
    let (server, fs) = storage(&["data"]).await;
    assert_eq!(fs.home(), "/");
    assert_eq!(
        names(&fs.list("/").await.unwrap()),
        [("data".into(), Kind::Dir)]
    );
    fs.mkdir("/photos").await.unwrap();
    fs.mkdir("/data/docs").await.unwrap();
    fs.create_file("/data/docs/a.txt").await.unwrap();
    assert_eq!(
        names(&fs.list("/").await.unwrap()),
        [("data".into(), Kind::Dir), ("photos".into(), Kind::Dir)]
    );
    assert_eq!(
        names(&fs.list("/data").await.unwrap()),
        [("docs".into(), Kind::Dir)]
    );
    // The folder's marker isn't one of its files.
    assert_eq!(
        names(&fs.list("/data/docs").await.unwrap()),
        [("a.txt".into(), Kind::File)]
    );
    assert_eq!(fs.stat("/data/docs").await.unwrap().kind, Kind::Dir);
    assert_eq!(fs.stat("/data/docs/a.txt").await.unwrap().kind, Kind::File);
    assert_eq!(fs.stat("/photos").await.unwrap().kind, Kind::Dir);
    assert!(matches!(
        fs.stat("/data/none").await,
        Err(FsError::NotFound { .. })
    ));
    assert!(fs.try_lstat("/nope").await.unwrap().is_none());
    // Renames: a file, then its folder (every object moves).
    fs.rename("/data/docs/a.txt", "/data/docs/b.txt")
        .await
        .unwrap();
    fs.rename("/data/docs", "/data/papers").await.unwrap();
    assert_eq!(server.keys("data"), ["papers/", "papers/b.txt"]);
    fs.create_file("/data/c.txt").await.unwrap();
    assert!(matches!(
        fs.rename("/data/c.txt", "/data/papers").await,
        Err(FsError::Exists { .. })
    ));
    assert!(matches!(
        fs.rename("/photos", "/pictures").await,
        Err(FsError::Unsupported { .. })
    ));
    // Deletes.
    assert!(matches!(
        fs.remove("/data/papers", false).await,
        Err(FsError::Failed { .. })
    ));
    fs.remove("/data/papers", true).await.unwrap();
    fs.remove("/data/c.txt", false).await.unwrap();
    assert!(server.keys("data").is_empty());
    fs.remove("/photos", false).await.unwrap();
    assert_eq!(
        names(&fs.list("/").await.unwrap()),
        [("data".into(), Kind::Dir)]
    );
    // What S3 doesn't have.
    assert!(matches!(
        fs.chmod("/data/x", 0o600).await,
        Err(FsError::Unsupported { .. })
    ));
    assert!(matches!(
        fs.open_write("/data/x", Some(10)).await,
        Err(FsError::Unsupported { .. })
    ));
    assert!(fs.open_write("/", None).await.is_err());
    assert!(!fs.can_resume() && !fs.keeps_metadata() && !fs.is_local());
    // Keys that aren't one plain name aren't listed (they could be copied out of their folder).
    for key in ["odd/..", "odd/./x", "odd/ok.txt"] {
        fs.s3()
            .unwrap()
            .client()
            .put("data", key, Vec::new())
            .await
            .unwrap();
    }
    assert_eq!(
        names(&fs.list("/data/odd").await.unwrap()),
        [("ok.txt".into(), Kind::File)]
    );
    // A temporary link.
    fs.create_file("/data/shared.txt").await.unwrap();
    let link = fs
        .s3()
        .unwrap()
        .temporary_link("/data/shared.txt", Duration::from_secs(600))
        .await
        .unwrap();
    assert!(
        link.contains("/data/shared.txt?") && link.contains("X-Amz-Expires=600"),
        "{link}"
    );
}

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
            let events = self.0.lock().unwrap().clone();
            if done(&events) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("timed out waiting for {what}");
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

fn request(from: Fs, sources: Vec<String>, to: Fs, destination: &str, policy: Policy) -> Request {
    Request {
        from,
        sources,
        to,
        destination: destination.to_owned(),
        options: Options {
            policy,
            preserve_times: true,
            preserve_permissions: true,
        },
        remove_sources: false,
    }
}

fn local_tree(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let files = [
        ("a.bin", 300_000),
        ("empty", 0),
        ("sub/b.bin", PART),
        ("sub/deeper/c.txt", 12),
    ];
    std::fs::create_dir_all(dir.join("sub/deeper")).unwrap();
    files
        .iter()
        .enumerate()
        .map(|(seed, (name, len))| {
            let data = bytes(seed as u64, *len);
            std::fs::write(dir.join(name), &data).unwrap();
            ((*name).to_owned(), data)
        })
        .collect()
}

async fn read(fs: &Fs, path: &str) -> Vec<u8> {
    let mut reader = fs.open_read(path, 0).await.unwrap();
    let mut data = Vec::new();
    reader.read_to_end(&mut data).await.unwrap();
    data
}

#[tokio::test(flavor = "multi_thread")]
async fn transfers_both_ways_and_inside() {
    let (server, s3) = storage(&["data"]).await;
    let here = tempfile::tempdir().unwrap();
    let back = tempfile::tempdir().unwrap();
    let source = here.path().join("project");
    let expected = local_tree(&source);
    let events = Events::default();
    let queue = Queue::new(tokio::runtime::Handle::current(), 3, events.sink());

    let up = queue.add(request(
        Fs::local(),
        vec![source.display().to_string()],
        s3.clone(),
        "/data",
        Policy::Ask,
    ));
    let done = events.finished(up).await;
    assert_eq!(done.state, State::Done, "{done:?}");
    assert_eq!((done.files_total, done.files_done), (4, 4));
    assert!(!done.from_remote && done.to_remote);
    for (name, data) in &expected {
        assert_eq!(
            &server.object("data", &format!("project/{name}")).unwrap(),
            data,
            "{name}"
        );
    }
    assert_eq!(server.uploads_in_progress(), 0);

    let down = queue.add(request(
        s3.clone(),
        vec!["/data/project".into()],
        Fs::local(),
        &back.path().display().to_string(),
        Policy::Ask,
    ));
    assert_eq!(events.finished(down).await.state, State::Done);
    for (name, data) in &expected {
        assert_eq!(
            &std::fs::read(back.path().join("project").join(name)).unwrap(),
            data,
            "{name}"
        );
    }

    // Inside the storage: the server copies.
    let inside = queue.add(request(
        s3.clone(),
        vec!["/data/project/sub".into()],
        s3.clone(),
        "/data/copies",
        Policy::Ask,
    ));
    assert_eq!(events.finished(inside).await.state, State::Done);
    assert_eq!(read(&s3, "/data/copies/sub/b.bin").await, expected[2].1);
}

#[tokio::test(flavor = "multi_thread")]
async fn uploads_start_over_when_paused_and_leave_nothing_when_cancelled() {
    let (server, s3) = storage(&["data"]).await;
    let here = tempfile::tempdir().unwrap();
    let big = here.path().join("big.bin");
    let data = bytes(7, 48 * 1024 * 1024);
    std::fs::write(&big, &data).unwrap();
    let events = Events::default();
    let queue = Queue::new(tokio::runtime::Handle::current(), 2, events.sink());
    let upload = || {
        request(
            Fs::local(),
            vec![big.display().to_string()],
            s3.clone(),
            "/data",
            Policy::Overwrite,
        )
    };

    let id = queue.add(upload());
    events
        .until("some bytes", |_| {
            events.last(id).is_some_and(|p| p.bytes_done > 0)
        })
        .await;
    queue.pause(id);
    events
        .until("the pause", |list| {
            list.iter().any(|event| matches!(event, Event::Progress(p) if p.id == id && p.state == State::Paused))
        })
        .await;
    // The parts sent so far are dropped on the server; the count goes back to the start.
    let deadline = Instant::now() + Duration::from_secs(10);
    while server.uploads_in_progress() > 0 {
        assert!(
            Instant::now() < deadline,
            "the paused upload wasn't aborted"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(server.object("data", "big.bin"), None);
    queue.resume(id);
    let done = events.finished(id).await;
    assert_eq!(done.state, State::Done, "{done:?}");
    assert_eq!(done.bytes_done, data.len() as u64);
    assert!(server.object("data", "big.bin").unwrap() == data);

    // Cancelled: the object that was there stays as it was, and no upload is left.
    let id = queue.add(upload());
    events
        .until("some bytes", |_| {
            events.last(id).is_some_and(|p| p.bytes_done > 0)
        })
        .await;
    queue.cancel(id);
    assert_eq!(events.finished(id).await.state, State::Cancelled);
    let deadline = Instant::now() + Duration::from_secs(10);
    while server.uploads_in_progress() > 0 {
        assert!(
            Instant::now() < deadline,
            "the cancelled upload wasn't aborted"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(server.object("data", "big.bin").unwrap() == data);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_partial_object_is_replaced_not_resumed() {
    let (server, s3) = storage(&["data"]).await;
    let here = tempfile::tempdir().unwrap();
    let file = here.path().join("data.bin");
    let data = bytes(11, 300_000);
    std::fs::write(&file, &data).unwrap();
    s3.s3()
        .unwrap()
        .client()
        .put("data", "data.bin", data[..100_000].to_vec())
        .await
        .unwrap();
    let events = Events::default();
    let queue = Queue::new(tokio::runtime::Handle::current(), 2, events.sink());
    let id = queue.add(request(
        Fs::local(),
        vec![file.display().to_string()],
        s3.clone(),
        "/data",
        Policy::Resume,
    ));
    assert_eq!(events.finished(id).await.state, State::Done);
    assert!(server.object("data", "data.bin").unwrap() == data);
}
