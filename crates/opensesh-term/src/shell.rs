//! Which program a local terminal runs, where, and with which environment (PLAN §6.2,
//! ADR 0012).
//!
//! - **Program:** the user's shell unless the caller names one. Unix: `$SHELL` if executable,
//!   else the passwd entry, else `/bin/sh`, started as a *non-login* shell (like GNOME Terminal,
//!   foot and Alacritty). Windows: the absolute path of `pwsh.exe` found on `PATH`, else Windows
//!   PowerShell, else `%ComSpec%`. Programs are always resolved to an absolute path here: on
//!   Windows `CreateProcessW` doesn't search `PATH` for them.
//! - **Directory:** `$HOME` (Unix) or `%USERPROFILE%` (Windows) unless the caller gives an
//!   existing directory.
//! - **Environment:** the inherited one (on Windows `portable-pty` refreshes it from the
//!   registry, so PATH edits reach new tabs), plus `TERM`, `COLORTERM`, `TERM_PROGRAM`,
//!   `TERM_PROGRAM_VERSION` (and `WSLENV` so WSL sees them, `PWD` on Unix), minus variables that
//!   belong to OpenSesh itself or would break programs in the terminal ([`REMOVED_VARIABLES`]).
//!
//! Building the command reads the environment, the registry (Windows) or the passwd database
//! (Unix): it runs on the PTY backend's own thread, never on the GUI thread.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use portable_pty::CommandBuilder;

/// `TERM` for programs in the terminal (PLAN §6.2 default).
pub const TERM: &str = "xterm-256color";
/// `COLORTERM`: 24-bit color works.
pub const COLORTERM: &str = "truecolor";
/// `TERM_PROGRAM`.
pub const TERM_PROGRAM: &str = "OpenSesh";
/// `TERM_PROGRAM_VERSION`.
pub const TERM_PROGRAM_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Removed from the inherited environment: startup-notification tokens meant for OpenSesh
/// (`XDG_ACTIVATION_TOKEN`, `DESKTOP_STARTUP_ID`), AppImage runtime variables (`APPIMAGE`,
/// `APPDIR`, `OWD`), stale sizes that override the real one in ncurses (`LINES`, `COLUMNS`) and
/// OpenSesh's own debug switches.
pub const REMOVED_VARIABLES: [&str; 10] = [
    "XDG_ACTIVATION_TOKEN",
    "DESKTOP_STARTUP_ID",
    "APPIMAGE",
    "APPDIR",
    "OWD",
    "LINES",
    "COLUMNS",
    "OPENSESH_DEBUG_PANIC",
    "OPENSESH_NO_CRASH_DIALOG",
    "OPENSESH_LOG",
];

/// The variables shared with WSL through `WSLENV` on Windows.
const WSL_SHARED: [&str; 4] = ["TERM", "COLORTERM", "TERM_PROGRAM", "TERM_PROGRAM_VERSION"];

/// What a local terminal runs.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct ShellCommand {
    /// Program to run: an absolute path, or a name looked up in `PATH`. `None` runs the user's
    /// shell (see the module documentation).
    pub program: Option<PathBuf>,
    /// Arguments after the program name.
    pub args: Vec<OsString>,
    /// Working directory; `None` or a missing directory means the home directory.
    pub cwd: Option<PathBuf>,
    /// Extra environment variables, applied last (they win over the defaults).
    pub env: Vec<(OsString, OsString)>,
}

impl std::fmt::Debug for ShellCommand {
    /// Environment values are left out: they may hold secrets.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let env_names: Vec<&OsString> = self.env.iter().map(|(name, _)| name).collect();
        f.debug_struct("ShellCommand")
            .field("program", &self.program)
            .field("args", &self.args)
            .field("cwd", &self.cwd)
            .field("env", &env_names)
            .finish()
    }
}

impl ShellCommand {
    /// The user's shell in the home directory.
    #[must_use]
    pub fn user_shell() -> Self {
        Self::default()
    }

    /// A specific program (absolute path or a name looked up in `PATH`).
    #[must_use]
    pub fn program(program: impl Into<PathBuf>) -> Self {
        Self {
            program: Some(program.into()),
            ..Self::default()
        }
    }

