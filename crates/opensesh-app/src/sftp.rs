//! SFTP in the app (Sprint 8, ADR 0028), without Qt: the transfer queue every window shares, the
//! file systems of the open file panes (by id, so a transfer names its two panes), opening a
//! server's files (on a terminal's connection, or on a connection of their own), editing remote
//! files, and the `[sftp]` settings.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock, PoisonError, RwLock};

use opensesh_core::config::{SftpSettings, TransferPolicy};
use opensesh_ssh::SshError;
use opensesh_ssh::connect::{self, Connection, Notes};
use opensesh_ssh::prompt::Asker;
use opensesh_ssh::sftp::Fs;
use opensesh_ssh::sftp::remote::Remote;
use opensesh_ssh::sftp::transfer::{Event, Options, Policy, Queue};
use opensesh_term::backend::TermSize;

static SETTINGS: LazyLock<RwLock<SftpSettings>> =
    LazyLock::new(|| RwLock::new(SftpSettings::default()));

/// The `[sftp]` settings as last applied.
#[must_use]
pub fn settings() -> SftpSettings {
    SETTINGS
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Applies the `[sftp]` settings (the queue's parallel limit, the options of new transfers).
pub fn apply_settings(settings: &SftpSettings) {
    *SETTINGS.write().unwrap_or_else(PoisonError::into_inner) = settings.clone();
    if let Some(queue) = queue() {
        queue.set_parallel(usize::try_from(settings.parallel).unwrap_or(3));
    }
}

fn policy(policy: TransferPolicy) -> Policy {
    match policy {
        TransferPolicy::Ask => Policy::Ask,
        TransferPolicy::Overwrite => Policy::Overwrite,
        TransferPolicy::Newer => Policy::OverwriteIfNewer,
        TransferPolicy::Resume => Policy::Resume,
        TransferPolicy::Skip => Policy::Skip,
        TransferPolicy::Rename => Policy::Rename,
    }
}

/// The options of a new transfer, from the settings.
#[must_use]
pub fn options() -> Options {
    let settings = settings();
    Options {
        policy: policy(settings.policy),
        preserve_times: settings.preserve_times,
        preserve_permissions: settings.preserve_permissions,
    }
}

type Sink = Box<dyn Fn(Event) + Send + Sync>;

static SINK: Mutex<Option<Sink>> = Mutex::new(None);

/// Where the queue's events go (the `Transfers` singleton, which moves them to the GUI thread).
pub fn set_sink(sink: Sink) {
    *SINK.lock().unwrap_or_else(PoisonError::into_inner) = Some(sink);
}

static QUEUE: OnceLock<Option<Queue>> = OnceLock::new();

/// The transfer queue (started on first use); `None` without the SSH runtime.
#[must_use]
pub fn queue() -> Option<&'static Queue> {
    QUEUE
        .get_or_init(|| {
            let runtime = opensesh_ssh::runtime()?;
            let parallel = usize::try_from(settings().parallel).unwrap_or(3);
            Some(Queue::new(
                runtime.handle().clone(),
                parallel,
                Arc::new(|event| {
                    if let Some(sink) = SINK.lock().unwrap_or_else(PoisonError::into_inner).as_ref()
                    {
                        sink(event);
                    }
                }),
            ))
        })
        .as_ref()
}

static PANES: LazyLock<Mutex<HashMap<i32, Fs>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT_PANE: AtomicI32 = AtomicI32::new(1);

/// A new id for a file pane.
#[must_use]
pub fn new_pane_id() -> i32 {
    NEXT_PANE.fetch_add(1, Ordering::Relaxed)
}

/// Records the file system pane `id` shows (again after a reconnection).
pub fn register(id: i32, fs: Fs) {
    PANES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(id, fs);
}

/// Forgets pane `id` (closed, or disconnected).
pub fn unregister(id: i32) {
    PANES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&id);
}

/// The file system pane `id` shows.
#[must_use]
pub fn pane(id: i32) -> Option<Fs> {
    PANES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&id)
        .cloned()
}

/// Whose files a remote pane shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A terminal session's connection (its id): no second login.
    Terminal(i32),
    /// A saved host, on a connection of its own.
    Host(String),
    /// Quick-connect text, on a connection of its own.
    Target(String),
}

