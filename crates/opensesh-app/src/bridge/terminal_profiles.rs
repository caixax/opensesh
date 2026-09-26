//! `TerminalProfiles` QML singleton: the terminal profiles (`profiles/*.toml`), themes
//! (`themes/*.toml`) and keyword highlighting rules (`highlights.toml`) of PLAN §6.2 to §6.5, for
//! the settings pages, and the library every `TerminalItem` reads
//! ([`crate::terminal::profiles`]).
//!
//! - Lists reach QML as JSON text (`profiles`, `themes`, `highlightSets`), parsed with
//!   `JSON.parse`; `revision` grows with every change, and terminals bind to it.
//! - Edits validate in Rust, update the library, and queue an atomic save on the background
//!   writer; the GUI thread never touches the disk after startup.
//! - The files are watched: an external edit (a text editor, a synced folder) is applied live,
//!   once our own saves in flight have settled ([`crate::saves`]).
//! - Theme import and export read and write the chosen file off the GUI thread.

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
        /// Terminal profiles, themes and highlighting rules.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(i32, revision, READ, NOTIFY = changed)]
        #[qproperty(QString, profiles, READ, NOTIFY = changed)]
        #[qproperty(QString, themes, READ, NOTIFY = changed)]
        #[qproperty(QString, highlight_sets, cxx_name = "highlightSets", READ, NOTIFY = changed)]
        #[qproperty(QStringList, problems, READ, NOTIFY = changed)]
        #[qproperty(QString, folder, READ, NOTIFY = changed)]
        type TerminalProfiles = super::TerminalProfilesRust;

        /// Anything changed (an edit here, or the files on disk).
        #[qsignal]
        fn changed(self: Pin<&mut Self>);

        /// A theme import finished: `ids` is a JSON list of the new theme ids (empty on error),
        /// `error` the reason it failed (technical text).
        #[qsignal]
        #[cxx_name = "themesImported"]
        fn themes_imported(self: Pin<&mut Self>, ids: QString, error: QString);

        /// A theme export finished: the file written, and an error (empty when it worked).
        #[qsignal]
        #[cxx_name = "themeExported"]
        fn theme_exported(self: Pin<&mut Self>, path: QString, error: QString);

        /// Something the user should hear about: `save-failed` or `read-only` (a file from a
        /// newer OpenSesh or one that couldn't be parsed), with technical detail.
        #[qsignal]
        fn problem(self: Pin<&mut Self>, kind: QString, detail: QString);

        /// One profile as JSON: `id`, `name`, `isDefault`, `readOnly`, `set` (the options it
        /// sets), `values` (every option, resolved) and `inherited` (what it would have without
        /// its own values). Empty for an unknown id.
        #[qinvokable]
        #[cxx_name = "profileJson"]
        fn profile_json(self: &Self, id: &QString) -> QString;

        /// Sets option `key` of profile `id` to a JSON value. Returns an error text, empty when
        /// the value was stored.
        #[qinvokable]
        #[cxx_name = "setOption"]
        fn set_option(
            self: Pin<&mut Self>,
            id: &QString,
            key: &QString,
            value: &QString,
        ) -> QString;

        /// Makes option `key` of profile `id` inherit again.
        #[qinvokable]
        #[cxx_name = "resetOption"]
        fn reset_option(self: Pin<&mut Self>, id: &QString, key: &QString);

        /// Creates a profile called `name`, a copy of `copyFrom` (empty or the default profile:
        /// a profile that inherits everything). Returns its id, empty on error.
        #[qinvokable]
        #[cxx_name = "createProfile"]
        fn create_profile(self: Pin<&mut Self>, name: &QString, copy_from: &QString) -> QString;

        /// Renames a profile. Returns whether it worked.
        #[qinvokable]
        #[cxx_name = "renameProfile"]
        fn rename_profile(self: Pin<&mut Self>, id: &QString, name: &QString) -> bool;

        /// Deletes a profile (not the default one). Returns whether it worked.
        #[qinvokable]
        #[cxx_name = "deleteProfile"]
        fn delete_profile(self: Pin<&mut Self>, id: &QString) -> bool;

        /// Allowed values of a choice option (e.g. `cursor_shape`), the encodings for
        /// `encoding`, the ANSI color names for `highlight_color`.
        #[qinvokable]
        fn choices(self: &Self, key: &QString) -> QStringList;

        /// Saves a user theme from `colors` (the JSON `colors` object of `themes`). A built-in
        /// or unknown `id` creates a new theme. Returns its id, empty on error.
        #[qinvokable]
        #[cxx_name = "saveTheme"]
        fn save_theme(
            self: Pin<&mut Self>,
            id: &QString,
            name: &QString,
            colors: &QString,
        ) -> QString;

        /// Copies a theme into a new user theme. Returns its id.
        #[qinvokable]
        #[cxx_name = "duplicateTheme"]
        fn duplicate_theme(self: Pin<&mut Self>, id: &QString) -> QString;

        /// Deletes a user theme. Returns whether it worked.
        #[qinvokable]
        #[cxx_name = "deleteTheme"]
        fn delete_theme(self: Pin<&mut Self>, id: &QString) -> bool;

        /// Imports the themes of a file (any supported format); reports with `themesImported`.
        #[qinvokable]
        #[cxx_name = "importTheme"]
        fn import_theme(self: Pin<&mut Self>, file: &QUrl);

        /// Exports theme `id` as `format` (`opensesh` or `alacritty`) to `file`; reports with
        /// `themeExported`.
        #[qinvokable]
        #[cxx_name = "exportTheme"]
        fn export_theme(self: Pin<&mut Self>, id: &QString, format: &QString, file: &QUrl);

        /// Saves a user rule set from `rules` (the JSON `rules` list of `highlightSets`). A
        /// built-in or unknown `id` creates a new set. Returns JSON `{"id": ..., "error": ...}`.
        #[qinvokable]
        #[cxx_name = "saveHighlightSet"]
        fn save_highlight_set(
            self: Pin<&mut Self>,
            id: &QString,
            name: &QString,
            rules: &QString,
        ) -> QString;

        /// Copies a rule set into a new user set. Returns its id.
        #[qinvokable]
        #[cxx_name = "duplicateHighlightSet"]
        fn duplicate_highlight_set(self: Pin<&mut Self>, id: &QString) -> QString;

        /// Deletes a user rule set. Returns whether it worked.
        #[qinvokable]
        #[cxx_name = "deleteHighlightSet"]
        fn delete_highlight_set(self: Pin<&mut Self>, id: &QString) -> bool;

        /// Why `pattern` isn't a usable rule, or empty when it is.
        #[qinvokable]
        #[cxx_name = "checkPattern"]
        fn check_pattern(self: &Self, pattern: &QString, ignore_case: bool) -> QString;
    }

    impl cxx_qt::Initialize for TerminalProfiles {}
    impl cxx_qt::Threading for TerminalProfiles {}
}

