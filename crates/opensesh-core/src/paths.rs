//! Location of the OpenSesh configuration, data and cache directories (PLAN §4.1).
//!
//! | Platform | Config | Data (vault, logs, recordings) | Cache |
//! |---|---|---|---|
//! | Linux / BSD | `$XDG_CONFIG_HOME/opensesh` | `$XDG_DATA_HOME/opensesh` | `$XDG_CACHE_HOME/opensesh` |
//! | Windows | `%APPDATA%\OpenSesh` | `%LOCALAPPDATA%\OpenSesh` | `%LOCALAPPDATA%\OpenSesh\cache` |
//! | macOS | `~/Library/Application Support/OpenSesh` | same as config | `~/Library/Caches/OpenSesh` |
//! | Portable | `<exe dir>/data` | `<exe dir>/data` | `<exe dir>/data/cache` |
//!
//! Portable mode is enabled when a file named [`PORTABLE_MARKER`] exists next to the executable.
//! On Windows the roaming profile only holds configuration: logs, recordings and secrets stay on
//! the local machine.

use std::io;
use std::path::{Path, PathBuf};

/// Name of the marker file that enables portable mode when placed next to the executable.
pub const PORTABLE_MARKER: &str = "portable";

/// Folder (next to the executable) that holds everything in portable mode.
pub const PORTABLE_DATA_DIR: &str = "data";

/// Directory name used on Linux and other XDG platforms.
const XDG_DIR_NAME: &str = "opensesh";

/// Directory name used on Windows and macOS.
const PRETTY_DIR_NAME: &str = "OpenSesh";

/// Errors that can happen while resolving the application directories.
#[derive(Debug, thiserror::Error)]
pub enum PathsError {
    /// The user's home directory could not be determined.
    #[error("could not determine the home directory of the current user")]
    NoHomeDir,
    /// The path of the running executable could not be determined.
    #[error("could not determine the path of the running executable")]
    CurrentExe(#[source] io::Error),
    /// A directory could not be created.
    #[error("could not create directory {path}")]
    CreateDir {
        /// Directory that failed to be created.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// The permissions of a private directory could not be restricted to the current user.
    #[error("could not restrict the permissions of {path} to the current user")]
    SetPermissions {
        /// Directory whose permissions could not be changed.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
}

/// Resolved set of directories used by the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    config_dir: PathBuf,
    data_dir: PathBuf,
    cache_dir: PathBuf,
    portable: bool,
}

/// Platform conventions for the per-user base directories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// Linux and other XDG-based systems.
    Xdg,
    /// Microsoft Windows.
    Windows,
    /// Apple macOS.
    MacOs,
}

impl Platform {
    /// Platform the binary was compiled for.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Xdg
        }
    }
}

/// Per-user base directories as reported by the operating system, before OpenSesh adds its own
/// folder name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseDirs {
    /// `$XDG_CONFIG_HOME`, `%APPDATA%` or `~/Library/Application Support`.
    pub config: PathBuf,
    /// `$XDG_DATA_HOME`, `%LOCALAPPDATA%` or `~/Library/Application Support`.
    pub data_local: PathBuf,
    /// `$XDG_CACHE_HOME`, `%LOCALAPPDATA%` or `~/Library/Caches`.
    pub cache: PathBuf,
}

impl BaseDirs {
    /// Queries the operating system for the per-user base directories.
    ///
    /// # Errors
    ///
    /// Returns [`PathsError::NoHomeDir`] if the home directory is unknown.
    pub fn from_system() -> Result<Self, PathsError> {
        let dirs = directories::BaseDirs::new().ok_or(PathsError::NoHomeDir)?;
        Ok(Self {
            config: dirs.config_dir().to_path_buf(),
            data_local: dirs.data_local_dir().to_path_buf(),
            cache: dirs.cache_dir().to_path_buf(),
        })
    }
}

impl AppPaths {
    /// Resolves the directories for the running executable: portable mode if the marker file
    /// exists next to it, the per-user platform directories otherwise.
    ///
    /// Nothing is created on disk; call [`AppPaths::ensure_dirs`] for that.
    ///
    /// # Errors
    ///
    /// Fails if the executable path or the home directory can't be determined.
    pub fn resolve() -> Result<Self, PathsError> {
        let exe = std::env::current_exe().map_err(PathsError::CurrentExe)?;
        if let Some(exe_dir) = exe.parent()
            && is_portable_dir(exe_dir)
        {
            return Ok(Self::portable(exe_dir));
        }
        Ok(Self::for_platform(
            Platform::current(),
            &BaseDirs::from_system()?,
        ))
    }

