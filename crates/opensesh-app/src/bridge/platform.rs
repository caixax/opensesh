//! `Platform` QML singleton: operating-system facts and helpers that need Qt's C++ API
//! (fonts, key names, translations) or the environment (desktop detection), and the local
//! shells this computer has (found on a background thread at startup).

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt string type from cxx-qt-lib.
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qstringlist.h");
        /// Qt string list type from cxx-qt-lib.
        type QStringList = cxx_qt_lib::QStringList;

        include!("cxx-qt-lib/qurl.h");
        /// Qt URL type from cxx-qt-lib.
        type QUrl = cxx_qt_lib::QUrl;
    }

    extern "RustQt" {
        /// OS facts and helpers.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, os, READ, CONSTANT)]
        #[qproperty(QString, desktop_name, cxx_name = "desktopName", READ, CONSTANT)]
        #[qproperty(bool, tiling, READ, CONSTANT)]
        #[qproperty(bool, debug_build, cxx_name = "debugBuild", READ, CONSTANT)]
        #[qproperty(QString, shells, READ, NOTIFY = shells_changed)]
        #[qproperty(QString, serial_ports, cxx_name = "serialPorts", READ, NOTIFY = serial_ports_changed)]
        #[qproperty(QString, containers, READ, NOTIFY = containers_changed)]
        type Platform = super::PlatformRust;

        /// `shells` changed (found, or found again).
        #[qsignal]
        #[cxx_name = "shellsChanged"]
        fn shells_changed(self: Pin<&mut Self>);

        /// Looks for the local shells again (in the background; `shellsChanged` follows). The
        /// list is JSON: `[{id, name, command, default}]`, the user's own shell first.
        #[qinvokable]
        #[cxx_name = "refreshShells"]
        fn refresh_shells(self: Pin<&mut Self>);

        /// `serialPorts` changed.
        #[qsignal]
        #[cxx_name = "serialPortsChanged"]
        fn serial_ports_changed(self: Pin<&mut Self>);

        /// Lists the serial ports again (in the background; `serialPortsChanged` follows when
        /// they changed). The list is JSON: `[{name, description}]`.
        #[qinvokable]
        #[cxx_name = "refreshSerialPorts"]
        fn refresh_serial_ports(self: Pin<&mut Self>);

        /// `containers` changed.
        #[qsignal]
        #[cxx_name = "containersChanged"]
        fn containers_changed(self: Pin<&mut Self>);

        /// Lists the running containers of `source` (`docker` or `podman`) or the running pods
        /// (`kube`, of `context` and `pod_namespace`; empty for the current context and every
        /// namespace) in the background. `containers` then holds `{source, context, namespace,
        /// items: [{name, detail, namespace, containers}], error}`.
        #[qinvokable]
        #[cxx_name = "refreshContainers"]
        fn refresh_containers(
            self: Pin<&mut Self>,
            source: &QString,
            context: &QString,
            pod_namespace: &QString,
        );

        /// Resolves a `windowDecorations` setting (`auto` depends on the desktop).
        #[qinvokable]
        #[cxx_name = "effectiveDecorations"]
        fn effective_decorations(self: &Self, configured: &QString) -> QString;

        /// Installed font families, optionally only fixed-pitch ones.
        #[qinvokable]
        #[cxx_name = "fontFamilies"]
        fn font_families(self: &Self, monospace_only: bool) -> QStringList;

        /// Plays the system's alert sound (the terminal bell's "sound" style). Returns false
        /// where there is none (Wayland), so the caller can flash instead.
        #[qinvokable]
        fn beep(self: &Self) -> bool;

        /// The keyboard modifiers held right now (`Qt.ControlModifier` and so on), read from
        /// the system rather than from the last key event.
        #[qinvokable]
        #[cxx_name = "keyboardModifiers"]
        fn keyboard_modifiers(self: &Self) -> i32;

        /// Puts `text` on the clipboard (e.g. a host's `ssh` command).
        #[qinvokable]
        #[cxx_name = "copyText"]
        fn copy_text(self: &Self, text: &QString);

        /// Seconds since the user last typed, clicked or moved the mouse in any OpenSesh window
        /// (the vault's idle lock).
        #[qinvokable]
        #[cxx_name = "idleSeconds"]
        fn idle_seconds(self: &Self) -> i32;

        /// The local path of a `file:` URL from a file dialog (empty for other URLs).
        #[qinvokable]
        #[cxx_name = "localPath"]
        fn local_path(self: &Self, url: &QUrl) -> QString;

        /// Portable text of a key combination (`KeyEvent.key`, `KeyEvent.modifiers`), e.g.
        /// `Ctrl+Shift+P`: untranslated and stable, so it can be stored and shown.
        #[qinvokable]
        #[cxx_name = "keySequenceText"]
        fn key_sequence_text(self: &Self, key: i32, modifiers: i32) -> QString;

        /// Language codes the user can pick: `system`, `en` and every bundled translation.
        #[qinvokable]
        fn languages(self: &Self) -> QStringList;

        /// Name of a language in that language (`es` -> `español`).
        #[qinvokable]
        #[cxx_name = "languageName"]
        fn language_name(self: &Self, code: &QString) -> QString;

        /// Installs the translation for `code` and retranslates the UI live.
        #[qinvokable]
        #[cxx_name = "applyLanguage"]
        fn apply_language(self: &Self, code: &QString) -> bool;

        /// Debug builds with `OPENSESH_DEBUG_PANIC=1` only: panics inside this QML -> Rust call,
        /// to test the crash report and dialog end to end. Does nothing otherwise.
        #[qinvokable]
        #[cxx_name = "debugPanic"]
        fn debug_panic(self: &Self);
    }

    impl cxx_qt::Initialize for Platform {}
    impl cxx_qt::Threading for Platform {}
}