use core::pin::Pin;
use std::path::{Path, PathBuf};
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList, QUrl};
use opensesh_core::fsutil;
use opensesh_core::terminal::highlight::{
    ANSI_COLOR_NAMES, HIGHLIGHTS_FILE, HighlightColor, HighlightLibrary, HighlightRule,
    HighlightSet, HighlightStyle,
};
use opensesh_core::terminal::import::{self, Format};
use opensesh_core::terminal::profile::{
    DEFAULT_PROFILE, PROFILES_DIR, Profile, ProfileSet, profile_path,
};
use opensesh_core::terminal::settings::{
    self, BackspaceKey, BellStyle, CursorStyle, DeleteKey, ENCODINGS, Hinting, ImageFit, KEYS,
    Osc52Access, RightClick, valid_id,
};
use opensesh_core::terminal::theme::{
    PartialColors, THEMES_DIR, TerminalTheme, ThemeColors, ThemeSet, parse_color, theme_path,
};
use opensesh_core::theme::Rgba;
use opensesh_core::watch::FileWatcher;
use serde_json::{Map, Value as Json, json};

use crate::bridge::app_info::is_test_run;
use crate::saves::SaveTracker;
use crate::services;
use crate::terminal::profiles;

/// How long the files must be quiet before an external edit is reloaded.
const RELOAD_DEBOUNCE: Duration = Duration::from_millis(250);

/// Rust state behind `TerminalProfiles`.
#[derive(Default)]
pub struct TerminalProfilesRust {
    revision: i32,
    profiles: QString,
    themes: QString,
    highlight_sets: QString,
    problems: QStringList,
    folder: QString,
    config_dir: Option<PathBuf>,
    saves: SaveTracker,
    watchers: Vec<FileWatcher>,
    /// Problems found in the files at the last load.
    file_problems: Vec<String>,
}

impl std::fmt::Debug for TerminalProfilesRust {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalProfilesRust")
            .field("revision", &self.revision)
            .field("config_dir", &self.config_dir)
            .finish_non_exhaustive()
    }
}

/// Everything read from disk.
struct Disk {
    profiles: ProfileSet,
    themes: ThemeSet,
    highlights: HighlightLibrary,
    problems: Vec<String>,
}

/// Reads the profiles, themes and rules under `config_dir`.
fn load_disk(config_dir: &Path) -> Disk {
    let mut problems = Vec::new();
    let (profiles, found) = ProfileSet::load_dir(&config_dir.join(PROFILES_DIR));
    problems.extend(
        found
            .iter()
            .map(|p| format!("{}: {}", p.path.display(), p.message)),
    );
    let (themes, found) = ThemeSet::load_dir(&config_dir.join(THEMES_DIR));
    problems.extend(
        found
            .iter()
            .map(|p| format!("{}: {}", p.path.display(), p.message)),
    );
    let highlights_path = config_dir.join(HIGHLIGHTS_FILE);
    let (highlights, found) = HighlightLibrary::load_file(&highlights_path);
    problems.extend(
        found
            .iter()
            .map(|w| format!("{}: {w}", highlights_path.display())),
    );
    for problem in &problems {
        tracing::warn!("terminal settings: {problem}");
    }
    Disk {
        profiles,
        themes,
        highlights,
        problems,
    }
}

// ---- JSON --------------------------------------------------------------------------------------

/// A TOML option value as JSON.
fn toml_to_json(value: &toml::Value) -> Json {
    match value {
        toml::Value::String(text) => Json::String(text.clone()),
        toml::Value::Integer(number) => json!(number),
        toml::Value::Float(number) => json!(number),
        toml::Value::Boolean(flag) => Json::Bool(*flag),
        toml::Value::Array(items) => Json::Array(items.iter().map(toml_to_json).collect()),
        other => Json::String(other.to_string()),
    }
}