    /// Adds an argument.
    #[must_use]
    pub fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Sets the working directory.
    #[must_use]
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Adds an environment variable.
    #[must_use]
    pub fn env(mut self, name: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.push((name.into(), value.into()));
        self
    }
}

/// Errors while preparing a command.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ShellError {
    /// The program doesn't exist (or isn't executable).
    #[error("could not find the program `{0}`")]
    NotFound(String),
    /// No shell could be found at all.
    #[error("could not find a shell to run")]
    NoShell,
}

/// Builds the `portable-pty` command for `command`, with the program resolved to an absolute
/// path (also returned, for error messages).
pub(crate) fn command_builder(
    command: &ShellCommand,
) -> Result<(CommandBuilder, PathBuf), ShellError> {
    // Loads the inherited environment (and on Windows the registry's): no program yet, so the
    // program and arguments are pushed below instead of calling `arg()`, which panics on a
    // default-program builder.
    let mut builder = CommandBuilder::new_default_prog();
    let program = match &command.program {
        Some(program) => resolve_program(program, &builder)?,
        None => default_shell(&builder).ok_or(ShellError::NoShell)?,
    };
    let argv = builder.get_argv_mut();
    argv.push(program.clone().into_os_string());
    argv.extend(command.args.iter().cloned());

    let cwd = command
        .cwd
        .clone()
        .filter(|dir| dir.is_dir())
        .or_else(|| home_dir(&builder));
    if let Some(cwd) = &cwd {
        builder.cwd(cwd);
    }
    apply_environment(&mut builder);
    if cfg!(unix) {
        if let Some(cwd) = &cwd {
            // Lets the shell keep a symlinked path in `pwd` (as VTE does).
            builder.env("PWD", cwd);
        }
    }
    for (name, value) in &command.env {
        builder.env(name, value);
    }
    Ok((builder, program))
}

/// Sets and removes the variables described in the module documentation.
pub(crate) fn apply_environment(builder: &mut CommandBuilder) {
    for name in REMOVED_VARIABLES {
        builder.env_remove(name);
    }
    // OpenSesh sets this itself to get AltGr working; programs in the terminal must not inherit
    // it. A user's own value is left alone.
    if cfg!(windows)
        && builder
            .get_env("QT_QPA_PLATFORM")
            .is_some_and(|value| value == "windows:altgr")
    {
        builder.env_remove("QT_QPA_PLATFORM");
    }
    builder.env("TERM", TERM);
    builder.env("COLORTERM", COLORTERM);
    builder.env("TERM_PROGRAM", TERM_PROGRAM);
    builder.env("TERM_PROGRAM_VERSION", TERM_PROGRAM_VERSION);
    if cfg!(windows) {
        let existing = builder.get_env("WSLENV");
        // A non-UTF-8 WSLENV is left untouched rather than rebuilt.
        if let Some(existing) = existing.map_or(Some(None), |value| value.to_str().map(Some)) {
            let merged = merge_wslenv(existing);
            builder.env("WSLENV", merged);
        }
    }
}

/// Adds OpenSesh's variables to a `WSLENV` list (`NAME[/flags]` entries separated by `:`),
/// keeping existing entries and not duplicating names (compared case-insensitively).
#[must_use]
pub fn merge_wslenv(existing: Option<&str>) -> String {
    let mut entries: Vec<String> = existing
        .unwrap_or_default()
        .split(':')
        .filter(|entry| !entry.is_empty())
        .map(str::to_owned)
        .collect();
    for name in WSL_SHARED {
        let present = entries.iter().any(|entry| {
            entry
                .split('/')
                .next()
                .is_some_and(|entry_name| entry_name.eq_ignore_ascii_case(name))
        });
        if !present {
            entries.push(name.to_owned());
        }
    }
    entries.join(":")
}

