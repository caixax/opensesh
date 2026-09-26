//! `AppSettings` QML singleton: `config.toml` (PLAN §4.2, §6.1) exposed as QML properties.
//!
//! - Every property reads straight from the in-memory [`Config`]; setters validate, update it
//!   and queue an atomic save on the background writer (the GUI thread never touches disk).
//! - External edits of `config.toml` are picked up by a file watcher and applied live
//!   (`reloadedFromDisk`); our own writes are recognised and ignored.
//! - Problems reach QML as `problem(kind, detail)`: QML owns the translated sentence for each
//!   kind, and `detail` is technical text (a path, an OS or parser error).
//! - A file written by a newer OpenSesh, or one that can't be read (a syntax error), is never
//!   overwritten (`readOnly`, `readOnlyReason`); changes still apply in memory.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt string type from cxx-qt-lib.
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qstringlist.h");
        /// Qt string list type from cxx-qt-lib.
        type QStringList = cxx_qt_lib::QStringList;
    }

    extern "RustQt" {
        /// Persisted application settings.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, language, READ = language, WRITE = set_language, NOTIFY = settings_changed)]
        #[qproperty(QString, on_last_tab_closed, cxx_name = "onLastTabClosed", READ = on_last_tab_closed, WRITE = set_on_last_tab_closed, NOTIFY = settings_changed)]
        #[qproperty(bool, restore_sessions, cxx_name = "restoreSessions", READ = restore_sessions, WRITE = set_restore_sessions, NOTIFY = settings_changed)]
        #[qproperty(bool, confirm_close_with_sessions, cxx_name = "confirmCloseWithSessions", READ = confirm_close_with_sessions, WRITE = set_confirm_close_with_sessions, NOTIFY = settings_changed)]
        #[qproperty(bool, check_for_updates, cxx_name = "checkForUpdates", READ = check_for_updates, WRITE = set_check_for_updates, NOTIFY = settings_changed)]
        #[qproperty(QString, theme, READ = theme, WRITE = set_theme, NOTIFY = settings_changed)]
        #[qproperty(QString, accent, READ = accent, WRITE = set_accent, NOTIFY = settings_changed)]
        #[qproperty(QString, density, READ = density, WRITE = set_density, NOTIFY = settings_changed)]
        #[qproperty(f64, ui_scale, cxx_name = "uiScale", READ = ui_scale, WRITE = set_ui_scale, NOTIFY = settings_changed)]
        #[qproperty(QString, ui_font, cxx_name = "uiFont", READ = ui_font, WRITE = set_ui_font, NOTIFY = settings_changed)]
        #[qproperty(bool, reduce_motion, cxx_name = "reduceMotion", READ = reduce_motion, WRITE = set_reduce_motion, NOTIFY = settings_changed)]
        #[qproperty(QString, rail_position, cxx_name = "railPosition", READ = rail_position, WRITE = set_rail_position, NOTIFY = settings_changed)]
        #[qproperty(bool, rail_labels, cxx_name = "railLabels", READ = rail_labels, WRITE = set_rail_labels, NOTIFY = settings_changed)]
        #[qproperty(QString, side_panel_position, cxx_name = "sidePanelPosition", READ = side_panel_position, WRITE = set_side_panel_position, NOTIFY = settings_changed)]
        #[qproperty(QString, tabs_position, cxx_name = "tabsPosition", READ = tabs_position, WRITE = set_tabs_position, NOTIFY = settings_changed)]
        #[qproperty(bool, show_status_bar, cxx_name = "showStatusBar", READ = show_status_bar, WRITE = set_show_status_bar, NOTIFY = settings_changed)]
        #[qproperty(QString, window_decorations, cxx_name = "windowDecorations", READ = window_decorations, WRITE = set_window_decorations, NOTIFY = settings_changed)]
        #[qproperty(QString, terminal_profile, cxx_name = "terminalProfile", READ = terminal_profile, WRITE = set_terminal_profile, NOTIFY = settings_changed)]
        #[qproperty(QString, config_path, cxx_name = "configPath", READ = config_path, NOTIFY = status_changed)]
        #[qproperty(bool, read_only, cxx_name = "readOnly", READ = read_only, NOTIFY = status_changed)]
        #[qproperty(QString, read_only_reason, cxx_name = "readOnlyReason", READ = read_only_reason, NOTIFY = status_changed)]
        #[qproperty(QStringList, warnings, READ = warnings, NOTIFY = status_changed)]
        type AppSettings = super::AppSettingsRust;

        /// Any setting changed (from QML or from disk).
        #[qsignal]
        #[cxx_name = "settingsChanged"]
        fn settings_changed(self: Pin<&mut Self>);

        /// `configPath`, `readOnly`, `readOnlyReason` or `warnings` changed.
        #[qsignal]
        #[cxx_name = "statusChanged"]
        fn status_changed(self: Pin<&mut Self>);

        /// The file was edited outside the app and the new values were applied.
        #[qsignal]
        #[cxx_name = "reloadedFromDisk"]
        fn reloaded_from_disk(self: Pin<&mut Self>);

        /// Something the user should hear about. `kind` is one of `load-failed`, `load-newer`,
        /// `load-warnings` (at startup), `reload-failed` (an external edit was rejected),
        /// `save-blocked-newer`, `save-blocked-unreadable` (once per protection state) and
        /// `save-failed`. `detail` is technical text, possibly empty.
        #[qsignal]
        #[cxx_name = "problem"]
        fn problem(self: Pin<&mut Self>, kind: QString, detail: QString);

        fn language(self: &Self) -> QString;
        fn set_language(self: Pin<&mut Self>, value: QString);
        fn on_last_tab_closed(self: &Self) -> QString;
        fn set_on_last_tab_closed(self: Pin<&mut Self>, value: QString);
        fn restore_sessions(self: &Self) -> bool;
        fn set_restore_sessions(self: Pin<&mut Self>, value: bool);
        fn confirm_close_with_sessions(self: &Self) -> bool;
        fn set_confirm_close_with_sessions(self: Pin<&mut Self>, value: bool);
        fn check_for_updates(self: &Self) -> bool;
        fn set_check_for_updates(self: Pin<&mut Self>, value: bool);
        fn theme(self: &Self) -> QString;
        fn set_theme(self: Pin<&mut Self>, value: QString);
        fn accent(self: &Self) -> QString;
        fn set_accent(self: Pin<&mut Self>, value: QString);
        fn density(self: &Self) -> QString;
        fn set_density(self: Pin<&mut Self>, value: QString);
        fn ui_scale(self: &Self) -> f64;
        fn set_ui_scale(self: Pin<&mut Self>, value: f64);
        fn ui_font(self: &Self) -> QString;
        fn set_ui_font(self: Pin<&mut Self>, value: QString);
        fn reduce_motion(self: &Self) -> bool;
        fn set_reduce_motion(self: Pin<&mut Self>, value: bool);
        fn rail_position(self: &Self) -> QString;
        fn set_rail_position(self: Pin<&mut Self>, value: QString);
        fn rail_labels(self: &Self) -> bool;
        fn set_rail_labels(self: Pin<&mut Self>, value: bool);
        fn side_panel_position(self: &Self) -> QString;
        fn set_side_panel_position(self: Pin<&mut Self>, value: QString);
        fn tabs_position(self: &Self) -> QString;
        fn set_tabs_position(self: Pin<&mut Self>, value: QString);
        fn show_status_bar(self: &Self) -> bool;
        fn set_show_status_bar(self: Pin<&mut Self>, value: bool);
        fn window_decorations(self: &Self) -> QString;
        fn set_window_decorations(self: Pin<&mut Self>, value: QString);
        fn terminal_profile(self: &Self) -> QString;
        fn set_terminal_profile(self: Pin<&mut Self>, value: QString);
        fn config_path(self: &Self) -> QString;
        fn read_only(self: &Self) -> bool;
        fn read_only_reason(self: &Self) -> QString;
        fn warnings(self: &Self) -> QStringList;

        /// Allowed values of a choice setting (e.g. `"density"` -> `["comfortable", "compact"]`).
        #[qinvokable]
        fn choices(self: &Self, key: &QString) -> QStringList;

        /// Restores every setting to its default and saves. This also replaces a file that
        /// couldn't be read (it is kept as the first backup), but never a newer one.
        #[qinvokable]
        #[cxx_name = "resetToDefaults"]
        fn reset_to_defaults(self: Pin<&mut Self>);
    }

    impl cxx_qt::Initialize for AppSettings {}
    impl cxx_qt::Threading for AppSettings {}
}

