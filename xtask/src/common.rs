//! Helpers shared by the asset tasks (`icons`, `fonts`, `i18n`): pinned downloads verified by
//! sha256, manifest field checks and change-only writes.

use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};

/// Download cache, relative to the workspace root.
pub const CACHE_DIR: &str = "target/xtask-cache";

/// Returns the bytes of `url`, verified against `sha256`. A cached copy
/// (`target/xtask-cache/<cache_name>`) is used when its checksum still matches; otherwise the file
/// is downloaded again. Nothing is returned (or cached) before the checksum is verified, so
/// callers never parse unverified data.
///
/// # Errors
///
/// Fails on an unsafe cache name, a non-HTTPS URL, network errors, a body larger than
/// `max_bytes` or a checksum mismatch.
pub fn fetch_verified(
    root: &Path,
    cache_name: &str,
    url: &str,
    sha256: &str,
    max_bytes: u64,
) -> Result<Vec<u8>> {
    ensure!(
        is_safe_file_name(cache_name),
        "invalid cache file name `{cache_name}`"
    );
    ensure!(url.starts_with("https://"), "`{url}` is not an HTTPS URL");
    let cache_path = root.join(CACHE_DIR).join(cache_name);

    if let Ok(bytes) = std::fs::read(&cache_path) {
        if sha256_hex(&bytes) == sha256 {
            return Ok(bytes);
        }
        eprintln!("cached {cache_name} fails the checksum, downloading it again");
    }

    println!("downloading {url}");
    let bytes = crate::http::get(url, max_bytes)?;
    let actual = sha256_hex(&bytes);
    ensure!(
        actual == sha256,
        "checksum mismatch for {url}\n  expected {sha256}\n  actual   {actual}"
    );
    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&cache_path, &bytes)
        .with_context(|| format!("writing {}", cache_path.display()))?;
    Ok(bytes)
}

/// Lowercase hex sha256 digest.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

/// Same format as `sha256_hex` produces, so a checksum can't mismatch on letter case alone.
pub fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Versions (`1.48.0`, `2.0.0-rc.1+build.5`) only need `[0-9A-Za-z.+-]`, so a version can be part
/// of a URL or a file name.
pub fn is_safe_version(version: &str) -> bool {
    !version.is_empty()
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'+' | b'-'))
}

/// A plain file name: `[A-Za-z0-9._@+-]`, not starting with a dot, so it can't name a parent
/// directory, a hidden file or another folder.
pub fn is_safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'@' | b'+' | b'-'))
}

/// Writes `contents` only if the file differs, so unchanged runs don't touch timestamps (the app's
/// `build.rs` watches the generated folders). Returns whether the file was written.
///
/// # Errors
///
/// Fails if the file can't be written.
pub fn write_if_changed(path: &Path, contents: impl AsRef<[u8]>) -> Result<bool> {
    let contents = contents.as_ref();
    if std::fs::read(path).is_ok_and(|current| current == contents) {
        return Ok(false);
    }
    std::fs::write(path, contents).with_context(|| format!("writing {}", path.display()))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "3c2ecda3d25f6a9692d83f8036d9a526f7da584a51af74cd16eda4498c5c33d8";

    #[test]
    fn sha256_is_lowercase_hex() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(is_sha256_hex(&sha256_hex(b"abc")));
    }

    #[test]
    fn checksums_must_be_lowercase_sha256_hex() {
        assert!(is_sha256_hex(SHA));
        assert!(!is_sha256_hex(&SHA.to_uppercase()));
        assert!(!is_sha256_hex(&SHA[..63]));
        assert!(!is_sha256_hex(&format!("{SHA}0")));
        assert!(!is_sha256_hex(&SHA.replace('c', "g")));
        assert!(!is_sha256_hex(""));
    }

    #[test]
    fn versions_are_restricted() {
        assert!(is_safe_version("2.0.0-rc.1+build.5"));
        assert!(is_safe_version("4.1"));
        assert!(!is_safe_version(""));
        assert!(!is_safe_version("1.0.0/../../x"));
        assert!(!is_safe_version("1.0.0\\x"));
        assert!(!is_safe_version("1.0 0"));
        assert!(!is_safe_version("1.0.0?x=1"));
        assert!(!is_safe_version("1.0.0\u{e9}"));
    }

    #[test]
    fn file_names_cannot_escape_their_folder() {
        assert!(is_safe_file_name("Inter-Regular.ttf"));
        assert!(is_safe_file_name("@tabler__icons-3.48.0.tgz"));
        assert!(is_safe_file_name("jetbrains_mono-2.304.zip"));
        assert!(!is_safe_file_name(""));
        assert!(!is_safe_file_name(".."));
        assert!(!is_safe_file_name(".hidden"));
        assert!(!is_safe_file_name("a/b.ttf"));
        assert!(!is_safe_file_name("a\\b.ttf"));
        assert!(!is_safe_file_name("C:x.ttf"));
        assert!(!is_safe_file_name("a b.ttf"));
    }

    #[test]
    fn only_https_urls_are_fetched() {
        let root = Path::new("does-not-exist");
        let error = fetch_verified(root, "x.zip", "http://example.org/x.zip", SHA, 1)
            .unwrap_err()
            .to_string();
        assert!(error.contains("HTTPS"), "{error}");
        assert!(fetch_verified(root, "../x.zip", "https://example.org/x.zip", SHA, 1).is_err());
    }

    #[test]
    fn unchanged_files_are_not_rewritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.bin");
        assert!(write_if_changed(&path, [0_u8, 1, 2]).unwrap());
        assert!(!write_if_changed(&path, [0_u8, 1, 2]).unwrap());
        assert!(write_if_changed(&path, "text").unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), b"text");
    }
}