/// The user's shell, as an absolute path.
fn default_shell(builder: &CommandBuilder) -> Option<PathBuf> {
    #[cfg(unix)]
    {
        // `$SHELL` if executable, else the passwd entry, else `/bin/sh` (portable-pty checks
        // X_OK for both).
        Some(PathBuf::from(builder.get_shell()))
    }
    #[cfg(windows)]
    {
        let system_root = builder.get_env("SystemRoot").map(PathBuf::from);
        let system = |relative: &str| {
            system_root
                .as_ref()
                .map(|root| root.join(relative))
                .filter(|path| path.is_file())
        };
        find_in_path(builder, OsStr::new("pwsh.exe"))
            .or_else(|| system(r"System32\WindowsPowerShell\v1.0\powershell.exe"))
            .or_else(|| find_in_path(builder, OsStr::new("powershell.exe")))
            .or_else(|| {
                builder
                    .get_env("ComSpec")
                    .map(PathBuf::from)
                    .filter(|path| path.is_file())
            })
            .or_else(|| system(r"System32\cmd.exe"))
    }
}

/// An absolute path for `program`: absolute paths must exist, relative ones with a directory
/// part are taken from the current directory, bare names are looked up in `PATH`.
fn resolve_program(program: &Path, builder: &CommandBuilder) -> Result<PathBuf, ShellError> {
    let not_found = || ShellError::NotFound(program.display().to_string());
    if program.is_absolute() {
        return is_executable(program)
            .then(|| program.to_path_buf())
            .ok_or_else(not_found);
    }
    if program.components().count() > 1 {
        return std::env::current_dir()
            .ok()
            .map(|dir| dir.join(program))
            .filter(|path| is_executable(path))
            .ok_or_else(not_found);
    }
    find_in_path(builder, program.as_os_str()).ok_or_else(not_found)
}

/// Looks `name` up in the builder's `PATH` (absolute entries only; on Windows also with each
/// `PATHEXT` extension when `name` has none).
fn find_in_path(builder: &CommandBuilder, name: &OsStr) -> Option<PathBuf> {
    let path = builder.get_env("PATH")?;
    let extensions: Vec<OsString> = if cfg!(windows) && Path::new(name).extension().is_none() {
        builder
            .get_env("PATHEXT")
            .unwrap_or(OsStr::new(".COM;.EXE;.BAT;.CMD"))
            .to_string_lossy()
            .split(';')
            .filter(|extension| !extension.is_empty())
            .map(OsString::from)
            .collect()
    } else {
        Vec::new()
    };
    std::env::split_paths(path)
        .filter(|dir| dir.is_absolute())
        .find_map(|dir| {
            let candidate = dir.join(name);
            if is_executable(&candidate) {
                return Some(candidate);
            }
            extensions.iter().find_map(|extension| {
                let mut file_name = name.to_os_string();
                file_name.push(extension);
                let candidate = dir.join(file_name);
                is_executable(&candidate).then_some(candidate)
            })
        })
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    }
    #[cfg(windows)]
    {
        // App Execution Aliases (Store `pwsh.exe`) are zero-length reparse points that report
        // as files, and CreateProcessW accepts them.
        path.is_file()
    }
}

