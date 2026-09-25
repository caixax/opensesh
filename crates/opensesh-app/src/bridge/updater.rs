//! `Updater` QML singleton: checks GitHub Releases for a newer OpenSesh and, on a Windows
//! installation, installs it ([`crate::update`]). All network work runs on worker threads.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt string type from cxx-qt-lib.
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// Update checks and installation.
        ///
        /// `state`: `idle`, `checking`, `upToDate`, `available`, `downloading`, `installing` or
        /// `error` (see `error`). `installKind`: `installer` (can update itself), `portable`,
        /// `package` (Linux) or `other`.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, state, READ, NOTIFY = changed)]
        #[qproperty(QString, current_version, cxx_name = "currentVersion", READ, CONSTANT)]
        #[qproperty(QString, latest_version, cxx_name = "latestVersion", READ, NOTIFY = changed)]
        #[qproperty(QString, release_url, cxx_name = "releaseUrl", READ, NOTIFY = changed)]
        #[qproperty(QString, error, READ, NOTIFY = changed)]
        #[qproperty(QString, install_kind, cxx_name = "installKind", READ, CONSTANT)]
        #[qproperty(bool, can_install, cxx_name = "canInstall", READ, CONSTANT)]
        type Updater = super::UpdaterRust;

        /// `state`, `latestVersion`, `releaseUrl` or `error` changed.
        #[qsignal]
        fn changed(self: Pin<&mut Self>);

        /// A newer version was found (`latestVersion`, `releaseUrl`).
        #[qsignal]
        #[cxx_name = "updateAvailable"]
        fn update_available(self: Pin<&mut Self>);

        /// The installer started: quit now, it replaces the app and starts it again.
        #[qsignal]
        #[cxx_name = "quitForUpdate"]
        fn quit_for_update(self: Pin<&mut Self>);

        /// Asks GitHub for the latest release (does nothing while busy).
        #[qinvokable]
        fn check(self: Pin<&mut Self>);

        /// Downloads, verifies and starts the new installer (`canInstall` only).
        #[qinvokable]
        fn install(self: Pin<&mut Self>);
    }

    impl cxx_qt::Threading for Updater {}
}

use core::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;

use crate::update::{self, InstallKind, Release, Version};

/// Rust state behind `Updater`.
#[derive(Debug)]
pub struct UpdaterRust {
    state: QString,
    current_version: QString,
    latest_version: QString,
    release_url: QString,
    error: QString,
    install_kind: QString,
    can_install: bool,
    release: Option<Release>,
}

impl Default for UpdaterRust {
    fn default() -> Self {
        let kind = InstallKind::detect();
        Self {
            state: QString::from("idle"),
            current_version: QString::from(&Version::current().to_string()),
            latest_version: QString::default(),
            release_url: QString::default(),
            error: QString::default(),
            install_kind: QString::from(kind.as_str()),
            can_install: kind == InstallKind::WindowsInstaller,
            release: None,
        }
    }
}

impl qobject::Updater {
    fn set_state(mut self: Pin<&mut Self>, state: &str, error: &str) {
        {
            let mut rust = self.as_mut().rust_mut();
            rust.state = QString::from(state);
            rust.error = QString::from(error);
        }
        self.as_mut().changed();
    }

    fn busy(&self) -> bool {
        matches!(
            self.state.to_string().as_str(),
            "checking" | "downloading" | "installing"
        )
    }

    /// See the bridge declaration.
    pub fn check(mut self: Pin<&mut Self>) {
        if self.busy() {
            return;
        }
        self.as_mut().set_state("checking", "");
        let qt_thread = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("opensesh-update".to_owned())
            .spawn(move || {
                let result = update::check();
                let _ = qt_thread.queue(move |updater| updater.checked(result));
            });
        if spawned.is_err() {
            self.set_state("error", "could not start the update check");
        }
    }

    fn checked(mut self: Pin<&mut Self>, result: anyhow::Result<Option<Release>>) {
        match result {
            Ok(Some(release)) => {
                tracing::info!(version = %release.version, "an update is available");
                {
                    let mut rust = self.as_mut().rust_mut();
                    rust.latest_version = QString::from(&release.version.to_string());
                    rust.release_url = QString::from(&release.page);
                    rust.release = Some(release);
                }
                self.as_mut().set_state("available", "");
                self.as_mut().update_available();
            }
            Ok(None) => self.set_state("upToDate", ""),
            Err(error) => {
                tracing::warn!("update check failed: {error:#}");
                self.set_state("error", &format!("{error:#}"));
            }
        }
    }

    /// See the bridge declaration.
    pub fn install(mut self: Pin<&mut Self>) {
        if !self.can_install || self.busy() {
            return;
        }
        let Some(release) = self.release.clone() else {
            return;
        };
        self.as_mut().set_state("downloading", "");
        let qt_thread = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("opensesh-update".to_owned())
            .spawn(move || {
                let result = update::download_installer(&release)
                    .and_then(|installer| update::start_installer(&installer));
                let _ = qt_thread.queue(move |updater| updater.installed(result));
            });
        if spawned.is_err() {
            self.set_state("error", "could not start the download");
        }
    }

    fn installed(mut self: Pin<&mut Self>, result: anyhow::Result<()>) {
        match result {
            Ok(()) => {
                tracing::info!("the update installer started; quitting");
                self.as_mut().set_state("installing", "");
                self.as_mut().quit_for_update();
            }
            Err(error) => {
                tracing::warn!("update failed: {error:#}");
                self.set_state("error", &format!("{error:#}"));
            }
        }
    }
}
