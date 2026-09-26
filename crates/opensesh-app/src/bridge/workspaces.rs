//! `Layouts` and `Workspaces` QML singletons (Sprint 4).
//!
//! - `Layouts` runs the split-tree operations of [`opensesh_core::workspace::layout`] on the
//!   layout of a tab, passed as JSON (`{"pane": 3}` or `{"split": "horizontal", "ratio": 0.5,
//!   "first": ..., "second": ...}`). It keeps no state: QML owns each tab's layout.
//! - `Workspaces` saves the windows, tabs, layouts and sessions to `workspaces/<id>.toml`,
//!   opens them again, and keeps the last session for "restore sessions at startup".
//!
//! **The QML workspace format** (what `save` takes and `open` returns):
//! `{"name": ..., "windows": [{"currentTab": 0, "tabs": [{"title": "", "color": "",
//! "pinned": false, "focused": <pane id>, "zoomed": <pane id or 0>, "layout": <tree of pane
//! ids>, "panes": [{"id": <pane id>, "kind": "local", "profile": "", "directory": ""}]}]}]}`.
//! The files store indexes instead of pane ids; `open` hands out new ids (new sessions).

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt string type from cxx-qt-lib.
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// Split-tree operations on a tab's layout (JSON in, JSON out).
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        type Layouts = super::LayoutsRust;

        /// A layout with one pane.
        #[qinvokable]
        fn single(self: &Self, pane: i32) -> QString;

        /// Splits `target` along `axis` (`horizontal`: side by side, `vertical`: stacked), with
        /// `pane` after it (right or below) when `after`. Empty on failure.
        #[qinvokable]
        fn split(
            self: &Self,
            layout: &QString,
            target: i32,
            axis: &QString,
            pane: i32,
            after: bool,
        ) -> QString;

        /// Removes `target`. Returns `{"layout": ..., "focus": <pane id>}`, or empty when it
        /// isn't there or is the last pane.
        #[qinvokable]
        fn close(self: &Self, layout: &QString, target: i32) -> QString;

        /// The pane next to `from` in `direction` (`left`, `right`, `up`, `down`), 0 for none.
        #[qinvokable]
        fn neighbor(self: &Self, layout: &QString, from: i32, direction: &QString) -> i32;

        /// Moves the divider on `pane`'s `direction` side by `step` (a share of the tab). Empty
        /// when there is none.
        #[qinvokable]
        fn resize(
            self: &Self,
            layout: &QString,
            pane: i32,
            direction: &QString,
            step: f64,
        ) -> QString;

        /// Sets the ratio of the split at `path` (a JSON list of booleans). Empty on failure.
        #[qinvokable]
        #[cxx_name = "setRatio"]
        fn set_ratio(self: &Self, layout: &QString, path: &QString, ratio: f64) -> QString;

        /// Exchanges two panes. Empty on failure.
        #[qinvokable]
        fn swap(self: &Self, layout: &QString, a: i32, b: i32) -> QString;

        /// Every split back to half and half.
        #[qinvokable]
        fn equalize(self: &Self, layout: &QString) -> QString;

        /// `{"panes": [{"pane", "x", "y", "width", "height"}], "dividers": [{"path", "axis",
        /// "area": {...}, "ratio"}]}` in the unit square.
        #[qinvokable]
        fn geometry(self: &Self, layout: &QString) -> QString;

        /// The pane ids in reading order, as a JSON list.
        #[qinvokable]
        fn panes(self: &Self, layout: &QString) -> QString;

        /// The same layout with pane ids replaced through `mapping` (a JSON object from old id
        /// to new id). Empty on failure.
        #[qinvokable]
        fn remap(self: &Self, layout: &QString, mapping: &QString) -> QString;
    }

    extern "RustQt" {
        /// Saved workspaces and the last session.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, workspaces, READ, NOTIFY = changed)]
        #[qproperty(QString, folder, READ, NOTIFY = changed)]
        type Workspaces = super::WorkspacesRust;

        /// The list of saved workspaces changed.
        #[qsignal]
        fn changed(self: Pin<&mut Self>);

        /// A save or removal failed: technical detail.
        #[qsignal]
        fn problem(self: Pin<&mut Self>, detail: QString);

        /// Saves `workspace` (the QML format) as `id`, or as a new workspace called `name` when
        /// `id` is empty. Returns the id, empty on error.
        #[qinvokable]
        fn save(self: Pin<&mut Self>, id: &QString, name: &QString, workspace: &QString)
        -> QString;

        /// The workspace `id` in the QML format, with new pane and tab ids; empty on error.
        #[qinvokable]
        fn open(self: &Self, id: &QString) -> QString;

        /// Renames a workspace. Returns whether it worked.
        #[qinvokable]
        fn rename(self: Pin<&mut Self>, id: &QString, name: &QString) -> bool;

        /// Deletes a workspace. Returns whether it existed.
        #[qinvokable]
        fn remove(self: Pin<&mut Self>, id: &QString) -> bool;

        /// Keeps `workspace` as the last session (written by the background writer; flushed at
        /// exit).
        #[qinvokable]
        #[cxx_name = "saveLastSession"]
        fn save_last_session(self: Pin<&mut Self>, workspace: &QString);

        /// The last session in the QML format with new ids, empty when there is none.
        #[qinvokable]
        #[cxx_name = "lastSession"]
        fn last_session(self: &Self) -> QString;

        /// Converts the QML format to the file format and back with new ids, as saving and
        /// opening would (tests and the smoke test).
        #[qinvokable]
        #[cxx_name = "roundTrip"]
        fn round_trip(self: &Self, workspace: &QString) -> QString;
    }

    impl cxx_qt::Initialize for Workspaces {}
    impl cxx_qt::Threading for Workspaces {}
}

