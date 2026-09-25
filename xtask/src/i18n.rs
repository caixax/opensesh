//! `cargo xtask i18n`: translation pipeline (Sprint 1).
//!
//! 1. Reads `assets/i18n/languages.toml`.
//! 2. Runs Qt's `lupdate` over `crates/opensesh-app/qml` for every listed language plus the
//!    pseudo-locale, producing `crates/opensesh-app/i18n/opensesh_<code>.ts`.
//! 3. Fills `opensesh_pseudo.ts` with a pseudo-translation of every source string (see
//!    `pseudo.rs`).
//! 4. Compiles every `.ts` into the `.qm` file the app bundles, with `lrelease`.
//!
//! Everything is generated in `target/xtask-i18n/` first and copied over only when it changed,
//! so an unchanged run doesn't touch the files the app's `build.rs` watches. With `--check`
//! nothing is written: the task fails if a committed `.ts` file is stale, or if a committed `.qm`
//! file differs from what lrelease builds (lrelease output is reproducible for a given Qt
//! version; CI checks with the pinned Qt).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;

use crate::common::write_if_changed;

/// Language list, relative to the workspace root.
const LANGUAGES: &str = "assets/i18n/languages.toml";
/// Sources scanned by lupdate, relative to the workspace root.
const QML_DIR: &str = "crates/opensesh-app/qml";
/// Committed `.ts` and `.qm` files, relative to the workspace root.
const I18N_DIR: &str = "crates/opensesh-app/i18n";
/// Scratch folder, relative to the workspace root.
const WORK_DIR: &str = "target/xtask-i18n";
/// File name prefix of every translation (`opensesh_<code>.ts`), as the app expects.
const PREFIX: &str = "opensesh_";
/// Code of the generated pseudo-locale.
const PSEUDO: &str = "pseudo";
/// Language of the source strings.
const SOURCE_LANGUAGE: &str = "en";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Languages {
    schema_version: u32,
    languages: Vec<String>,
}