/// A JSON value from QML as TOML: whole numbers become integers (the float options accept them).
fn json_to_toml(value: &Json) -> Option<toml::Value> {
    Some(match value {
        Json::Bool(flag) => toml::Value::Boolean(*flag),
        Json::String(text) => toml::Value::String(text.clone()),
        Json::Number(number) => match number.as_i64() {
            Some(whole) => toml::Value::Integer(whole),
            None => {
                let float = number.as_f64()?;
                // 12.0 from a slider is a whole number too.
                if float.fract() == 0.0 && float.abs() < 1e15 {
                    // Checked above: whole and small.
                    #[allow(clippy::cast_possible_truncation)]
                    toml::Value::Integer(float as i64)
                } else {
                    toml::Value::Float(float)
                }
            }
        },
        Json::Array(items) => {
            toml::Value::Array(items.iter().map(json_to_toml).collect::<Option<Vec<_>>>()?)
        }
        Json::Null | Json::Object(_) => return None,
    })
}

fn settings_json(settings: &settings::TerminalSettings) -> Json {
    let mut map = Map::new();
    for key in KEYS {
        if let Some(value) = settings.get(key) {
            map.insert((*key).to_owned(), toml_to_json(&value));
        }
    }
    Json::Object(map)
}

fn hex(color: Rgba) -> Json {
    Json::String(color.to_hex())
}

fn colors_json(colors: &ThemeColors) -> Json {
    json!({
        "foreground": hex(colors.foreground),
        "background": hex(colors.background),
        "cursor": hex(colors.cursor),
        "cursorText": hex(colors.cursor_text),
        "selectionBackground": hex(colors.selection_background),
        "selectionForeground": colors.selection_foreground.map_or(Json::String(String::new()), hex),
        "matchBackground": hex(colors.match_background),
        "matchForeground": hex(colors.match_foreground),
        "focusedMatchBackground": hex(colors.focused_match_background),
        "focusedMatchForeground": hex(colors.focused_match_foreground),
        "normal": colors.normal.iter().map(|c| hex(*c)).collect::<Vec<_>>(),
        "bright": colors.bright.iter().map(|c| hex(*c)).collect::<Vec<_>>(),
    })
}

/// The `colors` object of the themes JSON back into colors. Every color must be valid.
fn colors_from_json(value: &Json) -> Result<ThemeColors, String> {
    let object = value.as_object().ok_or("expected an object")?;
    let color = |key: &str| -> Result<Rgba, String> {
        object
            .get(key)
            .and_then(Json::as_str)
            .and_then(parse_color)
            .ok_or_else(|| format!("{key}: expected a color like #RRGGBB"))
    };
    let list = |key: &str| -> Result<[Rgba; 8], String> {
        let items = object
            .get(key)
            .and_then(Json::as_array)
            .filter(|items| items.len() == 8)
            .ok_or_else(|| format!("{key}: expected 8 colors"))?;
        let mut out = [Rgba::rgb(0, 0, 0); 8];
        for (slot, item) in out.iter_mut().zip(items) {
            *slot = item
                .as_str()
                .and_then(parse_color)
                .ok_or_else(|| format!("{key}: expected colors like #RRGGBB"))?;
        }
        Ok(out)
    };
    let selection_foreground = match object.get("selectionForeground").and_then(Json::as_str) {
        None | Some("") => None,
        Some(text) => Some(parse_color(text).ok_or("selectionForeground: not a color")?),
    };
    let partial = PartialColors {
        foreground: Some(color("foreground")?),
        background: Some(color("background")?),
        cursor: Some(color("cursor")?),
        cursor_text: Some(color("cursorText")?),
        selection_background: Some(color("selectionBackground")?),
        selection_foreground,
        match_background: color("matchBackground").ok(),
        match_foreground: color("matchForeground").ok(),
        focused_match_background: color("focusedMatchBackground").ok(),
        focused_match_foreground: color("focusedMatchForeground").ok(),
        normal: list("normal")?.map(Some),
        bright: list("bright")?.map(Some),
    };
    partial
        .complete()
        .map(|(colors, _)| colors)
        .map_err(|error| error.to_string())
}

fn theme_json(theme: &TerminalTheme) -> Json {
    json!({
        "id": theme.id,
        "name": theme.name,
        "builtin": theme.builtin,
        "dark": theme.colors.is_dark(),
        "author": theme.author,
        "license": theme.license,
        "source": theme.source,
        "colors": colors_json(&theme.colors),
    })
}

fn highlight_color_json(color: Option<HighlightColor>) -> Json {
    Json::String(color.map(|c| c.to_string()).unwrap_or_default())
}

fn highlight_set_json(set: &HighlightSet) -> Json {
    let rules: Vec<Json> = set
        .rules
        .iter()
        .map(|rule| {
            json!({
                "pattern": rule.pattern,
                "ignoreCase": rule.ignore_case,
                "foreground": highlight_color_json(rule.style.foreground),
                "background": highlight_color_json(rule.style.background),
                "bold": rule.style.bold,
                "underline": rule.style.underline,
            })
        })
        .collect();
    json!({ "id": set.id, "name": set.name, "builtin": set.builtin, "rules": rules })
}

