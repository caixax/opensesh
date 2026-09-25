//! Command-line arguments of the GUI binary.
//!
//! Qt parses its own options (`-platform`, `-style`, ...) from the same argument list, so
//! unknown arguments are ignored here instead of rejected.

use std::ffi::OsString;
use std::path::PathBuf;

/// What the process should show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// The regular application window.
    App,
    /// The component gallery (every `Os*` component in every state).
    Gallery,
    /// Show the crash dialog for a report written by the panic hook of another process.
    CrashReport(PathBuf),
    /// Print the version and exit.
    Version,
    /// Print the usage and exit.
    Help,
}

/// Parsed command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// What to show.
    pub mode: Mode,
    /// Render one frame, run the built-in checks and exit (for CI, usually with
    /// `QT_QPA_PLATFORM=offscreen`). Works with the main window, the gallery and the crash
    /// dialog.
    pub smoke_test: bool,
    /// Save screenshots of the window in every theme/density combination to this folder, then
    /// quit (main window and gallery).
    pub screenshot_dir: Option<PathBuf>,
}

/// Command-line usage text.
pub const USAGE: &str = "\
Usage: opensesh-app [OPTIONS] [QT OPTIONS]

Options:
  --smoke-test           Render one frame, run the built-in checks and exit (for CI);
                         combine with QT_QPA_PLATFORM=offscreen on headless machines
  --gallery              Show the component gallery instead of the main window
  --screenshots <DIR>    Save screenshots in every theme and density to DIR, then exit
  --crash-report <FILE>  Show the crash dialog for FILE (used internally by the panic hook)
  -V, --version          Print the version
  -h, --help             Print this help

Environment:
  OPENSESH_LOG           Log filter, e.g. `debug` or `opensesh_app=trace` (default: info)
  OPENSESH_NO_CRASH_DIALOG=1  Never spawn the crash dialog (headless runs)
  QT_QPA_PLATFORM        Qt platform plugin: wayland, xcb, windows, offscreen...";

/// Error for malformed OpenSesh options.
#[derive(Debug, PartialEq, Eq)]
pub struct CliError(pub String);

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CliError {}

/// Parses the arguments after the program name.
///
/// # Errors
///
/// Fails if `--crash-report` has no file argument.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Options, CliError> {
    let mut options = Options {
        mode: Mode::App,
        smoke_test: false,
        screenshot_dir: None,
    };
    // The first of `--version` / `--help` wins over everything else.
    let mut print: Option<Mode> = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--smoke-test") => options.smoke_test = true,
            Some("--gallery") => options.mode = Mode::Gallery,
            Some("--screenshots") => {
                let dir = args
                    .next()
                    .ok_or_else(|| CliError("--screenshots needs a folder argument".to_owned()))?;
                options.screenshot_dir = Some(PathBuf::from(dir));
            }
            Some("--crash-report") => {
                let file = args
                    .next()
                    .ok_or_else(|| CliError("--crash-report needs a file argument".to_owned()))?;
                options.mode = Mode::CrashReport(PathBuf::from(file));
            }
            Some("-V" | "--version") => print = print.or(Some(Mode::Version)),
            Some("-h" | "--help") => print = print.or(Some(Mode::Help)),
            // Anything else belongs to Qt (or is ignored by it).
            _ => {}
        }
    }
    Ok(match print {
        Some(mode) => Options {
            mode,
            smoke_test: false,
            screenshot_dir: None,
        },
        None => options,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(args: &[&str]) -> Result<Options, CliError> {
        parse(args.iter().map(OsString::from))
    }

    fn options(mode: Mode, smoke_test: bool) -> Options {
        Options {
            mode,
            smoke_test,
            screenshot_dir: None,
        }
    }

    #[test]
    fn no_arguments_run_the_app() {
        assert_eq!(parse_strs(&[]), Ok(options(Mode::App, false)));
    }

    #[test]
    fn qt_options_are_ignored() {
        assert_eq!(
            parse_strs(&["-platform", "xcb", "--smoke-test"]),
            Ok(options(Mode::App, true))
        );
    }

    #[test]
    fn crash_report_takes_a_file_and_combines_with_smoke_test() {
        let report = PathBuf::from("/tmp/crash.txt");
        assert_eq!(
            parse_strs(&["--crash-report", "/tmp/crash.txt"]),
            Ok(options(Mode::CrashReport(report.clone()), false))
        );
        assert_eq!(
            parse_strs(&["--smoke-test", "--crash-report", "/tmp/crash.txt"]),
            Ok(options(Mode::CrashReport(report), true))
        );
        assert!(parse_strs(&["--crash-report"]).is_err());
    }

    #[test]
    fn gallery_and_screenshots() {
        assert_eq!(
            parse_strs(&["--gallery", "--smoke-test"]),
            Ok(options(Mode::Gallery, true))
        );
        let parsed = parse_strs(&["--screenshots", "shots"]).unwrap();
        assert_eq!(parsed.mode, Mode::App);
        assert_eq!(parsed.screenshot_dir, Some(PathBuf::from("shots")));
        assert!(parse_strs(&["--screenshots"]).is_err());
    }

    #[test]
    fn version_and_help_win() {
        assert_eq!(
            parse_strs(&["--smoke-test", "-V"]),
            Ok(options(Mode::Version, false))
        );
        assert_eq!(parse_strs(&["--help"]), Ok(options(Mode::Help, false)));
        assert_eq!(
            parse_strs(&["--help", "--crash-report", "x", "--version"]),
            Ok(options(Mode::Help, false))
        );
    }
}