use core::pin::Pin;
use std::collections::HashMap;
use std::path::PathBuf;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use opensesh_core::fsutil;
use opensesh_core::workspace::layout::{Axis, Direction, Layout, Node, PaneId};
use opensesh_core::workspace::{
    self, LAST_SESSION_FILE, PaneState, TabState, WORKSPACES_DIR, WindowState, Workspace,
};
use serde_json::{Value as Json, json};

use crate::bridge::app_info::is_test_run;
use crate::services;
use crate::terminal::registry;

// ---- Layouts -------------------------------------------------------------------------------

/// Rust state behind `Layouts` (none).
#[derive(Debug, Default)]
pub struct LayoutsRust;

fn parse_layout(text: &QString) -> Option<Layout> {
    let node: Node = serde_json::from_str(&text.to_string()).ok()?;
    Layout::from_node(node).ok()
}

fn layout_text(layout: &Layout) -> QString {
    serde_json::to_string(layout)
        .map(|text| QString::from(&text))
        .unwrap_or_default()
}

fn parse_axis(text: &QString) -> Option<Axis> {
    match text.to_string().as_str() {
        "horizontal" => Some(Axis::Horizontal),
        "vertical" => Some(Axis::Vertical),
        _ => None,
    }
}

fn parse_direction(text: &QString) -> Option<Direction> {
    match text.to_string().as_str() {
        "left" => Some(Direction::Left),
        "right" => Some(Direction::Right),
        "up" => Some(Direction::Up),
        "down" => Some(Direction::Down),
        _ => None,
    }
}

impl qobject::Layouts {
    /// See the bridge declaration.
    pub fn single(&self, pane: i32) -> QString {
        layout_text(&Layout::single(pane))
    }

    /// See the bridge declaration.
    pub fn split(
        &self,
        layout: &QString,
        target: i32,
        axis: &QString,
        pane: i32,
        after: bool,
    ) -> QString {
        let (Some(mut layout), Some(axis)) = (parse_layout(layout), parse_axis(axis)) else {
            return QString::default();
        };
        if layout.split(target, axis, pane, after) {
            layout_text(&layout)
        } else {
            QString::default()
        }
    }