/// Why a server's files couldn't be opened: a code for the UI and a technical detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenError {
    /// `no-session`, `no-sftp`, `invalid`, or an [`SshError::code`].
    pub code: &'static str,
    /// What went wrong.
    pub detail: String,
}

impl From<SshError> for OpenError {
    fn from(error: SshError) -> Self {
        Self {
            code: match error {
                SshError::Refused { .. } => "no-sftp",
                _ => error.code(),
            },
            detail: error.to_string(),
        }
    }
}

/// Opens the files of `source`. A connection of its own asks `asker` its questions (host keys,
/// passwords) and tells `notes` how it goes.
///
/// # Errors
///
/// [`OpenError`] when there is no connection, no SFTP, or connecting failed.
pub async fn open_remote(
    source: Source,
    asker: &Asker,
    notes: &Notes,
) -> Result<Remote, OpenError> {
    let connection: Arc<Connection> = match source {
        Source::Terminal(id) => crate::terminal::registry::get(id)
            .and_then(|entry| entry.ssh_connection())
            .ok_or_else(|| OpenError {
                code: "no-session",
                detail: "the terminal isn't connected".to_owned(),
            })?,
        Source::Host(_) | Source::Target(_) => {
            // The terminal's settings don't matter to a connection that only carries files.
            let size = TermSize::new(80, 24);
            let start = match &source {
                Source::Host(id) => crate::ssh::for_host(id, size, "xterm-256color"),
                Source::Target(text) => crate::ssh::for_target(text, size, "xterm-256color"),
                Source::Terminal(_) => Err(String::new()),
            }
            .map_err(|detail| OpenError {
                code: "invalid",
                detail,
            })?;
            Arc::new(connect::connect(&start.connect, asker, notes).await?)
        }
    };
    Ok(Remote::open(connection).await?)
}

/// A private folder for the copies of files being edited (under the cache folder, `0700` on
/// Linux): one sub-folder per edit, so two files of the same name don't meet.
///
/// # Errors
///
/// When the folder can't be created.
pub fn edit_dir(serial: u64) -> std::io::Result<PathBuf> {
    let base = crate::services::get()
        .map(|services| services.paths.cache_dir().join("edit"))
        .unwrap_or_else(|| std::env::temp_dir().join("opensesh-edit"));
    let dir = base.join(serial.to_string());
    std::fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(dir)
}

/// `command` split into words (single and double quotes group words, and inside double quotes a
/// backslash escapes a quote only, so Windows paths keep theirs), with `{file}` replaced by
/// `file`; when there is no `{file}`, the file goes last.
#[must_use]
pub fn editor_command(command: &str, file: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    let mut chars = command.chars().peekable();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') if chars.peek() == Some(&'"') => {
                chars.next();
                word.push('"');
            }
            (Some(_), c) => word.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                in_word = true;
            }
            (None, c) if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            (None, c) => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        words.push(word);
    }
    let mut placed = false;
    for word in &mut words {
        if word.contains("{file}") {
            *word = word.replace("{file}", file);
            placed = true;
        }
    }
    if !placed && !words.is_empty() {
        words.push(file.to_owned());
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_commands() {
        assert_eq!(
            editor_command("code --wait", "/t/a b.txt"),
            ["code", "--wait", "/t/a b.txt"]
        );
        assert_eq!(
            editor_command(
                r#""C:\Program Files\Notepad++\notepad++.exe" -multiInst "{file}""#,
                r"C:\t\x.txt"
            ),
            [
                r"C:\Program Files\Notepad++\notepad++.exe",
                "-multiInst",
                r"C:\t\x.txt"
            ]
        );
        assert_eq!(
            editor_command("vim '+set nu' {file}", "f"),
            ["vim", "+set nu", "f"]
        );
        assert_eq!(
            editor_command(r#"say "a \"b\"""#, "f"),
            ["say", r#"a "b""#, "f"]
        );
        assert!(editor_command("   ", "f").is_empty());
    }

    #[test]
    fn panes_by_id() {
        let id = new_pane_id();
        assert!(pane(id).is_none());
        register(id, Fs::local());
        assert!(pane(id).is_some());
        unregister(id);
        assert!(pane(id).is_none());
    }
}