use core::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use opensesh_term::shells::{self, Shell};
use serde_json::json;

use cxx_qt_lib::{QString, QStringList, QUrl};
use opensesh_core::config::Decorations;
use opensesh_core::desktop::{self, DesktopInfo};

use crate::bridge::shim::ffi as shim;
use crate::services;

/// Rust state behind `Platform`.
#[derive(Debug)]
pub struct PlatformRust {
    os: QString,
    desktop_name: QString,
    tiling: bool,
    debug_build: bool,
    shells: QString,
    serial_ports: QString,
    containers: QString,
    desktop: DesktopInfo,
}

impl Default for PlatformRust {
    fn default() -> Self {
        let desktop = services::get().map_or_else(desktop::detect_current, |s| s.desktop.clone());
        Self {
            os: QString::from(std::env::consts::OS),
            desktop_name: QString::from(&desktop.name),
            tiling: desktop.tiling,
            debug_build: cfg!(debug_assertions),
            shells: QString::from("[]"),
            serial_ports: QString::from("[]"),
            containers: QString::from("{}"),
            desktop,
        }
    }
}

/// The shells as JSON for QML.
fn shells_json(list: &[Shell]) -> String {
    serde_json::Value::Array(
        list.iter()
            .map(|shell| {
                json!({
                    "id": shell.id,
                    "name": shell.name,
                    "command": shell.command,
                    "default": shell.default,
                })
            })
            .collect(),
    )
    .to_string()
}

/// What screenshots show instead of the shells of the machine taking them.
fn sample_shells() -> Vec<Shell> {
    let shell = |id: &str, name: &str, command: &str, default: bool| Shell {
        id: id.to_owned(),
        name: name.to_owned(),
        command: command.to_owned(),
        default,
    };
    if cfg!(windows) {
        vec![
            shell(
                "pwsh",
                "PowerShell 7",
                r#""C:\Program Files\PowerShell\7\pwsh.exe""#,
                true,
            ),
            shell(
                "powershell",
                "Windows PowerShell",
                r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
                false,
            ),
            shell(
                "cmd",
                "Command Prompt",
                r"C:\Windows\System32\cmd.exe",
                false,
            ),
            shell(
                "git-bash",
                "Git Bash",
                r#""C:\Program Files\Git\bin\bash.exe" --login -i"#,
                false,
            ),
            shell(
                "wsl:Ubuntu",
                "Ubuntu (WSL)",
                r"C:\Windows\System32\wsl.exe -d Ubuntu",
                false,
            ),
        ]
    } else {
        vec![
            shell("bash", "bash", "/bin/bash", true),
            shell("fish", "fish", "/usr/bin/fish", false),
            shell("zsh", "zsh", "/usr/bin/zsh", false),
            shell("sh", "sh", "/bin/sh", false),
        ]
    }
}

