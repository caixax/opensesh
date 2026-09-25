//! `AppSettings` QML singleton: `config.toml` (PLAN §4.2, §6.1) exposed as QML properties.
//!
//! - Every property reads straight from the in-memory [`Config`]; setters validate, update it
//!   and queue an atomic save on the background writer (the GUI thread never touches disk).
//! - External edits of `config.toml` are picked up by a file watcher and applied live
//!   (`reloadedFromDisk`); our own writes are recognised and ignored.
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

        /// Saving failed, or an external edit couldn't be applied.
        #[qsignal]
        #[cxx_name = "problem"]
        fn problem(self: Pin<&mut Self>, message: QString);

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
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
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
    warnings: Vec<String>,
    /// Texts of our recent saves, to ignore the watcher's echo of our own writes.
    recent_saves: Arc<RecentSaves>,
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
            warnings: Vec::new(),
            recent_saves: Arc::new(RecentSaves::default()),
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

/// Result of reading `config.toml` off the GUI thread.
enum Reload {
    Loaded {
        config: Box<Config>,
        warnings: Vec<String>,
        read_only: bool,
    },
    Failed(String),
}

fn read_config(path: &std::path::Path) -> Reload {
    match config::load_file(path) {
        Ok(loaded) => Reload::Loaded {
            config: Box::new(loaded.config),
            warnings: loaded.warnings.iter().map(ToString::to_string).collect(),
            read_only: loaded.read_only,
        },
        Err(error) => Reload::Failed(error.to_string()),
    }
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
            Protection::NewerSchema => {
                Some("config.toml was written by a newer OpenSesh, so changes are not saved.")
            }
            Protection::Unreadable => {
                Some("config.toml has an error, so changes are not saved until it is fixed.")
            }
        };
        if let Some(message) = blocked {
            self.as_mut().problem(qstring(message));
            return;
        }
        let Some(services) = services::get() else {
            return;
        };
        let text = self.config.to_toml_string();
        self.recent_saves.remember(&text);
        let qt_thread = self.qt_thread();
        services.writer.write(
            path,
            text.into_bytes(),
            fsutil::DEFAULT_BACKUPS,
            Some(Box::new(move |path, result| {
                if let Err(error) = result {
                    let message = format!("Could not save {}: {error}", path.display());
                    // The object may already be gone at exit; nothing to report then.
                    let _ = qt_thread.queue(move |settings| {
                        settings.problem(QString::from(&message));
                    });
                }
            })),
        );
    }

    /// Applies a read of `config.toml` on the GUI thread: the first load at startup, or a reload
    /// after an external edit (which is logged and announced with `reloadedFromDisk`).
    fn apply_reload(mut self: Pin<&mut Self>, reload: Reload, initial: bool) {
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
                    state.protection = if read_only {
                        Protection::NewerSchema
                    } else {
                        Protection::None
                    };
                }
                self.as_mut().status_changed();
                if changed && !initial {
                    tracing::info!("config.toml changed on disk; settings reloaded");
                    self.as_mut().settings_changed();
                    self.as_mut().reloaded_from_disk();
                }
            }
            Reload::Failed(message) => {
                if initial {
                    tracing::warn!("using the default settings: {message}");
                } else {
                    tracing::warn!("ignoring external edit of config.toml: {message}");
                }
                {
                    let mut state = self.as_mut().rust_mut();
                    state.warnings = vec![message.clone()];
                    state.protection = Protection::Unreadable;
                }
                self.as_mut().status_changed();
                self.as_mut().problem(QString::from(&message));
            }
        }
    }

    /// Starts watching `config.toml` for external edits.
    fn start_watcher(mut self: Pin<&mut Self>, path: PathBuf) {
        let qt_thread = self.qt_thread();
        let recent_saves = Arc::clone(&self.recent_saves);
        let watched = path.clone();
        let watcher = FileWatcher::spawn(&path, RELOAD_DEBOUNCE, move || {
            let current = std::fs::read_to_string(&watched).ok();
            // Any of our recent saves may be what's on disk right now (a newer one can still be
            // waiting in the writer), so only content we never wrote counts as an external edit.
            if current
                .as_deref()
                .is_some_and(|text| recent_saves.contains(text))
            {
                return;
            }
            let reload = read_config(&watched);
            let _ = qt_thread.queue(move |settings| settings.apply_reload(reload, false));
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
            {
                let mut state = self.as_mut().rust_mut();
                state.protection = Protection::None;
                state.warnings.clear();
            }
            self.as_mut().status_changed();
        }
        let changed = replace(&mut self.as_mut().rust_mut().config, Config::default());
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

/// The last few `config.toml` texts we queued for writing. The watcher reads the file while
/// newer saves may still be pending, so comparing only against the latest save would mistake
/// our own earlier write for an external edit and revert the newer in-memory value.
#[derive(Debug, Default)]
struct RecentSaves {
    texts: Mutex<VecDeque<String>>,
}

impl RecentSaves {
    const CAPACITY: usize = 16;

    fn remember(&self, text: &str) {
        if let Ok(mut texts) = self.texts.lock() {
            if texts.len() == Self::CAPACITY {
                texts.pop_front();
            }
            texts.push_back(text.to_owned());
        }
    }

    fn contains(&self, text: &str) -> bool {
        self.texts
            .lock()
            .map(|texts| texts.iter().any(|saved| saved == text))
            .unwrap_or(false)
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
        let reload = read_config(&path);
        if let Reload::Loaded { warnings, .. } = &reload {
            for warning in warnings {
                tracing::warn!("config.toml: {warning}");
            }
        }
        self.as_mut().rust_mut().path = Some(path.clone());
        self.as_mut().apply_reload(reload, true);
        self.as_mut().start_watcher(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_recent_save_is_recognised_as_our_own() {
        let saves = RecentSaves::default();
        saves.remember("a = 1");
        saves.remember("a = 2");
        // The file may still hold the older write while the newer one is pending.
        assert!(saves.contains("a = 1"));
        assert!(saves.contains("a = 2"));
        assert!(!saves.contains("a = 3"), "an external edit is not an echo");
    }

    #[test]
    fn only_the_last_saves_are_kept() {
        let saves = RecentSaves::default();
        for i in 0..=RecentSaves::CAPACITY {
            saves.remember(&format!("v{i}"));
        }
        assert!(!saves.contains("v0"));
        assert!(saves.contains(&format!("v{}", RecentSaves::CAPACITY)));
    }
}
