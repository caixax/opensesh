//! `cargo xtask conpty`: the bundled Windows pseudoconsole host ([ADR 0014]).
//!
//! 1. Reads `assets/conpty/conpty.toml`.
//! 2. Downloads the pinned `Microsoft.Windows.Console.ConPTY` NuGet package (cached under
//!    `target/xtask-cache/`) and verifies its sha256 before opening it.
//! 3. Extracts the listed x64 files (`conpty.dll`, `OpenConsole.exe`), checks each one against its
//!    own sha256 and keeps them in `target/xtask-cache/conpty-<version>-x64/`.
//! 4. Copies them next to the app executables: `target/debug` and `target/release` (whichever
//!    exist, in `CARGO_TARGET_DIR` when it is set), or the `--dest` folders.
//! 5. Copies the upstream license to `assets/conpty/` and regenerates `THIRD_PARTY_NOTICES.md`.
//!
//! portable-pty loads `conpty.dll` with a bare name, so the copy in the executable's folder wins
//! the DLL search order, and that `conpty.dll` starts the `OpenConsole.exe` beside it. The app
//! still works without the files: portable-pty then uses the ConPTY built into Windows.
//!
//! `--remove` deletes the copies again (to test the built-in ConPTY), without using the network.
//!
//! [ADR 0014]: ../../docs/adr/0014-bundled-conpty.md

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io::{Cursor, Read, Seek};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;

use crate::common::{
    CACHE_DIR, fetch_verified, is_safe_file_name, is_safe_version, is_sha256_hex, sha256_hex,
    write_if_changed,
};

/// Manifest location, relative to the workspace root.
const MANIFEST: &str = "assets/conpty/conpty.toml";
/// Folder of the license copy, relative to the workspace root.
pub const LICENSE_DIR: &str = "assets/conpty";
/// Build profiles whose output folders receive the files.
const PROFILES: [&str; 2] = ["debug", "release"];
/// Upper bound for the downloaded package (1.24.260710001 is 1.7 MB).
const MAX_NUPKG_BYTES: u64 = 32 * 1024 * 1024;
/// Upper bound for one extracted file (`OpenConsole.exe` x64 is 1.1 MB).
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
/// Upper bound for the license text.
const MAX_LICENSE_BYTES: u64 = 64 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    schema_version: u32,
    pub package: Package,
    /// Entry path in the package -> file copied next to the executable.
    pub files: BTreeMap<String, BundledFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    /// NuGet package id, as shown in the notices.
    pub id: String,
    pub version: String,
    /// HTTPS URL of the `.nupkg`.
    pub url: String,
    /// sha256 of the `.nupkg`.
    pub sha256: String,
    pub license: String,
    /// HTTPS URL of the upstream license text (the package ships none).
    pub license_url: String,
    pub license_sha256: String,
    /// File name of the license copy in `assets/conpty/`.
    pub license_output: String,
    pub homepage: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundledFile {
    /// File name next to the executable.
    pub output: String,
    /// sha256 of the extracted file.
    pub sha256: String,
}

/// Command-line options of `cargo xtask conpty`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Folders to copy the files into, instead of the Cargo profile folders.
    dest: Vec<PathBuf>,
    /// Delete the copies instead of installing them.
    remove: bool,
}

impl Options {
    /// Parses the arguments that follow `conpty`.
    ///
    /// # Errors
    ///
    /// Fails on an unknown argument or a `--dest` without a folder.
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self> {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--dest") => {
                    let dir = args.next().context("--dest needs a folder")?;
                    options.dest.push(PathBuf::from(dir));
                }
                Some("--remove") => options.remove = true,
                _ => bail!(
                    "unknown conpty argument `{}` (see `cargo xtask help`)",
                    arg.to_string_lossy()
                ),
            }
        }
        Ok(options)
    }
}

/// Package ids may only use `[A-Za-z0-9.]`; the id names the cache file.
fn is_safe_package_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('.')
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.')
}

fn is_https_url(url: &str) -> bool {
    url.starts_with("https://") && !url.contains(char::is_whitespace)
}

