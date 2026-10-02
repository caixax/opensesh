//! S3 storage against a real server (PLAN Sprint 12): a 1 GiB file uploaded through the transfer
//! queue (a multipart upload in 32 MB parts), downloaded back, both with the same SHA-256, and a
//! temporary link to it.
//!
//! The server comes from `scripts/s3-test-server.sh start` (RustFS on 127.0.0.1:9000), so the
//! test is ignored by default:
//!
//! ```sh
//! cargo test -p opensesh-ssh --test real_s3 -- --ignored
//! ```
//!
//! `OPENSESH_S3_ENDPOINT` points it at another server with the same keys.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers"
)]

use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use opensesh_s3::{S3, S3Spec};
use opensesh_ssh::sftp::Fs;
use opensesh_ssh::sftp::s3::S3Fs;
use opensesh_ssh::sftp::transfer::{Event, Options, Policy, Progress, Queue, Request, State};
use secrecy::SecretString;
use sha2::{Digest, Sha256};

/// What `scripts/s3-test-server.sh` sets up.
const ACCESS_KEY: &str = "opensesh-test";
const SECRET_KEY: &str = "opensesh-test-secret-key";

const SIZE: u64 = 1024 * 1024 * 1024;

fn storage() -> (S3, Fs) {
    let endpoint = std::env::var("OPENSESH_S3_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:9000".to_owned());
    let s3 = S3::new(&S3Spec {
        endpoint,
        region: String::new(),
        path_style: true,
        access_key: ACCESS_KEY.to_owned(),
        secret_key: SecretString::from(SECRET_KEY),
    })
    .unwrap();
    (s3.clone(), Fs::S3(Arc::new(S3Fs::new(s3))))
}

/// Writes `SIZE` pseudo-random bytes to `path`: their SHA-256.
fn big_file(path: &Path) -> Vec<u8> {
    let mut file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut hash = Sha256::new();
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut chunk = vec![0_u8; 8 * 1024 * 1024];
    for _ in 0..SIZE / chunk.len() as u64 {
        for word in chunk.chunks_exact_mut(8) {
            // xorshift64
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            word.copy_from_slice(&state.to_le_bytes());
        }
        hash.update(&chunk);
        file.write_all(&chunk).unwrap();
    }
    file.flush().unwrap();
    hash.finalize().to_vec()
}

fn sha256_of(path: &Path) -> Vec<u8> {
    let mut file = std::fs::File::open(path).unwrap();
    let mut hash = Sha256::new();
    let mut chunk = vec![0_u8; 8 * 1024 * 1024];
    loop {
        let read = file.read(&mut chunk).unwrap();
        if read == 0 {
            break;
        }
        hash.update(&chunk[..read]);
    }
    hash.finalize().to_vec()
}

#[derive(Clone, Default)]
struct Events(Arc<Mutex<Vec<Event>>>);

impl Events {
    fn sink(&self) -> opensesh_ssh::sftp::transfer::EventSink {
        let events = Arc::clone(&self.0);
        Arc::new(move |event| events.lock().unwrap().push(event))
    }

    async fn finished(&self, id: u64) -> Progress {
        let deadline = Instant::now() + Duration::from_secs(1800);
        loop {
            let done = self
                .0
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find_map(|event| match event {
                    Event::Progress(progress)
                        if progress.id == id && progress.state.is_finished() =>
                    {
                        Some(progress.clone())
                    }
                    _ => None,
                });
            if let Some(done) = done {
                return done;
            }
            assert!(Instant::now() < deadline, "the transfer didn't finish");
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
}

fn request(from: Fs, source: String, to: Fs, destination: String) -> Request {
    Request {
        from,
        sources: vec![source],
        to,
        destination,
        options: Options {
            policy: Policy::Overwrite,
            preserve_times: true,
            preserve_permissions: true,
        },
        remove_sources: false,
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs scripts/s3-test-server.sh"]
async fn a_gibibyte_up_and_down_with_the_same_checksum() {
    let (s3, fs) = storage();
    let bucket = format!("opensesh-test-{}", std::process::id());
    s3.create_bucket(&bucket).await.unwrap();
    let here = tempfile::tempdir().unwrap();
    let source = here.path().join("big.bin");
    let expected = big_file(&source);
    let back = here.path().join("back");
    std::fs::create_dir(&back).unwrap();
    let events = Events::default();
    let queue = Queue::new(tokio::runtime::Handle::current(), 3, events.sink());

    let started = Instant::now();
    let up = queue.add(request(
        Fs::local(),
        source.display().to_string(),
        fs.clone(),
        format!("/{bucket}"),
    ));
    let done = events.finished(up).await;
    assert_eq!(done.state, State::Done, "{done:?}");
    eprintln!("1 GiB uploaded in {:.1} s", started.elapsed().as_secs_f64());
    let object = s3.head(&bucket, "big.bin").await.unwrap().unwrap();
    assert_eq!(object.size, SIZE);
    std::fs::remove_file(&source).unwrap();

    let started = Instant::now();
    let down = queue.add(request(
        fs.clone(),
        format!("/{bucket}/big.bin"),
        Fs::local(),
        back.display().to_string(),
    ));
    let done = events.finished(down).await;
    assert_eq!(done.state, State::Done, "{done:?}");
    eprintln!(
        "1 GiB downloaded in {:.1} s",
        started.elapsed().as_secs_f64()
    );
    assert_eq!(
        sha256_of(&back.join("big.bin")),
        expected,
        "the checksums differ"
    );

    let link = s3
        .presign_get(&bucket, "big.bin", Duration::from_secs(600))
        .await
        .unwrap();
    assert!(link.contains("X-Amz-Expires=600"), "{link}");

    fs.remove(&format!("/{bucket}"), true).await.unwrap();
    assert!(!s3.bucket_exists(&bucket).await.unwrap());
}
