//! `AppInfo` QML singleton: read-only startup data for the QML side (version, run mode, crash
//! report, folders). The values are fixed by `main` before the QML engine starts, through
//! [`set_startup`].

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt string type from cxx-qt-lib.
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qurl.h");
        /// Qt URL type from cxx-qt-lib.
        type QUrl = cxx_qt_lib::QUrl;
    }

    extern "RustQt" {
        /// Read-only application information exposed to QML as the `AppInfo` singleton.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, version, READ, CONSTANT)]
        #[qproperty(QString, app_id, cxx_name = "appId", READ, CONSTANT)]
        #[qproperty(bool, smoke_test, cxx_name = "smokeTest", READ, CONSTANT)]
        #[qproperty(bool, gallery, READ, CONSTANT)]
        #[qproperty(bool, window_alpha, cxx_name = "windowAlpha", READ, CONSTANT)]
        #[qproperty(QString, screenshot_dir, cxx_name = "screenshotDir", READ, CONSTANT)]
        #[qproperty(QString, crash_report, cxx_name = "crashReport", READ, CONSTANT)]
        #[qproperty(
            QString,
            crash_report_path,
            cxx_name = "crashReportPath",
            READ,
            CONSTANT
        )]
        #[qproperty(QUrl, logs_folder, cxx_name = "logsFolder", READ, CONSTANT)]
        #[qproperty(QUrl, config_folder, cxx_name = "configFolder", READ, CONSTANT)]
        type AppInfo = super::AppInfoRust;
    }
}

use cxx_qt_lib::{QString, QUrl};
use opensesh_core::identity;

/// Data captured at startup and shown by QML.
#[derive(Debug, Clone, Default)]
pub struct Startup {
    /// Automated smoke test: run the built-in checks after the first frame, then quit.
    pub smoke_test: bool,
    /// Show the component gallery instead of the main window.
    pub gallery: bool,
    /// Save screenshots of every theme/density combination here, then quit.
    pub screenshot_dir: Option<PathBuf>,
    /// Directory that holds the log files.
    pub logs_dir: PathBuf,
    /// Directory that holds `config.toml`.
    pub config_dir: PathBuf,
    /// Crash report shown by the crash dialog, with the file it was read from.
    pub crash_report: Option<(PathBuf, String)>,
}

static STARTUP: OnceLock<Startup> = OnceLock::new();

/// Whether this run is a `--smoke-test`.
#[must_use]
pub fn is_smoke_test() -> bool {
    STARTUP.get().is_some_and(|startup| startup.smoke_test)
}

/// Whether this run must leave the user's files alone: a smoke test or a screenshot run reads
/// the settings but never writes them (or creates folders for them).
#[must_use]
pub fn is_test_run() -> bool {
    STARTUP
        .get()
        .is_some_and(|startup| startup.smoke_test || startup.screenshot_dir.is_some())
}

/// Whether the main window has an alpha channel (a profile asked for a translucent terminal
/// background when the app started).
static WINDOW_ALPHA: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Records that windows are created with an alpha channel (before QML loads).
pub fn set_window_alpha(enabled: bool) {
    WINDOW_ALPHA.store(enabled, std::sync::atomic::Ordering::Relaxed);
}

/// Records the startup data. Only the first call has an effect; it must happen before the QML
/// engine instantiates the singleton.
pub fn set_startup(startup: Startup) {
    if STARTUP.set(startup).is_err() {
        tracing::warn!("AppInfo startup data was already set; ignoring the new value");
    }
}

/// Rust state behind the `AppInfo` singleton.
#[derive(Debug)]
pub struct AppInfoRust {
    version: QString,
    app_id: QString,
    smoke_test: bool,
    gallery: bool,
    window_alpha: bool,
    screenshot_dir: QString,
    crash_report: QString,
    crash_report_path: QString,
    logs_folder: QUrl,
    config_folder: QUrl,
}

fn folder_url(path: &Path) -> QUrl {
    QUrl::from_local_file(&QString::from(&path.display().to_string()))
}

impl Default for AppInfoRust {
    fn default() -> Self {
        let startup = STARTUP.get().cloned().unwrap_or_default();
        let (report_path, report) = startup
            .crash_report
            .map(|(path, text)| (path.display().to_string(), text))
            .unwrap_or_default();
        Self {
            version: QString::from(identity::VERSION),
            app_id: QString::from(identity::APP_ID),
            smoke_test: startup.smoke_test,
            gallery: startup.gallery,
            window_alpha: WINDOW_ALPHA.load(std::sync::atomic::Ordering::Relaxed),
            screenshot_dir: startup
                .screenshot_dir
                .map(|dir| QString::from(&dir.display().to_string()))
                .unwrap_or_default(),
            crash_report: QString::from(&report),
            crash_report_path: QString::from(&report_path),
            logs_folder: folder_url(&startup.logs_dir),
            config_folder: folder_url(&startup.config_dir),
        }
    }
}
