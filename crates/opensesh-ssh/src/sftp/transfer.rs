//! The transfer queue (PLAN Sprint 8): copies files and folders between any two [`Fs`] (this
//! computer and a server, or two servers), with a parallel limit shared by every job.
//!
//! A job first walks its sources (folders recursively), creates the folders, then copies the
//! files, several at a time. For each file already at the destination the job's overwrite policy
//! decides; `Ask` puts the question to the UI and waits. Progress (bytes, files, speed and ETA)
//! goes to the event sink a few times a second.
//!
//! - **Pause** stops each file after its current chunk and keeps its offset; **resume** goes on
//!   from there.
//! - **Resume** as a policy continues a file that is shorter at the destination, once the last
//!   64 KiB it has match the source's; otherwise the file is copied again.
//! - **Cancel** stops the job and deletes the file it was writing from the start (a partial file it
//!   was resuming stays).
//! - A job that failed (a lost connection) can be **retried**, with new file systems: files already
//!   copied are skipped and partial ones resumed.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Semaphore, oneshot, watch};

use super::FsError;
use super::entry::{Entry, Kind};
use super::fs::Fs;
use super::path;
use super::remote::Remote;

/// Identifies a job.
pub type JobId = u64;

/// What to do with a file that is already at the destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Policy {
    /// Ask the user, file by file (or once for all).
    #[default]
    Ask,
    /// Replace it.
    Overwrite,
    /// Replace it when the source is newer.
    OverwriteIfNewer,
    /// Continue a shorter file; replace a different one.
    Resume,
    /// Leave it.
    Skip,
    /// Copy under a new name: `name (1).ext`.
    Rename,
}

impl Policy {
    /// From the settings' and the UI's name; unknown names ask.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text {
            "overwrite" => Self::Overwrite,
            "newer" => Self::OverwriteIfNewer,
            "resume" => Self::Resume,
            "skip" => Self::Skip,
            "rename" => Self::Rename,
            _ => Self::Ask,
        }
    }

    /// The name in settings and in the UI.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Overwrite => "overwrite",
            Self::OverwriteIfNewer => "newer",
            Self::Resume => "resume",
            Self::Skip => "skip",
            Self::Rename => "rename",
        }
    }
}

/// How a job copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Options {
    /// Files already at the destination.
    pub policy: Policy,
    /// Keep modification times.
    pub preserve_times: bool,
    /// Keep permission bits.
    pub preserve_permissions: bool,
}

/// A transfer to queue.
#[derive(Debug, Clone)]
pub struct Request {
    /// Where the sources are.
    pub from: Fs,
    /// Files and folders to copy.
    pub sources: Vec<String>,
    /// Where they go.
    pub to: Fs,
    /// The folder they go into.
    pub destination: String,
    /// How.
    pub options: Options,
    /// Delete the sources once everything is copied (a move between file systems).
    pub remove_sources: bool,
}

/// Where a job is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Waiting for its turn.
    Queued,
    /// Walking its folders.
    Scanning,
    /// Copying.
    Running,
    /// Paused by the user.
    Paused,
    /// Waiting for an answer about a file in the way.
    Asking,
    /// Finished.
    Done,
    /// Stopped by an error; `retry` may finish it.
    Failed {
        /// [`FsError::code`].
        code: &'static str,
        /// Why.
        message: String,
    },
    /// Stopped by the user.
    Cancelled,
}

impl State {
    /// Whether the job is over.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        matches!(self, Self::Done | Self::Failed { .. } | Self::Cancelled)
    }

    /// The name in the UI's data.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Scanning => "scanning",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Asking => "asking",
            Self::Done => "done",
            Self::Failed { .. } => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// A job as the UI shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    /// The job.
    pub id: JobId,
    /// The first source's name, and how many more.
    pub label: String,
    /// Where it goes.
    pub destination: String,
    /// From a server.
    pub from_remote: bool,
    /// To a server.
    pub to_remote: bool,
    /// Where it is.
    pub state: State,
    /// Files to copy (known after scanning).
    pub files_total: u64,
    /// Files copied or skipped.
    pub files_done: u64,
    /// Files left as they were.
    pub files_skipped: u64,
    /// Bytes to copy.
    pub bytes_total: u64,
    /// Bytes copied (skipped files count as done).
    pub bytes_done: u64,
    /// Bytes per second, smoothed.
    pub speed: f64,
    /// Seconds left at that speed.
    pub eta: Option<u64>,
    /// The file being copied.
    pub current: String,
}