/// The `rules` list of the highlight sets JSON back into rules. Every rule must be valid.
fn rules_from_json(value: &Json) -> Result<Vec<HighlightRule>, String> {
    let items = value.as_array().ok_or("expected a list of rules")?;
    if items.len() > opensesh_core::terminal::highlight::MAX_RULES {
        return Err("too many rules".to_owned());
    }
    items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let number = index + 1;
            let text = |key: &str| item.get(key).and_then(Json::as_str).unwrap_or_default();
            let flag = |key: &str| item.get(key).and_then(Json::as_bool).unwrap_or(false);
            let color = |key: &str| -> Result<Option<HighlightColor>, String> {
                match text(key) {
                    "" => Ok(None),
                    value => value
                        .parse()
                        .map(Some)
                        .map_err(|_| format!("rule {number}: `{value}` is not a color")),
                }
            };
            let rule = HighlightRule {
                pattern: text("pattern").to_owned(),
                ignore_case: flag("ignoreCase"),
                style: HighlightStyle {
                    foreground: color("foreground")?,
                    background: color("background")?,
                    bold: flag("bold"),
                    underline: flag("underline"),
                },
            };
            rule.check()
                .map_err(|error| format!("rule {number}: {error}"))?;
            Ok(rule)
        })
        .collect()
}

/// A local path from a `file:` URL chosen in a file dialog.
fn local_path(url: &QUrl) -> Option<PathBuf> {
    url.to_local_file()
        .map(|path| PathBuf::from(path.to_string()))
        .filter(|path| !path.as_os_str().is_empty())
}

impl qobject::TerminalProfiles {
    /// Rebuilds the JSON properties from the library and announces the change.
    fn refresh(mut self: Pin<&mut Self>) {
        let library = profiles::current();
        let profile_list: Vec<Json> = library
            .profiles
            .list()
            .iter()
            .map(|profile| {
                json!({
                    "id": profile.id,
                    "name": profile.name,
                    "isDefault": profile.is_default(),
                    "readOnly": profile.read_only,
                })
            })
            .collect();
        let theme_list: Vec<Json> = library.themes.list().into_iter().map(theme_json).collect();
        let set_list: Vec<Json> = library
            .highlights
            .sets
            .iter()
            .map(highlight_set_json)
            .collect();
        let problems: QStringList = self.file_problems.iter().map(QString::from).collect();
        {
            let mut state = self.as_mut().rust_mut();
            // Wraps after 2^31 changes; QML only compares it for change.
            state.revision = i32::try_from(library.revision % 0x7FFF_FFFF).unwrap_or(0);
            state.profiles = QString::from(&Json::Array(profile_list).to_string());
            state.themes = QString::from(&Json::Array(theme_list).to_string());
            state.highlight_sets = QString::from(&Json::Array(set_list).to_string());
            state.problems = problems;
        }
        self.as_mut().changed();
    }

    /// Queues an atomic save of `text` to `path` (not in test runs: they keep changes in memory).
    fn queue_write(mut self: Pin<&mut Self>, path: PathBuf, text: String) {
        let Some(services) = services::get().filter(|_| !is_test_run()) else {
            return;
        };
        let seq = self.as_mut().rust_mut().saves.queue();
        let qt_thread = self.qt_thread();
        services.writer.write(
            path,
            text.into_bytes(),
            fsutil::DEFAULT_BACKUPS,
            Some(Box::new(move |path, result| {
                let failure = result
                    .err()
                    .map(|error| format!("{}: {error}", path.display()));
                let _ = qt_thread.queue(move |object| object.save_done(seq, failure));
            })),
        );
    }

    /// Queues the removal of `path` (not in test runs).
    fn queue_remove(mut self: Pin<&mut Self>, path: PathBuf) {
        let Some(services) = services::get().filter(|_| !is_test_run()) else {
            return;
        };
        let seq = self.as_mut().rust_mut().saves.queue();
        let qt_thread = self.qt_thread();
        services.writer.remove(
            path,
            Some(Box::new(move |path, result| {
                let failure = result
                    .err()
                    .map(|error| format!("{}: {error}", path.display()));
                let _ = qt_thread.queue(move |object| object.save_done(seq, failure));
            })),
        );
    }

    /// A save or removal finished (GUI thread).
    fn save_done(mut self: Pin<&mut Self>, seq: u64, failure: Option<String>) {
        self.as_mut().rust_mut().saves.finished(seq);
        if let Some(detail) = failure {
            tracing::warn!("could not save terminal settings: {detail}");
            self.as_mut()
                .problem(QString::from("save-failed"), QString::from(&detail));
        }
        if self.as_mut().rust_mut().saves.take_reload() {
            self.reload_in_background();
        }
    }

    /// Saves profile `id` (unless it's protected).
    fn save_profile(self: Pin<&mut Self>, id: &str) {
        let (Some(dir), Some(profile)) = (
            self.config_dir.clone(),
            profiles::current().profiles.get(id).cloned(),
        ) else {
            return;
        };
        let text = profile.to_toml_string();
        self.queue_write(profile_path(&dir, id), text);
    }

    fn save_highlights(mut self: Pin<&mut Self>) {
        let Some(dir) = self.config_dir.clone() else {
            return;
        };
        let library = profiles::current();
        if library.highlights.read_only {
            self.as_mut().problem(
                QString::from("read-only"),
                QString::from(&dir.join(HIGHLIGHTS_FILE).display().to_string()),
            );
            return;
        }
        let text = library.highlights.to_toml_string();
        self.queue_write(dir.join(HIGHLIGHTS_FILE), text);
    }