use core::pin::Pin;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};
use opensesh_core::config::{
    self, Accent, Config, Decorations, LastTabAction, RailPosition, SidePanelPosition, TabsPosition,
};
use opensesh_core::fsutil;
use opensesh_core::theme::{Density, ThemeMode};
use opensesh_core::watch::FileWatcher;

use crate::services;

/// How long `config.toml` must be quiet before an external edit is reloaded.
const RELOAD_DEBOUNCE: Duration = Duration::from_millis(250);

/// Why `config.toml` must not be overwritten.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Protection {
    /// Saves go through.
    None,
    /// Written by a newer OpenSesh: a downgrade must not destroy it.
    NewerSchema,
    /// It couldn't be read (usually a syntax error from a hand edit): saving the in-memory
    /// settings would throw the user's edit away.
    Unreadable,
}

/// Rust state behind `AppSettings`.
pub struct AppSettingsRust {
    config: Config,
    path: Option<PathBuf>,
    protection: Protection,
    /// Whether the user was already told that saves are blocked in the current protection state.
    blocked_reported: bool,
    /// The last save error reported, so a persistent one (a read-only directory) is reported
    /// once rather than on every change. Cleared by a successful write.
    last_failure: Option<String>,
    warnings: Vec<String>,
    /// Our own writes, to tell the watcher's echo of them from external edits.
    own_writes: OwnWrites,
    watcher: Option<FileWatcher>,
}