/// Reads and validates `assets/conpty/conpty.toml`.
///
/// # Errors
///
/// Fails if the manifest can't be read or parsed, or has an invalid field.
pub fn load_manifest(root: &Path) -> Result<Manifest> {
    let path = root.join(MANIFEST);
    let manifest: Manifest = toml::from_str(
        &std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?,
    )
    .with_context(|| format!("parsing {}", path.display()))?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

/// Checks every field that ends up in a URL, a file name or the notices before any is used.
fn validate_manifest(manifest: &Manifest) -> Result<()> {
    ensure!(
        manifest.schema_version == 1,
        "unsupported conpty.toml schema_version {}",
        manifest.schema_version
    );
    let package = &manifest.package;
    ensure!(
        is_safe_package_id(&package.id),
        "invalid package id `{}`",
        package.id
    );
    ensure!(
        is_safe_version(&package.version),
        "invalid package version `{}`",
        package.version
    );
    ensure!(
        is_https_url(&package.url),
        "package url must be an HTTPS URL"
    );
    ensure!(
        is_https_url(&package.license_url),
        "license_url must be an HTTPS URL"
    );
    ensure!(
        is_sha256_hex(&package.sha256) && is_sha256_hex(&package.license_sha256),
        "sha256 and license_sha256 must be 64 lowercase hex characters"
    );
    ensure!(
        !package.license.trim().is_empty() && !package.homepage.trim().is_empty(),
        "license and homepage must not be empty"
    );
    ensure!(
        is_safe_file_name(&package.license_output),
        "invalid license_output `{}`",
        package.license_output
    );
    ensure!(!manifest.files.is_empty(), "conpty.toml lists no files");
    let mut outputs = BTreeSet::new();
    for (entry, file) in &manifest.files {
        ensure!(!entry.is_empty(), "conpty.toml has an empty package entry");
        ensure!(
            is_safe_file_name(&file.output),
            "`{}` must be a plain file name",
            file.output
        );
        ensure!(
            is_sha256_hex(&file.sha256),
            "the sha256 of `{entry}` must be 64 lowercase hex characters"
        );
        // Windows file names are case-insensitive.
        ensure!(
            outputs.insert(file.output.to_ascii_lowercase()),
            "`{}` is written twice",
            file.output
        );
    }
    Ok(())
}

/// Runs the task from the workspace root.
///
/// # Errors
///
/// Fails on network errors, checksum mismatches, missing package entries or I/O errors (for
/// example when a running OpenSesh keeps `OpenConsole.exe` open).
pub fn run(root: &Path, options: &Options) -> Result<()> {
    let manifest = load_manifest(root)?;
    let destinations = destinations(root, options)?;
    if options.remove {
        for dir in &destinations {
            remove_from(dir, &manifest)?;
        }
        return Ok(());
    }

    let package = &manifest.package;
    let nupkg = fetch_verified(
        root,
        &format!(
            "{}.{}.nupkg",
            package.id.to_ascii_lowercase(),
            package.version
        ),
        &package.url,
        &package.sha256,
        MAX_NUPKG_BYTES,
    )?;
    let files = extract(&nupkg, &manifest)?;

    // Joined per component, so the printed path uses one kind of separator on Windows.
    let cache = CACHE_DIR
        .split('/')
        .fold(root.to_path_buf(), |path, part| path.join(part))
        .join(format!("conpty-{}-x64", package.version));
    std::fs::create_dir_all(&cache).with_context(|| format!("creating {}", cache.display()))?;
    for (name, bytes) in &files {
        write_if_changed(&cache.join(name), bytes)?;
    }
    println!(
        "{} {} extracted to {}",
        package.id,
        package.version,
        cache.display()
    );

    let license = fetch_verified(
        root,
        &format!("conpty-{}-{}", package.version, package.license_output),
        &package.license_url,
        &package.license_sha256,
        MAX_LICENSE_BYTES,
    )?;
    // The repository stores text with LF line endings (.gitattributes); upstream uses CRLF.
    let license = String::from_utf8(license)
        .context("the ConPTY license is not UTF-8")?
        .replace("\r\n", "\n");
    let license_path = root.join(LICENSE_DIR).join(&package.license_output);
    write_if_changed(&license_path, license)?;
    println!("license {LICENSE_DIR}/{}", package.license_output);
    crate::notices::write(root)?;

    if destinations.is_empty() {
        println!(
            "ConPTY is only used on Windows: nothing copied (pass --dest <folder> to copy the \
             files somewhere)"
        );
    }
    for dir in &destinations {
        install_into(dir, &files)?;
    }
    Ok(())
}

/// The folders that receive the files: the `--dest` folders, or on Windows the Cargo profile
/// folders that exist.
fn destinations(root: &Path, options: &Options) -> Result<Vec<PathBuf>> {
    if !options.dest.is_empty() {
        for dir in &options.dest {
            ensure!(dir.is_dir(), "--dest {} is not a folder", dir.display());
        }
        return Ok(options.dest.clone());
    }
    if !cfg!(windows) {
        return Ok(Vec::new());
    }
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
    Ok(dirs)
}

/// Cargo's target folder: `CARGO_TARGET_DIR` (or `CARGO_BUILD_TARGET_DIR`) when set, relative
/// paths resolved against the current folder as Cargo does, otherwise `<workspace>/target`.
/// A `build.target-dir` set only in a Cargo config file is not seen: pass `--dest` then.
///
/// # Errors
///
/// Fails if the current folder can't be read while resolving a relative path.
pub fn cargo_target_dir(root: &Path) -> Result<PathBuf> {
    let configured = ["CARGO_TARGET_DIR", "CARGO_BUILD_TARGET_DIR"]
        .iter()
        .filter_map(std::env::var_os)
        .find(|value| !value.is_empty());
    match configured {
        Some(dir) => {
            let dir = PathBuf::from(dir);
            if dir.is_absolute() {
                Ok(dir)
            } else {
                Ok(std::env::current_dir()
                    .context("reading the current folder")?
                    .join(dir))
            }
        }
        None => Ok(root.join("target")),
    }
}

/// Reads every listed file out of the verified package and checks it against its own sha256.
/// Returns `(output name, bytes)` pairs.
fn extract(nupkg: &[u8], manifest: &Manifest) -> Result<Vec<(String, Vec<u8>)>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(nupkg)).context("opening the nupkg")?;
    let mut files = Vec::with_capacity(manifest.files.len());
    for (entry, file) in &manifest.files {
        let bytes = read_entry(&mut archive, entry, MAX_FILE_BYTES)?;
        let actual = sha256_hex(&bytes);
        ensure!(
            actual == file.sha256,
            "checksum mismatch for {entry}\n  expected {}\n  actual   {actual}",
            file.sha256
        );
        files.push((file.output.clone(), bytes));
    }
    Ok(files)
}