    fn save_theme_file(self: Pin<&mut Self>, theme: &TerminalTheme) {
        let Some(dir) = self.config_dir.clone() else {
            return;
        };
        let text = theme.to_toml_string();
        self.queue_write(theme_path(&dir, &theme.id), text);
    }

    /// Reads the files again off the GUI thread and applies what changed.
    fn reload_in_background(self: Pin<&mut Self>) {
        let Some(dir) = self.config_dir.clone() else {
            return;
        };
        let qt_thread = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("opensesh-profiles".to_owned())
            .spawn(move || {
                let disk = load_disk(&dir);
                let _ = qt_thread.queue(move |object| object.apply_disk(disk));
            });
        if let Err(error) = spawned {
            tracing::warn!("could not reload the terminal settings: {error}");
        }
    }

    /// Applies what was read from disk, unless our own saves are still in flight.
    fn apply_disk(mut self: Pin<&mut Self>, disk: Disk) {
        if self.saves.pending() {
            self.as_mut().rust_mut().saves.reload_wanted = true;
            return;
        }
        let library = profiles::current();
        let changed = library.profiles != disk.profiles
            || library.themes != disk.themes
            || library.highlights != disk.highlights;
        self.as_mut().rust_mut().file_problems = disk.problems;
        if changed {
            tracing::info!("terminal profiles, themes or rules changed on disk; applied");
            profiles::update(|profiles, themes, highlights| {
                *profiles = disk.profiles;
                *themes = disk.themes;
                *highlights = disk.highlights;
            });
        }
        self.refresh();
    }

    fn start_watchers(mut self: Pin<&mut Self>, dir: &Path) {
        let mut watchers = Vec::new();
        let reload = {
            let qt_thread = self.qt_thread();
            move || {
                let _ = qt_thread.queue(|object| object.reload_in_background());
            }
        };
        for sub in [PROFILES_DIR, THEMES_DIR] {
            let reload = reload.clone();
            match FileWatcher::spawn_dir(&dir.join(sub), ".toml", RELOAD_DEBOUNCE, reload) {
                Ok(watcher) => watchers.push(watcher),
                Err(error) => tracing::warn!("{sub}/ won't hot-reload: {error}"),
            }
        }
        match FileWatcher::spawn(&dir.join(HIGHLIGHTS_FILE), RELOAD_DEBOUNCE, reload) {
            Ok(watcher) => watchers.push(watcher),
            Err(error) => tracing::warn!("{HIGHLIGHTS_FILE} won't hot-reload: {error}"),
        }
        self.as_mut().rust_mut().watchers = watchers;
    }

    // ---- Invokables ------------------------------------------------------------------------

    /// See the bridge declaration.
    pub fn profile_json(&self, id: &QString) -> QString {
        let id = id.to_string();
        let library = profiles::current();
        let Some(profile) = library.profiles.get(&id) else {
            return QString::default();
        };
        let values = library.settings(&id);
        let inherited = if profile.is_default() {
            settings::TerminalSettings::default()
        } else {
            library.settings(DEFAULT_PROFILE)
        };
        let set = profile.terminal.set_keys();
        QString::from(
            &json!({
                "id": profile.id,
                "name": profile.name,
                "isDefault": profile.is_default(),
                "readOnly": profile.read_only,
                "set": set,
                "values": settings_json(&values),
                "inherited": settings_json(&inherited),
            })
            .to_string(),
        )
    }

    /// See the bridge declaration.
    pub fn set_option(
        mut self: Pin<&mut Self>,
        id: &QString,
        key: &QString,
        value: &QString,
    ) -> QString {
        let (id, key) = (id.to_string(), key.to_string());
        let Some(value) = serde_json::from_str::<Json>(&value.to_string())
            .ok()
            .as_ref()
            .and_then(json_to_toml)
        else {
            return QString::from("not a value");
        };
        let library = profiles::current();
        let Some(profile) = library.profiles.get(&id) else {
            return QString::from("no such profile");
        };
        if profile.read_only {
            return QString::from("the profile file is read-only");
        }
        let mut terminal = profile.terminal.clone();
        if let Err(error) = terminal.set(&key, &value) {
            return QString::from(&error.to_string());
        }
        if terminal == profile.terminal {
            return QString::default();
        }
        profiles::update(|profiles, _, _| {
            if let Some(mut profile) = profiles.get(&id).cloned() {
                profile.terminal = terminal;
                profiles.insert(profile);
            }
        });
        self.as_mut().save_profile(&id);
        self.refresh();
        QString::default()
    }

    /// See the bridge declaration.
    pub fn reset_option(mut self: Pin<&mut Self>, id: &QString, key: &QString) {
        let (id, key) = (id.to_string(), key.to_string());
        let library = profiles::current();
        let Some(profile) = library.profiles.get(&id) else {
            return;
        };
        if profile.read_only || !profile.terminal.is_set(&key) {
            return;
        }
        profiles::update(|profiles, _, _| {
            if let Some(mut profile) = profiles.get(&id).cloned() {
                profile.terminal.clear(&key);
                profiles.insert(profile);
            }
        });
        self.as_mut().save_profile(&id);
        self.refresh();
    }