    /// See the bridge declaration.
    pub fn close(&self, layout: &QString, target: i32) -> QString {
        let Some(mut layout) = parse_layout(layout) else {
            return QString::default();
        };
        match layout.close(target) {
            Some(focus) => QString::from(&json!({ "layout": layout, "focus": focus }).to_string()),
            None => QString::default(),
        }
    }

    /// See the bridge declaration.
    pub fn neighbor(&self, layout: &QString, from: i32, direction: &QString) -> i32 {
        let (Some(layout), Some(direction)) = (parse_layout(layout), parse_direction(direction))
        else {
            return 0;
        };
        layout.neighbor(from, direction).unwrap_or(0)
    }

    /// See the bridge declaration.
    pub fn resize(&self, layout: &QString, pane: i32, direction: &QString, step: f64) -> QString {
        let (Some(mut layout), Some(direction)) =
            (parse_layout(layout), parse_direction(direction))
        else {
            return QString::default();
        };
        if layout.resize(pane, direction, step) {
            layout_text(&layout)
        } else {
            QString::default()
        }
    }

    /// See the bridge declaration.
    pub fn set_ratio(&self, layout: &QString, path: &QString, ratio: f64) -> QString {
        let Some(mut layout) = parse_layout(layout) else {
            return QString::default();
        };
        let Ok(path) = serde_json::from_str::<Vec<bool>>(&path.to_string()) else {
            return QString::default();
        };
        if layout.set_ratio(&path, ratio) {
            layout_text(&layout)
        } else {
            QString::default()
        }
    }

    /// See the bridge declaration.
    pub fn swap(&self, layout: &QString, a: i32, b: i32) -> QString {
        let Some(mut layout) = parse_layout(layout) else {
            return QString::default();
        };
        if layout.swap(a, b) {
            layout_text(&layout)
        } else {
            QString::default()
        }
    }

    /// See the bridge declaration.
    pub fn equalize(&self, layout: &QString) -> QString {
        let Some(mut layout) = parse_layout(layout) else {
            return QString::default();
        };
        layout.equalize();
        layout_text(&layout)
    }

    /// See the bridge declaration.
    pub fn geometry(&self, layout: &QString) -> QString {
        let Some(layout) = parse_layout(layout) else {
            return QString::from("{\"panes\":[],\"dividers\":[]}");
        };
        QString::from(
            &json!({ "panes": layout.rects(), "dividers": layout.dividers() }).to_string(),
        )
    }

    /// See the bridge declaration.
    pub fn panes(&self, layout: &QString) -> QString {
        let panes = parse_layout(layout)
            .map(|layout| layout.panes())
            .unwrap_or_default();
        QString::from(&Json::from(panes).to_string())
    }

    /// See the bridge declaration.
    pub fn remap(&self, layout: &QString, mapping: &QString) -> QString {
        let Some(layout) = parse_layout(layout) else {
            return QString::default();
        };
        let Ok(mapping) = serde_json::from_str::<HashMap<String, PaneId>>(&mapping.to_string())
        else {
            return QString::default();
        };
        let mapped = layout.map_panes(|id| mapping.get(&id.to_string()).copied().unwrap_or(id));
        // The mapping must keep every id distinct.
        match Layout::from_node(mapped.root().clone()) {
            Ok(mapped) => layout_text(&mapped),
            Err(_) => QString::default(),
        }
    }
}

// ---- Workspace conversions -----------------------------------------------------------------