impl qobject::Platform {
    /// See the bridge declaration.
    pub fn refresh_shells(self: Pin<&mut Self>) {
        if crate::bridge::app_info::screenshot_run() {
            let text = shells_json(&sample_shells());
            let mut this = self;
            this.as_mut().rust_mut().shells = QString::from(&text);
            this.shells_changed();
            return;
        }
        let qt_thread = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("opensesh-shells".to_owned())
            .spawn(move || {
                let text = shells_json(&shells::discover());
                let _ = qt_thread.queue(move |mut object| {
                    if object.shells.to_string() != text {
                        object.as_mut().rust_mut().shells = QString::from(&text);
                        object.shells_changed();
                    }
                });
            });
        if let Err(error) = spawned {
            tracing::warn!("could not look for the local shells: {error}");
        }
    }

    /// See the bridge declaration.
    pub fn refresh_serial_ports(self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        let screenshots = crate::bridge::app_info::screenshot_run();
        let spawned = std::thread::Builder::new()
            .name("opensesh-ports".to_owned())
            .spawn(move || {
                let ports = if screenshots {
                    // Screenshots show sample ports, never this computer's.
                    vec![
                        opensesh_proto_misc::serial::PortInfo {
                            name: if cfg!(windows) {
                                "COM3"
                            } else {
                                "/dev/ttyUSB0"
                            }
                            .to_owned(),
                            description: "FTDI FT232R USB UART".to_owned(),
                        },
                        opensesh_proto_misc::serial::PortInfo {
                            name: if cfg!(windows) {
                                "COM4"
                            } else {
                                "/dev/ttyACM0"
                            }
                            .to_owned(),
                            description: "Arduino Uno".to_owned(),
                        },
                    ]
                } else {
                    opensesh_proto_misc::serial::ports()
                };
                let text = serde_json::Value::Array(
                    ports
                        .iter()
                        .map(|port| json!({ "name": port.name, "description": port.description }))
                        .collect(),
                )
                .to_string();
                let _ = qt_thread.queue(move |mut object| {
                    if object.serial_ports.to_string() != text {
                        object.as_mut().rust_mut().serial_ports = QString::from(&text);
                        object.serial_ports_changed();
                    }
                });
            });
        if let Err(error) = spawned {
            tracing::warn!("could not list the serial ports: {error}");
        }
    }

    /// See the bridge declaration.
    pub fn refresh_containers(
        self: Pin<&mut Self>,
        source: &QString,
        context: &QString,
        pod_namespace: &QString,
    ) {
        use opensesh_core::hosts::containers::Engine;
        use opensesh_proto_misc::containers::{self, Source};

        let name = source.to_string();
        let given = |text: &QString| {
            let text = text.to_string().trim().to_owned();
            (!text.is_empty()).then_some(text)
        };
        let (context, namespace) = (given(context), given(pod_namespace));
        let source = match name.as_str() {
            "docker" => Source::Engine(Engine::Docker),
            "podman" => Source::Engine(Engine::Podman),
            "kube" => Source::Kube {
                context: context.clone(),
                namespace: namespace.clone(),
            },
            _ => return,
        };
        // Test runs and screenshots never run these programs.
        let samples = crate::bridge::app_info::is_test_run();
        let qt_thread = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("opensesh-containers".to_owned())
            .spawn(move || {
                let listed = if samples {
                    Ok(containers::samples(&source))
                } else {
                    containers::list(&source)
                };
                let (items, error) = match listed {
                    Ok(items) => (items, String::new()),
                    Err(error) => (Vec::new(), error),
                };
                let text = json!({
                    "source": name,
                    "context": context.unwrap_or_default(),
                    "namespace": namespace.unwrap_or_default(),
                    "items": items
                        .iter()
                        .map(|item| json!({
                            "name": item.name,
                            "detail": item.detail,
                            "namespace": item.namespace,
                            "containers": item.containers,
                        }))
                        .collect::<Vec<_>>(),
                    "error": error,
                })
                .to_string();
                let _ = qt_thread.queue(move |mut object| {
                    object.as_mut().rust_mut().containers = QString::from(&text);
                    object.containers_changed();
                });
            });
        if let Err(error) = spawned {
            tracing::warn!("could not list the containers: {error}");
        }
    }

