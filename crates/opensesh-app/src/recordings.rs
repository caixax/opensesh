//! Session recordings (Sprint 10): asciicast v2 files of what a pane shows, started and stopped
//! from the pane's menu. They go to `recordings/` in the data folder (a test run's own folder in
//! test runs), named `<date>_<time>_<name>.cast` (UTC). A recording is a tap on the pane's
//! session: stopping removes it, and closing the pane (or restarting its shell) ends it with its
//! session. Only the output and the sizes are written, never the keys typed; a password typed
//! at a prompt isn't echoed, so it isn't in the output either.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};

use opensesh_term::recording;
use opensesh_term::session::TapId;

use crate::services;
use crate::terminal::registry;

/// The extension of recordings.
pub const EXTENSION: &str = "cast";

/// The longest header line read for the list (a header is a few hundred bytes).
const HEADER_LIMIT: u64 = 64 * 1024;

/// Told when a pane starts or stops recording (the `Recordings` singleton refreshes).
pub type Sink = Arc<dyn Fn() + Send + Sync>;

/// The panes that record: the tap and the file.
static ACTIVE: LazyLock<Mutex<HashMap<i32, (TapId, PathBuf)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static SINK: Mutex<Option<Sink>> = Mutex::new(None);

fn active() -> MutexGuard<'static, HashMap<i32, (TapId, PathBuf)>> {
    ACTIVE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Sets who is told when a pane starts or stops recording.
pub fn set_sink(sink: Sink) {
    *SINK.lock().unwrap_or_else(PoisonError::into_inner) = Some(sink);
}

fn notify() {
    let sink = SINK.lock().unwrap_or_else(PoisonError::into_inner).clone();
    if let Some(sink) = sink {
        sink();
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// The folder recordings go to; `None` before the app is ready.
#[must_use]
pub fn folder() -> Option<PathBuf> {
    if let Some(test) = crate::bridge::app_info::test_run_folder() {
        return Some(test.join("recordings"));
    }
    services::get().map(|services| services.paths.data_dir().join("recordings"))
}

/// The file name of a recording of `name` started at `seconds` (Unix time); `attempt` numbers a
/// second one started in the same second.
#[must_use]
pub fn file_name(name: &str, seconds: u64, attempt: u32) -> String {
    let stem = format!(
        "{}_{}",
        opensesh_ssh::log::timestamp(seconds),
        crate::ssh::safe_name(name)
    );
    if attempt <= 1 {
        format!("{stem}.{EXTENSION}")
    } else {
        format!("{stem}-{attempt}.{EXTENSION}")
    }
}

/// Creates `folder` (private on Unix: recordings show whatever the terminal showed).
fn create_folder(folder: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(folder)
}

/// Starts recording pane `pane` into a new file in `folder`, titled `title`, for a terminal of
/// type `term`. The file's path, or why it didn't start (for the user: paths, no secrets).
///
/// # Errors
///
/// When the pane has no session or records already, or the file can't be created.
pub fn start_in(folder: &Path, pane: i32, title: &str, term: &str) -> Result<PathBuf, String> {
    let entry = registry::get(pane).ok_or_else(|| "the pane has no session".to_owned())?;
    let mut active = active();
    if active.contains_key(&pane) {
        return Err("the pane is recording already".to_owned());
    }
    create_folder(folder).map_err(|error| format!("{}: {error}", folder.display()))?;
    let seconds = now_secs();
    let mut attempt = 1;
    let (path, recorder) = loop {
        let path = folder.join(file_name(title, seconds, attempt));
        match recording::recorder(&path, title, term) {
            Ok(recorder) => break (path, recorder),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && attempt < 100 => {
                attempt += 1;
            }
            Err(error) => return Err(format!("{}: {error}", path.display())),
        }
    };
    let tap = entry.session().add_tap(Box::new(recorder));
    active.insert(pane, (tap, path.clone()));
    drop(active);
    tracing::info!(pane, "a session recording started");
    notify();
    Ok(path)
}

/// [`start_in`] the recordings folder.
///
/// # Errors
///
/// As [`start_in`], and before the app is ready.
pub fn start(pane: i32, title: &str, term: &str) -> Result<PathBuf, String> {
    let folder = folder().ok_or_else(|| "the app isn't ready".to_owned())?;
    start_in(&folder, pane, title, term)
}

/// Stops recording pane `pane`: the file's path (complete once the writer drains, a moment
/// later); `None` when it wasn't recording.
pub fn stop(pane: i32) -> Option<PathBuf> {
    let (tap, path) = active().remove(&pane)?;
    if let Some(entry) = registry::get(pane) {
        entry.session().remove_tap(tap);
    }
    tracing::info!(pane, "a session recording stopped");
    notify();
    Some(path)
}

/// The panes that record, in no order.
#[must_use]
pub fn panes() -> Vec<i32> {
    active().keys().copied().collect()
}

/// The files being written.
#[must_use]
pub fn files() -> Vec<PathBuf> {
    active().values().map(|(_, path)| path.clone()).collect()
}

/// Pane `pane`'s session ended: its recording ended with it.
pub fn session_closed(pane: i32) {
    if active().remove(&pane).is_some() {
        tracing::info!(pane, "a session recording ended with its session");
        notify();
    }
}

/// A recording in the folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Where it is.
    pub path: PathBuf,
    /// The file's name.
    pub name: String,
    /// The title in its header (the pane's name when it was recorded), if any.
    pub title: String,
    /// Its size in bytes.
    pub size: u64,
    /// When it last changed (Unix time), 0 when unknown.
    pub modified: u64,
}

/// The title in a recording's header, if the file starts with one.
fn header_title(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut line = String::new();
    BufReader::new(file.take(HEADER_LIMIT))
        .read_line(&mut line)
        .ok()?;
    let header: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    header["title"].as_str().map(str::to_owned)
}

/// The recordings in `folder`, the newest first (blocking: call it off the GUI thread).
#[must_use]
pub fn list(folder: &Path) -> Vec<Entry> {
    let Ok(read) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut entries: Vec<Entry> = read
        .filter_map(Result::ok)
        .filter_map(|item| {
            let path = item.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some(EXTENSION) {
                return None;
            }
            let metadata = item.metadata().ok().filter(std::fs::Metadata::is_file)?;
            let modified = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |elapsed| elapsed.as_secs());
            Some(Entry {
                name: item.file_name().to_string_lossy().into_owned(),
                title: header_title(&path).unwrap_or_default(),
                size: metadata.len(),
                modified,
                path,
            })
        })
        .collect();
    entries.sort_by(|a, b| b.modified.cmp(&a.modified).then(b.name.cmp(&a.name)));
    entries
}

