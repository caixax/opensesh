//! Background file writer, so saving never blocks the GUI thread (PLAN §0 rule 10).
//!
//! Writes are debounced per path: while new contents for the same file keep arriving (e.g. a
//! slider being dragged), only the latest one is written, once the path has been quiet for the
//! debounce interval. [`FileWriter::flush`] writes everything pending right away (used at exit).

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::fsutil::{self, WriteOutcome};

/// Called on the writer thread once a write finished (or failed).
pub type WriteCallback = Box<dyn FnOnce(&Path, io::Result<WriteOutcome>) + Send>;

struct Job {
    bytes: Vec<u8>,
    backups: usize,
    done: Option<WriteCallback>,
}

enum Message {
    Write { path: PathBuf, job: Job },
    Flush(Sender<()>),
}

/// Owns the writer thread. Dropping it writes pending files and joins the thread.
pub struct FileWriter {
    sender: Mutex<Option<Sender<Message>>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl std::fmt::Debug for FileWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileWriter").finish_non_exhaustive()
    }
}

impl FileWriter {
    /// Starts the writer thread.
    ///
    /// # Errors
    ///
    /// Fails if the thread can't be spawned.
    pub fn spawn(debounce: Duration) -> io::Result<Self> {
        let (sender, receiver) = mpsc::channel::<Message>();
        let thread = std::thread::Builder::new()
            .name("opensesh-writer".to_owned())
            .spawn(move || {
                let mut pending: BTreeMap<PathBuf, Job> = BTreeMap::new();
                loop {
                    let message = if pending.is_empty() {
                        receiver.recv().map_err(|_| RecvTimeoutError::Disconnected)
                    } else {
                        receiver.recv_timeout(debounce)
                    };
                    match message {
                        Ok(Message::Write { path, job }) => {
                            // A newer version replaces the pending one (its callback is dropped).
                            pending.insert(path, job);
                        }
                        Ok(Message::Flush(reply)) => {
                            write_all(&mut pending);
                            // The flusher may have given up waiting; nothing to do then.
                            let _ = reply.send(());
                        }
                        Err(RecvTimeoutError::Timeout) => write_all(&mut pending),
                        Err(RecvTimeoutError::Disconnected) => {
                            write_all(&mut pending);
                            break;
                        }
                    }
                }
            })?;
        Ok(Self {
            sender: Mutex::new(Some(sender)),
            thread: Mutex::new(Some(thread)),
        })
    }

    /// Queues `bytes` to be written atomically to `path` with `backups` rotated backups.
    /// Returns immediately; `done` runs on the writer thread with the result.
    pub fn write(
        &self,
        path: PathBuf,
        bytes: Vec<u8>,
        backups: usize,
        done: Option<WriteCallback>,
    ) {
        let job = Job {
            bytes,
            backups,
            done,
        };
        let message = Message::Write { path, job };
        let sent = self
            .sender
            .lock()
            .ok()
            .and_then(|sender| sender.as_ref().map(|s| s.send(message)));
        if !matches!(sent, Some(Ok(()))) {
            tracing::error!("file writer is stopped; a save was dropped");
        }
    }

    /// Writes everything pending and waits until it is on disk.
    pub fn flush(&self) {
        let (reply, done) = mpsc::channel();
        let sent = self
            .sender
            .lock()
            .ok()
            .and_then(|sender| sender.as_ref().map(|s| s.send(Message::Flush(reply))));
        if matches!(sent, Some(Ok(()))) {
            // An error means the thread is gone, so there is nothing left to wait for.
            let _ = done.recv();
        }
    }
}

impl Drop for FileWriter {
    fn drop(&mut self) {
        // Closing the channel makes the thread write what's pending and exit.
        if let Ok(mut sender) = self.sender.lock() {
            sender.take();
        }
        if let Some(thread) = self.thread.lock().ok().and_then(|mut t| t.take())
            && thread.join().is_err()
        {
            tracing::error!("file writer thread panicked");
        }
    }
}

fn write_all(pending: &mut BTreeMap<PathBuf, Job>) {
    for (path, job) in std::mem::take(pending) {
        let result = fsutil::atomic_write(&path, &job.bytes, job.backups);
        if let Err(error) = &result {
            tracing::warn!(path = %path.display(), "could not save file: {error}");
        }
        if let Some(done) = job.done {
            done(&path, result);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn flush_writes_the_latest_version_only_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let writer = FileWriter::spawn(Duration::from_secs(60)).unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        for version in 0..5 {
            let calls = Arc::clone(&calls);
            writer.write(
                path.clone(),
                format!("v{version}").into_bytes(),
                5,
                Some(Box::new(move |_, result| {
                    calls.lock().unwrap().push(result.is_ok());
                })),
            );
        }
        writer.flush();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "v4");
        assert_eq!(
            *calls.lock().unwrap(),
            vec![true],
            "superseded writes are skipped"
        );
        assert!(
            !fsutil::backup_path(&path, 1).exists(),
            "no intermediate versions hit disk"
        );
    }

    #[test]
    fn debounced_writes_happen_without_flush() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");
        let writer = FileWriter::spawn(Duration::from_millis(20)).unwrap();
        let (done_tx, done_rx) = mpsc::channel();
        writer.write(
            path.clone(),
            b"x".to_vec(),
            0,
            Some(Box::new(move |_, result| {
                done_tx.send(result.is_ok()).unwrap();
            })),
        );
        assert_eq!(done_rx.recv_timeout(Duration::from_secs(10)), Ok(true));
        assert_eq!(std::fs::read(&path).unwrap(), b"x");
    }

    #[test]
    fn errors_reach_the_callback() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("file");
        std::fs::write(&blocker, b"").unwrap();
        let writer = FileWriter::spawn(Duration::from_millis(1)).unwrap();
        let (done_tx, done_rx) = mpsc::channel();
        writer.write(
            blocker.join("impossible.toml"),
            b"x".to_vec(),
            0,
            Some(Box::new(move |_, result| {
                done_tx.send(result.is_err()).unwrap();
            })),
        );
        writer.flush();
        assert_eq!(done_rx.recv_timeout(Duration::from_secs(10)), Ok(true));
    }

    #[test]
    fn dropping_the_writer_writes_pending_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("late.toml");
        let writer = FileWriter::spawn(Duration::from_secs(60)).unwrap();
        writer.write(path.clone(), b"saved at exit".to_vec(), 0, None);
        drop(writer);
        assert_eq!(std::fs::read(&path).unwrap(), b"saved at exit");
    }
}
