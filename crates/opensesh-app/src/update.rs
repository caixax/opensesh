//! Update checks against GitHub Releases (PLAN §6.1).
//!
//! Nothing here runs unless the user enabled "Check for updates" (off by default) or asked for a
//! check: the only request is `GET` of the latest release's metadata, with no identifier beyond
//! the app version in the `User-Agent`. On a Windows installation made by the NSIS installer, an
//! update downloads the new installer, checks it against the release's `SHA256SUMS.txt` and runs
//! it silently (`/S /UPDATE`); it replaces the app and starts it again. Portable copies and Linux
//! packages are only told where the new version is.
//!
//! Every function here blocks on the network: call them from a worker thread.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// The latest published (non-draft, non-prerelease) release.
const LATEST_URL: &str = "https://api.github.com/repos/caixax/opensesh/releases/latest";
/// Suffix of the Windows installer asset (`OpenSesh-<version>-windows-x64-setup.exe`).
const INSTALLER_SUFFIX: &str = "-windows-x64-setup.exe";
const SUMS_ASSET: &str = "SHA256SUMS.txt";
/// Largest accepted downloads.
const MAX_METADATA: u64 = 1024 * 1024;
const MAX_INSTALLER: u64 = 512 * 1024 * 1024;
/// Name of the uninstaller the NSIS installer writes next to the app.
const UNINSTALLER: &str = "Uninstall OpenSesh.exe";

static AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| {
    ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(5)
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_global(Some(Duration::from_secs(600)))
        .build()
        .into()
});

/// A `major.minor.patch` version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u32, pub u32, pub u32);

impl Version {
    /// Parses `1.2.3` or `v1.2.3`. Pre-releases (`1.2.3-rc1`) and anything else give `None`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.strip_prefix('v').unwrap_or(text).split('.');
        let mut next = || parts.next()?.parse::<u32>().ok();
        let version = Self(next()?, next()?, next()?);
        parts.next().is_none().then_some(version)
    }

    /// The version of this build.
    #[must_use]
    pub fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).unwrap_or(Self(0, 0, 0))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// How this copy of the app was installed, which decides what an update can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallKind {
    /// Installed by the NSIS installer: can update itself.
    WindowsInstaller,
    /// The portable zip (a `portable` marker next to the executable).
    WindowsPortable,
    /// A Linux package in `/usr`: updated by the package manager or `install.sh`.
    LinuxPackage,
    /// A development build or an unknown layout.
    Other,
}

impl InstallKind {
    /// Detects the kind of the running executable.
    #[must_use]
    pub fn detect() -> Self {
        let Ok(exe) = std::env::current_exe() else {
            return Self::Other;
        };
        let Some(dir) = exe.parent() else {
            return Self::Other;
        };
        Self::from_layout(dir, cfg!(windows))
    }

    fn from_layout(dir: &Path, windows: bool) -> Self {
        if windows {
            if dir.join(opensesh_core::paths::PORTABLE_MARKER).is_file() {
                Self::WindowsPortable
            } else if dir.join(UNINSTALLER).is_file() {
                Self::WindowsInstaller
            } else {
                Self::Other
            }
        } else if dir.starts_with("/usr") {
            Self::LinuxPackage
        } else {
            Self::Other
        }
    }

    /// A short name for QML: `installer`, `portable`, `package` or `other`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WindowsInstaller => "installer",
            Self::WindowsPortable => "portable",
            Self::LinuxPackage => "package",
            Self::Other => "other",
        }
    }
}

/// A published release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// Its version.
    pub version: Version,
    /// Its page on GitHub.
    pub page: String,
    /// Its downloadable files.
    pub assets: Vec<Asset>,
}

/// A file of a release.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Asset {
    /// File name.
    pub name: String,
    /// Download URL.
    #[serde(rename = "browser_download_url")]
    pub url: String,
    /// Size in bytes.
    pub size: u64,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

fn get(url: &str, accept: &str, limit: u64) -> Result<Vec<u8>> {
    let mut response = AGENT
        .get(url)
        .header(
            "User-Agent",
            concat!("OpenSesh/", env!("CARGO_PKG_VERSION"), " (update check)"),
        )
        .header("Accept", accept)
        .call()
        .with_context(|| format!("GET {url}"))?;
    response
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .with_context(|| format!("reading {url}"))
}

/// Parses the GitHub API answer for the latest release.
fn parse_release(json: &[u8]) -> Result<Option<Release>> {
    let release: ApiRelease = serde_json::from_slice(json).context("reading the release data")?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let Some(version) = Version::parse(&release.tag_name) else {
        return Ok(None);
    };
    Ok(Some(Release {
        version,
        page: release.html_url,
        assets: release.assets,
    }))
}

/// Asks GitHub for the latest release. Returns it when it is newer than this build.
///
/// # Errors
///
/// Fails on network errors or unexpected answers.
pub fn check() -> Result<Option<Release>> {
    let json = match get(LATEST_URL, "application/vnd.github+json", MAX_METADATA) {
        Ok(json) => json,
        // No published release at all.
        Err(error)
            if matches!(
                error.downcast_ref::<ureq::Error>(),
                Some(ureq::Error::StatusCode(404))
            ) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    Ok(parse_release(&json)?.filter(|release| release.version > Version::current()))
}

/// The sha256 listed for `name` in a `SHA256SUMS.txt` (`<hex>  <name>` lines).
fn listed_sha256<'a>(sums: &'a str, name: &str) -> Option<&'a str> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.split_once(char::is_whitespace)?;
        (file.trim_start_matches([' ', '*']) == name && hash.len() == 64).then_some(hash)
    })
}