/// Reads one regular file out of the archive, refusing anything larger than `max_bytes` (the
/// declared size first, then the actual stream). Reading to the end makes the zip crate verify
/// the entry's CRC-32.
fn read_entry<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
    max_bytes: u64,
) -> Result<Vec<u8>> {
    let file = archive
        .by_name(name)
        .with_context(|| format!("{name} not found in the package"))?;
    ensure!(file.is_file(), "{name} is not a regular file");
    ensure!(file.size() <= max_bytes, "{name} is unexpectedly large");
    let mut bytes = Vec::new();
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .with_context(|| format!("reading {name} from the package"))?;
    ensure!(
        u64::try_from(bytes.len()).is_ok_and(|len| len <= max_bytes),
        "{name} is unexpectedly large"
    );
    Ok(bytes)
}

/// Copies the files into `dir`, leaving identical copies untouched.
fn install_into(dir: &Path, files: &[(String, Vec<u8>)]) -> Result<()> {
    for (name, bytes) in files {
        let path = dir.join(name);
        let written = write_if_changed(&path, bytes).with_context(|| {
            format!(
                "installing {} (if OpenSesh or a test is still running, close it and retry)",
                path.display()
            )
        })?;
        let state = if written { "installed" } else { "up to date" };
        println!("{state} {}", path.display());
    }
    Ok(())
}

