//! `UiState` QML singleton: window geometry and panel state remembered between runs
//! (`state.toml` in the data directory).

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt string type from cxx-qt-lib.
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// Remembered UI state. Change the properties, then call `save()`.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(i32, window_width, cxx_name = "windowWidth")]
        #[qproperty(i32, window_height, cxx_name = "windowHeight")]
        #[qproperty(bool, has_position, cxx_name = "hasPosition")]
        #[qproperty(i32, window_x, cxx_name = "windowX")]
        #[qproperty(i32, window_y, cxx_name = "windowY")]
        #[qproperty(bool, maximized)]
        #[qproperty(bool, side_panel_open, cxx_name = "sidePanelOpen")]
        #[qproperty(i32, side_panel_width, cxx_name = "sidePanelWidth")]
        #[qproperty(QString, active_view, cxx_name = "activeView")]
        type UiState = super::UiStateRust;

        /// Queues a save of the current values (debounced, off the GUI thread).
        #[qinvokable]
        fn save(self: &Self);
    }
}

use cxx_qt_lib::QString;
use opensesh_core::state::{self, UiState};

use crate::services;

/// Rust state behind `UiState`.
#[derive(Debug)]
pub struct UiStateRust {
    window_width: i32,
    window_height: i32,
    has_position: bool,
    window_x: i32,
    window_y: i32,
    maximized: bool,
    side_panel_open: bool,
    side_panel_width: i32,
    active_view: QString,
}

fn to_i32(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

fn to_u32(value: i32) -> u32 {
    u32::try_from(value.max(0)).unwrap_or(0)
}

impl From<UiState> for UiStateRust {
    fn from(state: UiState) -> Self {
        Self {
            window_width: to_i32(state.width),
            window_height: to_i32(state.height),
            has_position: state.x.is_some() && state.y.is_some(),
            window_x: state.x.unwrap_or(0),
            window_y: state.y.unwrap_or(0),
            maximized: state.maximized,
            side_panel_open: state.side_panel_open,
            side_panel_width: to_i32(state.side_panel_width),
            active_view: QString::from(&state.active_view),
        }
    }
}

impl UiStateRust {
    fn to_state(&self) -> UiState {
        UiState {
            width: to_u32(self.window_width),
            height: to_u32(self.window_height),
            x: self.has_position.then_some(self.window_x),
            y: self.has_position.then_some(self.window_y),
            maximized: self.maximized,
            side_panel_open: self.side_panel_open,
            side_panel_width: to_u32(self.side_panel_width),
            active_view: self.active_view.to_string(),
            ..UiState::default()
        }
        .sanitized()
    }
}

impl Default for UiStateRust {
    fn default() -> Self {
        let state = services::get()
            .map(|services| state::load_state(&services.paths.data_dir().join(state::STATE_FILE)))
            .unwrap_or_default();
        state.into()
    }
}

impl qobject::UiState {
    /// See the bridge declaration.
    pub fn save(&self) {
        let Some(services) = services::get() else {
            return;
        };
        let text = match toml::to_string(&self.to_state()) {
            Ok(text) => text,
            Err(error) => {
                tracing::warn!("could not serialize the UI state: {error}");
                return;
            }
        };
        services.writer.write(
            services.paths.data_dir().join(state::STATE_FILE),
            text.into_bytes(),
            0,
            None,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trips_through_the_qt_mirror() {
        let state = UiState {
            width: 1400,
            height: 900,
            x: Some(-8),
            y: Some(20),
            maximized: true,
            side_panel_open: true,
            side_panel_width: 350,
            active_view: "settings".into(),
            ..UiState::default()
        };
        let mirror = UiStateRust::from(state.clone());
        assert_eq!(mirror.to_state(), state);
    }

    #[test]
    fn missing_position_stays_missing() {
        let mirror = UiStateRust::from(UiState::default());
        assert!(!mirror.has_position);
        assert_eq!(mirror.to_state().x, None);
    }
}
