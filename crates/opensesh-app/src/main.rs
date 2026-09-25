//! OpenSesh desktop application.

// Release builds on Windows are GUI programs: no console window. Logs still go to the log file.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod bridge;
mod cli;
mod crash;
mod gui;
mod logging;
mod platform;
mod services;

use std::io::Write as _;
use std::process::ExitCode;

use anyhow::Result;
use cli::{Mode, Options};
use opensesh_core::{AppPaths, identity};

use crate::bridge::app_info::{self, Startup};
use crate::crash::DialogPolicy;
use crate::logging::LogGuard;

fn main() -> ExitCode {
    platform::attach_parent_console();

    // Write errors are ignored on purpose: a closed pipe must not turn `--version` into a panic.
    let options = match cli::parse(std::env::args_os().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "opensesh-app: {error}\n\n{}", cli::USAGE);
            return ExitCode::from(2);
        }
    };
    match options.mode {
        Mode::Version => {
            let _ = writeln!(
                std::io::stdout(),
                "{} {}",
                identity::APP_NAME,
                identity::VERSION
            );
            return ExitCode::SUCCESS;
        }
        Mode::Help => {
            let _ = writeln!(std::io::stdout(), "{}", cli::USAGE);
            return ExitCode::SUCCESS;
        }
        Mode::App | Mode::Gallery | Mode::CrashReport(_) => {}
    }

    // Owned here, not inside `run`, so the file log is still alive when an error from `run` is
    // logged below.
    let mut log_guard: Option<LogGuard> = None;
    match run(options, &mut log_guard) {
        Ok(code) => code,
        Err(error) => {
            let message = format!("{error:#}");
            if log_guard.is_some() {
                // Goes to stderr as well, through the stderr layer.
                tracing::error!("{message}");
            } else {
                let _ = writeln!(std::io::stderr(), "opensesh-app: {message}");
            }
            platform::show_fatal_error(&message);
            ExitCode::FAILURE
        }
    }
}

fn run(options: Options, log_guard: &mut Option<LogGuard>) -> Result<ExitCode> {
    let paths = AppPaths::resolve()?;
    paths.ensure_dirs()?;
    let logs_dir = paths.logs_dir();
    *log_guard = Some(logging::init(&logs_dir)?);

    let dialog =
        if options.mode == Mode::App && !options.smoke_test && options.screenshot_dir.is_none() {
            DialogPolicy::Spawn
        } else {
            DialogPolicy::Never
        };
    crash::install(logs_dir.clone(), dialog);

    tracing::info!(
        version = identity::VERSION,
        portable = paths.is_portable(),
        config_dir = %paths.config_dir().display(),
        data_dir = %paths.data_dir().display(),
        ?options,
        "starting {}",
        identity::APP_NAME
    );

    let initial_language = opensesh_core::config::load_file(
        &paths.config_dir().join(opensesh_core::config::CONFIG_FILE),
    )
    .map(|loaded| loaded.config.general.language)
    .unwrap_or_else(|_| "system".to_owned());

    let gallery = options.mode == Mode::Gallery;
    let (qml, crash_report) = match options.mode {
        Mode::CrashReport(report) => {
            let text = crash::read_report(&report)?;
            (gui::CRASH_DIALOG_QML, Some((report, text)))
        }
        Mode::App => (gui::MAIN_QML, None),
        Mode::Gallery => (gui::GALLERY_QML, None),
        Mode::Version | Mode::Help => return Ok(ExitCode::SUCCESS),
    };
    if let Some(dir) = &options.screenshot_dir {
        std::fs::create_dir_all(dir)?;
    }
    app_info::set_startup(Startup {
        smoke_test: options.smoke_test,
        gallery,
        screenshot_dir: options.screenshot_dir.clone(),
        logs_dir,
        config_dir: paths.config_dir().to_path_buf(),
        crash_report,
    });
    services::init(paths)?;

    let result = gui::run(qml, &initial_language);
    // Settings and UI state may still be waiting in the writer's debounce window.
    services::flush();
    let code = result?;
    tracing::info!(code, "event loop finished");

    if code == 0 && options.smoke_test {
        let warnings = bridge::shim::qml_warning_count();
        if warnings > 0 {
            tracing::error!(
                warnings,
                "smoke test: QML produced warnings (see the log above)"
            );
            return Ok(ExitCode::from(SMOKE_QML_WARNINGS));
        }
    }
    Ok(exit_code(code))
}

/// Smoke-test exit code when the QML ran but logged warnings.
const SMOKE_QML_WARNINGS: u8 = 6;

/// Maps the Qt event loop result to a process exit code.
fn exit_code(code: i32) -> ExitCode {
    u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_loop_codes_map_to_process_codes() {
        assert_eq!(exit_code(0), ExitCode::SUCCESS);
        assert_eq!(exit_code(3), ExitCode::from(3));
        assert_eq!(exit_code(-1), ExitCode::FAILURE);
        assert_eq!(exit_code(1000), ExitCode::FAILURE);
    }
}
