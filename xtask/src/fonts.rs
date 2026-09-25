//! `cargo xtask fonts`: reproducible bundled fonts (PLAN §5.1, Sprint 1).
//!
//! 1. Reads `assets/fonts/fonts.toml`.
//! 2. Downloads the pinned release zip of every source (cached under `target/xtask-cache/`) and
//!    verifies its sha256 before opening it.
//! 3. Extracts only the listed font files, byte for byte, to `crates/opensesh-app/fonts/` (the
//!    app's `build.rs` compiles every `.ttf` / `.otf` there into the Qt resources, and the app
//!    registers them with `QFontDatabase` at startup).
//! 4. Copies the upstream licenses to `assets/fonts/LICENSES/` and regenerates
//!    `THIRD_PARTY_NOTICES.md`.
//!
//! The generated files are committed, so regular builds work offline. Re-running the task
//! rewrites nothing when the manifest is unchanged.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read, Seek};
use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use crate::common::{
    fetch_verified, is_safe_file_name, is_safe_version, is_sha256_hex, write_if_changed,
};

/// Manifest location, relative to the workspace root.
const MANIFEST: &str = "assets/fonts/fonts.toml";
/// Output folder for the bundled fonts, relative to the workspace root.
const FONTS_OUT: &str = "crates/opensesh-app/fonts";
/// Folder for upstream license texts, relative to the workspace root.
pub const LICENSES_OUT: &str = "assets/fonts/LICENSES";
/// Upper bound for a downloaded release zip (Inter 4.1 is about 34 MB).
const MAX_ZIP_BYTES: u64 = 128 * 1024 * 1024;
/// Upper bound for one extracted font file.
const MAX_FONT_BYTES: u64 = 16 * 1024 * 1024;
/// Upper bound for an extracted license text.
const MAX_LICENSE_BYTES: u64 = 256 * 1024;
/// Extensions of the files the app registers as fonts (see `register_bundled_fonts`).
const FONT_EXTENSIONS: [&str; 2] = [".ttf", ".otf"];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    schema_version: u32,
    pub sources: BTreeMap<String, Source>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// Family name, as shown in the notices.
    pub name: String,
    pub version: String,
    /// HTTPS URL of the official release zip.
    pub url: String,
    /// sha256 of the zip.
    pub sha256: String,
    pub license: String,
    /// Path of the license text inside the zip.
    pub license_file: String,
    /// File name of the license copy in `assets/fonts/LICENSES/`.
    pub license_output: String,
    pub homepage: String,
    /// Zip entry path -> file name in `crates/opensesh-app/fonts/`.
    pub files: BTreeMap<String, String>,
}

/// Source ids may only use `[a-z0-9_-]`; they name the cache file.
fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-'))
}

fn is_font_file_name(name: &str) -> bool {
    is_safe_file_name(name)
        && FONT_EXTENSIONS
            .iter()
            .any(|ext| name.len() > ext.len() && name.ends_with(ext))
}

/// Checks every field that ends up in a URL, a file name or the notices before any is used.
fn validate_source(id: &str, source: &Source) -> Result<()> {
    ensure!(is_safe_id(id), "invalid font source name `{id}`");
    ensure!(
        !source.name.trim().is_empty(),
        "font source `{id}` needs a name"
    );
    ensure!(
        is_safe_version(&source.version),
        "font source `{id}` has an invalid version `{}`",
        source.version
    );
    ensure!(
        source.url.starts_with("https://") && !source.url.contains(char::is_whitespace),
        "font source `{id}` url must be an HTTPS URL"
    );
    ensure!(
        is_sha256_hex(&source.sha256),
        "font source `{id}` sha256 must be 64 lowercase hex characters"
    );
    ensure!(
        is_safe_file_name(&source.license_output),
        "font source `{id}` has an invalid license_output `{}`",
        source.license_output
    );
    ensure!(
        !source.files.is_empty(),
        "font source `{id}` lists no files"
    );
    for (entry, output) in &source.files {
        ensure!(
            !entry.is_empty(),
            "font source `{id}` has an empty zip entry"
        );
        ensure!(
            is_font_file_name(output),
            "font source `{id}`: `{output}` must be a plain .ttf or .otf file name"
        );
    }
    Ok(())
}

