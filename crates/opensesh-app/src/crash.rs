//! Panic hook: logs the panic, writes a crash report next to the logs and shows a crash dialog.
//!
//! The dialog runs in a **separate process** (`opensesh-app --crash-report <file>`) because the
//! crashing process may be in any state, and panics that cross the Qt/Rust FFI boundary abort
//! right after the hook returns. The dialog is plain QML, so no QtWidgets is needed.
//!
//! Only the first panic of a process writes a new report and opens the dialog. A panic inside a
//! cxx/cxx-qt callback always triggers a second one (cxx's unwind guard panics with "panic in ffi
//! function ..., aborting"); later panics are appended to the first report so the root cause is
//! never overwritten.
//!
//! Qt fatal messages (`qFatal`, e.g. a missing platform plugin) are not panics, but Qt aborts right
//! after logging them, before the asynchronous file log is written. [`record_qt_fatal`] writes
//! them synchronously as a crash report (without a dialog).

use std::any::Any;
use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use opensesh_core::identity;

/// Set to `1` to never spawn the crash dialog (CI, tests, headless runs). The dialog process
/// itself runs with it set, so a crash inside the dialog can't loop.
pub const NO_DIALOG_ENV: &str = "OPENSESH_NO_CRASH_DIALOG";

/// Largest crash report the dialog will load.
const MAX_REPORT_BYTES: u64 = 1024 * 1024;

/// Where reports go; set once by [`install`].
static LOGS_DIR: OnceLock<PathBuf> = OnceLock::new();
/// Distinguishes reports written in the same second by the same process.
static REPORT_SEQ: AtomicU32 = AtomicU32::new(0);
/// Set by the first panic of the process.
static PANICKED: AtomicBool = AtomicBool::new(false);
/// Report written for the first panic; later panics are appended to it.
static FIRST_REPORT: OnceLock<PathBuf> = OnceLock::new();

/// Whether a panic should open the crash dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogPolicy {
    /// Spawn the dialog process (unless disabled with [`NO_DIALOG_ENV`]).
    Spawn,
    /// Only log and write the report.
    Never,
}

/// Installs the panic hook. The previous (default) hook still runs afterwards, so the panic is
/// also printed to stderr.
pub fn install(logs_dir: PathBuf, policy: DialogPolicy) {
    if LOGS_DIR.set(logs_dir).is_err() {
        tracing::warn!("crash reporting was already installed");
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = payload_message(info.payload());
        let location = info.location().map_or_else(
            || "unknown location".to_owned(),
            |l| format!("{}:{}:{}", l.file(), l.line(), l.column()),
        );
        let thread = std::thread::current()
            .name()
            .unwrap_or("<unnamed>")
            .to_owned();
        tracing::error!(target: "opensesh::panic", %thread, %location, "panic: {message}");

        let first_panic = !PANICKED.swap(true, Ordering::SeqCst);
        match FIRST_REPORT.get() {
            Some(report) if !first_panic => append_followup(report, &message, &location, &thread),
            // Without a first report (not written yet by another thread, or writing failed),
            // write a new one, but only the first panic may open the dialog.
            _ => write_panic_report(&message, &location, &thread, first_panic, policy),
        }
        previous(info);
    }));
}

/// Writes a Qt fatal message synchronously as a crash report, because Qt aborts the process as
/// soon as the message handler returns. Does nothing before [`install`].
pub fn record_qt_fatal(category: &str, message: &str) {
    let Some(logs_dir) = LOGS_DIR.get() else {
        return;
    };
    let location = if category.is_empty() {
        "Qt".to_owned()
    } else {
        format!("Qt ({category})")
    };
    let report = render_report(&ReportDetails {
        kind: "Qt fatal error",
        message,
        location: &location,
        thread: std::thread::current().name().unwrap_or("<unnamed>"),
        backtrace: "(not captured for Qt fatal errors)",
        unix_time: unix_time(),
    });
    if let Err(error) = write_new_report(logs_dir, &report) {
        tracing::error!(target: "opensesh::panic", "could not write the crash report: {error:#}");
    }
}

/// Reads a crash report for the dialog, with a size limit and lossy UTF-8 decoding.
///
/// # Errors
///
/// Fails if the file can't be opened or read.
pub fn read_report(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("opening crash report {}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(MAX_REPORT_BYTES)
        .read_to_end(&mut bytes)
        .with_context(|| format!("reading crash report {}", path.display()))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn write_panic_report(
    message: &str,
    location: &str,
    thread: &str,
    first_panic: bool,
    policy: DialogPolicy,
) {
    let Some(logs_dir) = LOGS_DIR.get() else {
        return;
    };
    let backtrace = std::backtrace::Backtrace::force_capture().to_string();
    let report = render_report(&ReportDetails {
        kind: "panic",
        message,
        location,
        thread,
        backtrace: &backtrace,
        unix_time: unix_time(),
    });
    match write_new_report(logs_dir, &report) {
        Ok(path) => {
            tracing::error!(target: "opensesh::panic", report = %path.display(), "crash report written");
            if first_panic && policy == DialogPolicy::Spawn && !dialog_disabled_by_env() {
                spawn_dialog(&path);
            }
            // Keeps the first report that was written; a racing thread's report stays separate.
            let _ = FIRST_REPORT.set(path);
        }
        Err(error) => {
            tracing::error!(target: "opensesh::panic", "could not write the crash report: {error:#}");
        }
    }
}

/// Adds a later panic of the same process to the first report.
fn append_followup(report: &Path, message: &str, location: &str, thread: &str) {
    let mut section = String::new();
    // Writing to a String can't fail.
    let _ = writeln!(section, "\nfollowed by another panic:");
    let _ = writeln!(section, "  thread: {thread}");
    let _ = writeln!(section, "  location: {location}");
    let _ = writeln!(section, "  message: {message}");
    let result = OpenOptions::new()
        .append(true)
        .open(report)
        .and_then(|mut file| file.write_all(section.as_bytes()));
    if let Err(error) = result {
        tracing::error!(target: "opensesh::panic", "could not update the crash report: {error}");
    }
}

/// Extracts the message of a panic payload (`&str` or `String`, as produced by `panic!`).
fn payload_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "<non-string panic payload>".to_owned()
    }
}