/// Deletes the copies from `dir`, so the app falls back to the ConPTY built into Windows.
fn remove_from(dir: &Path, manifest: &Manifest) -> Result<()> {
    for file in manifest.files.values() {
        let path = dir.join(&file.output);
        match std::fs::remove_file(&path) {
            Ok(()) => println!("removed {}", path.display()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "removing {} (if OpenSesh is still running, close it and retry)",
                        path.display()
                    )
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    const SHA: &str = "175640566a3b59c4b132070ee96c2c77e5ab7edd2e92732a5eb3610bbf63d90e";

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
    }

    fn manifest() -> Manifest {
        Manifest {
            schema_version: 1,
            package: Package {
                id: "Microsoft.Windows.Console.ConPTY".into(),
                version: "1.24.260710001".into(),
                url: "https://api.nuget.org/v3-flatcontainer/x/1/x.nupkg".into(),
                sha256: SHA.into(),
                license: "MIT".into(),
                license_url: "https://raw.githubusercontent.com/microsoft/terminal/v1/LICENSE"
                    .into(),
                license_sha256: SHA.into(),
                license_output: "LICENSE-MIT.txt".into(),
                homepage: "https://github.com/microsoft/terminal".into(),
            },
            files: BTreeMap::from([(
                "runtimes/win-x64/native/conpty.dll".into(),
                BundledFile {
                    output: "conpty.dll".into(),
                    sha256: sha256_hex(b"dll"),
                },
            )]),
        }
    }

    #[test]
    fn manifest_in_repo_parses() {
        let manifest = load_manifest(&repo_root()).unwrap();
        let outputs: Vec<&str> = manifest
            .files
            .values()
            .map(|file| file.output.as_str())
            .collect();
        assert_eq!(outputs, ["OpenConsole.exe", "conpty.dll"]);
        assert!(manifest.package.url.contains(&manifest.package.version));
    }

    #[test]
    fn valid_manifest_passes() {
        assert!(validate_manifest(&manifest()).is_ok());
    }

    #[test]
    fn manifest_fields_are_validated() {
        let mut bad = manifest();
        bad.package.url = "http://api.nuget.org/x.nupkg".into();
        assert!(validate_manifest(&bad).is_err());

        let mut bad = manifest();
        bad.package.license_url = "https://example.org/LICENSE with space".into();
        assert!(validate_manifest(&bad).is_err());

        let mut bad = manifest();
        bad.package.sha256 = SHA.to_uppercase();
        assert!(validate_manifest(&bad).is_err());

        let mut bad = manifest();
        bad.package.id = "../conpty".into();
        assert!(validate_manifest(&bad).is_err());

        let mut bad = manifest();
        bad.package.version = "1.24/../x".into();
        assert!(validate_manifest(&bad).is_err());

        let mut bad = manifest();
        bad.package.license_output = "../LICENSE".into();
        assert!(validate_manifest(&bad).is_err());

        let mut bad = manifest();
        bad.files.clear();
        assert!(validate_manifest(&bad).is_err());

        for output in [
            "../conpty.dll",
            "x64/conpty.dll",
            "x64\\conpty.dll",
            ".dll",
            "",
        ] {
            let mut bad = manifest();
            bad.files.insert(
                "other".into(),
                BundledFile {
                    output: output.into(),
                    sha256: SHA.into(),
                },
            );
            assert!(validate_manifest(&bad).is_err(), "{output}");
        }

        let mut bad = manifest();
        bad.files.insert(
            "runtimes/win-arm64/native/conpty.dll".into(),
            BundledFile {
                output: "ConPTY.DLL".into(),
                sha256: SHA.into(),
            },
        );
        assert!(validate_manifest(&bad).is_err());
    }

    #[test]
    fn options_are_parsed() {
        assert_eq!(Options::parse([]).unwrap(), Options::default());
        let options =
            Options::parse(["--dest", "a", "--remove", "--dest", "b"].map(OsString::from)).unwrap();
        assert_eq!(options.dest, [PathBuf::from("a"), PathBuf::from("b")]);
        assert!(options.remove);
        assert!(Options::parse([OsString::from("--dest")]).is_err());
        assert!(Options::parse([OsString::from("--force")]).is_err());
    }

    fn nupkg_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut out);
            for (name, bytes) in entries {
                writer
                    .start_file(
                        *name,
                        zip::write::SimpleFileOptions::default()
                            .compression_method(zip::CompressionMethod::Stored),
                    )
                    .unwrap();
                writer.write_all(bytes).unwrap();
            }
            writer.finish().unwrap();
        }
        out.into_inner()
    }

    #[test]
    fn files_are_extracted_and_checked() {
        let manifest = manifest();
        let nupkg = nupkg_with(&[("runtimes/win-x64/native/conpty.dll", b"dll")]);
        let files = extract(&nupkg, &manifest).unwrap();
        assert_eq!(files, [("conpty.dll".to_owned(), b"dll".to_vec())]);

        let tampered = nupkg_with(&[("runtimes/win-x64/native/conpty.dll", b"DLL")]);
        let error = extract(&tampered, &manifest).unwrap_err().to_string();
        assert!(error.contains("checksum mismatch"), "{error}");

        let missing = nupkg_with(&[("runtimes/win-x86/native/conpty.dll", b"dll")]);
        assert!(extract(&missing, &manifest).is_err());
    }

    #[test]
    fn install_and_remove_touch_only_the_listed_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("opensesh-app.exe"), "app").unwrap();
        let files = [("conpty.dll".to_owned(), b"dll".to_vec())];
        install_into(dir.path(), &files).unwrap();
        assert_eq!(
            std::fs::read(dir.path().join("conpty.dll")).unwrap(),
            b"dll"
        );
        // A second run finds an identical copy.
        install_into(dir.path(), &files).unwrap();

        remove_from(dir.path(), &manifest()).unwrap();
        assert!(!dir.path().join("conpty.dll").exists());
        assert!(dir.path().join("opensesh-app.exe").is_file());
        // Removing again is not an error.
        remove_from(dir.path(), &manifest()).unwrap();
    }

    #[test]
    fn a_missing_dest_folder_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let options = Options {
            dest: vec![dir.path().join("missing")],
            remove: false,
        };
        assert!(destinations(dir.path(), &options).is_err());
        let options = Options {
            dest: vec![dir.path().to_path_buf()],
            remove: true,
        };
        assert_eq!(
            destinations(dir.path(), &options).unwrap(),
            [dir.path().to_path_buf()]
        );
    }
}
