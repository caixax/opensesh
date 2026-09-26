//! Tabs, splits and saved workspaces (PLAN Sprint 4).
//!
//! - [`layout`]: the split tree of one tab and its operations.
//! - This module: a workspace (windows, their tabs with a layout each, and what runs in each
//!   pane) as saved in `workspaces/<id>.toml` and in the last session file, which "restore
//!   sessions at startup" reads.
//!
//! In a saved workspace a layout's pane ids are indexes into its tab's `pane` list; the app
//! gives them new ids (new sessions) when it restores the workspace.

pub mod layout;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use layout::{Layout, Node};

/// Folder of the saved workspaces inside the config directory.
pub const WORKSPACES_DIR: &str = "workspaces";

/// The last session, in the data directory (it is state, not a setting).
pub const LAST_SESSION_FILE: &str = "last-session.toml";

/// Current layout of the workspace files.
pub const SCHEMA_VERSION: i64 = 1;

/// Pane kind of a local terminal.
pub const LOCAL: &str = "local";

const HEADER: &str = "# OpenSesh workspace: windows, their tabs, the split layout of each tab and\n\
                      # what runs in every pane. Opening it starts new sessions in the same places.\n";

fn schema_version() -> i64 {
    SCHEMA_VERSION
}

fn local() -> String {
    LOCAL.to_owned()
}

/// What runs in a pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneState {
    /// `local` for a local terminal (the only kind until SSH arrives).
    #[serde(default = "local")]
    pub kind: String,
    /// Terminal profile id; empty for the default one.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub profile: String,
    /// Working directory to start in (the shell's last OSC 7 directory); empty for home.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub directory: String,
}

impl Default for PaneState {
    fn default() -> Self {
        Self {
            kind: local(),
            profile: String::new(),
            directory: String::new(),
        }
    }
}

/// One tab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabState {
    /// The name the user gave the tab; empty to follow the focused terminal's title.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// Tab color: a name from [`crate::theme::TAB_COLOR_NAMES`]; empty for none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub color: String,
    /// Pinned tabs come first and skip "close others".
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pinned: bool,
    /// Index of the focused pane in `panes`.
    #[serde(default)]
    pub focused: usize,
    /// Index of the maximized pane, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoomed: Option<usize>,
    /// The split tree; its pane ids are indexes into `panes`.
    pub layout: Node,
    /// What runs in each pane.
    #[serde(default, rename = "pane")]
    pub panes: Vec<PaneState>,
}

/// One window.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    /// Index of the tab shown.
    #[serde(default)]
    pub current_tab: usize,
    /// Its tabs, in order.
    #[serde(default, rename = "tab")]
    pub tabs: Vec<TabState>,
}

/// A saved workspace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    /// Layout version of the file.
    #[serde(default = "schema_version")]
    pub schema_version: i64,
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// Its windows; the first one is the main window.
    #[serde(default, rename = "window")]
    pub windows: Vec<WindowState>,
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            name: String::new(),
            windows: Vec::new(),
        }
    }
}

/// Why a workspace file can't be used.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    /// The file couldn't be read.
    #[error("could not read {path}")]
    Read {
        /// The file.
        path: String,
        /// Why.
        #[source]
        source: std::io::Error,
    },
    /// Not valid TOML, or not a workspace.
    #[error("{path} is not a workspace: {message}")]
    Syntax {
        /// The file.
        path: String,
        /// The parser's message.
        message: String,
    },
}

impl TabState {
    /// A tab with one pane.
    #[must_use]
    pub fn single(pane: PaneState) -> Self {
        Self {
            title: String::new(),
            color: String::new(),
            pinned: false,
            focused: 0,
            zoomed: None,
            layout: Node::Pane { pane: 0 },
            panes: vec![pane],
        }
    }