    /// See the bridge declaration.
    pub fn create_profile(
        mut self: Pin<&mut Self>,
        name: &QString,
        copy_from: &QString,
    ) -> QString {
        let name = name.to_string().trim().to_owned();
        if name.is_empty() {
            return QString::default();
        }
        let copy_from = copy_from.to_string();
        let library = profiles::current();
        let id = library.profiles.unique_id(&name);
        let mut profile = Profile::new(&id, &name);
        if let Some(source) = library
            .profiles
            .get(&copy_from)
            .filter(|source| !source.is_default())
        {
            profile.terminal = source.terminal.clone();
        }
        profiles::update(|profiles, _, _| profiles.insert(profile));
        self.as_mut().save_profile(&id);
        self.refresh();
        QString::from(&id)
    }

    /// See the bridge declaration.
    pub fn rename_profile(mut self: Pin<&mut Self>, id: &QString, name: &QString) -> bool {
        let (id, name) = (id.to_string(), name.to_string().trim().to_owned());
        let library = profiles::current();
        let Some(profile) = library.profiles.get(&id) else {
            return false;
        };
        if name.is_empty() || profile.read_only {
            return false;
        }
        profiles::update(|profiles, _, _| {
            if let Some(mut profile) = profiles.get(&id).cloned() {
                profile.name = name;
                profiles.insert(profile);
            }
        });
        self.as_mut().save_profile(&id);
        self.refresh();
        true
    }

    /// See the bridge declaration.
    pub fn delete_profile(mut self: Pin<&mut Self>, id: &QString) -> bool {
        let id = id.to_string();
        if id == DEFAULT_PROFILE || profiles::current().profiles.get(&id).is_none() {
            return false;
        }
        profiles::update(|profiles, _, _| {
            profiles.remove(&id);
        });
        if let Some(dir) = self.config_dir.clone() {
            self.as_mut().queue_remove(profile_path(&dir, &id));
        }
        self.refresh();
        true
    }

    /// See the bridge declaration.
    pub fn choices(&self, key: &QString) -> QStringList {
        fn names<T: Copy>(all: &[T], name: fn(T) -> &'static str) -> Vec<String> {
            all.iter().map(|value| name(*value).to_owned()).collect()
        }
        let values: Vec<String> = match key.to_string().as_str() {
            "hinting" => names(Hinting::ALL, Hinting::as_str),
            "cursor_shape" => names(CursorStyle::ALL, CursorStyle::as_str),
            "background_image_fit" => names(ImageFit::ALL, ImageFit::as_str),
            "right_click" => names(RightClick::ALL, RightClick::as_str),
            "osc52" => names(Osc52Access::ALL, Osc52Access::as_str),
            "bell" => names(BellStyle::ALL, BellStyle::as_str),
            "backspace" => names(BackspaceKey::ALL, BackspaceKey::as_str),
            "delete" => names(DeleteKey::ALL, DeleteKey::as_str),
            "encoding" => ENCODINGS.iter().map(|name| (*name).to_owned()).collect(),
            "highlight_color" => ANSI_COLOR_NAMES.iter().map(|n| (*n).to_owned()).collect(),
            "theme_format" => vec!["opensesh".to_owned(), "alacritty".to_owned()],
            "import_patterns" => Format::ALL
                .iter()
                .flat_map(|format| format.patterns().iter().map(|p| (*p).to_owned()))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
            other => {
                tracing::warn!(key = other, "choices() asked for an unknown option");
                Vec::new()
            }
        };
        values.iter().map(QString::from).collect()
    }

    /// See the bridge declaration.
    pub fn save_theme(
        mut self: Pin<&mut Self>,
        id: &QString,
        name: &QString,
        colors: &QString,
    ) -> QString {
        let (id, name) = (id.to_string(), name.to_string().trim().to_owned());
        let Ok(colors) = serde_json::from_str::<Json>(&colors.to_string())
            .map_err(|error| error.to_string())
            .and_then(|value| colors_from_json(&value))
        else {
            return QString::default();
        };
        let library = profiles::current();
        let existing = library.themes.get(&id).filter(|theme| !theme.builtin);
        let name = if name.is_empty() {
            existing.map_or_else(|| "Custom theme".to_owned(), |theme| theme.name.clone())
        } else {
            name
        };
        let theme = TerminalTheme {
            id: existing.map_or_else(|| library.themes.unique_id(&name), |t| t.id.clone()),
            name,
            author: existing.map(|t| t.author.clone()).unwrap_or_default(),
            license: existing.map(|t| t.license.clone()).unwrap_or_default(),
            source: existing.map(|t| t.source.clone()).unwrap_or_default(),
            builtin: false,
            colors,
        };
        let new_id = theme.id.clone();
        self.as_mut().save_theme_file(&theme);
        profiles::update(|_, themes, _| themes.insert(theme));
        self.refresh();
        QString::from(&new_id)
    }

    /// See the bridge declaration.
    pub fn duplicate_theme(mut self: Pin<&mut Self>, id: &QString) -> QString {
        let library = profiles::current();
        let Some(source) = library.themes.get(&id.to_string()).cloned() else {
            return QString::default();
        };
        // "Dracula" becomes "Dracula (copy)".
        let name = format!("{} (copy)", source.name);
        let theme = TerminalTheme {
            id: library.themes.unique_id(&name),
            name,
            builtin: false,
            ..source
        };
        let new_id = theme.id.clone();
        self.as_mut().save_theme_file(&theme);
        profiles::update(|_, themes, _| themes.insert(theme));
        self.refresh();
        QString::from(&new_id)
    }

