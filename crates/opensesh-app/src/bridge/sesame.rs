//! Sprint 0 "hello world" QObject: a door you can knock on from QML.

#[cxx_qt::bridge]
pub mod qobject {
    extern "RustQt" {
        /// Counts how many times the user knocked on the door. Both properties are read-only for
        /// QML; `knock()` is the only way to change them.
        #[qobject]
        #[qml_element]
        #[qproperty(i32, knocks, READ, NOTIFY)]
        #[qproperty(bool, open, READ, NOTIFY)]
        type SesameDoor = super::SesameDoorRust;

        /// Knocks once more on the door.
        #[qinvokable]
        fn knock(self: Pin<&mut Self>);
    }
}

use core::pin::Pin;

use cxx_qt::CxxQtType;

/// Number of knocks after which the door opens.
pub const KNOCKS_TO_OPEN: i32 = 3;

/// Rust state behind the `SesameDoor` QML element.
#[derive(Debug, Default)]
pub struct SesameDoorRust {
    knocks: i32,
    open: bool,
}

impl qobject::SesameDoor {
    /// Counts one more knock and opens the door after [`KNOCKS_TO_OPEN`] knocks.
    pub fn knock(mut self: Pin<&mut Self>) {
        #[cfg(debug_assertions)]
        debug_panic_if_requested();

        let old_knocks = *self.knocks();
        let was_open = *self.open();
        let knocks = next_knock_count(old_knocks);
        let open = is_open(knocks);

        // Update all state before notifying, so handlers never observe a half-updated object,
        // and only notify what really changed.
        {
            let mut state = self.as_mut().rust_mut();
            state.knocks = knocks;
            state.open = open;
        }
        if knocks != old_knocks {
            self.as_mut().knocks_changed();
        }
        if open != was_open {
            self.as_mut().open_changed();
        }
    }
}

/// Environment variable that makes the Knock button panic in debug builds.
#[cfg(debug_assertions)]
pub const DEBUG_PANIC_ENV: &str = "OPENSESH_DEBUG_PANIC";

/// Debug builds only: with `OPENSESH_DEBUG_PANIC=1` the Knock button panics inside a QML ->
/// Rust call, to test the panic hook and the crash dialog end to end (see
/// docs/testing/manual-matrix.md). Release builds don't contain this code.
#[cfg(debug_assertions)]
#[allow(clippy::panic)] // Deliberate, debug-only test hook.
fn debug_panic_if_requested() {
    if std::env::var_os(DEBUG_PANIC_ENV).is_some_and(|value| !value.is_empty() && value != "0") {
        panic!("{DEBUG_PANIC_ENV} is set: simulated panic in SesameDoor::knock");
    }
}

/// Knock counter arithmetic, kept separate so it can be unit-tested without Qt.
#[must_use]
pub const fn next_knock_count(current: i32) -> i32 {
    current.saturating_add(1)
}

/// Whether the door is open after `knocks` knocks.
#[must_use]
pub const fn is_open(knocks: i32) -> bool {
    knocks >= KNOCKS_TO_OPEN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knocking_increments_and_saturates() {
        assert_eq!(next_knock_count(0), 1);
        assert_eq!(next_knock_count(41), 42);
        assert_eq!(next_knock_count(i32::MAX), i32::MAX);
    }

    #[test]
    fn door_opens_after_three_knocks() {
        assert!(!is_open(0));
        assert!(!is_open(KNOCKS_TO_OPEN - 1));
        assert!(is_open(KNOCKS_TO_OPEN));
    }
}
