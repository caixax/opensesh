//! `cargo xtask shaders`: compiles the Qt Quick material shaders with Qt's `qsb` (ADR 0013).
//!
//! Every `crates/opensesh-app/shaders/<name>.vert|.frag` becomes `<name>.vert.qsb` /
//! `<name>.frag.qsb` next to it, with the shader variants Qt Quick needs on every backend
//! (GLSL for OpenGL and OpenGL ES, HLSL for Direct3D, MSL for Metal; SPIR-V for Vulkan is always
//! included). Vertex shaders also get the batchable variant (`qsb -b`): without it, Qt Quick
//! silently drops geometry that it merges into a batch.
//!
//! The `.qsb` files are committed, so only people who edit shaders need qsb (the Qt Shader Tools
//! module). The files are generated in `target/xtask-shaders/` first and copied over only when
//! they changed. qsb output is byte-for-byte reproducible for a given Qt version (checked with
//! 6.10.3: the same bytes from two runs, other folders and other file names), so `--check`
//! compares bytes. Like the `.qm` files, the committed `.qsb` files are built with the pinned Qt,
//! 6.10.3; Qt 6.8 reads them (the `.qsb` format is version 9 in both).

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};

use crate::common::write_if_changed;

/// Shader sources and committed `.qsb` files, relative to the workspace root.
const SHADER_DIR: &str = "crates/opensesh-app/shaders";
/// Scratch folder, relative to the workspace root.
const WORK_DIR: &str = "target/xtask-shaders";
/// Qt version whose qsb builds the committed files (the Qt that CI and the dev boxes use).
const PINNED_QT: &str = "6.10.3";
/// Shading languages to generate: GLSL ES 2 / GLSL 1.20 / GLSL 1.50 (OpenGL), HLSL shader model
/// 5.0 (Direct3D 11 and 12) and MSL 1.2 (Metal). This is the set Qt Quick's own materials use
/// (`qsb --qt6`).
const TARGETS: [&str; 6] = ["--glsl", "100 es,120,150", "--hlsl", "50", "--msl", "12"];

/// A shader stage, from the source file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Vertex,
    Fragment,
}

impl Stage {
    fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()? {
            "vert" => Some(Self::Vertex),
            "frag" => Some(Self::Fragment),
            _ => None,
        }
    }
}

/// qsb arguments (before `-o <output> <input>`) for a stage.
fn qsb_args(stage: Stage) -> Vec<&'static str> {
    let mut args = Vec::with_capacity(TARGETS.len() + 1);
    if stage == Stage::Vertex {
        // Batchable variant: required for nodes that the scene graph merges (ADR 0013).
        args.push("-b");
    }
    args.extend(TARGETS);
    args
}