impl std::fmt::Debug for AppSettingsRust {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppSettingsRust")
            .field("config", &self.config)
            .field("path", &self.path)
            .field("protection", &self.protection)
            .finish_non_exhaustive()
    }
}

impl Default for AppSettingsRust {
    fn default() -> Self {
        Self {
            config: Config::default(),
            path: None,
            protection: Protection::None,
            blocked_reported: false,
            last_failure: None,
            warnings: Vec::new(),
            own_writes: OwnWrites::default(),
            watcher: None,
        }
    }
}

fn qstring(value: &str) -> QString {
    QString::from(value)
}

fn qstring_list(values: impl IntoIterator<Item = String>) -> QStringList {
    values.into_iter().map(|v| QString::from(&v)).collect()
}

/// Result of reading `config.toml`.
enum Reload {
    Loaded {
        config: Box<Config>,
        warnings: Vec<String>,
        read_only: bool,
    },
    /// The file can't be used; the text is a technical detail for the user.
    Failed(String),
}

/// Parses the text of `config.toml` (read from `path`).
fn parse_config(path: &Path, text: &str) -> Reload {
    match Config::from_toml_str(text) {
        Ok((config, warnings, read_only)) => Reload::Loaded {
            config: Box::new(config),
            warnings: warnings.iter().map(ToString::to_string).collect(),
            read_only,
        },
        Err(message) => Reload::Failed(format!("{} is not valid TOML: {message}", path.display())),
    }
}

/// Reads and parses `config.toml`, returning the text that was read (to recognise our own
/// writes) with the result. A missing file means the defaults.
fn read_config(path: &Path) -> (Option<String>, Reload) {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let reload = parse_config(path, &text);
            (Some(text), reload)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (
            None,
            Reload::Loaded {
                config: Box::default(),
                warnings: Vec::new(),
                read_only: false,
            },
        ),
        Err(error) => (
            None,
            Reload::Failed(format!("could not read {}: {error}", path.display())),
        ),
    }
}