/// A file in the way, put to the user.
#[derive(Debug, Clone, PartialEq)]
pub struct Conflict {
    /// The job.
    pub job: JobId,
    /// The destination path.
    pub path: String,
    /// The file being copied.
    pub source: Entry,
    /// The one in the way.
    pub existing: Entry,
    /// It is shorter and could be continued.
    pub resumable: bool,
}

/// The user's answer to a [`Conflict`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// Replace.
    Overwrite,
    /// Replace when newer.
    OverwriteIfNewer,
    /// Continue it.
    Resume,
    /// Leave it.
    Skip,
    /// New name.
    Rename,
    /// Stop the job.
    Cancel,
}

impl Choice {
    /// From the UI's name (`overwrite`, `newer`, `resume`, `skip`, `rename`); anything else
    /// cancels.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text {
            "overwrite" => Self::Overwrite,
            "newer" => Self::OverwriteIfNewer,
            "resume" => Self::Resume,
            "skip" => Self::Skip,
            "rename" => Self::Rename,
            _ => Self::Cancel,
        }
    }

    fn policy(self) -> Option<Policy> {
        match self {
            Self::Overwrite => Some(Policy::Overwrite),
            Self::OverwriteIfNewer => Some(Policy::OverwriteIfNewer),
            Self::Resume => Some(Policy::Resume),
            Self::Skip => Some(Policy::Skip),
            Self::Rename => Some(Policy::Rename),
            Self::Cancel => None,
        }
    }
}

/// What the queue tells the UI.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A job was added or changed.
    Progress(Progress),
    /// A file is in the way (the job waits for [`Queue::answer`]).
    Conflict(Conflict),
    /// A job was cleared from the list.
    Removed(JobId),
}

/// Where events go. Called from the SFTP runtime's threads; must return at once.
pub type EventSink = Arc<dyn Fn(Event) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Control {
    Run,
    Pause,
    Cancel,
}

/// One file to copy.
#[derive(Debug, Clone)]
struct Item {
    from: String,
    to: String,
    entry: Entry,
    /// Continue at this offset without asking (after a pause).
    resume_at: Option<u64>,
}

/// Counters a job's file tasks update.
#[derive(Debug, Default)]
struct Counters {
    files_total: AtomicU64,
    files_done: AtomicU64,
    files_skipped: AtomicU64,
    bytes_total: AtomicU64,
    bytes_done: AtomicU64,
}

struct Job {
    request: Request,
    control: watch::Sender<Control>,
    counters: Arc<Counters>,
    progress: Progress,
    answer: Option<oneshot::Sender<(Choice, bool)>>,
}

struct Inner {
    runtime: tokio::runtime::Handle,
    permits: Arc<Semaphore>,
    parallel: AtomicUsize,
    sink: EventSink,
    next: AtomicU64,
    jobs: Mutex<HashMap<JobId, Job>>,
}