    /// Directories for a portable installation rooted at `exe_dir`.
    #[must_use]
    pub fn portable(exe_dir: &Path) -> Self {
        let root = exe_dir.join(PORTABLE_DATA_DIR);
        Self {
            config_dir: root.clone(),
            cache_dir: root.join("cache"),
            data_dir: root,
            portable: true,
        }
    }

    /// Directories for a regular installation given the platform conventions and base folders.
    #[must_use]
    pub fn for_platform(platform: Platform, base: &BaseDirs) -> Self {
        match platform {
            Platform::Xdg => Self {
                config_dir: base.config.join(XDG_DIR_NAME),
                data_dir: base.data_local.join(XDG_DIR_NAME),
                cache_dir: base.cache.join(XDG_DIR_NAME),
                portable: false,
            },
            Platform::Windows => {
                let local = base.data_local.join(PRETTY_DIR_NAME);
                Self {
                    config_dir: base.config.join(PRETTY_DIR_NAME),
                    cache_dir: local.join("cache"),
                    data_dir: local,
                    portable: false,
                }
            }
            Platform::MacOs => Self {
                config_dir: base.config.join(PRETTY_DIR_NAME),
                data_dir: base.data_local.join(PRETTY_DIR_NAME),
                cache_dir: base.cache.join(PRETTY_DIR_NAME),
                portable: false,
            },
        }
    }

    /// Directory for configuration files (`config.toml`, `hosts.toml`, ...).
    #[must_use]
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    /// Directory for application data (vault, logs, recordings).
    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Directory for disposable cached data.
    #[must_use]
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Directory for the application logs.
    #[must_use]
    pub fn logs_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }

    /// Directory for session recordings.
    #[must_use]
    pub fn recordings_dir(&self) -> PathBuf {
        self.data_dir.join("recordings")
    }

    /// Whether the paths come from a portable installation.
    #[must_use]
    pub fn is_portable(&self) -> bool {
        self.portable
    }

    /// Creates the config, data, cache and logs directories if they don't exist.
    ///
    /// On Unix, missing directories (and missing parents such as `~/.config`) are created with
    /// mode `0o700`, as the XDG base directory spec asks. The data directory holds the vault and
    /// session logs, so an existing one that other users can access is tightened to `0o700` as
    /// well. Existing config and cache directories keep their permissions: they may be
    /// user-chosen folders (e.g. a synced config folder, PLAN §4.1).
    ///
    /// # Errors
    ///
    /// Returns [`PathsError::CreateDir`] if a directory can't be created, or
    /// [`PathsError::SetPermissions`] if the data directory can't be made private.
    pub fn ensure_dirs(&self) -> Result<(), PathsError> {
        for dir in [&self.config_dir, &self.cache_dir, &self.data_dir] {
            create_dir(dir)?;
        }
        restrict_to_owner(&self.data_dir)?;
        create_dir(&self.logs_dir())
    }
}

/// Whether `dir` contains the portable-mode marker file.
#[must_use]
pub fn is_portable_dir(dir: &Path) -> bool {
    dir.join(PORTABLE_MARKER).is_file()
}

/// Creates `path` and its missing parents; on Unix they are created with mode `0o700`.
fn create_dir(path: &Path) -> Result<(), PathsError> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(path)
        .map_err(|source| PathsError::CreateDir {
            path: path.to_path_buf(),
            source,
        })
}

/// Removes group and other access from an existing directory, only if it has any.
#[cfg(unix)]
fn restrict_to_owner(path: &Path) -> Result<(), PathsError> {
    use std::os::unix::fs::PermissionsExt;

    let map_err = |source| PathsError::SetPermissions {
        path: path.to_path_buf(),
        source,
    };
    let mode = std::fs::metadata(path)
        .map_err(map_err)?
        .permissions()
        .mode();
    if mode & 0o077 == 0 {
        return Ok(());
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).map_err(map_err)
}