/// How a read of `config.toml` is being applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Load {
    /// The first load, at startup.
    Startup,
    /// After an external edit.
    External,
}

impl qobject::AppSettings {
    /// Applies a validated change to the config, saves and notifies. `apply` returns whether
    /// anything changed.
    fn change(mut self: Pin<&mut Self>, apply: impl FnOnce(&mut Config) -> bool) {
        let changed = apply(&mut self.as_mut().rust_mut().config);
        // Always notify: an invalid value from QML must snap the control back.
        self.as_mut().settings_changed();
        if changed {
            self.save();
        }
    }

    /// Parses a choice setting coming from QML; logs and ignores unknown values.
    fn set_choice<T: FromStr + PartialEq>(
        self: Pin<&mut Self>,
        key: &str,
        value: &QString,
        slot: impl FnOnce(&mut Config) -> &mut T,
    ) {
        let text = value.to_string();
        match text.parse::<T>() {
            Ok(parsed) => self.change(|config| replace(slot(config), parsed)),
            Err(_) => {
                tracing::warn!(key, value = %text, "ignoring invalid setting from QML");
                self.change(|_| false);
            }
        }
    }

    fn set_flag(self: Pin<&mut Self>, value: bool, slot: impl FnOnce(&mut Config) -> &mut bool) {
        self.change(|config| replace(slot(config), value));
    }