    /// See the bridge declaration.
    pub fn delete_theme(mut self: Pin<&mut Self>, id: &QString) -> bool {
        let id = id.to_string();
        let removable = profiles::current()
            .themes
            .get(&id)
            .is_some_and(|theme| !theme.builtin);
        if !removable {
            return false;
        }
        profiles::update(|_, themes, _| {
            themes.remove(&id);
        });
        if let Some(dir) = self.config_dir.clone() {
            self.as_mut().queue_remove(theme_path(&dir, &id));
        }
        self.refresh();
        true
    }

    /// See the bridge declaration.
    pub fn import_theme(self: Pin<&mut Self>, file: &QUrl) {
        let qt_thread = self.qt_thread();
        let Some(path) = local_path(file) else {
            let _ = qt_thread.queue(|mut object| {
                object
                    .as_mut()
                    .themes_imported(QString::default(), QString::from("not a local file"));
            });
            return;
        };
        let spawned = std::thread::Builder::new()
            .name("opensesh-theme-import".to_owned())
            .spawn(move || {
                let result = import::import_file(&path).map_err(|error| error.to_string());
                let _ = qt_thread.queue(move |object| object.finish_import(result));
            });
        if let Err(error) = spawned {
            tracing::warn!("could not start a theme import: {error}");
        }
    }

    /// Stores imported themes (GUI thread).
    fn finish_import(mut self: Pin<&mut Self>, result: Result<Vec<import::ImportedTheme>, String>) {
        let imported = match result {
            Ok(imported) => imported,
            Err(error) => {
                tracing::info!("theme import failed: {error}");
                self.as_mut()
                    .themes_imported(QString::default(), QString::from(&error));
                return;
            }
        };
        let mut ids = Vec::new();
        let mut themes_to_save = Vec::new();
        profiles::update(|_, themes, _| {
            for theme in imported {
                for warning in &theme.warnings {
                    tracing::info!(theme = %theme.name, "import: {warning}");
                }
                let id = themes.unique_id(&theme.name);
                let theme = TerminalTheme {
                    id: id.clone(),
                    name: theme.name,
                    author: theme.author,
                    license: theme.license,
                    source: String::new(),
                    builtin: false,
                    colors: theme.colors,
                };
                themes.insert(theme.clone());
                themes_to_save.push(theme);
                ids.push(id);
            }
        });
        for theme in &themes_to_save {
            self.as_mut().save_theme_file(theme);
        }
        self.as_mut().refresh();
        self.as_mut().themes_imported(
            QString::from(&Json::from(ids).to_string()),
            QString::default(),
        );
    }

    /// See the bridge declaration.
    pub fn export_theme(mut self: Pin<&mut Self>, id: &QString, format: &QString, file: &QUrl) {
        let library = profiles::current();
        let theme = library.themes.get(&id.to_string()).cloned();
        let (Some(theme), Some(path), Some(services)) = (theme, local_path(file), services::get())
        else {
            self.as_mut().theme_exported(
                QString::default(),
                QString::from("nothing to export, or not a local file"),
            );
            return;
        };
        let text = match format.to_string().as_str() {
            "alacritty" => import::to_alacritty(&theme),
            _ => theme.to_toml_string(),
        };
        let qt_thread = self.qt_thread();
        services.writer.write(
            path,
            text.into_bytes(),
            0,
            Some(Box::new(move |path, result| {
                let shown = path.display().to_string();
                let error = result
                    .err()
                    .map(|error| error.to_string())
                    .unwrap_or_default();
                let _ = qt_thread.queue(move |mut object| {
                    object
                        .as_mut()
                        .theme_exported(QString::from(&shown), QString::from(&error));
                });
            })),
        );
    }

    /// See the bridge declaration.
    pub fn save_highlight_set(
        mut self: Pin<&mut Self>,
        id: &QString,
        name: &QString,
        rules: &QString,
    ) -> QString {
        let reply =
            |id: &str, error: &str| QString::from(&json!({ "id": id, "error": error }).to_string());
        let (id, name) = (id.to_string(), name.to_string().trim().to_owned());
        let rules = match serde_json::from_str::<Json>(&rules.to_string())
            .map_err(|error| error.to_string())
            .and_then(|value| rules_from_json(&value))
        {
            Ok(rules) => rules,
            Err(error) => return reply("", &error),
        };
        let library = profiles::current();
        if library.highlights.read_only {
            return reply("", "the rules file is read-only");
        }
        let existing = library.highlights.get(&id).filter(|set| !set.builtin);
        let name = if name.is_empty() {
            existing.map_or_else(|| "My rules".to_owned(), |set| set.name.clone())
        } else {
            name
        };
        let new_id =
            existing.map_or_else(|| library.highlights.unique_id(&name), |set| set.id.clone());
        if !valid_id(&new_id) {
            return reply("", "invalid id");
        }
        let set = HighlightSet {
            id: new_id.clone(),
            name,
            rules,
            builtin: false,
        };
        profiles::update(|_, _, highlights| {
            highlights.put(set);
        });
        self.as_mut().save_highlights();
        self.refresh();
        reply(&new_id, "")
    }