/// Runs the pipeline. In `check` mode nothing is written and the result says whether the
/// committed translations are up to date; otherwise it is always `true`.
///
/// # Errors
///
/// Fails if the language list is invalid, the Qt linguist tools can't be found or fail, or on
/// I/O errors.
pub fn run(root: &Path, check: bool) -> Result<bool> {
    let codes = load_languages(root)?;
    // Look both tools up before changing anything, so a missing tool fails early.
    let lupdate = find_tool("lupdate")?;
    let lrelease = find_tool("lrelease")?;

    let work = native_path(root, WORK_DIR);
    if work.exists() {
        std::fs::remove_dir_all(&work).with_context(|| format!("removing {}", work.display()))?;
    }
    std::fs::create_dir_all(&work)?;
    let i18n = native_path(root, I18N_DIR);
    let qml = native_path(root, QML_DIR);

    // Every language is generated and compiled in the scratch folder first, so a failing tool
    // leaves the committed files untouched instead of a new `.ts` next to a stale `.qm`.
    let mut outputs = Vec::new();
    for code in &codes {
        let ts_name = format!("{PREFIX}{code}.ts");
        let qm_name = format!("{PREFIX}{code}.qm");
        let committed = i18n.join(&ts_name);
        let generated_path = work.join(&ts_name);
        if committed.is_file() {
            // lupdate merges into the existing file, which keeps the finished translations.
            std::fs::copy(&committed, &generated_path)
                .with_context(|| format!("copying {}", committed.display()))?;
        }
        let target_language = if code == PSEUDO {
            // English plural rules (one / other) for the pseudo-locale's numerus forms.
            SOURCE_LANGUAGE
        } else {
            code.as_str()
        };
        run_lupdate(&lupdate, &qml, &work, &ts_name, target_language, check)?;

        let mut ts = std::fs::read_to_string(&generated_path)
            .with_context(|| format!("reading {}", generated_path.display()))?
            .replace("\r\n", "\n");
        if code == PSEUDO {
            ts = crate::pseudo::fill_ts(&ts)
                .with_context(|| format!("pseudo-translating {ts_name}"))?;
        }
        std::fs::write(&generated_path, &ts)?;

        run_lrelease(&lrelease, &work, &ts_name, &qm_name, check)?;
        let qm = std::fs::read(work.join(&qm_name))
            .with_context(|| format!("lrelease wrote no {qm_name}"))?;
        ensure!(!qm.is_empty(), "lrelease wrote an empty {qm_name}");
        outputs.push((ts_name, ts, qm_name, qm));
    }

    let mut problems = Vec::new();
    for (ts_name, ts, qm_name, qm) in &outputs {
        if check {
            let current_ts = std::fs::read_to_string(i18n.join(ts_name))
                .ok()
                .map(|text| text.replace("\r\n", "\n"));
            if current_ts.as_deref() != Some(ts.as_str()) {
                problems.push(format!("{I18N_DIR}/{ts_name} is out of date"));
            }
            // lrelease output is byte-for-byte reproducible for a given Qt version, so a `.qm`
            // that differs was not rebuilt after its `.ts` changed (or came from another Qt).
            match std::fs::read(i18n.join(qm_name)) {
                Err(_) => problems.push(format!("{I18N_DIR}/{qm_name} is missing")),
                Ok(current_qm) if current_qm != *qm => problems.push(format!(
                    "{I18N_DIR}/{qm_name} does not match what lrelease builds from {ts_name}"
                )),
                Ok(_) => {}
            }
        } else {
            std::fs::create_dir_all(&i18n)?;
            if write_if_changed(&i18n.join(ts_name), ts)? {
                println!("updated {I18N_DIR}/{ts_name}");
            }
            if write_if_changed(&i18n.join(qm_name), qm)? {
                println!("updated {I18N_DIR}/{qm_name}");
            }
        }
    }

    for orphan in orphan_files(&i18n, &codes)? {
        let name = orphan
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let is_qm = orphan.extension().is_some_and(|ext| ext == "qm");
        if check {
            problems.push(format!(
                "{I18N_DIR}/{name} belongs to no language in {LANGUAGES}"
            ));
        } else if is_qm {
            // A bundled .qm would still show up in the language selector.
            std::fs::remove_file(&orphan)?;
            println!("removed {I18N_DIR}/{name} (not listed in {LANGUAGES})");
        } else {
            eprintln!(
                "warning: {I18N_DIR}/{name} belongs to no language in {LANGUAGES}; \
                 delete it if the language was dropped on purpose"
            );
        }
    }

    for problem in &problems {
        eprintln!("i18n: {problem}");
    }
    Ok(problems.is_empty())
}

/// `root` joined with a `/`-separated relative path, with native separators (the tools print
/// the paths they are given).
fn native_path(root: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(root.to_path_buf(), |path, part| path.join(part))
}

/// Reads `assets/i18n/languages.toml` and returns the codes to build, pseudo-locale last.
fn load_languages(root: &Path) -> Result<Vec<String>> {
    let path = root.join(LANGUAGES);
    let languages: Languages = toml::from_str(
        &std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?,
    )
    .with_context(|| format!("parsing {}", path.display()))?;
    validate_languages(&languages)?;
    let mut codes = languages.languages;
    codes.push(PSEUDO.to_owned());
    Ok(codes)
}

fn validate_languages(languages: &Languages) -> Result<()> {
    ensure!(
        languages.schema_version == 1,
        "unsupported languages.toml schema_version {}",
        languages.schema_version
    );
    let mut seen = BTreeSet::new();
    for code in &languages.languages {
        ensure!(
            code != PSEUDO,
            "`{PSEUDO}` is always generated; remove it from {LANGUAGES}"
        );
        ensure!(
            code.split('_').next() != Some(SOURCE_LANGUAGE),
            "`{code}`: English is the source language and can't be a translation"
        );
        ensure!(
            is_language_code(code),
            "`{code}` is not a language code the app accepts, such as `de` or `pt_BR`"
        );
        ensure!(seen.insert(code), "`{code}` is listed twice");
    }
    Ok(())
}

