//! `cargo xtask vttest`: builds the pinned upstream vttest for the terminal harness tests.
//!
//! Distributions package different vttest releases (Ubuntu 22.04 has 20210210, Fedora 43
//! 20241204, Debian 13 20241208, Arch only in the AUR), and the screens differ between releases.
//! The harness goldens are recorded with one known release, built here:
//!
//! 1. Downloads `vttest-<VERSION>.tgz` from invisible-island.net (cached under
//!    `target/xtask-cache/`) and verifies its sha256 before opening it.
//! 2. Unpacks it into `<target>/vttest/vttest-<VERSION>/` (regular files and folders only).
//! 3. Runs `./configure` and `make` there, which needs a C compiler and make.
//! 4. Copies the program to `<target>/vttest/vttest`, next to a stamp file, so later runs skip
//!    the build (`--force` rebuilds).
//!
//! `<target>` is Cargo's target folder (`CARGO_TARGET_DIR` when set). vttest is a Unix program,
//! so on Windows the task only says how to run it inside WSL.

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};

/// Upstream release built by the task.
pub const VERSION: &str = "20251205";
/// Official release tarball (a signature is published next to it as `.tgz.asc`).
const URL: &str = "https://invisible-island.net/archives/vttest/vttest-20251205.tgz";
/// sha256 of the tarball, verified on 2026-09-25.
const SHA256: &str = "cd6886f9aefe6a3f6c566fa61271a55710901a71849c630bf5376aa984bf77cc";
/// Upper bound for the tarball (20251205 is 243 KB).
const MAX_TARBALL_BYTES: u64 = 8 * 1024 * 1024;
/// Upper bound for one file in the tarball (the largest, `configure`, is 222 KB).
const MAX_ENTRY_BYTES: u64 = 4 * 1024 * 1024;
/// Upper bound for everything unpacked.
const MAX_TOTAL_BYTES: u64 = 32 * 1024 * 1024;

/// Command-line options of `cargo xtask vttest`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Rebuild even when the stamp says the program is up to date.
    force: bool,
}

impl Options {
    /// Parses the arguments that follow `vttest`.
    ///
    /// # Errors
    ///
    /// Fails on an unknown argument.
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self> {
        let mut options = Self::default();
        for arg in args {
            match arg.to_str() {
                Some("--force") => options.force = true,
                _ => bail!(
                    "unknown vttest argument `{}` (see `cargo xtask help`)",
                    arg.to_string_lossy()
                ),
            }
        }
        Ok(options)
    }
}

/// Folder with the build tree, the program and its stamp: `<target>/vttest`.
fn output_dir(root: &Path) -> Result<PathBuf> {
    Ok(crate::conpty::cargo_target_dir(root)?.join("vttest"))
}

/// Contents of the stamp file written after a successful build.
fn stamp_text() -> String {
    format!("vttest {VERSION} {SHA256}\n")
}

/// Runs the task from the workspace root.
///
/// # Errors
///
/// Fails on network errors, a checksum mismatch, an unexpected tarball entry, or when
/// `configure` or `make` fails.
pub fn run(root: &Path, options: &Options) -> Result<()> {
    if cfg!(windows) {
        println!(
            "vttest: skipped. vttest is a Unix program (termios, a C compiler and make): run \
             `cargo xtask vttest` inside WSL or on Linux, and the harness tests there."
        );
        return Ok(());
    }
    let out = output_dir(root)?;
    let program = out.join("vttest");
    let stamp = out.join("vttest.stamp");
    if !options.force
        && program.is_file()
        && std::fs::read_to_string(&stamp).is_ok_and(|text| text == stamp_text())
    {
        println!(
            "vttest {VERSION} is up to date: {} (--force rebuilds it)",
            program.display()
        );
        return Ok(());
    }

    let tarball = crate::common::fetch_verified(
        root,
        &format!("vttest-{VERSION}.tgz"),
        URL,
        SHA256,
        MAX_TARBALL_BYTES,
    )?;
    let source = out.join(format!("vttest-{VERSION}"));
    if source.exists() {
        std::fs::remove_dir_all(&source)
            .with_context(|| format!("removing the old {}", source.display()))?;
    }
    // A failed build must not leave a stamp that claims an old program is current.
    for stale in [&stamp, &program] {
        if stale.exists() {
            std::fs::remove_file(stale).with_context(|| format!("removing {}", stale.display()))?;
        }
    }
    std::fs::create_dir_all(&out).with_context(|| format!("creating {}", out.display()))?;
    unpack(&tarball, &out)?;
    println!("unpacked {}", source.display());

    // An absolute path: `configure` finds its sources through `$0`.
    run_step(Command::new(source.join("configure")).current_dir(&source))
        .context("configure failed (it needs a C compiler: gcc or clang)")?;
    run_step(Command::new("make").current_dir(&source))
        .context("make failed (is make installed?)")?;

    let built = source.join("vttest");
    ensure!(built.is_file(), "make did not produce {}", built.display());
    std::fs::copy(&built, &program).with_context(|| format!("copying {}", built.display()))?;
    std::fs::write(&stamp, stamp_text()).with_context(|| format!("writing {}", stamp.display()))?;
    println!("vttest {VERSION} built: {}", program.display());
    Ok(())
}

/// Runs one build step with inherited output, failing on a non-zero exit.
fn run_step(command: &mut Command) -> Result<()> {
    let program = command.get_program().to_string_lossy().into_owned();
    println!("running {program}");
    let status = command
        .status()
        .with_context(|| format!("starting {program}"))?;
    ensure!(status.success(), "{program} exited with {status}");
    Ok(())
}