fn home_dir(builder: &CommandBuilder) -> Option<PathBuf> {
    let name = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    builder
        .get_env(name)
        .map(PathBuf::from)
        .filter(|dir| dir.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wslenv_merge_keeps_entries_and_adds_ours_once() {
        assert_eq!(
            merge_wslenv(None),
            "TERM:COLORTERM:TERM_PROGRAM:TERM_PROGRAM_VERSION"
        );
        assert_eq!(
            merge_wslenv(Some("USERPROFILE/p:term/u")),
            "USERPROFILE/p:term/u:COLORTERM:TERM_PROGRAM:TERM_PROGRAM_VERSION"
        );
        assert_eq!(
            merge_wslenv(Some("::FOO:")),
            "FOO:TERM:COLORTERM:TERM_PROGRAM:TERM_PROGRAM_VERSION"
        );
    }

    #[test]
    fn environment_policy() {
        let mut builder = CommandBuilder::new_default_prog();
        builder.env("LINES", "5");
        builder.env("COLUMNS", "20");
        builder.env("XDG_ACTIVATION_TOKEN", "token");
        builder.env("OPENSESH_NO_CRASH_DIALOG", "1");
        builder.env("QT_QPA_PLATFORM", "windows:altgr");
        builder.env("TERM", "dumb");
        apply_environment(&mut builder);
        for name in REMOVED_VARIABLES {
            assert_eq!(builder.get_env(name), None, "{name}");
        }
        assert_eq!(builder.get_env("TERM"), Some(OsStr::new("xterm-256color")));
        assert_eq!(builder.get_env("COLORTERM"), Some(OsStr::new("truecolor")));
        assert_eq!(
            builder.get_env("TERM_PROGRAM"),
            Some(OsStr::new("OpenSesh"))
        );
        assert_eq!(
            builder.get_env("TERM_PROGRAM_VERSION"),
            Some(OsStr::new(env!("CARGO_PKG_VERSION")))
        );
        if cfg!(windows) {
            assert_eq!(builder.get_env("QT_QPA_PLATFORM"), None);
            let wslenv = builder.get_env("WSLENV").and_then(OsStr::to_str).unwrap();
            assert!(wslenv.ends_with("TERM:COLORTERM:TERM_PROGRAM:TERM_PROGRAM_VERSION"));
        } else {
            // Not ours to remove outside Windows.
            assert!(builder.get_env("QT_QPA_PLATFORM").is_some());
        }
        // A user's own platform choice survives.
        let mut builder = CommandBuilder::new_default_prog();
        builder.env("QT_QPA_PLATFORM", "offscreen");
        apply_environment(&mut builder);
        assert_eq!(
            builder.get_env("QT_QPA_PLATFORM"),
            Some(OsStr::new("offscreen"))
        );
    }

    #[test]
    fn default_shell_is_an_absolute_existing_program() {
        let (builder, program) = command_builder(&ShellCommand::user_shell()).unwrap();
        assert!(program.is_absolute(), "{}", program.display());
        assert!(program.is_file(), "{}", program.display());
        assert_eq!(builder.get_argv().len(), 1);
        assert_eq!(builder.get_argv()[0], program.as_os_str());
        if cfg!(windows) {
            let name = program
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_lowercase();
            assert!(
                ["pwsh.exe", "powershell.exe", "cmd.exe"].contains(&name.as_str()),
                "{name}"
            );
        }
        let cwd = builder.get_cwd().map(PathBuf::from).unwrap();
        assert!(cwd.is_dir());
        if cfg!(unix) {
            assert_eq!(builder.get_env("PWD"), Some(cwd.as_os_str()));
        }
    }

    #[test]
    fn programs_resolve_through_path() {
        let name = if cfg!(windows) { "cmd" } else { "sh" };
        let (builder, program) = command_builder(
            &ShellCommand::program(name)
                .arg("-x")
                .env("OPENSESH_TEST_VALUE", "42"),
        )
        .unwrap();
        assert!(program.is_absolute());
        assert_eq!(builder.get_argv()[1], "-x");
        assert_eq!(
            builder.get_env("OPENSESH_TEST_VALUE"),
            Some(OsStr::new("42"))
        );
        assert_eq!(
            command_builder(&ShellCommand::program("opensesh-no-such-program")).map(|_| ()),
            Err(ShellError::NotFound("opensesh-no-such-program".to_owned()))
        );
        let missing = if cfg!(windows) {
            r"C:\opensesh\missing.exe"
        } else {
            "/opensesh/missing"
        };
        assert!(matches!(
            command_builder(&ShellCommand::program(missing)),
            Err(ShellError::NotFound(_))
        ));
    }

    #[test]
    fn cwd_falls_back_to_home() {
        let dir = tempfile::tempdir().unwrap();
        let (builder, _) = command_builder(&ShellCommand::user_shell().cwd(dir.path())).unwrap();
        assert_eq!(builder.get_cwd().map(PathBuf::from).unwrap(), dir.path());
        let (builder, _) =
            command_builder(&ShellCommand::user_shell().cwd("/opensesh/no/such/dir")).unwrap();
        assert_ne!(
            builder.get_cwd().map(PathBuf::from),
            Some(PathBuf::from("/opensesh/no/such/dir"))
        );
    }

    #[test]
    fn debug_output_hides_environment_values() {
        let command = ShellCommand::user_shell().env("API_TOKEN", "secret-value");
        let text = format!("{command:?}");
        assert!(text.contains("API_TOKEN"));
        assert!(!text.contains("secret-value"));
    }
}