/// Runs the task. In `check` mode nothing is written and the result says whether the committed
/// `.qsb` files are up to date; otherwise it is always `true`.
///
/// # Errors
///
/// Fails if there are no shader sources, qsb can't be found or fails, or on I/O errors.
pub fn run(root: &Path, check: bool) -> Result<bool> {
    let dir = native_path(root, SHADER_DIR);
    let sources = shader_sources(&dir)?;
    ensure!(
        !sources.is_empty(),
        "no .vert or .frag files in {SHADER_DIR}"
    );
    let (qsb, version) = find_qsb()?;
    println!("shaders: using {} ({version})", qsb.display());

    let work = native_path(root, WORK_DIR);
    if work.exists() {
        std::fs::remove_dir_all(&work).with_context(|| format!("removing {}", work.display()))?;
    }
    std::fs::create_dir_all(&work)?;

    // Everything is compiled before any committed file changes, so a shader that fails to
    // compile leaves the tree untouched.
    let mut outputs = Vec::new();
    for (source, stage) in &sources {
        let name = file_name(source);
        let output_name = format!("{name}.qsb");
        let output = work.join(&output_name);
        let result = Command::new(&qsb)
            .args(qsb_args(*stage))
            .arg("-o")
            .arg(&output)
            .arg(source)
            .output()
            .with_context(|| format!("running {}", qsb.display()))?;
        if !result.status.success() {
            bail!(
                "qsb failed for {name} ({}):\n{}{}",
                result.status,
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
        }
        let bytes =
            std::fs::read(&output).with_context(|| format!("qsb wrote no {output_name}"))?;
        ensure!(!bytes.is_empty(), "qsb wrote an empty {output_name}");
        outputs.push((output_name, bytes));
    }

    let mut problems = Vec::new();
    for (output_name, bytes) in &outputs {
        let committed = dir.join(output_name);
        if check {
            match std::fs::read(&committed) {
                Err(_) => problems.push(format!("{SHADER_DIR}/{output_name} is missing")),
                Ok(current) if current != *bytes => problems.push(format!(
                    "{SHADER_DIR}/{output_name} does not match what qsb builds from its source"
                )),
                Ok(_) => {}
            }
        } else if write_if_changed(&committed, bytes)? {
            println!("updated {SHADER_DIR}/{output_name}");
        }
    }

    let expected: Vec<&str> = outputs.iter().map(|(name, _)| name.as_str()).collect();
    for orphan in orphan_qsb_files(&dir, &expected)? {
        let name = file_name(&orphan);
        if check {
            problems.push(format!("{SHADER_DIR}/{name} has no .vert or .frag source"));
        } else {
            std::fs::remove_file(&orphan)
                .with_context(|| format!("removing {}", orphan.display()))?;
            println!("removed {SHADER_DIR}/{name} (no source)");
        }
    }

    for problem in &problems {
        eprintln!("shaders: {problem}");
    }
    if !problems.is_empty() && version != PINNED_QT {
        eprintln!(
            "shaders: note: this is qsb {version}; the committed files are built with qsb \
             {PINNED_QT}, and other versions may produce different bytes"
        );
    }
    Ok(problems.is_empty())
}

/// `root` joined with a `/`-separated relative path, with native separators.
fn native_path(root: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(root.to_path_buf(), |path, part| path.join(part))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The `.vert` and `.frag` files in `dir`, sorted by name.
fn shader_sources(dir: &Path) -> Result<Vec<(PathBuf, Stage)>> {
    let entries = std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?;
    let mut sources = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path.is_file()
            && let Some(stage) = Stage::from_path(&path)
        {
            sources.push((path, stage));
        }
    }
    sources.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(sources)
}

/// `.qsb` files in `dir` that no source produces.
fn orphan_qsb_files(dir: &Path, expected: &[&str]) -> Result<Vec<PathBuf>> {
    let mut orphans = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        let name = file_name(&path);
        if path.extension().is_some_and(|ext| ext == "qsb") && !expected.contains(&name.as_str()) {
            orphans.push(path);
        }
    }
    orphans.sort();
    Ok(orphans)
}

/// Finds a Qt 6 `qsb` and returns it with its version: first in the `bin` folders that qmake
/// reports (the `QMAKE` environment variable, else `qmake6` / `qmake` on `PATH`, the same lookup
/// cxx-qt uses), then as `qsb`, `qsb6` or `qsb-qt6` on `PATH`.
fn find_qsb() -> Result<(PathBuf, String)> {
    let exe = format!("qsb{}", std::env::consts::EXE_SUFFIX);
    let mut candidates: Vec<PathBuf> = qt_bin_dirs()
        .into_iter()
        .map(|dir| dir.join(&exe))
        .collect();
    candidates.extend(["qsb", "qsb6", "qsb-qt6"].map(PathBuf::from));
    for candidate in candidates {
        if let Some(version) = qsb_version(&candidate) {
            return Ok((candidate, version));
        }
    }
    bail!(
        "could not find Qt's shader baker `qsb` (Qt Shader Tools). Only needed after editing \
         {SHADER_DIR}; the compiled .qsb files are committed. Install it into the Qt that QMAKE \
         points at, or put `qsb` on PATH:\n\
         \x20 aqtinstall (Windows, macOS, Linux): aqt install-qt <host> desktop {PINNED_QT} <arch> \
         -m qtshadertools --noarchives -O <Qt folder>\n\
         \x20 Arch:   pacman -S qt6-shadertools\n\
         \x20 Debian: apt install qt6-shader-baker\n\
         \x20 Fedora: dnf install qt6-qtshadertools\n\
         Use qsb {PINNED_QT} for files you commit (see docs/dev-setup.md)."
    )
}

