//! Input encoding: what the terminal writes to the PTY for keys, mouse, paste and focus.
//! Pure functions over the terminal modes, unit-tested against xterm conventions.

pub mod keys;
pub mod mouse;
pub mod paste;

use alacritty_terminal::term::TermMode;

/// The terminal state that input encoding depends on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InputModes {
    /// The engine's modes: application cursor and keypad (DECCKM, DECKPAM), bracketed paste,
    /// focus reporting, mouse reporting and encodings, alternate screen and alternate scroll,
    /// line feed / new line mode.
    pub term: TermMode,
    /// X10 mouse reporting (`CSI ? 9 h`), which the engine doesn't track; the side parser in
    /// [`crate::osc`] does.
    pub x10_mouse: bool,
}

/// Keyboard modifiers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    /// Shift.
    pub shift: bool,
    /// Control.
    pub ctrl: bool,
    /// Alt (Option on macOS).
    pub alt: bool,
    /// Super / Windows / Command.
    pub meta: bool,
}

impl Modifiers {
    /// No modifier held.
    pub const NONE: Self = Self {
        shift: false,
        ctrl: false,
        alt: false,
        meta: false,
    };

    /// Whether no modifier is held.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self == Self::NONE
    }
}