/// Codes the app's `general.language` setting accepts (`de`, `pt_BR`), minus the special values
/// `system` and `pseudo`, so every built translation can be selected.
fn is_language_code(code: &str) -> bool {
    code != "system" && code != PSEUDO && opensesh_core::config::valid_language(code)
}

/// `opensesh_*.ts` / `.qm` files in `dir` whose code isn't built any more.
fn orphan_files(dir: &Path, codes: &[String]) -> Result<Vec<PathBuf>> {
    let mut orphans = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(orphans);
    };
    for entry in entries {
        let path = entry?.path();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let code = name.strip_prefix(PREFIX).and_then(|rest| {
            rest.strip_suffix(".ts")
                .or_else(|| rest.strip_suffix(".qm"))
        });
        if let Some(code) = code
            && !codes.iter().any(|known| known == code)
        {
            orphans.push(path);
        }
    }
    orphans.sort();
    Ok(orphans)
}

fn run_lupdate(
    lupdate: &Path,
    qml: &Path,
    work: &Path,
    ts_name: &str,
    target_language: &str,
    silent: bool,
) -> Result<()> {
    let mut command = Command::new(lupdate);
    // Relative `-ts`, so lupdate reports `opensesh_<code>.ts` rather than a scratch path.
    command.current_dir(work);
    if silent {
        command.arg("-silent");
    }
    // No source locations: they would make every QML edit that moves a line a translation
    // change, and `--check` must only fail when the strings themselves change.
    command
        .args([
            "-locations",
            "none",
            "-no-obsolete",
            "-extensions",
            "qml,js",
        ])
        .args(["-source-language", SOURCE_LANGUAGE])
        .args(["-target-language", target_language])
        .arg(qml)
        .arg("-ts")
        .arg(ts_name);
    let status = command
        .status()
        .with_context(|| format!("running {}", lupdate.display()))?;
    ensure!(status.success(), "lupdate failed for {ts_name} ({status})");
    Ok(())
}

fn run_lrelease(
    lrelease: &Path,
    work: &Path,
    ts_name: &str,
    qm_name: &str,
    silent: bool,
) -> Result<()> {
    let mut command = Command::new(lrelease);
    command.current_dir(work);
    if silent {
        command.arg("-silent");
    }
    // Unfinished translations are left out, so a half-translated language falls back to English
    // instead of showing drafts.
    let status = command
        .args(["-nounfinished", ts_name, "-qm", qm_name])
        .status()
        .with_context(|| format!("running {}", lrelease.display()))?;
    ensure!(status.success(), "lrelease failed for {ts_name} ({status})");
    Ok(())
}

/// Finds a Qt 6 linguist tool (`lupdate`, `lrelease`): first in the `bin` folder that qmake
/// reports (the `QMAKE` environment variable, else `qmake6` / `qmake` on `PATH`, the same lookup
/// cxx-qt uses), then as `<name>`, `<name>-qt6` or `<name>6` on `PATH`. Tools that report a Qt
/// major version below 6 are skipped.
fn find_tool(name: &str) -> Result<PathBuf> {
    let exe = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let mut skipped = Vec::new();
    for dir in qt_bin_dirs() {
        let path = dir.join(&exe);
        if path.is_file() {
            if is_qt6_tool(&path) {
                return Ok(path);
            }
            skipped.push(path.display().to_string());
        }
    }
    for candidate in [name.to_owned(), format!("{name}-qt6"), format!("{name}6")] {
        if is_qt6_tool(Path::new(&candidate)) {
            return Ok(PathBuf::from(candidate));
        }
    }
    let skipped = if skipped.is_empty() {
        String::new()
    } else {
        format!(
            "\nSkipped because they are not Qt 6 tools: {}",
            skipped.join(", ")
        )
    };
    bail!(
        "could not find a Qt 6 `{name}`. Install the Qt 6 linguist tools, then point the QMAKE \
         environment variable (or `qmake6` / `qmake` on PATH) at that Qt, or put `{name}` on PATH:\n\
         \x20 Arch:           pacman -S qt6-tools\n\
         \x20 Debian, Ubuntu: apt install qt6-l10n-tools\n\
         \x20 Fedora:         dnf install qt6-linguist\n\
         \x20 Windows, macOS: a Qt installed with aqtinstall or the Qt online installer already \
         includes it (in <Qt>/bin){skipped}"
    )
}