/// Windows and macOS already keep per-user profile folders private.
#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)] // Same signature as the Unix version.
fn restrict_to_owner(_path: &Path) -> Result<(), PathsError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> BaseDirs {
        BaseDirs {
            config: PathBuf::from("/cfg"),
            data_local: PathBuf::from("/data"),
            cache: PathBuf::from("/cache"),
        }
    }

    #[test]
    fn xdg_layout_uses_lowercase_folder_in_each_base() {
        let paths = AppPaths::for_platform(Platform::Xdg, &base());
        assert_eq!(paths.config_dir(), Path::new("/cfg/opensesh"));
        assert_eq!(paths.data_dir(), Path::new("/data/opensesh"));
        assert_eq!(paths.cache_dir(), Path::new("/cache/opensesh"));
        assert_eq!(paths.logs_dir(), PathBuf::from("/data/opensesh/logs"));
        assert_eq!(
            paths.recordings_dir(),
            PathBuf::from("/data/opensesh/recordings")
        );
        assert!(!paths.is_portable());
    }

    #[test]
    fn windows_layout_keeps_only_config_in_roaming_profile() {
        let paths = AppPaths::for_platform(Platform::Windows, &base());
        assert_eq!(paths.config_dir(), Path::new("/cfg/OpenSesh"));
        assert_eq!(paths.data_dir(), Path::new("/data/OpenSesh"));
        assert_eq!(paths.cache_dir(), Path::new("/data/OpenSesh/cache"));
        assert_eq!(paths.logs_dir(), PathBuf::from("/data/OpenSesh/logs"));
    }

    #[test]
    fn macos_layout_uses_pretty_folder_name() {
        let paths = AppPaths::for_platform(Platform::MacOs, &base());
        assert_eq!(paths.config_dir(), Path::new("/cfg/OpenSesh"));
        assert_eq!(paths.data_dir(), Path::new("/data/OpenSesh"));
        assert_eq!(paths.cache_dir(), Path::new("/cache/OpenSesh"));
    }

    #[test]
    fn portable_layout_keeps_everything_under_data_folder() {
        let paths = AppPaths::portable(Path::new("/opt/opensesh"));
        assert_eq!(paths.config_dir(), Path::new("/opt/opensesh/data"));
        assert_eq!(paths.data_dir(), Path::new("/opt/opensesh/data"));
        assert_eq!(paths.cache_dir(), Path::new("/opt/opensesh/data/cache"));
        assert_eq!(paths.logs_dir(), PathBuf::from("/opt/opensesh/data/logs"));
        assert!(paths.is_portable());
    }

    #[test]
    fn portable_marker_is_detected_only_as_a_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_portable_dir(dir.path()));

        std::fs::create_dir(dir.path().join(PORTABLE_MARKER)).unwrap();
        assert!(
            !is_portable_dir(dir.path()),
            "a directory named `portable` must not enable portable mode"
        );

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(PORTABLE_MARKER), b"").unwrap();
        assert!(is_portable_dir(dir.path()));
    }

    #[test]
    fn ensure_dirs_creates_every_directory() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::portable(dir.path());
        paths.ensure_dirs().unwrap();
        assert!(paths.config_dir().is_dir());
        assert!(paths.data_dir().is_dir());
        assert!(paths.cache_dir().is_dir());
        assert!(paths.logs_dir().is_dir());
        // Idempotent.
        paths.ensure_dirs().unwrap();
    }

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[cfg(unix)]
    #[test]
    fn new_directories_are_private_on_unix() {
        let dir = tempfile::tempdir().unwrap();
        let base = BaseDirs {
            config: dir.path().join("config-home"),
            data_local: dir.path().join("data-home"),
            cache: dir.path().join("cache-home"),
        };
        let paths = AppPaths::for_platform(Platform::Xdg, &base);
        paths.ensure_dirs().unwrap();
        for created in [
            paths.config_dir(),
            paths.cache_dir(),
            paths.data_dir(),
            &base.config,
            &base.cache,
        ] {
            assert_eq!(mode(created), 0o700, "{}", created.display());
        }
    }

    #[cfg(unix)]
    #[test]
    fn existing_data_dir_is_tightened_but_config_dir_is_left_alone() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let base = BaseDirs {
            config: dir.path().join("config-home"),
            data_local: dir.path().join("data-home"),
            cache: dir.path().join("cache-home"),
        };
        let paths = AppPaths::for_platform(Platform::Xdg, &base);
        for existing in [paths.config_dir(), paths.data_dir()] {
            std::fs::create_dir_all(existing).unwrap();
            std::fs::set_permissions(existing, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        paths.ensure_dirs().unwrap();
        assert_eq!(mode(paths.data_dir()), 0o700);
        assert_eq!(mode(paths.config_dir()), 0o755);
    }

    #[test]
    fn system_base_dirs_resolve_on_a_normal_account() {
        let base = BaseDirs::from_system().unwrap();
        assert!(base.config.is_absolute());
        assert!(base.data_local.is_absolute());
        assert!(base.cache.is_absolute());
    }
}
