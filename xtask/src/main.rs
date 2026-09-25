//! Developer tasks for OpenSesh, run with `cargo xtask <task>` (alias in `.cargo/config.toml`).

// A command-line tool: printing is its user interface.
#![allow(clippy::print_stdout, clippy::print_stderr)]

mod common;
mod fonts;
mod http;
mod i18n;
mod icons;
mod lint_qml;
mod notices;
mod pseudo;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};

const USAGE: &str = "\
Usage: cargo xtask <task>

Tasks:
  icons      Download the pinned icon packages (Lucide, Tabler, Simple Icons), verify their sha256
             and regenerate the committed icons, the placeholder app logo, the license copies and
             THIRD_PARTY_NOTICES.md (assets/icons/icons.toml).
  fonts      Download the pinned font releases (Inter, JetBrains Mono), verify their sha256 and
             extract the bundled TTFs, the license copies and THIRD_PARTY_NOTICES.md
             (assets/fonts/fonts.toml).
  i18n       Update the .ts files with lupdate for every language in assets/i18n/languages.toml
             plus the pseudo-locale, fill the pseudo-locale and compile the .qm files with
             lrelease. Needs the Qt 6 linguist tools (found through QMAKE or PATH).
             --check: change nothing; fail if the committed .ts files are out of date.
  lint-qml   Check QML sources for hardcoded colors and strings without qsTr().
             Optional argument: directory to scan (default: crates/opensesh-app/qml).
  help       Show this message.

Only `icons` and `fonts` use the network, and downloads are cached in target/xtask-cache.";

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
        Some("fonts") => {
            fonts::run(&root)?;
            Ok(ExitCode::SUCCESS)
        }
        Some("i18n") => {
            let mut check = false;
            for arg in args {
                match arg.to_str() {
                    Some("--check") => check = true,
                    _ => {
                        eprintln!(
                            "unknown i18n argument `{}`\n\n{USAGE}",
                            arg.to_string_lossy()
                        );
                        return Ok(ExitCode::FAILURE);
                    }
                }
            }
            if i18n::run(&root, check)? {
                if check {
                    println!("i18n: translations are up to date");
                }
                Ok(ExitCode::SUCCESS)
            } else {
                eprintln!("i18n: run `cargo xtask i18n` and commit the result");
                Ok(ExitCode::FAILURE)
            }
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