    /// Queues an atomic save of the current config.
    fn save(mut self: Pin<&mut Self>) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let blocked = match self.protection {
            Protection::None => None,
            Protection::NewerSchema => Some("save-blocked-newer"),
            Protection::Unreadable => Some("save-blocked-unreadable"),
        };
        if let Some(kind) = blocked {
            // The Settings notice explains the state; one toast per state is enough.
            if !self.blocked_reported {
                self.as_mut().rust_mut().blocked_reported = true;
                self.as_mut().problem(qstring(kind), QString::default());
            }
            return;
        }
        let Some(services) = services::get() else {
            return;
        };
        let text = self.config.to_toml_string();
        let seq = self.as_mut().rust_mut().own_writes.queued(text.clone());
        let qt_thread = self.qt_thread();
        services.writer.write(
            path,
            text.clone().into_bytes(),
            fsutil::DEFAULT_BACKUPS,
            Some(Box::new(move |path, result| {
                let failure = result
                    .err()
                    .map(|error| format!("{}: {error}", path.display()));
                // The object may already be gone at exit; nothing to report then.
                let _ = qt_thread.queue(move |settings| settings.write_done(seq, text, failure));
            })),
        );
    }

    /// A queued write finished (on the GUI thread). `failure` is the error detail.
    fn write_done(mut self: Pin<&mut Self>, seq: u64, text: String, failure: Option<String>) {
        if let Some(detail) = failure {
            self.as_mut().rust_mut().own_writes.settled(seq, None);
            tracing::warn!("could not save config.toml: {detail}");
            if self.last_failure.as_ref() != Some(&detail) {
                self.as_mut().rust_mut().last_failure = Some(detail.clone());
                self.as_mut()
                    .problem(qstring("save-failed"), QString::from(&detail));
            }
            return;
        }
        self.as_mut().rust_mut().last_failure = None;
        // The file now holds exactly this text, so warnings about the old content are stale.
        let warnings: Vec<String> = match Config::from_toml_str(&text) {
            Ok((_, warnings, _)) => warnings.iter().map(ToString::to_string).collect(),
            Err(_) => Vec::new(),
        };
        self.as_mut().rust_mut().own_writes.settled(seq, Some(text));
        if self.protection == Protection::None && self.warnings != warnings {
            self.as_mut().rust_mut().warnings = warnings;
            self.as_mut().status_changed();
        }
    }

    /// Sets the protection; a new state may be reported again.
    fn set_protection(mut self: Pin<&mut Self>, protection: Protection) {
        let mut state = self.as_mut().rust_mut();
        if state.protection != protection {
            state.protection = protection;
            state.blocked_reported = false;
        }
    }

    /// Applies a read of `config.toml` on the GUI thread. An external edit that changed
    /// something is logged and announced with `reloadedFromDisk`; a rejected one with `problem`.
    fn apply_reload(mut self: Pin<&mut Self>, reload: Reload, load: Load) {
        match reload {
            Reload::Loaded {
                config,
                warnings,
                read_only,
            } => {
                let changed = self.config != *config;
                {
                    let mut state = self.as_mut().rust_mut();
                    state.config = *config;
                    state.warnings = warnings;
                }
                self.as_mut().set_protection(if read_only {
                    Protection::NewerSchema
                } else {
                    Protection::None
                });
                self.as_mut().status_changed();
                if changed && load == Load::External {
                    tracing::info!("config.toml changed on disk; settings reloaded");
                    self.as_mut().settings_changed();
                    self.as_mut().reloaded_from_disk();
                }
            }
            Reload::Failed(detail) => {
                match load {
                    Load::Startup => tracing::warn!("using the default settings: {detail}"),
                    Load::External => {
                        tracing::warn!("ignoring external edit of config.toml: {detail}");
                    }
                }
                self.as_mut().rust_mut().warnings = vec![detail.clone()];
                self.as_mut().set_protection(Protection::Unreadable);
                self.as_mut().status_changed();
                if load == Load::External {
                    self.as_mut()
                        .problem(qstring("reload-failed"), QString::from(&detail));
                }
            }
        }
    }

    /// The watcher saw `config.toml` change; `text` is what it read (on the GUI thread).
    fn file_changed(self: Pin<&mut Self>, text: Option<String>, reload: Reload) {
        // While the file is protected we never write it, so anything on disk is the user's.
        let echo = self.protection == Protection::None
            && text
                .as_deref()
                .is_some_and(|text| self.own_writes.is_echo(text));
        if !echo {
            self.apply_reload(reload, Load::External);
        }
    }

    /// Starts watching `config.toml` for external edits.
    fn start_watcher(mut self: Pin<&mut Self>, path: PathBuf) {
        let qt_thread = self.qt_thread();
        let watched = path.clone();
        let watcher = FileWatcher::spawn(&path, RELOAD_DEBOUNCE, move || {
            // Read and parse here, off the GUI thread; whether it's our own write is decided on
            // the GUI thread, which knows every save in flight.
            let (text, reload) = read_config(&watched);
            let _ = qt_thread.queue(move |settings| settings.file_changed(text, reload));
        });
        match watcher {
            Ok(watcher) => self.as_mut().rust_mut().watcher = Some(watcher),
            Err(error) => tracing::warn!("config.toml won't hot-reload: {error:#}"),
        }
    }

    /// See the bridge declaration.
    pub fn choices(&self, key: &QString) -> QStringList {
        fn names<T: Copy>(all: &[T], name: fn(T) -> &'static str) -> Vec<String> {
            all.iter().map(|value| name(*value).to_owned()).collect()
        }
        let values = match key.to_string().as_str() {
            "theme" => names(&ThemeMode::ALL, ThemeMode::as_str),
            "density" => names(&Density::ALL, Density::as_str),
            "onLastTabClosed" => names(LastTabAction::ALL, LastTabAction::as_str),
            "railPosition" => names(RailPosition::ALL, RailPosition::as_str),
            "sidePanelPosition" => names(SidePanelPosition::ALL, SidePanelPosition::as_str),
            "tabsPosition" => names(TabsPosition::ALL, TabsPosition::as_str),
            "windowDecorations" => names(Decorations::ALL, Decorations::as_str),
            other => {
                tracing::warn!(key = other, "choices() asked for an unknown setting");
                Vec::new()
            }
        };
        qstring_list(values)
    }

    /// See the bridge declaration.
    pub fn reset_to_defaults(mut self: Pin<&mut Self>) {
        // An explicit, confirmed reset may replace a file that couldn't be read; the writer keeps
        // it as `config.toml.bak.1`. A newer file stays protected.
        let replace_broken = self.protection == Protection::Unreadable;
        if replace_broken {
            self.as_mut().rust_mut().warnings.clear();
            self.as_mut().set_protection(Protection::None);
            self.as_mut().status_changed();
        }
        // Settings this build doesn't know (from a newer OpenSesh) are not ours to reset.
        let defaults = Config {
            extra: self.config.extra.clone(),
            ..Config::default()
        };
        let changed = replace(&mut self.as_mut().rust_mut().config, defaults);
        self.as_mut().settings_changed();
        if changed || replace_broken {
            self.save();
        }
    }

    // Getters and setters (see the bridge declaration).

    pub fn language(&self) -> QString {
        qstring(&self.config.general.language)
    }
    pub fn set_language(self: Pin<&mut Self>, value: QString) {
        let text = value.to_string();
        if config::valid_language(&text) {
            self.change(|c| replace(&mut c.general.language, text));
        } else {
            tracing::warn!(value = %text, "ignoring invalid language");
            self.change(|_| false);
        }
    }
    pub fn on_last_tab_closed(&self) -> QString {
        qstring(self.config.general.on_last_tab_closed.as_str())
    }
    pub fn set_on_last_tab_closed(self: Pin<&mut Self>, value: QString) {
        self.set_choice("onLastTabClosed", &value, |c| {
            &mut c.general.on_last_tab_closed
        });
    }
    pub fn restore_sessions(&self) -> bool {
        self.config.general.restore_sessions
    }
    pub fn set_restore_sessions(self: Pin<&mut Self>, value: bool) {
        self.set_flag(value, |c| &mut c.general.restore_sessions);
    }
    pub fn confirm_close_with_sessions(&self) -> bool {
        self.config.general.confirm_close_with_sessions
    }
    pub fn set_confirm_close_with_sessions(self: Pin<&mut Self>, value: bool) {
        self.set_flag(value, |c| &mut c.general.confirm_close_with_sessions);
    }
    pub fn check_for_updates(&self) -> bool {
        self.config.general.check_for_updates
    }
    pub fn set_check_for_updates(self: Pin<&mut Self>, value: bool) {
        self.set_flag(value, |c| &mut c.general.check_for_updates);
    }
    pub fn theme(&self) -> QString {
        qstring(self.config.appearance.theme.as_str())
    }
    pub fn set_theme(self: Pin<&mut Self>, value: QString) {
        self.set_choice("theme", &value, |c| &mut c.appearance.theme);
    }
    pub fn accent(&self) -> QString {
        qstring(&self.config.appearance.accent.to_config_string())
    }
    pub fn set_accent(self: Pin<&mut Self>, value: QString) {
        self.set_choice::<Accent>("accent", &value, |c| &mut c.appearance.accent);
    }
    pub fn density(&self) -> QString {
        qstring(self.config.appearance.density.as_str())
    }
    pub fn set_density(self: Pin<&mut Self>, value: QString) {
        self.set_choice("density", &value, |c| &mut c.appearance.density);
    }
    pub fn ui_scale(&self) -> f64 {
        self.config.appearance.ui_scale
    }
    pub fn set_ui_scale(self: Pin<&mut Self>, value: f64) {
        match config::valid_ui_scale(value) {
            Some(scale) => self.change(|c| {
                let changed = (c.appearance.ui_scale - scale).abs() > f64::EPSILON;
                c.appearance.ui_scale = scale;
                changed
            }),
            None => {
                tracing::warn!(value, "ignoring invalid UI scale");
                self.change(|_| false);
            }
        }
    }
    pub fn terminal_profile(&self) -> QString {
        qstring(&self.config.terminal.profile)
    }
    pub fn set_terminal_profile(self: Pin<&mut Self>, value: QString) {
        let text = value.to_string().trim().to_owned();
        if opensesh_core::terminal::settings::valid_id(&text) {
            self.change(|c| replace(&mut c.terminal.profile, text));
        } else {
            tracing::warn!(value = %text, "ignoring an invalid terminal profile id from QML");
            self.change(|_| false);
        }
    }
    pub fn ui_font(&self) -> QString {
        qstring(&self.config.appearance.ui_font)
    }
    pub fn set_ui_font(self: Pin<&mut Self>, value: QString) {
        let text = value.to_string().trim().to_owned();
        self.change(|c| replace(&mut c.appearance.ui_font, text));
    }
    pub fn reduce_motion(&self) -> bool {
        self.config.appearance.reduce_motion
    }
    pub fn set_reduce_motion(self: Pin<&mut Self>, value: bool) {
        self.set_flag(value, |c| &mut c.appearance.reduce_motion);
    }
    pub fn rail_position(&self) -> QString {
        qstring(self.config.appearance.rail_position.as_str())
    }
    pub fn set_rail_position(self: Pin<&mut Self>, value: QString) {
        self.set_choice("railPosition", &value, |c| &mut c.appearance.rail_position);
    }
    pub fn rail_labels(&self) -> bool {
        self.config.appearance.rail_labels
    }
    pub fn set_rail_labels(self: Pin<&mut Self>, value: bool) {
        self.set_flag(value, |c| &mut c.appearance.rail_labels);
    }
    pub fn side_panel_position(&self) -> QString {
        qstring(self.config.appearance.side_panel_position.as_str())
    }
    pub fn set_side_panel_position(self: Pin<&mut Self>, value: QString) {
        self.set_choice("sidePanelPosition", &value, |c| {
            &mut c.appearance.side_panel_position
        });
    }
    pub fn tabs_position(&self) -> QString {
        qstring(self.config.appearance.tabs_position.as_str())
    }
    pub fn set_tabs_position(self: Pin<&mut Self>, value: QString) {
        self.set_choice("tabsPosition", &value, |c| &mut c.appearance.tabs_position);
    }
    pub fn show_status_bar(&self) -> bool {
        self.config.appearance.show_status_bar
    }
    pub fn set_show_status_bar(self: Pin<&mut Self>, value: bool) {
        self.set_flag(value, |c| &mut c.appearance.show_status_bar);
    }
    pub fn window_decorations(&self) -> QString {
        qstring(self.config.appearance.window_decorations.as_str())
    }
    pub fn set_window_decorations(self: Pin<&mut Self>, value: QString) {
        self.set_choice("windowDecorations", &value, |c| {
            &mut c.appearance.window_decorations
        });
    }
    pub fn config_path(&self) -> QString {
        self.path
            .as_ref()
            .map(|path| QString::from(&path.display().to_string()))
            .unwrap_or_default()
    }
    pub fn read_only(&self) -> bool {
        self.protection != Protection::None
    }
    pub fn read_only_reason(&self) -> QString {
        qstring(match self.protection {
            Protection::None => "",
            Protection::NewerSchema => "newer",
            Protection::Unreadable => "unreadable",
        })
    }
    pub fn warnings(&self) -> QStringList {
        qstring_list(self.warnings.iter().cloned())
    }
}