/// Deletes the recording `path` in `folder` (blocking). Refused for any other file, and while
/// it is being recorded.
///
/// # Errors
///
/// Why it wasn't deleted.
pub fn remove_in(folder: &Path, path: &Path) -> Result<(), String> {
    let inside = path.parent() == Some(folder)
        && path.extension().and_then(|ext| ext.to_str()) == Some(EXTENSION);
    if !inside {
        return Err(format!("{} isn't a recording", path.display()));
    }
    if files().iter().any(|file| file == path) {
        return Err(format!("{} is being recorded", path.display()));
    }
    std::fs::remove_file(path).map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use opensesh_term::backend::TermSize;

    use super::*;

    fn temp_folder(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("opensesh-recordings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn file_names_carry_the_time_and_a_safe_name() {
        // 2026-09-27 10:11:12 UTC.
        let seconds = 1_790_503_872;
        assert_eq!(
            file_name("web 01/prod", seconds, 1),
            "2026-09-27_10-11-12_web_01_prod.cast"
        );
        assert_eq!(
            file_name("", seconds, 2),
            "2026-09-27_10-11-12_session-2.cast"
        );
    }

    #[test]
    fn a_pane_is_recorded_listed_and_deleted() {
        let folder = temp_folder("pane");
        let pane = 2_000_001;
        registry::open_player(
            pane,
            folder.join("none.cast"),
            registry::LocalOptions {
                size: TermSize::new(20, 4),
                palette: opensesh_term::palette::Palette::default(),
                options: opensesh_term::session::SessionOptions::default(),
                term: "xterm-256color".to_owned(),
                directory: String::new(),
                program: None,
            },
        )
        .unwrap();
        let path = start_in(&folder, pane, "web 01", "xterm-256color").unwrap();
        assert!(path.starts_with(&folder));
        assert!(panes().contains(&pane));
        assert!(files().contains(&path));
        assert!(start_in(&folder, pane, "web 01", "xterm-256color").is_err());
        assert!(remove_in(&folder, &path).is_err(), "not while it records");
        assert_eq!(stop(pane), Some(path.clone()));
        assert_eq!(stop(pane), None);
        // The writer finishes once the engine drops the tap.
        let deadline = Instant::now() + Duration::from_secs(5);
        while header_title(&path).is_none() {
            assert!(Instant::now() < deadline, "the header is written");
            std::thread::sleep(Duration::from_millis(10));
        }
        let listed = list(&folder);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title, "web 01");
        assert!(remove_in(&folder, &folder.join("other.txt")).is_err());
        remove_in(&folder, &path).unwrap();
        assert!(list(&folder).is_empty());
        registry::close(pane);
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
