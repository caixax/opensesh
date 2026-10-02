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
        #[qproperty(QString, repository_url, cxx_name = "repositoryUrl", READ, CONSTANT)]
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

        /// Test runs only (smoke tests and screenshots): starts the in-process SSH test server
        /// (user `tester`, password `right password`, jumps allowed, "drop" drops the
        /// connection, SFTP over `testFolder()/remote`) on 127.0.0.1 and returns its port; 0 in
        /// normal runs or when it can't start. From then on every SSH connection of the run goes
        /// to it.
        #[qinvokable]
        #[cxx_name = "startSshTestServer"]
        fn start_ssh_test_server(self: &Self) -> i32;

        /// Test runs only: starts the in-process telnet test server on 127.0.0.1 and returns its
        /// port (0 in normal runs or when it can't start). From then on every telnet connection
        /// of the run goes to it.
        #[qinvokable]
        #[cxx_name = "startTelnetTestServer"]
        fn start_telnet_test_server(self: &Self) -> i32;

        /// Test runs only: a temporary folder with `local` and `remote` sample files (the
        /// server's side), made fresh by `startSshTestServer` and removed at exit; empty in
        /// normal runs.
        #[qinvokable]
        #[cxx_name = "testFolder"]
        fn test_folder(self: &Self) -> QString;

        /// Test runs only: starts a tiny HTTP server on 127.0.0.1 that answers every request
        /// with `OpenSesh tunnel test` (for the tunnels' smoke steps) and returns its port; 0 in
        /// normal runs.
        #[qinvokable]
        #[cxx_name = "startHttpTestServer"]
        fn start_http_test_server(self: &Self) -> i32;

        /// Test runs only: writes `text` to `path` inside `testFolder()` (an editor saving a
        /// file, for the smoke test); false anywhere else.
        #[qinvokable]
        #[cxx_name = "writeTestFile"]
        fn write_test_file(self: &Self, path: &QString, text: &QString) -> bool;
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

impl qobject::AppInfo {
    /// See the bridge declaration.
    pub fn start_ssh_test_server(&self) -> i32 {
        if !is_test_run() {
            return 0;
        }
        let Some(runtime) = opensesh_ssh::runtime() else {
            return 0;
        };
        let folder = test_folder_path();
        if let Err(error) = sample_files(&folder) {
            tracing::warn!("no sample files for the smoke test: {error}");
        }
        let rules = opensesh_ssh::testing::Rules {
            password: true,
            jump: true,
            droppable: true,
            sftp_root: Some(folder.join("remote")),
            monitor: true,
            mosh: true,
            ..opensesh_ssh::testing::Rules::default()
        };
        // Binding a local port takes no time: the smoke test waits for it.
        match runtime.block_on(opensesh_ssh::testing::serve(rules)) {
            Ok(port) => {
                crate::ssh::set_test_server(port);
                i32::from(port)
            }
            Err(error) => {
                tracing::warn!("could not start the SSH test server: {error}");
                0
            }
        }
    }

    /// See the bridge declaration.
    pub fn start_telnet_test_server(&self) -> i32 {
        if !is_test_run() {
            return 0;
        }
        let Some(runtime) = opensesh_ssh::runtime() else {
            return 0;
        };
        match runtime.block_on(opensesh_proto_misc::telnet::testing::serve()) {
            Ok(port) => {
                crate::terminals::set_telnet_test_server(port);
                i32::from(port)
            }
            Err(error) => {
                tracing::warn!("could not start the telnet test server: {error}");
                0
            }
        }
    }
}

impl qobject::AppInfo {
    /// See the bridge declaration.
    pub fn test_folder(&self) -> QString {
        if is_test_run() {
            QString::from(&test_folder_path().display().to_string())
        } else {
            QString::default()
        }
    }

    /// See the bridge declaration.
    pub fn write_test_file(&self, path: &QString, text: &QString) -> bool {
        let Some(folder) = test_run_folder() else {
            return false;
        };
        let path = PathBuf::from(path.to_string());
        let inside = path
            .components()
            .all(|part| part != std::path::Component::ParentDir)
            && path.starts_with(&folder);
        inside && std::fs::write(&path, text.to_string()).is_ok()
    }
}

/// What the test HTTP server answers.
pub const HTTP_TEST_BODY: &str = "OpenSesh tunnel test";

impl qobject::AppInfo {
    /// See the bridge declaration.
    pub fn start_http_test_server(&self) -> i32 {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        if !is_test_run() {
            return 0;
        }
        let Some(runtime) = opensesh_ssh::runtime() else {
            return 0;
        };
        // Binding a local port takes no time.
        let listener = match runtime.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")) {
            Ok(listener) => listener,
            Err(error) => {
                tracing::warn!("could not start the HTTP test server: {error}");
                return 0;
            }
        };
        let port = listener.local_addr().map_or(0, |address| address.port());
        runtime.spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    let mut buffer = [0_u8; 1024];
                    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                        match socket.read(&mut buffer).await {
                            Ok(0) | Err(_) => return,
                            Ok(read) => request.extend_from_slice(&buffer[..read]),
                        }
                    }
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\
                         Access-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{HTTP_TEST_BODY}",
                        HTTP_TEST_BODY.len()
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        i32::from(port)
    }
}

/// A test run's temporary folder (one per process).
fn test_folder_path() -> PathBuf {
    std::env::temp_dir().join(format!("opensesh-smoke-{}", std::process::id()))
}

/// The folder a test run keeps its files in (removed at exit); `None` in normal runs.
#[must_use]
pub fn test_run_folder() -> Option<PathBuf> {
    is_test_run().then(test_folder_path)
}

/// Removes a test run's temporary folder, if the run made one.
pub fn remove_test_folder() {
    let folder = test_folder_path();
    if is_test_run()
        && folder.exists()
        && let Err(error) = std::fs::remove_dir_all(&folder)
    {
        tracing::warn!("could not remove the test run's folder: {error}");
    }
}

/// Fresh sample files for the smoke test: a few on each side, a folder of 300 files, and one of
/// 10,000 empty files (the SFTP view's timing).
fn sample_files(folder: &Path) -> std::io::Result<()> {
    if folder.exists() {
        std::fs::remove_dir_all(folder)?;
    }
    let remote = folder.join("remote");
    let local = folder.join("local");
    std::fs::create_dir_all(remote.join("docs"))?;
    std::fs::create_dir_all(remote.join("logs"))?;
    std::fs::create_dir_all(remote.join("many"))?;
    for n in 0..10_000 {
        std::fs::File::create(remote.join("many").join(format!("file-{n:05}")))?;
    }
    std::fs::create_dir_all(local.join("project"))?;
    std::fs::write(
        remote.join("docs").join("readme.txt"),
        "OpenSesh smoke test\n",
    )?;
    std::fs::write(remote.join(".profile"), "# hidden\n")?;
    for n in 0..300 {
        std::fs::write(
            remote.join("logs").join(format!("app-{n:03}.log")),
            format!("line {n}\n"),
        )?;
    }
    std::fs::write(local.join("project").join("main.rs"), "fn main() {}\n")?;
    std::fs::write(local.join("notes.txt"), "upload me\n".repeat(1000))?;
    Ok(())
}

/// Whether this is a screenshot run (which shows sample data instead of the machine's).
#[must_use]
pub fn screenshot_run() -> bool {
    STARTUP
        .get()
        .is_some_and(|startup| startup.screenshot_dir.is_some())
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
    repository_url: QString,
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
            repository_url: QString::from(identity::REPOSITORY),
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