    /// See the bridge declaration.
    pub fn duplicate_highlight_set(mut self: Pin<&mut Self>, id: &QString) -> QString {
        let library = profiles::current();
        let Some(source) = library.highlights.get(&id.to_string()).cloned() else {
            return QString::default();
        };
        if library.highlights.read_only {
            return QString::default();
        }
        let name = format!("{} (copy)", source.name);
        let set = HighlightSet {
            id: library.highlights.unique_id(&name),
            name,
            builtin: false,
            ..source
        };
        let new_id = set.id.clone();
        profiles::update(|_, _, highlights| {
            highlights.put(set);
        });
        self.as_mut().save_highlights();
        self.refresh();
        QString::from(&new_id)
    }

    /// See the bridge declaration.
    pub fn delete_highlight_set(mut self: Pin<&mut Self>, id: &QString) -> bool {
        let id = id.to_string();
        let library = profiles::current();
        if library.highlights.read_only || library.highlights.get(&id).is_none_or(|set| set.builtin)
        {
            return false;
        }
        profiles::update(|_, _, highlights| {
            highlights.remove(&id);
        });
        self.as_mut().save_highlights();
        self.refresh();
        true
    }

    /// See the bridge declaration.
    pub fn check_pattern(&self, pattern: &QString, ignore_case: bool) -> QString {
        let rule = HighlightRule {
            pattern: pattern.to_string(),
            ignore_case,
            style: HighlightStyle::default(),
        };
        QString::from(&rule.check().err().unwrap_or_default())
    }
}

impl cxx_qt::Initialize for qobject::TerminalProfiles {
    fn initialize(mut self: Pin<&mut Self>) {
        let Some(services) = services::get() else {
            tracing::warn!("TerminalProfiles created before services; using the defaults");
            self.refresh();
            return;
        };
        let dir = services.paths.config_dir().to_path_buf();
        let test_run = is_test_run();
        for sub in [PROFILES_DIR, THEMES_DIR] {
            // The watchers need the directories; a failure only costs hot reload.
            if test_run {
                break;
            }
            if let Err(error) = std::fs::create_dir_all(dir.join(sub)) {
                tracing::warn!("could not create {sub}/: {error}");
            }
        }
        // A few small files read at startup, before the first frame.
        let disk = load_disk(&dir);
        {
            let mut state = self.as_mut().rust_mut();
            state.folder = QString::from(&dir.display().to_string());
            state.config_dir = Some(dir.clone());
            state.file_problems.clone_from(&disk.problems);
        }
        profiles::update(|profiles, themes, highlights| {
            *profiles = disk.profiles;
            *themes = disk.themes;
            *highlights = disk.highlights;
        });
        if !test_run {
            self.as_mut().start_watchers(&dir);
        }
        self.refresh();
    }
}

/// Keeps the option names the settings pages use in step with the model.
#[cfg(test)]
mod tests {
    use super::*;
    use opensesh_core::terminal::settings::TerminalOverrides;

    #[test]
    fn json_values_map_to_the_option_types() {
        let mut layer = TerminalOverrides::default();
        let set = |layer: &mut TerminalOverrides, key: &str, text: &str| {
            let value = json_to_toml(&serde_json::from_str::<Json>(text).unwrap()).unwrap();
            layer.set(key, &value)
        };
        set(&mut layer, "font_size", "12").unwrap();
        set(&mut layer, "font_size", "12.5").unwrap();
        set(&mut layer, "padding", "8.0").unwrap();
        set(
            &mut layer,
            "font_fallbacks",
            r#"["Noto Sans CJK JP", "Noto Color Emoji"]"#,
        )
        .unwrap();
        set(&mut layer, "bell", r#""sound""#).unwrap();
        assert!(set(&mut layer, "padding", "8.5").is_err());
        assert_eq!(layer.font_size, Some(12.5));
        assert_eq!(layer.padding, Some(8));
        assert!(json_to_toml(&Json::Null).is_none());
    }

    #[test]
    fn theme_colors_survive_json() {
        let set = ThemeSet::default();
        for theme in set.list() {
            let colors = colors_from_json(&colors_json(&theme.colors)).unwrap();
            assert_eq!(colors, theme.colors, "{}", theme.id);
        }
        assert!(colors_from_json(&json!({ "foreground": "#fff" })).is_err());
    }

    #[test]
    fn rules_survive_json_and_bad_ones_are_explained() {
        let library = HighlightLibrary::default();
        for set in &library.sets {
            let value = highlight_set_json(set);
            let rules = rules_from_json(&value["rules"]).unwrap();
            assert_eq!(rules, set.rules, "{}", set.id);
        }
        let error = rules_from_json(&json!([{ "pattern": "(" }])).unwrap_err();
        assert!(error.starts_with("rule 1:"), "{error}");
        let error =
            rules_from_json(&json!([{ "pattern": "x", "foreground": "chartreuse" }])).unwrap_err();
        assert!(error.contains("chartreuse"), "{error}");
    }

    #[test]
    fn settings_json_has_every_option() {
        let value = settings_json(&settings::TerminalSettings::default());
        assert_eq!(value.as_object().unwrap().len(), KEYS.len());
        assert_eq!(value["term"], "xterm-256color");
        assert_eq!(value["font_fallbacks"], json!([]));
    }
}