/// The QML format to the file format: pane ids become indexes.
fn workspace_from_qml(value: &Json) -> Result<Workspace, String> {
    let object = value.as_object().ok_or("expected an object")?;
    let name = object
        .get("name")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned();
    let windows = object
        .get("windows")
        .and_then(Json::as_array)
        .ok_or("expected windows")?;
    let mut out = Workspace {
        name,
        ..Workspace::default()
    };
    for window in windows {
        let tabs = window
            .get("tabs")
            .and_then(Json::as_array)
            .ok_or("expected tabs")?;
        let current_tab = window
            .get("currentTab")
            .and_then(Json::as_u64)
            .and_then(|index| usize::try_from(index).ok())
            .unwrap_or(0);
        let mut state = WindowState {
            current_tab,
            tabs: Vec::new(),
        };
        for tab in tabs {
            let panes = tab
                .get("panes")
                .and_then(Json::as_array)
                .ok_or("expected panes")?;
            let mut index_of = HashMap::new();
            let mut pane_states = Vec::new();
            for (index, pane) in panes.iter().enumerate() {
                let id = pane
                    .get("id")
                    .and_then(Json::as_i64)
                    .and_then(|id| PaneId::try_from(id).ok())
                    .ok_or("a pane without an id")?;
                let text = |key: &str| {
                    pane.get(key)
                        .and_then(Json::as_str)
                        .unwrap_or_default()
                        .to_owned()
                };
                index_of.insert(id, PaneId::try_from(index).map_err(|e| e.to_string())?);
                pane_states.push(PaneState {
                    kind: Some(text("kind"))
                        .filter(|kind| !kind.is_empty())
                        .unwrap_or_else(|| workspace::LOCAL.to_owned()),
                    profile: text("profile"),
                    directory: text("directory"),
                });
            }
            let node: Node =
                serde_json::from_value(tab.get("layout").cloned().unwrap_or(Json::Null))
                    .map_err(|error| format!("layout: {error}"))?;
            let layout = Layout::from_node(node)?;
            if layout.panes().iter().any(|id| !index_of.contains_key(id)) {
                return Err("the layout names a pane that isn't listed".to_owned());
            }
            let mapped = layout.map_panes(|id| index_of.get(&id).copied().unwrap_or(id));
            let pane_index = |key: &str| {
                tab.get(key)
                    .and_then(Json::as_i64)
                    .and_then(|id| PaneId::try_from(id).ok())
                    .and_then(|id| index_of.get(&id).copied())
                    .and_then(|index| usize::try_from(index).ok())
            };
            let text = |key: &str| {
                tab.get(key)
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_owned()
            };
            let mut tab_state = TabState {
                title: text("title"),
                color: text("color"),
                pinned: tab.get("pinned").and_then(Json::as_bool).unwrap_or(false),
                focused: pane_index("focused").unwrap_or(0),
                zoomed: pane_index("zoomed"),
                layout: mapped.root().clone(),
                panes: pane_states,
            };
            tab_state.validate()?;
            state.tabs.push(tab_state);
        }
        if state.current_tab >= state.tabs.len() {
            state.current_tab = 0;
        }
        if !state.tabs.is_empty() {
            out.windows.push(state);
        }
    }
    Ok(out)
}

/// The file format to the QML format, with new ids from `allocate`.
fn workspace_to_qml(workspace: &Workspace, mut allocate: impl FnMut() -> PaneId) -> Json {
    let windows: Vec<Json> = workspace
        .windows
        .iter()
        .map(|window| {
            let tabs: Vec<Json> = window
                .tabs
                .iter()
                .map(|tab| {
                    let ids: Vec<PaneId> = tab.panes.iter().map(|_| allocate()).collect();
                    let id_of = |index: usize| ids.get(index).copied().unwrap_or(0);
                    let layout = Layout::from_node(tab.layout.clone())
                        .map(|layout| {
                            layout.map_panes(|index| usize::try_from(index).map_or(0, id_of))
                        })
                        .map(|layout| serde_json::to_value(&layout).unwrap_or(Json::Null))
                        .unwrap_or(Json::Null);
                    let panes: Vec<Json> = tab
                        .panes
                        .iter()
                        .enumerate()
                        .map(|(index, pane)| {
                            json!({
                                "id": id_of(index),
                                "kind": pane.kind,
                                "profile": pane.profile,
                                "directory": pane.directory,
                            })
                        })
                        .collect();
                    json!({
                        "id": allocate(),
                        "title": tab.title,
                        "color": tab.color,
                        "pinned": tab.pinned,
                        "focused": id_of(tab.focused),
                        "zoomed": tab.zoomed.map_or(0, id_of),
                        "layout": layout,
                        "panes": panes,
                    })
                })
                .collect();
            json!({ "currentTab": window.current_tab, "tabs": tabs })
        })
        .collect();
    json!({ "name": workspace.name, "windows": windows })
}

