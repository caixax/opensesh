//! `AppInfo` QML singleton: read-only startup data for the QML side (version, run mode, crash
//! report). The values are fixed by `main` before the QML engine starts, through [`set_startup`].

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
        #[qproperty(QString, crash_report, cxx_name = "crashReport", READ, CONSTANT)]
        #[qproperty(
            QString,
            crash_report_path,
            cxx_name = "crashReportPath",
            READ,
            CONSTANT
        )]
        #[qproperty(QUrl, logs_folder, cxx_name = "logsFolder", READ, CONSTANT)]
        type AppInfo = super::AppInfoRust;
    }
}

use cxx_qt_lib::{QString, QUrl};
use opensesh_core::identity;

/// Data captured at startup and shown by QML.
#[derive(Debug, Clone, Default)]
pub struct Startup {
    /// Whether the app runs as an automated smoke test (quit after the first frame).
    pub smoke_test: bool,
    /// Directory that holds the log files.
    pub logs_dir: std::path::PathBuf,
    /// Crash report shown by the crash dialog, with the file it was read from.
    pub crash_report: Option<(std::path::PathBuf, String)>,
}

static STARTUP: OnceLock<Startup> = OnceLock::new();

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
    crash_report: QString,
    crash_report_path: QString,
    logs_folder: QUrl,
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
            crash_report: QString::from(&report),
            crash_report_path: QString::from(&report_path),
            logs_folder: QUrl::from_local_file(&QString::from(
                &startup.logs_dir.display().to_string(),
            )),
        }
    }
}