/// Reads and validates `assets/fonts/fonts.toml`.
///
/// # Errors
///
/// Fails if the manifest can't be read or parsed, has an invalid field, or two entries write the
/// same output file.
pub fn load_manifest(root: &Path) -> Result<Manifest> {
    let manifest_path = root.join(MANIFEST);
    let manifest: Manifest = toml::from_str(
        &std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?,
    )
    .with_context(|| format!("parsing {}", manifest_path.display()))?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn validate_manifest(manifest: &Manifest) -> Result<()> {
    ensure!(
        manifest.schema_version == 1,
        "unsupported fonts.toml schema_version {}",
        manifest.schema_version
    );
    let mut fonts = BTreeSet::new();
    let mut licenses = BTreeSet::new();
    for (id, source) in &manifest.sources {
        validate_source(id, source)?;
        ensure!(
            licenses.insert(source.license_output.as_str()),
            "license_output `{}` is used twice",
            source.license_output
        );
        for output in source.files.values() {
            // Case-insensitive file systems (Windows, macOS) would merge `A.ttf` and `a.ttf`.
            ensure!(
                fonts.insert(output.to_ascii_lowercase()),
                "font file `{output}` is written twice"
            );
        }
    }
    Ok(())
}

/// Runs the whole pipeline from the workspace root.
///
/// # Errors
///
/// Fails on network errors, checksum mismatches, missing zip entries or I/O errors.
pub fn run(root: &Path) -> Result<()> {
    let manifest = load_manifest(root)?;
    let fonts_out = root.join(FONTS_OUT);
    let licenses_out = root.join(LICENSES_OUT);
    std::fs::create_dir_all(&fonts_out)?;
    std::fs::create_dir_all(&licenses_out)?;

    let mut written = BTreeSet::new();
    for (id, source) in &manifest.sources {
        let zip = fetch_verified(
            root,
            &format!("{id}-{}.zip", source.version),
            &source.url,
            &source.sha256,
            MAX_ZIP_BYTES,
        )?;
        let mut archive = zip::ZipArchive::new(Cursor::new(zip.as_slice()))
            .with_context(|| format!("opening the {} zip", source.name))?;

        let license = read_entry(&mut archive, &source.license_file, MAX_LICENSE_BYTES)?;
        write_if_changed(&licenses_out.join(&source.license_output), &license)?;
        println!(
            "license {LICENSES_OUT}/{} <- {id}:{}",
            source.license_output, source.license_file
        );

        for (entry, output) in &source.files {
            let font = read_entry(&mut archive, entry, MAX_FONT_BYTES)?;
            write_if_changed(&fonts_out.join(output), &font)?;
            written.insert(output.clone());
            println!("font {output:<28} <- {id}:{entry}");
        }
    }
    remove_stale_fonts(&fonts_out, &written)?;

    crate::notices::write(root)
}

/// Reads one regular file out of the archive, refusing anything larger than `max_bytes` (the
/// declared size is checked first, then the actual stream is capped too). Reading to the end
/// makes the zip crate verify the entry's CRC-32.
fn read_entry<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
    max_bytes: u64,
) -> Result<Vec<u8>> {
    let file = archive
        .by_name(name)
        .with_context(|| format!("{name} not found in the zip"))?;
    ensure!(file.is_file(), "{name} is not a regular file");
    ensure!(file.size() <= max_bytes, "{name} is unexpectedly large");
    let mut bytes = Vec::new();
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .with_context(|| format!("reading {name} from the zip"))?;
    ensure!(
        u64::try_from(bytes.len()).is_ok_and(|len| len <= max_bytes),
        "{name} is unexpectedly large"
    );
    Ok(bytes)
}

