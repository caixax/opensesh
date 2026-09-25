//! Developer tasks for OpenSesh, run with `cargo xtask <task>` (alias in `.cargo/config.toml`).

// A command-line tool: printing is its user interface.
#![allow(clippy::print_stdout, clippy::print_stderr)]

mod http;
mod icons;
mod lint_qml;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};

const USAGE: &str = "\
Usage: cargo xtask <task>

Tasks:
  icons      Download the pinned icon packages, verify their sha256 and regenerate the committed
             icons, the placeholder app logo, the license copies and THIRD_PARTY_NOTICES.md.
             This is the only task that uses the network.
  lint-qml   Check QML sources for hardcoded colors and strings without qsTr().
             Optional argument: directory to scan (default: crates/opensesh-app/qml).
  help       Show this message.";

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode> {
    // `args_os` rather than `args`, which panics on arguments that aren't valid UTF-8.
    let mut args = std::env::args_os().skip(1);
    let task = args.next();
    let root = workspace_root()?;

    let Some(task) = task else {
        eprintln!("{USAGE}");
        return Ok(ExitCode::FAILURE);
    };
    match task.to_str() {
        Some("icons") => {
            icons::run(&root)?;
            Ok(ExitCode::SUCCESS)
        }
        Some("lint-qml") => {
            // Kept as an `OsString`, so directories with non-UTF-8 names work too.
            let dir = args
                .next()
                .map_or_else(|| lint_qml::default_qml_root(&root), PathBuf::from);
            let findings = lint_qml::lint_dir(&dir)?;
            for finding in &findings {
                eprintln!("{finding}");
            }
            if findings.is_empty() {
                println!("lint-qml: no issues in {}", dir.display());
                Ok(ExitCode::SUCCESS)
            } else {
                eprintln!("lint-qml: {} issue(s) found", findings.len());
                Ok(ExitCode::FAILURE)
            }
        }
        Some("help" | "-h" | "--help") => {
            println!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        // Also covers task names that aren't valid UTF-8, which can't match any task.
        _ => {
            eprintln!("unknown task `{}`\n\n{USAGE}", task.to_string_lossy());
            Ok(ExitCode::FAILURE)
        }
    }
}

/// The xtask crate lives in `<workspace>/xtask`, so the workspace root is its parent.
fn workspace_root() -> Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .context("xtask must live inside the workspace")
}