/// Our own writes of `config.toml`, in the order they were queued on the background writer.
/// The writer handles them in order and may skip a superseded one, so when write `n` finishes,
/// every write queued before it is settled too. Used on the GUI thread only.
#[derive(Debug, Default)]
struct OwnWrites {
    next_seq: u64,
    /// Queued and not yet finished, oldest first.
    pending: VecDeque<(u64, String)>,
    /// What the last successful write put on disk.
    last_written: Option<String>,
}

impl OwnWrites {
    /// Bound for `pending` if the writer stopped answering (it never should).
    const MAX_PENDING: usize = 64;

    /// Remembers a queued write and returns its sequence number.
    fn queued(&mut self, text: String) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        if self.pending.len() == Self::MAX_PENDING {
            self.pending.pop_front();
        }
        self.pending.push_back((seq, text));
        seq
    }

    /// Write `seq` finished: `written` is its text if it reached the disk.
    fn settled(&mut self, seq: u64, written: Option<String>) {
        while self
            .pending
            .front()
            .is_some_and(|(queued, _)| *queued <= seq)
        {
            self.pending.pop_front();
        }
        if written.is_some() {
            self.last_written = written;
        }
    }

    /// Whether `text` (just read from disk) is one of our writes: in flight, or the last one
    /// that landed. Anything else, including an older text of ours restored by the user, is an
    /// external edit.
    fn is_echo(&self, text: &str) -> bool {
        self.last_written.as_deref() == Some(text)
            || self.pending.iter().any(|(_, pending)| pending == text)
    }
}

