//! UI state remembered between runs (window geometry, panels, last view).
//!
//! It lives in `state.toml` in the **data** directory: it is machine-local state, not a setting
//! the user edits or syncs. Any problem reading it silently falls back to the defaults.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::fsutil::{self, WriteOutcome};

/// Name of the state file inside the data directory.
pub const STATE_FILE: &str = "state.toml";

/// Smallest window size we restore (logical pixels).
pub const MIN_WINDOW_SIZE: (u32, u32) = (640, 420);

/// Largest window size we restore; anything bigger is treated as corrupt. It is larger than any
/// real screen, so the window shell still fits the size to the screen it opens on.
const MAX_WINDOW_SIZE: u32 = 16_384;

/// A saved coordinate must be closer than this to the origin. Nothing real is further away:
/// X11 coordinates are 16-bit, and Windows parks minimized windows at (-32000, -32000).
const MAX_WINDOW_OFFSET: u32 = 32_000;

/// Remembered UI state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiState {
    /// Layout version of this file.
    pub schema_version: u32,
    /// Window width in logical pixels.
    pub width: u32,
    /// Window height in logical pixels.
    pub height: u32,
    /// Window position; `None` lets the window manager decide (always the case on Wayland).
    pub x: Option<i32>,
    /// Window position; `None` lets the window manager decide.
    pub y: Option<i32>,
    /// Whether the window was maximized.
    pub maximized: bool,
    /// Whether the side panel was open.
    pub side_panel_open: bool,
    /// Width of the side panel.
    pub side_panel_width: u32,
    /// Identifier of the last active view (e.g. `hosts`).
    pub active_view: String,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            width: 1200,
            height: 760,
            x: None,
            y: None,
            maximized: false,
            side_panel_open: false,
            side_panel_width: 320,
            active_view: "hosts".to_owned(),
        }
    }
}

impl UiState {
    /// Replaces obviously broken values with the defaults: a size under the minimum or larger
    /// than any screen, a position that is incomplete or far outside any desktop, and so on.
    /// (Values that aren't whole numbers, like `nan` or `inf`, already fail to load.)
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        let defaults = Self::default();
        let valid = |value: u32, min: u32| (min..=MAX_WINDOW_SIZE).contains(&value);
        if !valid(self.width, MIN_WINDOW_SIZE.0) || !valid(self.height, MIN_WINDOW_SIZE.1) {
            self.width = defaults.width;
            self.height = defaults.height;
        }
        let on_a_desktop = |coordinate: Option<i32>| {
            coordinate.is_some_and(|value| value.unsigned_abs() < MAX_WINDOW_OFFSET)
        };
        if !on_a_desktop(self.x) || !on_a_desktop(self.y) {
            self.x = None;
            self.y = None;
        }
        if !(160..=1200).contains(&self.side_panel_width) {
            self.side_panel_width = defaults.side_panel_width;
        }
        let view_ok = !self.active_view.is_empty()
            && self.active_view.len() <= 32
            && self
                .active_view
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'_');
        if !view_ok {
            self.active_view = defaults.active_view;
        }
        self
    }
}

/// Reads the state; any error gives the defaults.
#[must_use]
pub fn load_state(path: &Path) -> UiState {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str::<UiState>(&text).ok())
        .unwrap_or_default()
        .sanitized()
}

/// Writes the state atomically (no backups: it's disposable).
///
/// # Errors
///
/// Fails if the state can't be serialized or written.
pub fn save_state(path: &Path, state: &UiState) -> std::io::Result<WriteOutcome> {
    let text = toml::to_string(state).map_err(std::io::Error::other)?;
    fsutil::atomic_write(path, text.as_bytes(), 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STATE_FILE);
        let state = UiState {
            width: 1400,
            height: 900,
            x: Some(-20),
            y: Some(10),
            maximized: true,
            side_panel_open: true,
            side_panel_width: 400,
            active_view: "settings".into(),
            ..UiState::default()
        };
        save_state(&path, &state).unwrap();
        assert_eq!(load_state(&path), state);
    }

    #[test]
    fn missing_or_corrupt_files_give_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STATE_FILE);
        assert_eq!(load_state(&path), UiState::default());
        std::fs::write(&path, "width = \"huge\"").unwrap();
        assert_eq!(load_state(&path), UiState::default());
    }

    #[test]
    fn partial_files_keep_the_other_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STATE_FILE);
        std::fs::write(&path, "maximized = true\n").unwrap();
        let state = load_state(&path);
        assert!(state.maximized);
        assert_eq!(state.width, UiState::default().width);
    }

    #[test]
    fn broken_values_are_sanitized() {
        let state = UiState {
            width: 10,
            height: 99_999,
            side_panel_width: 5,
            active_view: "../../etc".into(),
            ..UiState::default()
        }
        .sanitized();
        let defaults = UiState::default();
        assert_eq!(
            (state.width, state.height),
            (defaults.width, defaults.height)
        );
        assert_eq!(state.side_panel_width, defaults.side_panel_width);
        assert_eq!(state.active_view, defaults.active_view);
    }

    #[test]
    fn sizes_larger_than_any_screen_are_rejected() {
        let defaults = UiState::default();
        for (width, height) in [(16_385, 900), (1400, 16_385), (u32::MAX, u32::MAX)] {
            let state = UiState {
                width,
                height,
                ..UiState::default()
            }
            .sanitized();
            assert_eq!(
                (state.width, state.height),
                (defaults.width, defaults.height),
                "{width}x{height}"
            );
        }
        // The limits themselves are kept; the window shell fits them to the screen.
        let largest = UiState {
            width: 16_384,
            height: 16_384,
            ..UiState::default()
        };
        assert_eq!(largest.clone().sanitized(), largest);
        let smallest = UiState {
            width: 640,
            height: 420,
            ..UiState::default()
        };
        assert_eq!(smallest.clone().sanitized(), smallest);
    }

    #[test]
    fn positions_off_every_desktop_are_dropped() {
        for (x, y) in [
            (Some(-32_000), Some(-32_000)), // A minimized window on Windows.
            (Some(i32::MAX), Some(0)),
            (Some(0), Some(i32::MIN)),
            (Some(100), None),
            (None, Some(100)),
        ] {
            let state = UiState {
                x,
                y,
                ..UiState::default()
            }
            .sanitized();
            assert_eq!((state.x, state.y), (None, None), "{x:?}, {y:?}");
        }
        // A monitor left of or above the primary one has negative coordinates.
        let left = UiState {
            x: Some(-1920),
            y: Some(-200),
            ..UiState::default()
        };
        assert_eq!(left.clone().sanitized(), left);
    }

    #[test]
    fn non_finite_or_fractional_numbers_give_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STATE_FILE);
        for text in [
            "width = inf\nheight = 900\n",
            "width = 1400\nheight = nan\n",
            "x = -inf\ny = 0\n",
            "side_panel_width = 1e400\n",
            "width = 1400.5\n",
        ] {
            std::fs::write(&path, text).unwrap();
            assert_eq!(load_state(&path), UiState::default(), "{text}");
        }
    }
}
