//! Logging setup: human-readable logs to stderr and to a daily-rotated file in the data
//! directory (`logs/opensesh.YYYY-MM-DD.log`).
//!
//! The filter comes from `OPENSESH_LOG` (same syntax as `RUST_LOG`) and defaults to `info`.
//! Secrets must never reach a log call (PLAN §0 rule 7); redaction helpers arrive with the vault.

use std::io::IsTerminal as _;
use std::path::Path;

use anyhow::{Context, Result};
use tracing_appender::non_blocking::{NonBlockingBuilder, WorkerGuard};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};

/// Environment variable with the log filter.
pub const FILTER_ENV: &str = "OPENSESH_LOG";

/// Filter used when `OPENSESH_LOG` is unset.
const DEFAULT_FILTER: &str = "info";

/// Number of daily log files kept on disk.
const MAX_LOG_FILES: usize = 14;

/// Keeps the background log writer alive; dropping it flushes the file.
#[derive(Debug)]
#[must_use = "dropping the guard stops file logging"]
pub struct LogGuard {
    _file: WorkerGuard,
}

/// Installs the global subscriber.
///
/// # Errors
///
/// Fails if the log file can't be created or a global subscriber is already installed. An
/// invalid `OPENSESH_LOG` value is not fatal: the default filter is used and a warning logged.
pub fn init(logs_dir: &Path) -> Result<LogGuard> {
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("opensesh")
        .filename_suffix("log")
        .max_log_files(MAX_LOG_FILES)
        .build(logs_dir)
        .with_context(|| format!("creating the log file in {}", logs_dir.display()))?;
    // Not lossy: under pressure logging blocks briefly instead of silently dropping lines.
    let (file_writer, file_guard) = NonBlockingBuilder::default()
        .lossy(false)
        .thread_name("opensesh-log")
        .finish(appender);

    let (filter, invalid_filter) = parse_filter(std::env::var(FILTER_ENV).ok().as_deref());

    tracing_subscriber::registry()
        .with(filter)
        .with(
            fmt::layer()
                .with_ansi(std::io::stderr().is_terminal())
                .with_writer(std::io::stderr),
        )
        .with(fmt::layer().with_ansi(false).with_writer(file_writer))
        .try_init()
        .context("installing the global log subscriber")?;

    if let Some(error) = invalid_filter {
        tracing::warn!("ignoring invalid {FILTER_ENV} value, using `{DEFAULT_FILTER}`: {error}");
    }
    Ok(LogGuard { _file: file_guard })
}

/// Builds the filter from the `OPENSESH_LOG` value. A malformed value falls back to the default
/// (a debugging knob must never stop the app); the parse error is returned for logging.
fn parse_filter(directives: Option<&str>) -> (EnvFilter, Option<String>) {
    match directives.map(str::trim) {
        Some(directives) if !directives.is_empty() => match EnvFilter::try_new(directives) {
            Ok(filter) => (filter, None),
            Err(error) => (EnvFilter::new(DEFAULT_FILTER), Some(error.to_string())),
        },
        _ => (EnvFilter::new(DEFAULT_FILTER), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_blank_filter_uses_the_default() {
        for value in [None, Some(""), Some("   ")] {
            let (filter, error) = parse_filter(value);
            assert_eq!(filter.to_string(), DEFAULT_FILTER);
            assert!(error.is_none());
        }
    }

    #[test]
    fn valid_filter_is_used() {
        let (filter, error) = parse_filter(Some("opensesh_app=trace,qt=warn"));
        assert!(error.is_none());
        assert!(filter.to_string().contains("opensesh_app=trace"));
    }

    #[test]
    fn invalid_filter_falls_back_to_the_default() {
        let (filter, error) = parse_filter(Some("opensesh_app=notalevel"));
        assert_eq!(filter.to_string(), DEFAULT_FILTER);
        assert!(error.is_some());
    }
}