/// `bin` folders of the Qt that qmake reports, without duplicates.
fn qt_bin_dirs() -> Vec<PathBuf> {
    let candidates: Vec<String> = match std::env::var("QMAKE") {
        Ok(qmake) if !qmake.is_empty() => vec![qmake],
        _ => vec!["qmake6".to_owned(), "qmake".to_owned()],
    };
    let mut dirs: Vec<PathBuf> = Vec::new();
    for qmake in &candidates {
        for property in ["QT_INSTALL_BINS", "QT_HOST_BINS"] {
            if let Some(dir) = qmake_query(qmake, property).map(PathBuf::from)
                && !dirs.contains(&dir)
            {
                dirs.push(dir);
            }
        }
    }
    dirs
}

fn qmake_query(qmake: &str, property: &str) -> Option<String> {
    let output = Command::new(qmake)
        .args(["-query", property])
        .output()
        .ok()?;
    let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (output.status.success() && !value.is_empty()).then_some(value)
}

/// Runs `<tool> -version` and checks that it belongs to Qt 6 or later.
fn is_qt6_tool(tool: &Path) -> bool {
    let Ok(output) = Command::new(tool).arg("-version").output() else {
        return false;
    };
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output.status.success() && qt_major_version(&text).is_some_and(|major| major >= 6)
}

/// Major version from output such as `lupdate version 6.10.3`.
fn qt_major_version(text: &str) -> Option<u32> {
    let (_, after) = text.split_once("version ")?;
    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_codes_are_the_ones_the_app_accepts() {
        for good in ["de", "es", "pt_BR", "fil"] {
            assert!(is_language_code(good), "{good}");
        }
        for bad in [
            "", "d", "deutsch", "DE", "pt-BR", "pt_br", "zh_Hant", "es_419", "../x", "de_",
            "system", "pseudo",
        ] {
            assert!(!is_language_code(bad), "{bad}");
        }
    }

    #[test]
    fn language_list_is_validated() {
        let list = |codes: &[&str]| Languages {
            schema_version: 1,
            languages: codes.iter().map(|code| (*code).to_owned()).collect(),
        };
        assert!(validate_languages(&list(&[])).is_ok());
        assert!(validate_languages(&list(&["es", "pt_BR"])).is_ok());
        assert!(validate_languages(&list(&["pseudo"])).is_err());
        assert!(validate_languages(&list(&["en"])).is_err());
        assert!(validate_languages(&list(&["en_GB"])).is_err());
        assert!(validate_languages(&list(&["es", "es"])).is_err());
        assert!(validate_languages(&list(&["es-ES"])).is_err());
        let mut future = list(&[]);
        future.schema_version = 2;
        assert!(validate_languages(&future).is_err());
    }

    #[test]
    fn languages_in_repo_parse() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let codes = load_languages(&root).unwrap();
        assert_eq!(codes.last().map(String::as_str), Some(PSEUDO));
    }

    #[test]
    fn qt_versions_are_parsed() {
        assert_eq!(qt_major_version("lupdate version 6.10.3\n"), Some(6));
        assert_eq!(qt_major_version("lrelease version 5.15.13"), Some(5));
        assert_eq!(qt_major_version("Usage: lupdate [options]"), None);
    }

    #[test]
    fn orphans_are_files_of_unbuilt_codes() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "opensesh_pseudo.ts",
            "opensesh_pseudo.qm",
            "opensesh_es.ts",
            "opensesh_es.qm",
            "opensesh_de.qm",
            "README.md",
        ] {
            std::fs::write(dir.path().join(name), "").unwrap();
        }
        let orphans = orphan_files(dir.path(), &["es".to_owned(), PSEUDO.to_owned()]).unwrap();
        let names: Vec<String> = orphans
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["opensesh_de.qm"]);
        assert!(
            orphan_files(&dir.path().join("missing"), &[])
                .unwrap()
                .is_empty()
        );
    }
}