/// Checks that a tarball entry path is a plain relative path inside `vttest-<VERSION>/`.
fn is_allowed_entry_path(path: &Path) -> bool {
    let top = format!("vttest-{VERSION}");
    let mut components = path.components();
    matches!(components.next(), Some(Component::Normal(first)) if first == top.as_str())
        && components.all(|component| matches!(component, Component::Normal(_)))
}

/// Unpacks the verified tarball into `out`. Only regular files and folders below
/// `vttest-<VERSION>/` are accepted, and sizes are capped. File modes and times are kept, so
/// `make` doesn't try to regenerate `configure`.
fn unpack(tarball: &[u8], out: &Path) -> Result<()> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(tarball));
    let mut total: u64 = 0;
    for entry in archive.entries().context("reading the vttest tarball")? {
        let mut entry = entry.context("reading the vttest tarball")?;
        let path = entry.path()?.into_owned();
        ensure!(
            is_allowed_entry_path(&path),
            "unexpected path {} in the vttest tarball",
            path.display()
        );
        let kind = entry.header().entry_type();
        ensure!(
            kind.is_file() || kind.is_dir(),
            "{} in the vttest tarball is not a file or folder",
            path.display()
        );
        let size = entry.size();
        total = total.saturating_add(size);
        ensure!(
            size <= MAX_ENTRY_BYTES && total <= MAX_TOTAL_BYTES,
            "{} in the vttest tarball is unexpectedly large",
            path.display()
        );
        let unpacked = entry
            .unpack_in(out)
            .with_context(|| format!("unpacking {}", path.display()))?;
        ensure!(
            unpacked,
            "refused to unpack {} outside {}",
            path.display(),
            out.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_harness_expects_the_pinned_release() {
        // The vttest goldens were recorded with this release; the harness checks the version it
        // runs, so both constants must move together.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let harness =
            std::fs::read_to_string(root.join("crates/opensesh-term/tests/vttest.rs")).unwrap();
        let expected = format!("const VTTEST_VERSION: &str = \"{VERSION}\";");
        assert!(
            harness.contains(&expected),
            "crates/opensesh-term/tests/vttest.rs must declare {expected}"
        );
    }

    /// A gzip tarball with raw entry names, so hostile paths can be written too (`set_path`
    /// refuses `..`).
    fn tarball(entries: &[(&str, tar::EntryType)]) -> Vec<u8> {
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut builder = tar::Builder::new(encoder);
        for (path, kind) in entries {
            let data: &[u8] = if kind.is_file() { b"int main;" } else { b"" };
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(*kind);
            header.set_size(data.len() as u64);
            header.set_mode(if kind.is_dir() { 0o755 } else { 0o644 });
            header.as_old_mut().name[..path.len()].copy_from_slice(path.as_bytes());
            if *kind == tar::EntryType::Symlink {
                header.set_link_name("/etc/passwd").unwrap();
            }
            header.set_cksum();
            builder.append(&header, data).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn options_are_parsed() {
        assert_eq!(Options::parse([]).unwrap(), Options::default());
        assert!(Options::parse([OsString::from("--force")]).unwrap().force);
        assert!(Options::parse([OsString::from("--dest")]).is_err());
    }

    #[test]
    fn pins_are_consistent() {
        assert!(URL.starts_with("https://"));
        assert!(URL.ends_with(&format!("/vttest-{VERSION}.tgz")));
        assert!(crate::common::is_sha256_hex(SHA256));
        assert!(stamp_text().contains(VERSION));
    }

    #[test]
    fn entry_paths_must_stay_in_the_release_folder() {
        let top = format!("vttest-{VERSION}");
        assert!(is_allowed_entry_path(Path::new(&top)));
        assert!(is_allowed_entry_path(&Path::new(&top).join("configure")));
        assert!(is_allowed_entry_path(
            &Path::new(&top).join("package").join("vttest.spec")
        ));
        assert!(!is_allowed_entry_path(Path::new("configure")));
        assert!(!is_allowed_entry_path(Path::new(
            "vttest-20210210/configure"
        )));
        assert!(!is_allowed_entry_path(
            &Path::new(&top).join("..").join("x")
        ));
        assert!(!is_allowed_entry_path(Path::new("/etc/passwd")));
        assert!(!is_allowed_entry_path(Path::new("")));
    }

    #[test]
    fn a_release_tarball_is_unpacked() {
        let top = format!("vttest-{VERSION}");
        let bytes = tarball(&[
            (&format!("{top}/"), tar::EntryType::Directory),
            (&format!("{top}/main.c"), tar::EntryType::Regular),
        ]);
        let dir = tempfile::tempdir().unwrap();
        unpack(&bytes, dir.path()).unwrap();
        assert_eq!(
            std::fs::read(dir.path().join(&top).join("main.c")).unwrap(),
            b"int main;"
        );
    }

    #[test]
    fn hostile_entries_are_refused() {
        let top = format!("vttest-{VERSION}");
        let cases = [
            (format!("{top}/../escape"), tar::EntryType::Regular),
            ("escape".to_owned(), tar::EntryType::Regular),
            (format!("{top}/link"), tar::EntryType::Symlink),
            (format!("{top}/fifo"), tar::EntryType::Fifo),
        ];
        for (path, kind) in cases {
            let bytes = tarball(&[(&path, kind)]);
            let dir = tempfile::tempdir().unwrap();
            assert!(unpack(&bytes, dir.path()).is_err(), "{path}");
            assert!(!dir.path().join("escape").exists(), "{path}");
            assert!(!dir.path().join(&top).join("link").exists(), "{path}");
        }
    }
}
