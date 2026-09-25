//! `cargo xtask dist windows`: the Windows release packages, written to `<target>/dist/`.
//!
//! Builds the release app, deploys Qt next to it (`windeployqt`), adds the bundled ConPTY
//! ([ADR 0014]) and the MSVC runtime, then makes the portable zip (with the `portable` marker) and
//! the NSIS installer (`packaging/windows/opensesh.nsi`). The Linux packages are built by
//! `scripts/linux/build.sh`, on each distribution with its own Qt.
//!
//! [ADR 0014]: ../../docs/adr/0014-bundled-conpty.md

use std::ffi::OsString;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};

use crate::conpty;

/// Version of every package: the workspace version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The GUI executable Cargo builds.
const BINARY: &str = "opensesh-app";
/// Its name in the Windows packages.
const WINDOWS_EXE: &str = "OpenSesh.exe";
/// The MSVC runtime DLLs the executable and Qt link against (redistributable, from System32).
const MSVC_RUNTIME: [&str; 5] = [
    "msvcp140.dll",
    "msvcp140_1.dll",
    "msvcp140_2.dll",
    "vcruntime140.dll",
    "vcruntime140_1.dll",
];

/// Runs `cargo xtask dist <format>` from the workspace root.
///
/// # Errors
///
/// Fails on an unknown format, a missing tool, a failed build or I/O errors.
pub fn run(root: &Path, args: impl IntoIterator<Item = OsString>) -> Result<()> {
    let args: Vec<OsString> = args.into_iter().collect();
    let [format] = args.as_slice() else {
        bail!("usage: cargo xtask dist windows");
    };
    let format = format.to_str().unwrap_or_default();
    let dist = conpty::cargo_target_dir(root)?.join("dist");
    fs::create_dir_all(&dist).with_context(|| format!("creating {}", dist.display()))?;
    match format {
        "windows" => windows(root, &dist),
        other => bail!("unknown dist format `{other}` (Linux packages: scripts/linux/build.sh)"),
    }
}

/// Builds the release app and returns the path of the executable.
fn build_release(root: &Path) -> Result<PathBuf> {
    run_command(
        Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .current_dir(root)
            .args(["build", "--release", "--locked", "-p", BINARY]),
    )?;
    let exe = conpty::cargo_target_dir(root)?
        .join("release")
        .join(format!("{BINARY}{}", std::env::consts::EXE_SUFFIX));
    ensure!(exe.is_file(), "{} was not built", exe.display());
    Ok(exe)
}

fn run_command(command: &mut Command) -> Result<()> {
    let description = format!("{command:?}");
    let status = command
        .status()
        .with_context(|| format!("starting {description}"))?;
    ensure!(status.success(), "{description} failed ({status})");
    Ok(())
}

fn copy(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::copy(from, to)
        .with_context(|| format!("copying {} to {}", from.display(), to.display()))?;
    Ok(())
}

fn fresh_dir(dir: &Path) -> Result<()> {
    if dir.exists() {
        fs::remove_dir_all(dir).with_context(|| format!("removing {}", dir.display()))?;
    }
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))
}

/// The Qt `bin` folder: `QMAKE`'s `QT_INSTALL_BINS`.
fn qt_bin_dir() -> Result<PathBuf> {
    let qmake = std::env::var_os("QMAKE").unwrap_or_else(|| "qmake6".into());
    let output = Command::new(&qmake)
        .args(["-query", "QT_INSTALL_BINS"])
        .output()
        .with_context(|| format!("running {}", qmake.to_string_lossy()))?;
    ensure!(output.status.success(), "qmake -query failed");
    let dir = String::from_utf8(output.stdout)
        .context("qmake printed a non-UTF-8 path")?
        .trim()
        .to_owned();
    ensure!(!dir.is_empty(), "qmake gave no QT_INSTALL_BINS");
    Ok(PathBuf::from(dir))
}

// ---- Windows -------------------------------------------------------------------------------