    /// Checks the tab: every pane index of the layout exists and appears once, and every pane is
    /// in the layout. Fixes what it can (the focused and zoomed indexes, ratios) and reports
    /// what it can't.
    ///
    /// # Errors
    ///
    /// A short reason when the tab can't be restored.
    pub fn validate(&mut self) -> Result<(), String> {
        let layout = Layout::from_node(self.layout.clone())?;
        let mut used = layout.panes();
        used.sort_unstable();
        let expected: Vec<i32> = (0..self.panes.len())
            .map(|index| i32::try_from(index).unwrap_or(i32::MAX))
            .collect();
        if used != expected {
            return Err(format!(
                "the layout's panes {used:?} don't match the {} pane(s) listed",
                self.panes.len()
            ));
        }
        self.layout = layout.root().clone();
        if self.focused >= self.panes.len() {
            self.focused = 0;
        }
        if self.zoomed.is_some_and(|zoomed| zoomed >= self.panes.len()) {
            self.zoomed = None;
        }
        Ok(())
    }
}

impl Workspace {
    /// Parses a workspace file's text. Tabs that can't be restored are dropped with a warning;
    /// windows left without tabs are dropped too.
    ///
    /// # Errors
    ///
    /// The parser's message when the text isn't a workspace at all.
    pub fn from_toml_str(text: &str) -> Result<(Self, Vec<String>), String> {
        let mut workspace: Self = toml::from_str(text).map_err(|error| error.to_string())?;
        let mut warnings = Vec::new();
        if workspace.schema_version > SCHEMA_VERSION {
            warnings.push(format!(
                "written by a newer OpenSesh (schema {}); some parts may be missing",
                workspace.schema_version
            ));
        }
        for (w, window) in workspace.windows.iter_mut().enumerate() {
            let mut kept = Vec::new();
            for (t, mut tab) in std::mem::take(&mut window.tabs).into_iter().enumerate() {
                match tab.validate() {
                    Ok(()) => kept.push(tab),
                    Err(reason) => warnings.push(format!("window {w}, tab {t} skipped: {reason}")),
                }
            }
            window.tabs = kept;
            if window.current_tab >= window.tabs.len() {
                window.current_tab = 0;
            }
        }
        workspace.windows.retain(|window| !window.tabs.is_empty());
        Ok((workspace, warnings))
    }

    /// The file's text.
    ///
    /// # Errors
    ///
    /// Only if a value can't be written as TOML (not expected for a valid workspace).
    pub fn to_toml_string(&self) -> Result<String, String> {
        let body = toml::to_string(self).map_err(|error| error.to_string())?;
        Ok(format!("{HEADER}\n{body}"))
    }

    /// Every pane of every tab.
    pub fn panes(&self) -> impl Iterator<Item = &PaneState> {
        self.windows
            .iter()
            .flat_map(|window| window.tabs.iter())
            .flat_map(|tab| tab.panes.iter())
    }