// ---- Workspaces ----------------------------------------------------------------------------

/// Rust state behind `Workspaces`.
#[derive(Debug, Default)]
pub struct WorkspacesRust {
    workspaces: QString,
    folder: QString,
    config_dir: Option<PathBuf>,
    data_dir: Option<PathBuf>,
}

impl qobject::Workspaces {
    fn dir(&self) -> Option<PathBuf> {
        self.config_dir.as_ref().map(|dir| dir.join(WORKSPACES_DIR))
    }

    /// Lists the folder off the GUI thread and updates `workspaces`.
    fn refresh(self: Pin<&mut Self>) {
        let Some(dir) = self.dir() else {
            return;
        };
        let qt_thread = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("opensesh-workspaces".to_owned())
            .spawn(move || {
                let list: Vec<Json> = workspace::list_dir(&dir)
                    .into_iter()
                    .map(|saved| {
                        json!({
                            "id": saved.id,
                            "name": saved.name,
                            "tabs": saved.tabs,
                            "panes": saved.panes,
                        })
                    })
                    .collect();
                let text = Json::from(list).to_string();
                let _ = qt_thread.queue(move |mut object| {
                    object.as_mut().rust_mut().workspaces = QString::from(&text);
                    object.as_mut().changed();
                });
            });
        if let Err(error) = spawned {
            tracing::warn!("could not list the workspaces: {error}");
        }
    }

    /// Writes `text` to `path` through the background writer, then refreshes the list.
    fn write(self: Pin<&mut Self>, path: PathBuf, text: String) {
        let Some(services) = services::get().filter(|_| !is_test_run()) else {
            return;
        };
        let qt_thread = self.qt_thread();
        services.writer.write(
            path,
            text.into_bytes(),
            fsutil::DEFAULT_BACKUPS,
            Some(Box::new(move |path, result| {
                let failure = result
                    .err()
                    .map(|error| format!("{}: {error}", path.display()));
                let _ = qt_thread.queue(move |mut object| {
                    if let Some(detail) = failure {
                        tracing::warn!("could not save a workspace: {detail}");
                        object.as_mut().problem(QString::from(&detail));
                    }
                    object.refresh();
                });
            })),
        );
    }

    /// See the bridge declaration.
    pub fn save(
        mut self: Pin<&mut Self>,
        id: &QString,
        name: &QString,
        workspace: &QString,
    ) -> QString {
        let Some(dir) = self.dir() else {
            return QString::default();
        };
        let parsed = serde_json::from_str::<Json>(&workspace.to_string())
            .map_err(|error| error.to_string())
            .and_then(|value| workspace_from_qml(&value));
        let mut workspace = match parsed {
            Ok(workspace) => workspace,
            Err(error) => {
                tracing::warn!("not saving a workspace: {error}");
                return QString::default();
            }
        };
        let name = name.to_string().trim().to_owned();
        let id = id.to_string();
        let id = if opensesh_core::terminal::settings::valid_id(&id) {
            id
        } else {
            workspace::unique_id(&dir, &name)
        };
        workspace.name = if name.is_empty() { id.clone() } else { name };
        let Ok(text) = workspace.to_toml_string() else {
            return QString::default();
        };
        // The writer creates the folder.
        self.as_mut().write(dir.join(format!("{id}.toml")), text);
        QString::from(&id)
    }