/// Deletes font files that are no longer listed in the manifest. `keep` holds the files just
/// written, so each of them is on disk.
///
/// On a case-insensitive file system (Windows, macOS), writing `Inter-Regular.ttf` over an
/// existing `inter-regular.ttf` keeps the old spelling. That file is the one just written, so it is
/// renamed to the manifest's spelling instead of being deleted. A case-sensitive file system lists
/// both names, and the old one is stale.
fn remove_stale_fonts(dir: &Path, keep: &BTreeSet<String>) -> Result<()> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_file()
            && let Some(name) = path.file_name()
        {
            names.push(name.to_string_lossy().into_owned());
        }
    }
    for name in &names {
        let is_font = FONT_EXTENSIONS
            .iter()
            .any(|ext| name.to_ascii_lowercase().ends_with(ext));
        if !is_font || keep.contains(name) {
            continue;
        }
        let path = dir.join(name);
        if let Some(wanted) = keep
            .iter()
            .find(|wanted| wanted.eq_ignore_ascii_case(name) && !names.contains(wanted))
        {
            std::fs::rename(&path, dir.join(wanted))
                .with_context(|| format!("renaming {} to {wanted}", path.display()))?;
            println!("renamed {} to {wanted}", path.display());
        } else {
            std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
            println!("removed stale {}", path.display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    const SHA: &str = "9883fdd4a49d4fb66bd8177ba6625ef9a64aa45899767dde3d36aa425756b11e";

    fn source() -> Source {
        Source {
            name: "Inter".into(),
            version: "4.1".into(),
            url: "https://github.com/rsms/inter/releases/download/v4.1/Inter-4.1.zip".into(),
            sha256: SHA.into(),
            license: "OFL-1.1".into(),
            license_file: "LICENSE.txt".into(),
            license_output: "inter-OFL.txt".into(),
            homepage: "https://rsms.me/inter/".into(),
            files: BTreeMap::from([(
                "extras/ttf/Inter-Regular.ttf".into(),
                "Inter-Regular.ttf".into(),
            )]),
        }
    }

    #[test]
    fn valid_sources_pass() {
        assert!(validate_source("inter", &source()).is_ok());
        assert!(validate_source("jetbrains_mono", &source()).is_ok());
    }

    #[test]
    fn source_fields_are_validated() {
        assert!(validate_source("Inter", &source()).is_err());
        assert!(validate_source("../inter", &source()).is_err());

        let mut bad = source();
        bad.url = "http://github.com/rsms/inter/releases/download/v4.1/Inter-4.1.zip".into();
        assert!(validate_source("inter", &bad).is_err());

        let mut bad = source();
        bad.sha256 = SHA.to_uppercase();
        assert!(validate_source("inter", &bad).is_err());

        let mut bad = source();
        bad.version = "4.1/../x".into();
        assert!(validate_source("inter", &bad).is_err());

        let mut bad = source();
        bad.license_output = "../OFL.txt".into();
        assert!(validate_source("inter", &bad).is_err());

        let mut bad = source();
        bad.files.clear();
        assert!(validate_source("inter", &bad).is_err());

        for output in [
            "../Inter.ttf",
            "Inter.woff2",
            ".ttf",
            "fonts/Inter.ttf",
            "Inter",
        ] {
            let mut bad = source();
            bad.files = BTreeMap::from([("Inter.ttf".into(), output.into())]);
            assert!(validate_source("inter", &bad).is_err(), "{output}");
        }
    }

    #[test]
    fn outputs_must_be_unique() {
        let mut other = source();
        other.license_output = "other-OFL.txt".into();
        other.files = BTreeMap::from([("x/INTER-REGULAR.TTF".into(), "inter-regular.ttf".into())]);
        let manifest = Manifest {
            schema_version: 1,
            sources: BTreeMap::from([("inter".into(), source()), ("other".into(), other)]),
        };
        assert!(validate_manifest(&manifest).is_err());
    }

    fn zip_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        {
            // Stored entries keep the test independent of the enabled compression features.
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
    fn entries_are_read_verbatim_and_capped() {
        let bytes = zip_with(&[("fonts/a.ttf", b"font bytes"), ("LICENSE.txt", b"OFL")]);
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes.as_slice())).unwrap();
        assert_eq!(
            read_entry(&mut archive, "fonts/a.ttf", 1024).unwrap(),
            b"font bytes"
        );
        assert_eq!(read_entry(&mut archive, "LICENSE.txt", 3).unwrap(), b"OFL");
        assert!(read_entry(&mut archive, "LICENSE.txt", 2).is_err());
        assert!(read_entry(&mut archive, "missing.ttf", 1024).is_err());
    }

    #[test]
    fn stale_fonts_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["Keep.ttf", "Old.ttf", "Old.OTF", "README.txt"] {
            std::fs::write(dir.path().join(name), name).unwrap();
        }
        remove_stale_fonts(dir.path(), &BTreeSet::from(["Keep.ttf".to_owned()])).unwrap();
        let mut left: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["Keep.ttf", "README.txt"]);
    }

    #[test]
    fn a_font_whose_name_only_changed_case_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("inter-regular.ttf"), "old").unwrap();
        // What `run` does: a case-insensitive file system writes through the old entry and keeps
        // its spelling; a case-sensitive one creates a second file.
        write_if_changed(&dir.path().join("Inter-Regular.ttf"), "new").unwrap();
        remove_stale_fonts(
            dir.path(),
            &BTreeSet::from(["Inter-Regular.ttf".to_owned()]),
        )
        .unwrap();
        let left: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, ["Inter-Regular.ttf"]);
        assert_eq!(
            std::fs::read(dir.path().join("Inter-Regular.ttf")).unwrap(),
            b"new"
        );
    }

    #[test]
    fn a_differently_cased_file_is_renamed_to_the_manifest_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("keep.TTF"), "font").unwrap();
        remove_stale_fonts(dir.path(), &BTreeSet::from(["Keep.ttf".to_owned()])).unwrap();
        let left: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, ["Keep.ttf"]);
    }

    #[test]
    fn manifest_in_repo_parses() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let manifest = load_manifest(&root).unwrap();
        assert!(manifest.sources.contains_key("inter"));
        assert!(manifest.sources.contains_key("jetbrains_mono"));
    }
}