/// `bin` folders of the Qt that qmake reports, without duplicates.
fn qt_bin_dirs() -> Vec<PathBuf> {
    let qmakes: Vec<String> = match std::env::var("QMAKE") {
        Ok(qmake) if !qmake.is_empty() => vec![qmake],
        _ => vec!["qmake6".to_owned(), "qmake".to_owned()],
    };
    let mut dirs: Vec<PathBuf> = Vec::new();
    for qmake in &qmakes {
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

/// Runs `<qsb> --version` and returns the version of a Qt 6 (or later) qsb.
fn qsb_version(qsb: &Path) -> Option<String> {
    let output = Command::new(qsb).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let version = parse_qsb_version(&text)?;
    let major: u32 = version.split('.').next()?.parse().ok()?;
    (major >= 6).then_some(version)
}

/// Version from `qsb --version` output, e.g. `qsb 6.10.3`.
fn parse_qsb_version(text: &str) -> Option<String> {
    let version = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("qsb "))?;
    let version = version.trim();
    (!version.is_empty() && version.chars().all(|c| c.is_ascii_digit() || c == '.'))
        .then(|| version.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_shaders_are_batchable_and_fragment_shaders_are_not() {
        let vertex = qsb_args(Stage::Vertex);
        let fragment = qsb_args(Stage::Fragment);
        assert_eq!(vertex[0], "-b");
        assert!(!fragment.contains(&"-b"));
        for args in [&vertex, &fragment] {
            assert!(
                args.windows(2)
                    .any(|pair| pair == ["--glsl", "100 es,120,150"])
            );
            assert!(args.windows(2).any(|pair| pair == ["--hlsl", "50"]));
            assert!(args.windows(2).any(|pair| pair == ["--msl", "12"]));
        }
    }

    #[test]
    fn stages_come_from_the_extension() {
        assert_eq!(
            Stage::from_path(Path::new("a/terminal_glyph.vert")),
            Some(Stage::Vertex)
        );
        assert_eq!(
            Stage::from_path(Path::new("terminal_glyph.frag")),
            Some(Stage::Fragment)
        );
        assert_eq!(Stage::from_path(Path::new("terminal_glyph.vert.qsb")), None);
        assert_eq!(Stage::from_path(Path::new("README")), None);
    }

    #[test]
    fn qsb_versions_are_parsed() {
        assert_eq!(parse_qsb_version("qsb 6.10.3\n").as_deref(), Some("6.10.3"));
        assert_eq!(parse_qsb_version("qsb 6.8.2").as_deref(), Some("6.8.2"));
        assert_eq!(parse_qsb_version("Qt Shader Baker"), None);
        assert_eq!(parse_qsb_version("qsb 6.10.3; rm -rf"), None);
        assert_eq!(parse_qsb_version(""), None);
    }

    #[test]
    fn sources_and_orphans_are_found() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "b.frag",
            "a.vert",
            "a.vert.qsb",
            "old.frag.qsb",
            "notes.txt",
        ] {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        let sources = shader_sources(dir.path()).unwrap();
        let names: Vec<String> = sources.iter().map(|(path, _)| file_name(path)).collect();
        assert_eq!(names, ["a.vert", "b.frag"]);
        assert_eq!(sources[0].1, Stage::Vertex);
        assert_eq!(sources[1].1, Stage::Fragment);

        let orphans = orphan_qsb_files(dir.path(), &["a.vert.qsb", "b.frag.qsb"]).unwrap();
        let orphans: Vec<String> = orphans.iter().map(|path| file_name(path)).collect();
        assert_eq!(orphans, ["old.frag.qsb"]);
    }
}