    /// See the bridge declaration.
    pub fn open(&self, id: &QString) -> QString {
        let Some(dir) = self.dir() else {
            return QString::default();
        };
        let id = id.to_string();
        if !opensesh_core::terminal::settings::valid_id(&id) {
            return QString::default();
        }
        // A small file the user asked to open.
        match Workspace::load(&dir.join(format!("{id}.toml"))) {
            Ok((workspace, warnings)) => {
                for warning in warnings {
                    tracing::warn!(id, "workspace: {warning}");
                }
                QString::from(&workspace_to_qml(&workspace, registry::allocate_id).to_string())
            }
            Err(error) => {
                tracing::warn!("{error}");
                QString::default()
            }
        }
    }

    /// See the bridge declaration.
    pub fn rename(mut self: Pin<&mut Self>, id: &QString, name: &QString) -> bool {
        let (Some(dir), id, name) = (
            self.dir(),
            id.to_string(),
            name.to_string().trim().to_owned(),
        ) else {
            return false;
        };
        if name.is_empty() || !opensesh_core::terminal::settings::valid_id(&id) {
            return false;
        }
        let path = dir.join(format!("{id}.toml"));
        let Ok((mut workspace, _)) = Workspace::load(&path) else {
            return false;
        };
        workspace.name = name;
        let Ok(text) = workspace.to_toml_string() else {
            return false;
        };
        self.as_mut().write(path, text);
        true
    }

    /// See the bridge declaration.
    pub fn remove(self: Pin<&mut Self>, id: &QString) -> bool {
        let (Some(dir), id) = (self.dir(), id.to_string()) else {
            return false;
        };
        if !opensesh_core::terminal::settings::valid_id(&id) {
            return false;
        }
        let path = dir.join(format!("{id}.toml"));
        let Some(services) = services::get().filter(|_| !is_test_run()) else {
            return false;
        };
        let qt_thread = self.qt_thread();
        services.writer.remove(
            path,
            Some(Box::new(move |_, _| {
                let _ = qt_thread.queue(|object| object.refresh());
            })),
        );
        true
    }

    /// See the bridge declaration.
    pub fn save_last_session(self: Pin<&mut Self>, workspace: &QString) {
        let Some(dir) = self.data_dir.clone() else {
            return;
        };
        let parsed = serde_json::from_str::<Json>(&workspace.to_string())
            .map_err(|error| error.to_string())
            .and_then(|value| workspace_from_qml(&value))
            .and_then(|workspace| workspace.to_toml_string());
        match parsed {
            Ok(text) => self.write(dir.join(LAST_SESSION_FILE), text),
            Err(error) => tracing::warn!("not saving the last session: {error}"),
        }
    }

    /// See the bridge declaration.
    pub fn last_session(&self) -> QString {
        let Some(dir) = self.data_dir.clone() else {
            return QString::default();
        };
        let path = dir.join(LAST_SESSION_FILE);
        if !path.exists() {
            return QString::default();
        }
        // One small file, read at startup.
        match Workspace::load(&path) {
            Ok((workspace, warnings)) => {
                for warning in warnings {
                    tracing::warn!("last session: {warning}");
                }
                QString::from(&workspace_to_qml(&workspace, registry::allocate_id).to_string())
            }
            Err(error) => {
                tracing::warn!("{error}");
                QString::default()
            }
        }
    }

    /// See the bridge declaration.
    pub fn round_trip(&self, workspace: &QString) -> QString {
        let parsed = serde_json::from_str::<Json>(&workspace.to_string())
            .map_err(|error| error.to_string())
            .and_then(|value| workspace_from_qml(&value))
            .and_then(|workspace| {
                let text = workspace.to_toml_string()?;
                Workspace::from_toml_str(&text).map(|(workspace, _)| workspace)
            });
        match parsed {
            Ok(workspace) => {
                QString::from(&workspace_to_qml(&workspace, registry::allocate_id).to_string())
            }
            Err(error) => {
                tracing::warn!("workspace round trip failed: {error}");
                QString::default()
            }
        }
    }
}