    /// Reads a workspace file.
    ///
    /// # Errors
    ///
    /// [`WorkspaceError`] when the file can't be read or isn't a workspace.
    pub fn load(path: &Path) -> Result<(Self, Vec<String>), WorkspaceError> {
        let text = std::fs::read_to_string(path).map_err(|source| WorkspaceError::Read {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_toml_str(&text).map_err(|message| WorkspaceError::Syntax {
            path: path.display().to_string(),
            message,
        })
    }
}

/// A saved workspace in the folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedWorkspace {
    /// File name without `.toml`.
    pub id: String,
    /// Display name (the id when the file has none).
    pub name: String,
    /// Tabs in all its windows.
    pub tabs: usize,
    /// Panes in all its tabs.
    pub panes: usize,
}

/// The workspaces in `dir`, by name. Unreadable files are skipped.
#[must_use]
pub fn list_dir(dir: &Path) -> Vec<SavedWorkspace> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<SavedWorkspace> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .filter_map(|path| {
            let id = path.file_stem()?.to_str()?.to_owned();
            if !crate::terminal::settings::valid_id(&id) {
                return None;
            }
            let (workspace, _) = Workspace::load(&path).ok()?;
            let tabs = workspace.windows.iter().map(|w| w.tabs.len()).sum();
            let panes = workspace.panes().count();
            let name = if workspace.name.trim().is_empty() {
                id.clone()
            } else {
                workspace.name.trim().to_owned()
            };
            Some(SavedWorkspace {
                id,
                name,
                tabs,
                panes,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
    out
}

/// Path of workspace `id` inside `config_dir`.
#[must_use]
pub fn workspace_path(config_dir: &Path, id: &str) -> PathBuf {
    config_dir.join(WORKSPACES_DIR).join(format!("{id}.toml"))
}

/// A new id for a workspace called `name`, not used in `dir`.
#[must_use]
pub fn unique_id(dir: &Path, name: &str) -> String {
    let base = crate::terminal::profile::slug(name);
    let base = if base.is_empty() {
        "workspace".to_owned()
    } else {
        base
    };
    let mut id = base.clone();
    let mut n = 2;
    while dir.join(format!("{id}.toml")).exists() {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

#[cfg(test)]
mod tests {
    use super::layout::Axis;
    use super::*;

    fn sample() -> Workspace {
        let mut layout = Layout::single(0);
        assert!(layout.split(0, Axis::Horizontal, 1, true));
        assert!(layout.split(1, Axis::Vertical, 2, true));
        assert!(layout.set_ratio(&[], 0.3));
        let tab = TabState {
            title: "api".into(),
            color: "yellow".into(),
            pinned: true,
            focused: 2,
            zoomed: Some(1),
            layout: layout.root().clone(),
            panes: vec![
                PaneState {
                    kind: LOCAL.into(),
                    profile: "ops".into(),
                    directory: "/srv/api".into(),
                },
                PaneState::default(),
                PaneState {
                    directory: "C:/Users/me".into(),
                    ..PaneState::default()
                },
            ],
        };
        Workspace {
            schema_version: SCHEMA_VERSION,
            name: "Deploy".into(),
            windows: vec![
                WindowState {
                    current_tab: 1,
                    tabs: vec![TabState::single(PaneState::default()), tab],
                },
                WindowState {
                    current_tab: 0,
                    tabs: vec![TabState::single(PaneState {
                        profile: "big".into(),
                        ..PaneState::default()
                    })],
                },
            ],
        }
    }

    #[test]
    fn a_workspace_round_trips_identically() {
        let workspace = sample();
        let text = workspace.to_toml_string().unwrap();
        assert!(text.starts_with("# OpenSesh workspace"));
        let (back, warnings) = Workspace::from_toml_str(&text).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(back, workspace);
        assert_eq!(back.panes().count(), 5);
    }

    #[test]
    fn broken_tabs_are_dropped_with_warnings() {
        let text = r#"
            name = "x"
            [[window]]
            current_tab = 5
            [[window.tab]]
            focused = 9
            layout = { split = "horizontal", ratio = 0.5, first = { pane = 0 }, second = { pane = 3 } }
            [[window.tab.pane]]
            [[window.tab.pane]]
            [[window.tab]]
            focused = 4
            zoomed = 8
            layout = { pane = 0 }
            [[window.tab.pane]]
            profile = "p"
            [[window]]
            [[window.tab]]
            layout = { pane = 0 }
        "#;
        let (workspace, warnings) = Workspace::from_toml_str(text).unwrap();
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert_eq!(
            workspace.windows.len(),
            1,
            "the window without a valid tab goes"
        );
        let window = &workspace.windows[0];
        assert_eq!(window.tabs.len(), 1);
        assert_eq!(window.current_tab, 0);
        assert_eq!(window.tabs[0].focused, 0);
        assert_eq!(window.tabs[0].zoomed, None);
        assert_eq!(window.tabs[0].panes[0].kind, LOCAL);
        assert!(Workspace::from_toml_str("window = 3").is_err());
    }

    #[test]
    fn workspaces_are_listed_and_named() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("deploy.toml"),
            sample().to_toml_string().unwrap(),
        )
        .unwrap();
        let mut unnamed = sample();
        unnamed.name = String::new();
        std::fs::write(
            dir.path().join("alpha.toml"),
            unnamed.to_toml_string().unwrap(),
        )
        .unwrap();
        std::fs::write(dir.path().join("broken.toml"), "[[window").unwrap();
        let list = list_dir(dir.path());
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "alpha");
        assert_eq!(list[1].name, "Deploy");
        assert_eq!((list[1].tabs, list[1].panes), (3, 5));
        assert_eq!(unique_id(dir.path(), "Deploy"), "deploy-2");
        assert_eq!(unique_id(dir.path(), "New one"), "new-one");
        assert_eq!(
            workspace_path(dir.path(), "x"),
            dir.path().join("workspaces").join("x.toml")
        );
    }
}