/// Downloads the release's Windows installer into the temporary folder and verifies it against
/// the release's `SHA256SUMS.txt`. Returns its path.
///
/// # Errors
///
/// Fails if the release has no installer or checksums, on network errors, or on a mismatch.
pub fn download_installer(release: &Release) -> Result<PathBuf> {
    let installer = release
        .assets
        .iter()
        .find(|asset| asset.name.ends_with(INSTALLER_SUFFIX))
        .context("the release has no Windows installer")?;
    let sums = release
        .assets
        .iter()
        .find(|asset| asset.name == SUMS_ASSET)
        .context("the release has no SHA256SUMS.txt")?;
    ensure!(
        installer
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)),
        "unexpected installer name {}",
        installer.name
    );
    let sums = String::from_utf8(get(&sums.url, "application/octet-stream", MAX_METADATA)?)
        .context("SHA256SUMS.txt is not text")?;
    let expected = listed_sha256(&sums, &installer.name)
        .context("SHA256SUMS.txt doesn't list the installer")?
        .to_ascii_lowercase();
    let bytes = get(&installer.url, "application/octet-stream", MAX_INSTALLER)?;
    let actual = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if actual != expected {
        bail!("the downloaded installer doesn't match its checksum");
    }
    let dir = std::env::temp_dir().join("OpenSesh-update");
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = dir.join(&installer.name);
    std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// Starts the downloaded installer silently over the current installation. The app must quit
/// right after: the installer waits for it, replaces it and starts it again.
///
/// # Errors
///
/// Fails if the installer can't be started.
pub fn start_installer(installer: &Path) -> Result<()> {
    let exe = std::env::current_exe().context("finding the running executable")?;
    let dir = exe.parent().context("the executable has no folder")?;
    std::process::Command::new(installer)
        .arg("/S")
        .arg("/UPDATE")
        // NSIS reads /D= raw, up to the end of the command line: it must come last, unquoted.
        .arg(format!("/D={}", dir.display()))
        .spawn()
        .with_context(|| format!("starting {}", installer.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Talks to GitHub: `cargo test -p opensesh-app -- --ignored real_release`.
    #[test]
    #[ignore = "uses the network"]
    fn real_release_check_and_installer_download() {
        let release = check().unwrap();
        println!("newer release: {release:?}");
        // Before the first release GitHub answers 404: nothing more to try.
        let Ok(json) = get(LATEST_URL, "application/vnd.github+json", MAX_METADATA) else {
            return;
        };
        if let Some(latest) = parse_release(&json).unwrap() {
            let path = download_installer(&latest).unwrap();
            println!("verified installer: {}", path.display());
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn versions_parse_and_compare() {
        assert_eq!(Version::parse("v0.1.0"), Some(Version(0, 1, 0)));
        assert_eq!(Version::parse("1.20.3"), Some(Version(1, 20, 3)));
        assert_eq!(Version::parse("1.2.3-rc1"), None);
        assert_eq!(Version::parse("1.2"), None);
        assert_eq!(Version::parse("1.2.3.4"), None);
        assert!(Version(0, 10, 0) > Version(0, 9, 12));
        assert!(Version(1, 0, 0) > Version(0, 99, 99));
    }

    #[test]
    fn github_answers_are_read_and_filtered() {
        let json = br#"{"tag_name":"v0.2.0","html_url":"https://github.com/caixax/opensesh/releases/tag/v0.2.0",
            "draft":false,"prerelease":false,"assets":[{"name":"SHA256SUMS.txt",
            "browser_download_url":"https://example.org/SHA256SUMS.txt","size":10}]}"#;
        let release = parse_release(json).unwrap().unwrap();
        assert_eq!(release.version, Version(0, 2, 0));
        assert_eq!(release.assets[0].name, "SHA256SUMS.txt");
        let prerelease = br#"{"tag_name":"v0.3.0","html_url":"x","prerelease":true}"#;
        assert!(parse_release(prerelease).unwrap().is_none());
    }

    #[test]
    fn checksums_are_found_by_name() {
        let hash = "a".repeat(64);
        let sums = format!(
            "{hash}  OpenSesh-0.2.0-windows-x64-setup.exe\n{}  other.zip\n",
            "b".repeat(64)
        );
        assert_eq!(
            listed_sha256(&sums, "OpenSesh-0.2.0-windows-x64-setup.exe"),
            Some(hash.as_str())
        );
        assert_eq!(listed_sha256(&sums, "missing.exe"), None);
    }

    #[test]
    fn the_install_kind_follows_the_layout() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            InstallKind::from_layout(dir.path(), true),
            InstallKind::Other
        );
        std::fs::write(dir.path().join(UNINSTALLER), b"").unwrap();
        assert_eq!(
            InstallKind::from_layout(dir.path(), true),
            InstallKind::WindowsInstaller
        );
        std::fs::write(dir.path().join("portable"), b"").unwrap();
        assert_eq!(
            InstallKind::from_layout(dir.path(), true),
            InstallKind::WindowsPortable
        );
        assert_eq!(
            InstallKind::from_layout(Path::new("/usr/bin"), false),
            InstallKind::LinuxPackage
        );
    }
}