impl cxx_qt::Initialize for qobject::Workspaces {
    fn initialize(mut self: Pin<&mut Self>) {
        let Some(services) = services::get() else {
            return;
        };
        let config = services.paths.config_dir().to_path_buf();
        let data = services.paths.data_dir().to_path_buf();
        {
            let mut state = self.as_mut().rust_mut();
            state.folder = QString::from(&config.join(WORKSPACES_DIR).display().to_string());
            state.config_dir = Some(config);
            state.data_dir = Some(data);
        }
        self.refresh();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qml_workspace() -> Json {
        json!({
            "name": "Deploy",
            "windows": [{
                "currentTab": 1,
                "tabs": [
                    {"title": "", "color": "", "pinned": false, "focused": 5, "zoomed": 0,
                     "layout": {"pane": 5},
                     "panes": [{"id": 5, "kind": "local", "profile": "", "directory": ""}]},
                    {"title": "api", "color": "yellow", "pinned": true, "focused": 9, "zoomed": 8,
                     "layout": {"split": "horizontal", "ratio": 0.3,
                                "first": {"pane": 7},
                                "second": {"split": "vertical", "ratio": 0.5,
                                           "first": {"pane": 8}, "second": {"pane": 9}}},
                     "panes": [
                        {"id": 7, "kind": "local", "profile": "ops", "directory": "/srv"},
                        {"id": 8, "kind": "local", "profile": "", "directory": ""},
                        {"id": 9, "kind": "local", "profile": "", "directory": "/tmp"}]}
                ]
            }]
        })
    }

    #[test]
    fn the_qml_format_survives_a_file_identically() {
        let workspace = workspace_from_qml(&qml_workspace()).unwrap();
        assert_eq!(workspace.windows[0].tabs[1].focused, 2);
        assert_eq!(workspace.windows[0].tabs[1].zoomed, Some(1));
        let text = workspace.to_toml_string().unwrap();
        let (back, warnings) = Workspace::from_toml_str(&text).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(back, workspace);

        let mut next = 100;
        let value = workspace_to_qml(&back, || {
            next += 1;
            next
        });
        let tab = &value["windows"][0]["tabs"][1];
        assert_eq!(tab["title"], "api");
        assert_eq!(tab["pinned"], true);
        let ids: Vec<i64> = tab["panes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pane| pane["id"].as_i64().unwrap())
            .collect();
        assert_eq!(ids.len(), 3);
        assert!(ids.iter().all(|id| *id > 100), "new ids");
        assert_eq!(tab["focused"].as_i64().unwrap(), ids[2]);
        assert_eq!(tab["zoomed"].as_i64().unwrap(), ids[1]);
        assert_eq!(tab["layout"]["first"]["pane"].as_i64().unwrap(), ids[0]);
        assert_eq!(tab["layout"]["ratio"].as_f64().unwrap(), 0.3);
        assert_eq!(tab["panes"][0]["profile"], "ops");

        // The same structure once more: converting again gives the same file.
        let again = workspace_from_qml(&value).unwrap();
        assert_eq!(again, workspace);
    }

    #[test]
    fn inconsistent_workspaces_are_refused() {
        let mut bad = qml_workspace();
        bad["windows"][0]["tabs"][0]["layout"] = json!({"pane": 6});
        assert!(workspace_from_qml(&bad).is_err());
        assert!(workspace_from_qml(&json!({"windows": 3})).is_err());
    }

    #[test]
    fn layout_operations_through_json() {
        let layouts = LayoutsRust;
        let _ = layouts;
        let layout = Layout::single(1);
        let text = serde_json::to_string(&layout).unwrap();
        assert_eq!(text, "{\"pane\":1}");
        assert_eq!(parse_axis(&QString::from("vertical")), Some(Axis::Vertical));
        assert_eq!(parse_direction(&QString::from("up")), Some(Direction::Up));
        assert!(parse_direction(&QString::from("sideways")).is_none());
    }
}