/// Stores `value` in `slot`; returns whether it changed.
fn replace<T: PartialEq>(slot: &mut T, value: T) -> bool {
    if *slot == value {
        false
    } else {
        *slot = value;
        true
    }
}

impl cxx_qt::Initialize for qobject::AppSettings {
    fn initialize(mut self: Pin<&mut Self>) {
        let Some(services) = services::get() else {
            tracing::warn!("AppSettings created before services; settings won't persist");
            return;
        };
        let path = services.paths.config_dir().join(config::CONFIG_FILE);
        // A single small file read at startup, before the first frame.
        let (_, reload) = read_config(&path);
        let startup_problem = match &reload {
            Reload::Loaded {
                read_only: true, ..
            } => Some(("load-newer", String::new())),
            Reload::Loaded { warnings, .. } if !warnings.is_empty() => {
                for warning in warnings {
                    tracing::warn!("config.toml: {warning}");
                }
                Some(("load-warnings", String::new()))
            }
            Reload::Loaded { .. } => None,
            Reload::Failed(detail) => Some(("load-failed", detail.clone())),
        };
        self.as_mut().rust_mut().path = Some(path.clone());
        self.as_mut().apply_reload(reload, Load::Startup);
        self.as_mut().start_watcher(path);
        // This runs while QML is still creating the singleton, before any Connections exist:
        // report on the next event loop turn so the shell hears it.
        if let Some((kind, detail)) = startup_problem {
            let _ = self.qt_thread().queue(move |settings| {
                settings.problem(qstring(kind), QString::from(&detail));
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_in_flight_and_the_last_one_are_echoes() {
        let mut writes = OwnWrites::default();
        let a = writes.queued("a = 1".to_owned());
        let _b = writes.queued("a = 2".to_owned());
        // The file may still hold the older write while the newer one is pending.
        assert!(writes.is_echo("a = 1"));
        assert!(writes.is_echo("a = 2"));
        assert!(!writes.is_echo("a = 3"), "an external edit is not an echo");
        writes.settled(a, Some("a = 1".to_owned()));
        assert!(writes.is_echo("a = 1"));
        assert!(writes.is_echo("a = 2"), "still in flight");
    }

    #[test]
    fn an_old_text_of_ours_restored_later_is_an_external_edit() {
        let mut writes = OwnWrites::default();
        writes.queued("theme = dark".to_owned());
        let light = writes.queued("theme = light".to_owned());
        // The writer skipped the superseded write and wrote the newer one.
        writes.settled(light, Some("theme = light".to_owned()));
        assert!(writes.is_echo("theme = light"));
        assert!(
            !writes.is_echo("theme = dark"),
            "the user put the old value back by hand"
        );
    }

    #[test]
    fn a_failed_write_is_forgotten() {
        let mut writes = OwnWrites::default();
        let seq = writes.queued("x".to_owned());
        writes.settled(seq, None);
        assert!(!writes.is_echo("x"));
    }

    #[test]
    fn pending_writes_are_bounded() {
        let mut writes = OwnWrites::default();
        for i in 0..=OwnWrites::MAX_PENDING {
            writes.queued(format!("v{i}"));
        }
        assert!(!writes.is_echo("v0"));
        assert!(writes.is_echo(&format!("v{}", OwnWrites::MAX_PENDING)));
    }

    #[test]
    fn unparsable_text_fails_with_the_path() {
        let reload = parse_config(Path::new("config.toml"), "[appearance");
        assert!(matches!(
            reload,
            Reload::Failed(detail) if detail.starts_with("config.toml is not valid TOML")
        ));
    }
}
