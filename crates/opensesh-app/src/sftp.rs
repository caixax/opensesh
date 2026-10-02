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

/// Opens the files of `source`: a server's ([`open_remote`]) or S3 storage's (an S3 host or an
/// `s3://` target, [`crate::s3::open`]), and where to start when the source says so (an `s3://`
/// path; empty otherwise).
///
/// # Errors
///
/// [`OpenError`] as for [`open_remote`] and [`crate::s3::open`].
pub async fn open_files(
    source: Source,
    asker: &Asker,
    notes: &Notes,
) -> Result<(Fs, String), OpenError> {
    let s3 = match &source {
        Source::Host(id) => crate::s3::host_of(Some(id), None),
        Source::Target(text) => crate::s3::host_of(None, Some(text)),
        Source::Terminal(_) => None,
    };
    if let Some((host, start)) = s3 {
        let fs = crate::s3::open(&host, &start, asker).await?;
        return Ok((fs, start));
    }
    let remote = open_remote(source, asker, notes).await?;
    Ok((Fs::Remote(Arc::new(remote)), String::new()))
}

/// The mark that says the shell integration is in an rc file.
const INTEGRATION_MARK: &str = "opensesh shell integration";

/// A few lines for a shell's rc file that report the current folder to the terminal (OSC 7)
/// before each prompt: `bash` or `zsh`.
#[must_use]
pub fn shell_integration(shell: &str) -> &'static str {
    match shell {
        "zsh" => concat!(
            "# opensesh shell integration: tell the terminal the current folder (OSC 7)\n",
            r#"__opensesh_osc7() { printf '\033]7;file://%s%s\033\\' "$HOST" "$PWD"; }"#,
            "\n",
            "precmd_functions+=(__opensesh_osc7)\n",
        ),
        _ => concat!(
            "# opensesh shell integration: tell the terminal the current folder (OSC 7)\n",
            r#"__opensesh_osc7() { printf '\033]7;file://%s%s\033\\' "${HOSTNAME:-$(hostname)}" "$PWD"; }"#,
            "\n",
            r#"PROMPT_COMMAND="__opensesh_osc7${PROMPT_COMMAND:+;$PROMPT_COMMAND}""#,
            "\n",
        ),
    }
}

/// The command that appends [`shell_integration`] to the user's `~/.bashrc` or `~/.zshrc` on
/// the server, unless it is there already.
#[must_use]
pub fn install_integration_command(shell: &str) -> String {
    let file = if shell == "zsh" {
        "\"$HOME/.zshrc\""
    } else {
        "\"$HOME/.bashrc\""
    };
    format!(
        "grep -qs {mark} {file} || printf '\\n%s' {snippet} >> {file}",
        mark = opensesh_ssh::sftp::path::shell_quote(INTEGRATION_MARK),
        snippet = opensesh_ssh::sftp::path::shell_quote(shell_integration(shell)),
    )
}

/// A private folder for the copies of files being edited (under the cache folder, `0700` on
/// Linux): one sub-folder per edit, so two files of the same name don't meet.
///
/// # Errors
///
/// When the folder can't be created.
pub fn edit_dir(serial: u64) -> std::io::Result<PathBuf> {
    // A test run leaves the cache alone: its copies go in its own folder.
    let base = if let Some(folder) = crate::bridge::app_info::test_run_folder() {
        folder.join("edit")
    } else {
        crate::services::get()
            .map(|services| services.paths.cache_dir().join("edit"))
            .unwrap_or_else(|| std::env::temp_dir().join("opensesh-edit"))
    };
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
    fn shell_integration_snippets() {
        let bash = shell_integration("bash");
        assert!(bash.contains(INTEGRATION_MARK));
        assert!(bash.contains(r"printf '\033]7;file://%s%s\033\\'"));
        assert!(bash.contains("PROMPT_COMMAND=\"__opensesh_osc7"));
        assert!(shell_integration("zsh").contains("precmd_functions+=(__opensesh_osc7)"));
        let command = install_integration_command("bash");
        assert!(
            command
                .starts_with("grep -qs 'opensesh shell integration' \"$HOME/.bashrc\" || printf")
        );
        assert!(command.ends_with(">> \"$HOME/.bashrc\""));
    }

    /// The install command, run twice by `sh` in a temporary home, adds the snippet once; an
    /// interactive shell then reports each folder it goes to (OSC 7). For each shell that is
    /// installed here.
    #[cfg(unix)]
    #[test]
    fn shell_integration_in_real_shells() {
        use std::io::Write as _;
        use std::process::{Command, Stdio};
        for shell in ["bash", "zsh"] {
            let installed = Command::new(shell)
                .args(["-c", "true"])
                .status()
                .is_ok_and(|status| status.success());
            if !installed {
                continue;
            }
            let home = tempfile::tempdir().unwrap();
            for _ in 0..2 {
                let status = Command::new("sh")
                    .args(["-c", &install_integration_command(shell)])
                    .env("HOME", home.path())
                    .status()
                    .unwrap();
                assert!(status.success(), "{shell}");
            }
            let rc = home
                .path()
                .join(if shell == "zsh" { ".zshrc" } else { ".bashrc" });
            let text = std::fs::read_to_string(&rc).unwrap();
            assert_eq!(text.matches(INTEGRATION_MARK).count(), 1, "{text}");
            assert!(text.ends_with(shell_integration(shell)), "{text}");

            let folder = home.path().join("a folder");
            std::fs::create_dir(&folder).unwrap();
            // zsh without its line editor (+Z) reads the commands from the pipe even when there
            // is a terminal.
            let args: &[&str] = if shell == "zsh" {
                &["-i", "+Z"]
            } else {
                &["-i"]
            };
            let mut child = Command::new(shell)
                .args(args)
                .env("HOME", home.path())
                .current_dir(home.path())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(b"cd 'a folder'\nexit\n")
                .unwrap();
            // An interactive shell ignores SIGTERM: one that hangs is killed.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
            while child.try_wait().unwrap().is_none() {
                if std::time::Instant::now() > deadline {
                    child.kill().unwrap();
                    panic!("{shell} -i didn't exit");
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            let mut text = String::new();
            std::io::Read::read_to_string(&mut child.stdout.take().unwrap(), &mut text).unwrap();
            let reported = format!("{}\x1b\\", folder.display());
            assert!(
                text.contains("\x1b]7;file://") && text.contains(&reported),
                "{shell} printed {text:?}"
            );
        }
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