    /// See the bridge declaration.
    pub fn effective_decorations(&self, configured: &QString) -> QString {
        let mode = configured.to_string().parse().unwrap_or(Decorations::Auto);
        QString::from(desktop::effective_decorations(mode, &self.desktop).as_str())
    }

    /// See the bridge declaration.
    pub fn font_families(&self, monospace_only: bool) -> QStringList {
        shim::font_families(monospace_only)
    }

    /// See the bridge declaration.
    pub fn beep(&self) -> bool {
        shim::platform_beep()
    }

    /// See the bridge declaration.
    pub fn keyboard_modifiers(&self) -> i32 {
        shim::keyboard_modifiers()
    }

    /// See the bridge declaration.
    pub fn copy_text(&self, text: &QString) {
        shim::clipboard_set_text(text);
    }

    /// See the bridge declaration.
    pub fn idle_seconds(&self) -> i32 {
        i32::try_from(shim::idle_milliseconds() / 1000).unwrap_or(i32::MAX)
    }

    /// See the bridge declaration.
    pub fn local_path(&self, url: &QUrl) -> QString {
        url.to_local_file().unwrap_or_default()
    }

    /// See the bridge declaration.
    pub fn key_sequence_text(&self, key: i32, modifiers: i32) -> QString {
        shim::key_sequence_text(key, modifiers)
    }

    /// See the bridge declaration.
    pub fn languages(&self) -> QStringList {
        let mut codes = vec!["system".to_owned(), "en".to_owned()];
        let bundled = shim::available_translations();
        for code in bundled.iter().map(ToString::to_string) {
            let testing_only = code == "pseudo";
            if (!testing_only || self.debug_build) && !codes.contains(&code) {
                codes.push(code);
            }
        }
        codes.iter().map(QString::from).collect()
    }

    /// See the bridge declaration.
    pub fn language_name(&self, code: &QString) -> QString {
        shim::language_native_name(code)
    }

    /// See the bridge declaration.
    pub fn debug_panic(&self) {
        #[cfg(debug_assertions)]
        debug_panic_if_requested();
    }

    /// See the bridge declaration.
    pub fn apply_language(&self, code: &QString) -> bool {
        let applied = shim::apply_translation(code);
        tracing::info!(language = %code, applied, "UI language");
        applied
    }
}

/// Environment variable that enables [`qobject::Platform::debug_panic`] in debug builds.
#[cfg(debug_assertions)]
pub const DEBUG_PANIC_ENV: &str = "OPENSESH_DEBUG_PANIC";

#[cfg(debug_assertions)]
#[allow(clippy::panic)] // Deliberate, debug-only test hook.
fn debug_panic_if_requested() {
    if std::env::var_os(DEBUG_PANIC_ENV).is_some_and(|value| !value.is_empty() && value != "0") {
        panic!("{DEBUG_PANIC_ENV} is set: simulated panic in Platform::debug_panic");
    }
}

impl cxx_qt::Initialize for qobject::Platform {
    fn initialize(self: Pin<&mut Self>) {
        self.refresh_shells();
    }
}