/// The transfer queue. Clones share it.
#[derive(Clone)]
pub struct Queue {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for Queue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Queue")
            .field("parallel", &self.inner.parallel.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

const CHUNK: usize = 256 * 1024;
/// How much of a partial file is compared with the source before continuing it.
const RESUME_CHECK: u64 = 64 * 1024;
const TICK: Duration = Duration::from_millis(250);

fn label(request: &Request) -> String {
    let style = request.from.style();
    let first = request
        .sources
        .first()
        .map(|source| path::file_name(style, source))
        .unwrap_or_default();
    match request.sources.len() {
        0 | 1 => first,
        more => format!("{first} +{}", more - 1),
    }
}

impl Queue {
    /// A queue running on `runtime` with `parallel` files at a time, telling `sink`.
    #[must_use]
    pub fn new(runtime: tokio::runtime::Handle, parallel: usize, sink: EventSink) -> Self {
        let parallel = parallel.clamp(1, 16);
        Self {
            inner: Arc::new(Inner {
                runtime,
                permits: Arc::new(Semaphore::new(parallel)),
                parallel: AtomicUsize::new(parallel),
                sink,
                next: AtomicU64::new(1),
                jobs: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// Changes how many files are copied at once (1 to 16); running files finish first.
    pub fn set_parallel(&self, parallel: usize) {
        let parallel = parallel.clamp(1, 16);
        let old = self.inner.parallel.swap(parallel, Ordering::SeqCst);
        if parallel > old {
            self.inner.permits.add_permits(parallel - old);
        } else if old > parallel {
            // Permits in use now are taken back as they come back.
            let wanted = old - parallel;
            let permits = Arc::clone(&self.inner.permits);
            self.inner.runtime.spawn(async move {
                let mut taken = permits.forget_permits(wanted);
                while taken < wanted {
                    if let Ok(permit) = Arc::clone(&permits).acquire_owned().await {
                        permit.forget();
                        taken += 1;
                    } else {
                        break;
                    }
                }
            });
        }
    }

    /// Queues `request`; returns its id.
    pub fn add(&self, request: Request) -> JobId {
        let id = self.inner.next.fetch_add(1, Ordering::SeqCst);
        let progress = Progress {
            id,
            label: label(&request),
            destination: request.destination.clone(),
            from_remote: request.from.remote().is_some(),
            to_remote: request.to.remote().is_some(),
            state: State::Queued,
            files_total: 0,
            files_done: 0,
            files_skipped: 0,
            bytes_total: 0,
            bytes_done: 0,
            speed: 0.0,
            eta: None,
            current: String::new(),
        };
        self.start(id, request, progress);
        id
    }

    fn start(&self, id: JobId, request: Request, progress: Progress) {
        let (control, control_rx) = watch::channel(Control::Run);
        let counters = Arc::new(Counters::default());
        {
            let mut jobs = self.jobs();
            jobs.insert(
                id,
                Job {
                    request: request.clone(),
                    control,
                    counters: Arc::clone(&counters),
                    progress: progress.clone(),
                    answer: None,
                },
            );
        }
        (self.inner.sink)(Event::Progress(progress));
        let queue = self.clone();
        self.inner.runtime.spawn(async move {
            let ticker = queue.clone();
            let ticking = tokio::spawn(async move { ticker.tick(id).await });
            let result = Box::pin(queue.run(id, request, control_rx, counters)).await;
            ticking.abort();
            let state = match result {
                Ok(()) => State::Done,
                Err(FsError::Cancelled) => State::Cancelled,
                Err(error) => State::Failed {
                    code: error.code(),
                    message: error.to_string(),
                },
            };
            queue.set_state(id, state);
        });
    }

    fn jobs(&self) -> std::sync::MutexGuard<'_, HashMap<JobId, Job>> {
        self.inner
            .jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Every job, oldest first.
    #[must_use]
    pub fn snapshot(&self) -> Vec<Progress> {
        let jobs = self.jobs();
        let mut list: Vec<Progress> = jobs.values().map(|job| job.progress.clone()).collect();
        list.sort_by_key(|progress| progress.id);
        list
    }

    /// Pauses job `id` (running files stop after their current chunk).
    pub fn pause(&self, id: JobId) {
        self.control(id, Control::Pause);
    }

    /// Resumes a paused job.
    pub fn resume(&self, id: JobId) {
        self.control(id, Control::Run);
    }

    /// Cancels job `id` (a question it was waiting on is answered "cancel").
    pub fn cancel(&self, id: JobId) {
        let answer = self.jobs().get_mut(&id).and_then(|job| job.answer.take());
        if let Some(answer) = answer {
            let _ = answer.send((Choice::Cancel, false));
        }
        self.control(id, Control::Cancel);
    }

    fn control(&self, id: JobId, control: Control) {
        let jobs = self.jobs();
        if let Some(job) = jobs.get(&id) {
            if job.progress.state.is_finished() {
                return;
            }
            job.control.send_replace(control);
        }
    }

    /// Answers the question job `id` is waiting on; `for_all` applies it to the job's next files.
    pub fn answer(&self, id: JobId, choice: Choice, for_all: bool) {
        let answer = self.jobs().get_mut(&id).and_then(|job| job.answer.take());
        if let Some(answer) = answer {
            let _ = answer.send((choice, for_all));
        }
    }

    /// Runs a failed or cancelled job again, with new file systems when given (a new
    /// connection): copied files are skipped, partial ones continued.
    pub fn retry(&self, id: JobId, from: Option<Fs>, to: Option<Fs>) {
        let restart = {
            let jobs = self.jobs();
            jobs.get(&id)
                .filter(|job| matches!(job.progress.state, State::Failed { .. } | State::Cancelled))
                .map(|job| {
                    let mut request = job.request.clone();
                    if let Some(from) = from {
                        request.from = from;
                    }
                    if let Some(to) = to {
                        request.to = to;
                    }
                    request.options.policy = Policy::Resume;
                    let mut progress = job.progress.clone();
                    progress.state = State::Queued;
                    progress.speed = 0.0;
                    progress.eta = None;
                    (request, progress)
                })
        };
        if let Some((request, progress)) = restart {
            self.start(id, request, progress);
        }
    }

    /// Forgets finished jobs.
    pub fn clear_finished(&self) {
        let removed: Vec<JobId> = {
            let mut jobs = self.jobs();
            let ids: Vec<JobId> = jobs
                .iter()
                .filter(|(_, job)| job.progress.state.is_finished())
                .map(|(id, _)| *id)
                .collect();
            for id in &ids {
                jobs.remove(id);
            }
            ids
        };
        for id in removed {
            (self.inner.sink)(Event::Removed(id));
        }
    }

    /// Updates job `id` and tells the sink.
    fn update(&self, id: JobId, change: impl FnOnce(&mut Progress)) {
        let progress = {
            let mut jobs = self.jobs();
            let Some(job) = jobs.get_mut(&id) else {
                return;
            };
            let counters = &job.counters;
            let progress = &mut job.progress;
            progress.files_total = counters.files_total.load(Ordering::Relaxed);
            progress.files_done = counters.files_done.load(Ordering::Relaxed);
            progress.files_skipped = counters.files_skipped.load(Ordering::Relaxed);
            progress.bytes_total = counters.bytes_total.load(Ordering::Relaxed);
            progress.bytes_done = counters.bytes_done.load(Ordering::Relaxed);
            change(progress);
            progress.clone()
        };
        (self.inner.sink)(Event::Progress(progress));
    }

    fn set_state(&self, id: JobId, state: State) {
        self.update(id, |progress| {
            if !matches!(state, State::Running) {
                progress.speed = 0.0;
                progress.eta = None;
            }
            if state == State::Done {
                progress.current.clear();
            }
            progress.state = state;
        });
    }

    /// Progress a few times a second while the job runs: speed (smoothed) and ETA.
    async fn tick(&self, id: JobId) {
        let mut last = (Instant::now(), None::<u64>);
        loop {
            tokio::time::sleep(TICK).await;
            self.update(id, |progress| {
                let now = Instant::now();
                let elapsed = now.duration_since(last.0).as_secs_f64();
                if progress.state == State::Running && elapsed > 0.0 {
                    let previous = last.1.unwrap_or(progress.bytes_done);
                    let moved = progress.bytes_done.saturating_sub(previous);
                    progress.speed = smoothed_speed(progress.speed, moved, elapsed);
                    let left = progress.bytes_total.saturating_sub(progress.bytes_done);
                    progress.eta = eta(left, progress.speed);
                } else if progress.state != State::Running {
                    progress.speed = 0.0;
                }
                last = (now, Some(progress.bytes_done));
            });
        }
    }

    /// Waits while the job is paused; `Err(Cancelled)` once cancelled.
    async fn wait_running(
        &self,
        id: JobId,
        control: &mut watch::Receiver<Control>,
    ) -> Result<(), FsError> {
        loop {
            let current = *control.borrow_and_update();
            match current {
                Control::Run => return Ok(()),
                Control::Cancel => return Err(FsError::Cancelled),
                Control::Pause => {
                    self.set_state(id, State::Paused);
                    if control.changed().await.is_err() {
                        return Err(FsError::Cancelled);
                    }
                    if *control.borrow() == Control::Run {
                        self.set_state(id, State::Running);
                    }
                }
            }
        }
    }

    async fn run(
        &self,
        id: JobId,
        request: Request,
        mut control: watch::Receiver<Control>,
        counters: Arc<Counters>,
    ) -> Result<(), FsError> {
        self.set_state(id, State::Scanning);
        let (mut dirs, mut items) = scan(&request, &counters, &mut control).await?;
        if request.from.same(&request.to)
            && let Some(remote) = request.from.remote()
        {
            self.set_state(id, State::Running);
            copy_on_server(remote, &request, &counters, &mut dirs, &mut items).await?;
        }
        for dir in &dirs {
            ensure_dir(&request.to, dir).await?;
        }
        self.set_state(id, State::Running);
        let policy = Arc::new(tokio::sync::Mutex::new(request.options.policy));
        let mut pending: Vec<Item> = items;
        while !pending.is_empty() {
            self.wait_running(id, &mut control).await?;
            let mut tasks = tokio::task::JoinSet::new();
            let mut paused = Vec::new();
            for item in std::mem::take(&mut pending) {
                // A pause stops handing out files; the rest wait with the paused ones.
                if *control.borrow() != Control::Run {
                    paused.push(item);
                    continue;
                }
                let permit = tokio::select! {
                    permit = Arc::clone(&self.inner.permits).acquire_owned() => permit.map_err(|_| FsError::Cancelled)?,
                    _ = control.changed() => {
                        paused.push(item);
                        continue;
                    }
                };
                let job = FileJob {
                    queue: self.clone(),
                    id,
                    request: request.clone(),
                    counters: Arc::clone(&counters),
                    policy: Arc::clone(&policy),
                    control: control.clone(),
                };
                tasks.spawn(async move {
                    let outcome = Box::pin(job.copy(item.clone())).await;
                    drop(permit);
                    (item, outcome)
                });
            }
            let mut first_error = None;
            while let Some(joined) = tasks.join_next().await {
                let Ok((item, outcome)) = joined else {
                    continue;
                };
                match outcome {
                    Ok(Outcome::Done | Outcome::Skipped) => {}
                    Ok(Outcome::Paused(offset)) => paused.push(Item {
                        resume_at: Some(offset),
                        ..item
                    }),
                    Err(FsError::Cancelled) => {
                        first_error = first_error.or(Some(FsError::Cancelled))
                    }
                    Err(error) => {
                        // The other files go on; the job fails at the end.
                        tracing::info!(job = id, "transfer of a file failed: {error}");
                        first_error = first_error.or(Some(error));
                    }
                }
            }
            if let Some(error) = first_error {
                return Err(error);
            }
            pending = paused;
        }
        if request.remove_sources {
            for source in &request.sources {
                request.from.remove(source, true).await?;
            }
        }
        Ok(())
    }
}

/// The speed after `moved` bytes in `elapsed` seconds: the first measure as it is, then 30% of
/// each new one, so the number doesn't jump with every tick.
fn smoothed_speed(speed: f64, moved: u64, elapsed: f64) -> f64 {
    #[allow(clippy::cast_precision_loss)] // Byte counts well below 2^52.
    let rate = moved as f64 / elapsed;
    if speed > 0.0 {
        speed * 0.7 + rate * 0.3
    } else {
        rate
    }
}

/// Seconds left for `left` bytes at `speed` (rounded up); none while there's no speed to go by.
fn eta(left: u64, speed: f64) -> Option<u64> {
    (speed > 1.0).then(|| {
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )] // A positive number of seconds.
        let secs = (left as f64 / speed).ceil() as u64;
        secs
    })
}

/// How a file ended.
enum Outcome {
    Done,
    Skipped,
    /// Paused after this many bytes.
    Paused(u64),
}

/// Walks the sources: the destination folders to create (parents first) and the files.
async fn scan(
    request: &Request,
    counters: &Counters,
    control: &mut watch::Receiver<Control>,
) -> Result<(Vec<String>, Vec<Item>), FsError> {
    let (from_style, to_style) = (request.from.style(), request.to.style());
    let mut dirs = Vec::new();
    let mut items = Vec::new();
    // (source path, destination path)
    let mut stack: Vec<(String, String)> = request
        .sources
        .iter()
        .rev()
        .map(|source| {
            let name = path::file_name(from_style, source);
            (
                source.clone(),
                path::join(to_style, &request.destination, &name),
            )
        })
        .collect();
    while let Some((from, to)) = stack.pop() {
        if *control.borrow() == Control::Cancel {
            return Err(FsError::Cancelled);
        }
        let mut entry = request.from.lstat(&from).await?;
        if entry.kind == Kind::Symlink {
            // A link to a file is copied as the file; links to folders are left out (loops).
            match entry.target_kind {
                Some(Kind::File) => {
                    let target = request.from.stat(&from).await?;
                    entry = Entry {
                        name: entry.name,
                        ..target
                    };
                }
                _ => {
                    counters.files_skipped.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
            }
        }
        match entry.kind {
            Kind::Dir => {
                dirs.push(to.clone());
                let mut children = request.from.list(&from).await?;
                children.sort_by(|a, b| b.name.cmp(&a.name));
                for child in children {
                    // A name this side can't hold as one name (a server's `a\b` on Windows)
                    // would land outside the folder: it is left out.
                    if !path::is_plain_name(to_style, &child.name) {
                        counters.files_skipped.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    stack.push((
                        path::join(from_style, &from, &child.name),
                        path::join(to_style, &to, &child.name),
                    ));
                }
            }
            Kind::File => {
                counters.files_total.fetch_add(1, Ordering::Relaxed);
                counters
                    .bytes_total
                    .fetch_add(entry.size, Ordering::Relaxed);
                items.push(Item {
                    from,
                    to,
                    entry,
                    resume_at: None,
                });
            }
            Kind::Symlink | Kind::Other => {
                counters.files_skipped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    Ok((dirs, items))
}

/// A copy within one server: each source whose destination is free is copied there with
/// `cp -R -p` (no bytes through this client), and its folders and files leave `dirs` and `items`.
/// The rest (a destination in the way, which the policy decides, or a server without a shell)
/// goes through the client like any copy.
async fn copy_on_server(
    remote: &Remote,
    request: &Request,
    counters: &Counters,
    dirs: &mut Vec<String>,
    items: &mut Vec<Item>,
) -> Result<(), FsError> {
    let style = request.to.style();
    for source in &request.sources {
        let to = path::join(style, &request.destination, &path::file_name(style, source));
        // A folder copied into itself would never end; a destination in the way is the
        // policy's.
        if within(&request.destination, source) || request.to.try_lstat(&to).await?.is_some() {
            continue;
        }
        match remote.copy_within(source, &to).await {
            Ok(()) => {
                let (done, rest): (Vec<Item>, Vec<Item>) = std::mem::take(items)
                    .into_iter()
                    .partition(|item| within(&item.to, &to));
                *items = rest;
                dirs.retain(|dir| !within(dir, &to));
                let bytes: u64 = done.iter().map(|item| item.entry.size).sum();
                counters
                    .files_done
                    .fetch_add(done.len() as u64, Ordering::Relaxed);
                counters.bytes_done.fetch_add(bytes, Ordering::Relaxed);
            }
            Err(FsError::Unsupported { .. }) => return Ok(()),
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// Whether POSIX path `candidate` is `root` or inside it.
fn within(candidate: &str, root: &str) -> bool {
    let root = root.trim_end_matches('/');
    candidate == root
        || root.is_empty()
        || candidate
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Creates folder `dir` unless it is there.
async fn ensure_dir(fs: &Fs, dir: &str) -> Result<(), FsError> {
    match fs.try_lstat(dir).await? {
        Some(entry) if entry.is_dir_like() => Ok(()),
        Some(_) => Err(FsError::Exists {
            path: dir.to_owned(),
        }),
        None => fs.mkdir(dir).await,
    }
}

/// Whether the last bytes of the partial file `to` match the source at the same offsets.
async fn tails_match(
    from: &Fs,
    from_path: &str,
    to: &Fs,
    to_path: &str,
    length: u64,
) -> Result<bool, FsError> {
    let check = length.min(RESUME_CHECK);
    let start = length - check;
    let read = |fs: Fs, path: String| async move {
        let mut reader = fs.open_read(&path, start).await?;
        let mut buffer = vec![0; usize::try_from(check).unwrap_or(0)];
        reader
            .read_exact(&mut buffer)
            .await
            .map_err(|error| FsError::io(&path, &error))?;
        Ok::<_, FsError>(buffer)
    };
    let (a, b) = tokio::join!(
        read(from.clone(), from_path.to_owned()),
        read(to.clone(), to_path.to_owned())
    );
    Ok(a? == b?)
}

/// One file's copy, with what it needs from its job.
struct FileJob {
    queue: Queue,
    id: JobId,
    request: Request,
    counters: Arc<Counters>,
    policy: Arc<tokio::sync::Mutex<Policy>>,
    control: watch::Receiver<Control>,
}

/// Where a file's copy starts.
enum Start {
    /// From the beginning, replacing what is there.
    Fresh,
    /// At this offset.
    At(u64),
    /// Not at all.
    Skip,
}

impl FileJob {
    async fn copy(mut self, item: Item) -> Result<Outcome, FsError> {
        let (from, to) = (self.request.from.clone(), self.request.to.clone());
        let mut target = item.to.clone();
        let start = match item.resume_at {
            Some(offset) => Start::At(offset),
            None => self.decide(&item, &mut target).await?,
        };
        let offset = match start {
            Start::Skip => {
                self.counters.files_skipped.fetch_add(1, Ordering::Relaxed);
                self.counters.files_done.fetch_add(1, Ordering::Relaxed);
                self.counters
                    .bytes_done
                    .fetch_add(item.entry.size, Ordering::Relaxed);
                return Ok(Outcome::Skipped);
            }
            Start::Fresh => 0,
            Start::At(offset) => offset,
        };
        if item.resume_at.is_none() {
            self.counters
                .bytes_done
                .fetch_add(offset, Ordering::Relaxed);
        }
        self.queue.update(self.id, |progress| {
            progress.current = path::file_name(to.style(), &target);
        });
        let mut reader = from.open_read(&item.from, offset).await?;
        let mut writer = to
            .open_write(&target, (offset > 0).then_some(offset))
            .await?;
        let mut buffer = vec![0; CHUNK];
        let mut written = offset;
        loop {
            let control = *self.control.borrow_and_update();
            match control {
                Control::Run => {}
                Control::Pause => {
                    writer
                        .shutdown()
                        .await
                        .map_err(|error| FsError::io(&target, &error))?;
                    return Ok(Outcome::Paused(written));
                }
                Control::Cancel => {
                    let _ = writer.shutdown().await;
                    drop(writer);
                    if offset == 0 {
                        let _ = to.remove(&target, false).await;
                    }
                    return Err(FsError::Cancelled);
                }
            }
            let read = reader
                .read(&mut buffer)
                .await
                .map_err(|error| FsError::io(&item.from, &error))?;
            if read == 0 {
                break;
            }
            let chunk = buffer.get(..read).unwrap_or_default();
            writer
                .write_all(chunk)
                .await
                .map_err(|error| FsError::io(&target, &error))?;
            let read = read as u64;
            written += read;
            self.counters.bytes_done.fetch_add(read, Ordering::Relaxed);
        }
        writer
            .shutdown()
            .await
            .map_err(|error| FsError::io(&target, &error))?;
        drop(writer);
        let options = self.request.options;
        if options.preserve_times
            && let Some(modified) = item.entry.modified
        {
            // A server that refuses is not worth failing the copy for.
            if let Err(error) = to.set_times(&target, modified, modified).await {
                tracing::info!("times not kept: {error}");
            }
        }
        if options.preserve_permissions
            && let Err(error) = to.chmod(&target, item.entry.mode).await
        {
            tracing::info!("permissions not kept: {error}");
        }
        self.counters.files_done.fetch_add(1, Ordering::Relaxed);
        Ok(Outcome::Done)
    }

    /// What to do when something is at the destination (`target` may get a new name).
    async fn decide(&mut self, item: &Item, target: &mut String) -> Result<Start, FsError> {
        let (from, to) = (&self.request.from, &self.request.to);
        let Some(existing) = to.try_lstat(target).await? else {
            return Ok(Start::Fresh);
        };
        if existing.is_dir_like() {
            return Err(FsError::Exists {
                path: target.clone(),
            });
        }
        let source = &item.entry;
        let shorter = existing.size < source.size;
        // One question at a time per job; the lock also carries "for all" answers.
        let mut policy_guard = self.policy.lock().await;
        let mut policy = *policy_guard;
        if policy == Policy::Ask {
            let (answer, question) = oneshot::channel();
            let conflict = Conflict {
                job: self.id,
                path: target.clone(),
                source: source.clone(),
                existing: existing.clone(),
                resumable: shorter,
            };
            {
                let mut jobs = self.queue.jobs();
                if let Some(job) = jobs.get_mut(&self.id) {
                    job.answer = Some(answer);
                }
            }
            self.queue.set_state(self.id, State::Asking);
            (self.queue.inner.sink)(Event::Conflict(conflict));
            let (choice, for_all) = question.await.unwrap_or((Choice::Cancel, false));
            self.queue.set_state(self.id, State::Running);
            policy = choice.policy().ok_or(FsError::Cancelled)?;
            if for_all {
                *policy_guard = policy;
            }
        }
        drop(policy_guard);
        Ok(match policy {
            Policy::Ask | Policy::Overwrite => Start::Fresh,
            Policy::Skip => Start::Skip,
            Policy::OverwriteIfNewer => {
                if source.modified > existing.modified {
                    Start::Fresh
                } else {
                    Start::Skip
                }
            }
            Policy::Resume => {
                if existing.size <= source.size
                    && existing.size > 0
                    && tails_match(from, &item.from, to, target, existing.size).await?
                {
                    if existing.size == source.size {
                        Start::Skip
                    } else {
                        Start::At(existing.size)
                    }
                } else {
                    Start::Fresh
                }
            }
            Policy::Rename => {
                let style = to.style();
                let dir = path::parent(style, target).unwrap_or_default();
                let name = path::file_name(style, target);
                let mut n = 1;
                loop {
                    let candidate = path::join(style, &dir, &path::numbered(&name, n));
                    if to.try_lstat(&candidate).await?.is_none() {
                        *target = candidate;
                        break Start::Fresh;
                    }
                    n += 1;
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_and_eta() {
        assert!((smoothed_speed(0.0, 1000, 0.5) - 2000.0).abs() < 1e-9);
        assert!((smoothed_speed(2000.0, 0, 0.25) - 1400.0).abs() < 1e-9);
        assert!((smoothed_speed(1000.0, 4000, 1.0) - 1900.0).abs() < 1e-9);
        assert_eq!(eta(10_000, 1000.0), Some(10));
        assert_eq!(eta(10_001, 1000.0), Some(11));
        assert_eq!(eta(0, 1000.0), Some(0));
        assert_eq!(eta(10_000, 0.5), None);
    }

    #[test]
    fn paths_within() {
        assert!(within("/a", "/a") && within("/a/b/c", "/a") && within("/a/b", "/a/"));
        assert!(!within("/ab", "/a") && !within("/", "/a") && !within("/b/a", "/a"));
        assert!(within("/anything", "/"));
    }

    #[test]
    fn names() {
        for policy in [
            Policy::Ask,
            Policy::Overwrite,
            Policy::OverwriteIfNewer,
            Policy::Resume,
            Policy::Skip,
            Policy::Rename,
        ] {
            assert_eq!(Policy::parse(policy.as_str()), policy);
        }
        assert_eq!(Choice::parse("newer"), Choice::OverwriteIfNewer);
        assert_eq!(Choice::parse("?"), Choice::Cancel);
        assert!(
            State::Failed {
                code: "x",
                message: String::new()
            }
            .is_finished()
        );
        assert!(!State::Paused.is_finished());
    }
}