/// Everything a crash report contains.
struct ReportDetails<'a> {
    kind: &'a str,
    message: &'a str,
    location: &'a str,
    thread: &'a str,
    backtrace: &'a str,
    unix_time: u64,
}

fn render_report(details: &ReportDetails<'_>) -> String {
    let mut report = String::new();
    // Writing to a String can't fail.
    let _ = writeln!(
        report,
        "{} {} crash report ({})",
        identity::APP_NAME,
        identity::VERSION,
        details.kind
    );
    let _ = writeln!(report, "time (unix): {}", details.unix_time);
    let _ = writeln!(
        report,
        "platform: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let _ = writeln!(report, "thread: {}", details.thread);
    let _ = writeln!(report, "location: {}", details.location);
    let _ = writeln!(report, "message: {}", details.message);
    let _ = writeln!(report, "\nbacktrace:\n{}", details.backtrace);
    report
}

/// Writes `report` to a new `crash-<unix time>-<pid>-<seq>.txt` file. Existing files are never
/// overwritten.
fn write_new_report(logs_dir: &Path, report: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(logs_dir)
        .with_context(|| format!("creating {}", logs_dir.display()))?;
    let path = logs_dir.join(format!(
        "crash-{}-{}-{}.txt",
        unix_time(),
        std::process::id(),
        REPORT_SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .with_context(|| format!("creating {}", path.display()))?;
    file.write_all(report.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

fn unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn dialog_disabled_by_env() -> bool {
    std::env::var_os(NO_DIALOG_ENV).is_some_and(|value| !value.is_empty() && value != "0")
}

fn spawn_dialog(report: &Path) {
    let result = std::env::current_exe().and_then(|exe| {
        Command::new(exe)
            .arg("--crash-report")
            .arg(report)
            .env(NO_DIALOG_ENV, "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    });
    if let Err(error) = result {
        tracing::error!(target: "opensesh::panic", "could not open the crash dialog: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn details<'a>(message: &'a str, backtrace: &'a str) -> ReportDetails<'a> {
        ReportDetails {
            kind: "panic",
            message,
            location: "src/main.rs:1:2",
            thread: "main",
            backtrace,
            unix_time: 1234,
        }
    }

    #[test]
    fn payload_messages_are_extracted() {
        let static_str: Box<dyn Any + Send> = Box::new("boom");
        assert_eq!(payload_message(static_str.as_ref()), "boom");
        let owned: Box<dyn Any + Send> = Box::new(String::from("kaboom"));
        assert_eq!(payload_message(owned.as_ref()), "kaboom");
        let other: Box<dyn Any + Send> = Box::new(42_u8);
        assert_eq!(
            payload_message(other.as_ref()),
            "<non-string panic payload>"
        );
    }

    #[test]
    fn report_contains_the_essentials() {
        let report = render_report(&details("boom", "<frames>"));
        assert!(report.starts_with(&format!(
            "OpenSesh {} crash report (panic)",
            identity::VERSION
        )));
        for needle in [
            "time (unix): 1234",
            "thread: main",
            "location: src/main.rs:1:2",
            "message: boom",
            "<frames>",
        ] {
            assert!(report.contains(needle), "missing {needle:?} in {report}");
        }
    }

    #[test]
    fn reports_never_overwrite_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path().join("logs");
        let first = write_new_report(&logs, "root cause").unwrap();
        let second = write_new_report(&logs, "consequence").unwrap();
        assert_ne!(first, second);
        assert!(first.starts_with(&logs));
        assert!(
            first
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("crash-")
        );
        assert_eq!(read_report(&first).unwrap(), "root cause");
        assert_eq!(read_report(&second).unwrap(), "consequence");
    }

    #[test]
    fn followup_panics_are_appended_after_the_root_cause() {
        let dir = tempfile::tempdir().unwrap();
        let report = write_new_report(dir.path(), "message: root cause\n").unwrap();
        append_followup(
            &report,
            "panic in ffi function knock, aborting.",
            "cxx/src/unwind.rs:1:1",
            "main",
        );
        let text = read_report(&report).unwrap();
        let root = text.find("root cause").unwrap();
        let followup = text.find("followed by another panic").unwrap();
        assert!(root < followup);
        assert!(text.contains("message: panic in ffi function knock, aborting."));
    }

    #[test]
    fn huge_reports_are_truncated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.txt");
        let len = usize::try_from(MAX_REPORT_BYTES).unwrap() + 10;
        std::fs::write(&path, "x".repeat(len)).unwrap();
        assert_eq!(read_report(&path).unwrap().len() as u64, MAX_REPORT_BYTES);
    }
}
