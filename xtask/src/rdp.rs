//! `cargo xtask rdp`: builds the RDP helper (`rdp/`, a workspace with a lock file of its own;
//! ADR 0034) and puts `opensesh-rdp` next to the app in `target/debug` and `target/release`
//! (whichever exist), where the app looks for it.
//!
//! The helper is built optimized even for a debug app: decoding RemoteFX unoptimized is too slow
//! to use. `--test-server` also builds and copies `opensesh-rdp-test-server`, which the smoke
//! test starts (it is never packaged). No network beyond Cargo's own downloads.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail, ensure};

use crate::conpty::cargo_target_dir;

/// The app's build folders the helper is copied into.
const PROFILES: [&str; 2] = ["debug", "release"];

/// `cargo xtask rdp` options.
#[derive(Debug, Default)]
pub struct Options {
    /// Also the test server.
    pub test_server: bool,
    /// An unoptimized helper (to debug it).
    pub debug: bool,
    /// Copy there instead of the app's build folders.
    pub dest: Vec<PathBuf>,
}

impl Options {
    /// Parses the task's arguments.
    ///
    /// # Errors
    ///
    /// On an unknown argument or a missing `--dest` value.
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self> {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--test-server") => options.test_server = true,
                Some("--debug") => options.debug = true,
                Some("--dest") => options
                    .dest
                    .push(PathBuf::from(args.next().context("--dest needs a folder")?)),
                _ => bail!("unknown argument {arg:?} for `cargo xtask rdp`"),
            }
        }
        Ok(options)
    }
}

/// A program's file name on this system.
#[must_use]
pub fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

/// Where the helper's workspace builds: its own folder inside the app's target folder, so the
/// two lock files never share build output.
///
/// # Errors
///
/// When the target folder can't be worked out.
pub fn helper_target_dir(root: &Path) -> Result<PathBuf> {
    Ok(cargo_target_dir(root)?.join("rdp"))
}

/// Builds the helper (and the test server with `test_server`) and returns the built programs.
///
/// # Errors
///
/// When Cargo fails.
pub fn build(root: &Path, debug: bool, test_server: bool) -> Result<Vec<PathBuf>> {
    let target = helper_target_dir(root)?;
    let mut command = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    command
        .arg("build")
        .arg("--locked")
        .arg("--manifest-path")
        .arg(root.join("rdp").join("Cargo.toml"))
        .args(["--bin", "opensesh-rdp"])
        .env("CARGO_TARGET_DIR", &target);
    if test_server {
        command.args(["--bin", "opensesh-rdp-test-server"]);
    }
    if !debug {
        command.arg("--release");
    }
    let status = command
        .status()
        .context("running cargo for the RDP helper")?;
    ensure!(status.success(), "building the RDP helper failed");
    let built = target.join(if debug { "debug" } else { "release" });
    let mut programs = vec![built.join(exe("opensesh-rdp"))];
    if test_server {
        programs.push(built.join(exe("opensesh-rdp-test-server")));
    }
    for program in &programs {
        ensure!(program.is_file(), "{} wasn't built", program.display());
    }
    Ok(programs)
}

/// Runs the task from the workspace root.
///
/// # Errors
///
/// When the build fails, there is nowhere to copy to, or a copy fails (a running OpenSesh keeps
/// the helper open on Windows).
pub fn run(root: &Path, options: &Options) -> Result<()> {
    let destinations: Vec<PathBuf> = if options.dest.is_empty() {
        let target = cargo_target_dir(root)?;
        let dirs: Vec<PathBuf> = PROFILES
            .iter()
            .map(|profile| target.join(profile))
            .filter(|dir| dir.is_dir())
            .collect();
        ensure!(
            !dirs.is_empty(),
            "no build output in {}: build the app first (cargo build -p opensesh-app) or pass --dest",
            target.display()
        );
        dirs
    } else {
        for dir in &options.dest {
            ensure!(dir.is_dir(), "--dest {} is not a folder", dir.display());
        }
        options.dest.clone()
    };
    let programs = build(root, options.debug, options.test_server)?;
    for dir in &destinations {
        for program in &programs {
            let name = program
                .file_name()
                .context("a program without a file name")?;
            let to = dir.join(name);
            std::fs::copy(program, &to)
                .with_context(|| format!("copying {} to {}", program.display(), to.display()))?;
            println!("{}", to.display());
        }
    }
    Ok(())
}