fn windows(root: &Path, dist: &Path) -> Result<()> {
    ensure!(cfg!(windows), "`dist windows` runs on Windows");
    let exe = build_release(root)?;
    let name = format!("OpenSesh-{VERSION}-windows-x64");
    let stage = dist.join(&name);
    fresh_dir(&stage)?;
    copy(&exe, &stage.join(WINDOWS_EXE))?;

    let windeployqt = qt_bin_dir()?.join("windeployqt.exe");
    ensure!(windeployqt.is_file(), "{} not found", windeployqt.display());
    run_command(
        Command::new(&windeployqt)
            .arg("--release")
            .arg("--qmldir")
            .arg(root.join("crates/opensesh-app/qml"))
            .args([
                "--no-translations",
                "--no-opengl-sw",
                "--no-compiler-runtime",
            ])
            .arg(stage.join(WINDOWS_EXE)),
    )?;
    let system32 =
        PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
            .join("System32");
    for dll in MSVC_RUNTIME {
        let source = system32.join(dll);
        if source.is_file() {
            copy(&source, &stage.join(dll))?;
        } else {
            println!("dist: {dll} not found in System32; the MSVC runtime must be installed");
        }
    }
    conpty::run(
        root,
        &conpty::Options::parse([OsString::from("--dest"), stage.clone().into()])?,
    )?;
    for doc in [
        "LICENSE",
        "THIRD_PARTY_NOTICES.md",
        "README.md",
        "CHANGELOG.md",
    ] {
        copy(&root.join(doc), &stage.join(doc))?;
    }

    // The portable zip: data next to the executable (the `portable` marker, PLAN §4.1).
    let zip_path = dist.join(format!("{name}-portable.zip"));
    write_zip(&stage, &name, &zip_path, &[("portable", b"")])?;
    println!("dist: {}", zip_path.display());

    let makensis = find_makensis()?;
    let setup = dist.join(format!("OpenSesh-{VERSION}-windows-x64-setup.exe"));
    run_command(
        Command::new(&makensis)
            .arg("/V2")
            .arg(format!("/DVERSION={VERSION}"))
            .arg(format!("/DSRCDIR={}", stage.display()))
            .arg(format!("/DOUTFILE={}", setup.display()))
            .arg(root.join("packaging/windows/opensesh.nsi")),
    )?;
    println!("dist: {}", setup.display());
    Ok(())
}

fn find_makensis() -> Result<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("NSIS_HOME") {
        candidates.push(PathBuf::from(home).join("makensis.exe"));
    }
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(dir) = std::env::var_os(var) {
            candidates.push(PathBuf::from(dir).join("NSIS").join("makensis.exe"));
        }
    }
    if let Some(found) = candidates.into_iter().find(|path| path.is_file()) {
        return Ok(found);
    }
    // On PATH.
    if Command::new("makensis").arg("/VERSION").output().is_ok() {
        return Ok(PathBuf::from("makensis"));
    }
    bail!("NSIS (makensis) not found: install it or set NSIS_HOME")
}

/// Zips every file under `dir` into `zip_path`, under the folder `top`, plus `extra` files.
fn write_zip(dir: &Path, top: &str, zip_path: &Path, extra: &[(&str, &[u8])]) -> Result<()> {
    let file =
        fs::File::create(zip_path).with_context(|| format!("creating {}", zip_path.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut files = Vec::new();
    collect_files(dir, &mut files)?;
    files.sort();
    for path in files {
        let relative = path
            .strip_prefix(dir)
            .context("a staged file outside the stage")?
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        zip.start_file(format!("{top}/{relative}"), options)?;
        zip.write_all(&fs::read(&path).with_context(|| format!("reading {}", path.display()))?)?;
    }
    for (name, contents) in extra {
        zip.start_file(format!("{top}/{name}"), options)?;
        zip.write_all(contents)?;
    }
    zip.finish()?;
    Ok(())
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            collect_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}
