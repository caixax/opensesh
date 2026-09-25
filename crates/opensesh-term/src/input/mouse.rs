//! Mouse reporting: what the terminal writes when a program asked for mouse events, and the
//! alternate scroll mode (wheel as arrow keys on the alternate screen).
//!
//! Protocols (which events are reported; the last one a program set wins):
//!
//! | DECSET | Name | Reports | State |
//! |---|---|---|---|
//! | 9 | X10 | button presses only, no modifiers, no wheel | [`InputModes::x10_mouse`] |
//! | 1000 | normal | presses, releases and wheel, with modifiers | `TermMode::MOUSE_REPORT_CLICK` |
//! | 1002 | button event | 1000 plus motion while a button is held | `TermMode::MOUSE_DRAG` |
//! | 1003 | any event | 1002 plus motion without a button | `TermMode::MOUSE_MOTION` |
//!
//! Encodings (how an event is written): SGR (1006, `CSI < b ; x ; y M/m`, no size limit), UTF-8
//! (1005, coordinates up to 2015) and the default `CSI M b x y` with one byte per value
//! (coordinates up to 223). Events beyond an encoding's limit are not reported, as in alacritty.
//! urxvt (1015) and SGR pixels (1016) are not supported (the engine ignores them, like VTE and
//! alacritty).
//!
//! The button code is `button + 4 * shift + 8 * alt + 16 * ctrl (+ 32 for motion)`, with left 0,
//! middle 1, right 2, wheel up/down/left/right 64-67, and 3 for a release in the non-SGR
//! encodings (xterm `button.c`).

use alacritty_terminal::term::TermMode;

use crate::input::keys::push_decimal;
use crate::input::{InputModes, Modifiers};

/// A mouse button, or a wheel direction (one wheel step is one press).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    /// Left button.
    Left,
    /// Middle button.
    Middle,
    /// Right button.
    Right,
    /// Wheel rotated away from the user (Qt `angleDelta().y() > 0`).
    WheelUp,
    /// Wheel rotated towards the user (Qt `angleDelta().y() < 0`).
    WheelDown,
    /// Horizontal wheel to the left (Qt `angleDelta().x() > 0`).
    WheelLeft,
    /// Horizontal wheel to the right (Qt `angleDelta().x() < 0`).
    WheelRight,
    /// No button: motion with no button held.
    None,
}

/// What the mouse did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseAction {
    /// A button was pressed, or the wheel moved one step.
    Press,
    /// A button was released.
    Release,
    /// The pointer moved to another cell. The caller reports motion only when the cell under the
    /// pointer changes, with the lowest held button (left, then middle, then right) or
    /// [`MouseButton::None`].
    Move,
}

/// One mouse event, in grid cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseInput {
    /// What happened.
    pub action: MouseAction,
    /// The button (or wheel direction) involved.
    pub button: MouseButton,
    /// Column of the cell under the pointer, 0-based, clamped to the grid.
    pub column: u16,
    /// Viewport row of the cell under the pointer, 0-based, clamped to the grid. Events on
    /// scrollback lines (display offset above 0) are not reported by the caller.
    pub row: u16,
    /// The modifiers held.
    pub mods: Modifiers,
}

/// The mouse reporting protocol a program enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseProtocol {
    /// X10 compatibility (DECSET 9): presses only.
    X10,
    /// Normal tracking (DECSET 1000): presses, releases and wheel.
    Normal,
    /// Button-event tracking (DECSET 1002): also motion while a button is held.
    ButtonEvent,
    /// Any-event tracking (DECSET 1003): also motion without a button (the item needs hover
    /// events).
    AnyEvent,
}

/// The active mouse protocol, if any.
///
/// X10 wins when [`InputModes::x10_mouse`] is set: the tracker clears it when a program sets
/// 1000, 1002 or 1003 afterwards, and the engine doesn't clear those when it sets 9.
#[must_use]
pub fn mouse_protocol(modes: &InputModes) -> Option<MouseProtocol> {
    let term = modes.term;
    if modes.x10_mouse {
        Some(MouseProtocol::X10)
    } else if term.contains(TermMode::MOUSE_MOTION) {
        Some(MouseProtocol::AnyEvent)
    } else if term.contains(TermMode::MOUSE_DRAG) {
        Some(MouseProtocol::ButtonEvent)
    } else if term.contains(TermMode::MOUSE_REPORT_CLICK) {
        Some(MouseProtocol::Normal)
    } else {
        None
    }
}

/// Whether mouse events go to the program instead of local selection and scrolling: a protocol
/// is on and Shift is not held. Shift bypasses reporting (xterm's behaviour), so the user can
/// always select text, and Shift+wheel scrolls locally.
#[must_use]
pub fn mouse_reporting_active(modes: &InputModes, mods: Modifiers) -> bool {
    mouse_protocol(modes).is_some() && !mods.shift
}

/// Encodes a mouse event for the active protocol and encoding. Returns `None` when the event
/// is not reported: no protocol, an event the protocol ignores (motion in normal mode, releases
/// in X10, wheel releases), or a position the encoding can't represent.
///
/// It doesn't check Shift: call [`mouse_reporting_active`] first.
#[must_use]
pub fn encode_mouse(input: &MouseInput, modes: &InputModes) -> Option<Vec<u8>> {
    let protocol = mouse_protocol(modes)?;
    let sgr = modes.term.contains(TermMode::SGR_MOUSE);
    let button = button_code(input.button);

    let (code, release) = if protocol == MouseProtocol::X10 {
        // Presses of the three buttons only, without modifiers.
        match (input.action, button) {
            (MouseAction::Press, Some(button @ 0..=2)) => (button, false),
            _ => return None,
        }
    } else {
        let modifiers = modifier_bits(input.mods);
        match (input.action, button) {
            (MouseAction::Press, Some(button)) => (button + modifiers, false),
            // Wheel "buttons" have no release.
            (MouseAction::Release, Some(button @ 0..=2)) => {
                let button = if sgr { button } else { 3 };
                (button + modifiers, true)
            }
            (MouseAction::Move, held) => {
                let button = match (protocol, held) {
                    (MouseProtocol::ButtonEvent | MouseProtocol::AnyEvent, Some(b @ 0..=2)) => b,
                    // Only MouseButton::None has no button code.
                    (MouseProtocol::AnyEvent, None) => 3,
                    _ => return None,
                };
                (32 + button + modifiers, false)
            }
            _ => return None,
        }
    };

    let column = u32::from(input.column) + 1;
    let row = u32::from(input.row) + 1;
    if sgr {
        let mut out = Vec::with_capacity(16);
        out.extend_from_slice(b"\x1b[<");
        push_decimal(&mut out, code);
        out.push(b';');
        push_decimal(&mut out, column);
        out.push(b';');
        push_decimal(&mut out, row);
        out.push(if release { b'm' } else { b'M' });
        Some(out)
    } else {
        let utf8 = modes.term.contains(TermMode::UTF8_MOUSE);
        let mut out = Vec::with_capacity(9);
        out.extend_from_slice(b"\x1b[M");
        for value in [code, column, row] {
            push_legacy(&mut out, 32 + value, utf8)?;
        }
        Some(out)
    }
}

/// Appends one value of the default (one byte, up to 255) or UTF-8 (up to 2047) encoding.
fn push_legacy(out: &mut Vec<u8>, value: u32, utf8: bool) -> Option<()> {
    if utf8 && value >= 0x80 {
        // Two-byte UTF-8, as xterm writes it (values up to 2047 only).
        if value > 0x7ff {
            return None;
        }
        out.push(0xc0 | u8::try_from(value >> 6).ok()?);
        out.push(0x80 | u8::try_from(value & 0x3f).ok()?);
    } else {
        out.push(u8::try_from(value).ok()?);
    }
    Some(())
}

/// The button part of the code (`None` for [`MouseButton::None`]).
fn button_code(button: MouseButton) -> Option<u32> {
    Some(match button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
        MouseButton::WheelUp => 64,
        MouseButton::WheelDown => 65,
        MouseButton::WheelLeft => 66,
        MouseButton::WheelRight => 67,
        MouseButton::None => return None,
    })
}

/// The modifier part of the code: Shift 4, Alt (Meta) 8, Ctrl 16. Super is not reported.
fn modifier_bits(mods: Modifiers) -> u32 {
    4 * u32::from(mods.shift) + 8 * u32::from(mods.alt) + 16 * u32::from(mods.ctrl)
}

/// The most arrow-key presses one [`alternate_scroll`] call writes.
const MAX_ALTERNATE_SCROLL_LINES: usize = 1000;

/// Alternate scroll (DECSET 1007): on the alternate screen, with the mode on and no mouse
/// reporting, the wheel sends cursor up or down keys (one per line) so pagers and editors
/// scroll. `lines > 0` scrolls up (wheel away from the user), `lines < 0` down. The keys follow
/// the cursor key mode: `CSI A` / `CSI B`, or `SS3 A` / `SS3 B` with DECCKM (as xterm and VTE).
///
/// Returns `None` when alternate scroll doesn't apply (then the wheel scrolls the scrollback)
/// or `lines` is 0. The caller skips it while Shift is held. At most 1000 lines are sent.
#[must_use]
pub fn alternate_scroll(lines: i32, modes: &InputModes) -> Option<Vec<u8>> {
    let term = modes.term;
    if lines == 0
        || !term.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL)
        || mouse_protocol(modes).is_some()
    {
        return None;
    }
    let final_byte = if lines > 0 { b'A' } else { b'B' };
    let introducer = if term.contains(TermMode::APP_CURSOR) {
        b'O'
    } else {
        b'['
    };
    let count = usize::try_from(lines.unsigned_abs())
        .unwrap_or(MAX_ALTERNATE_SCROLL_LINES)
        .min(MAX_ALTERNATE_SCROLL_LINES);
    Some([0x1b, introducer, final_byte].repeat(count))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT: MouseButton = MouseButton::Left;
    const MIDDLE: MouseButton = MouseButton::Middle;
    const RIGHT: MouseButton = MouseButton::Right;
    const NONE: MouseButton = MouseButton::None;
    const PRESS: MouseAction = MouseAction::Press;
    const RELEASE: MouseAction = MouseAction::Release;
    const MOVE: MouseAction = MouseAction::Move;

    /// Modes from a protocol (`"9"`, `"1000"`, `"1002"`, `"1003"`) and an encoding
    /// (`"default"`, `"utf8"`, `"sgr"`).
    fn modes(protocol: &str, encoding: &str) -> InputModes {
        let mut modes = InputModes::default();
        match protocol {
            "9" => modes.x10_mouse = true,
            "1000" => modes.term.insert(TermMode::MOUSE_REPORT_CLICK),
            "1002" => modes.term.insert(TermMode::MOUSE_DRAG),
            "1003" => modes.term.insert(TermMode::MOUSE_MOTION),
            "" => {}
            other => panic!("unknown protocol {other}"),
        }
        match encoding {
            "default" => {}
            "utf8" => modes.term.insert(TermMode::UTF8_MOUSE),
            "sgr" => modes.term.insert(TermMode::SGR_MOUSE),
            other => panic!("unknown encoding {other}"),
        }
        modes
    }

    fn mods(spec: &str) -> Modifiers {
        Modifiers {
            shift: spec.contains('S'),
            alt: spec.contains('A'),
            ctrl: spec.contains('C'),
            meta: spec.contains('M'),
        }
    }

    /// Encodes an event at a 1-based cell, as the research table states positions.
    fn at(
        protocol: &str,
        encoding: &str,
        action: MouseAction,
        button: MouseButton,
        mod_spec: &str,
        column1: u16,
        row1: u16,
    ) -> Option<Vec<u8>> {
        let input = MouseInput {
            action,
            button,
            column: column1 - 1,
            row: row1 - 1,
            mods: mods(mod_spec),
        };
        encode_mouse(&input, &modes(protocol, encoding))
    }

    /// An event at column 10, row 5 (1-based), the default position of the research table.
    fn ev(
        protocol: &str,
        encoding: &str,
        action: MouseAction,
        button: MouseButton,
        mod_spec: &str,
    ) -> Option<Vec<u8>> {
        at(protocol, encoding, action, button, mod_spec, 10, 5)
    }

    fn bytes(expected: &[u8]) -> Option<Vec<u8>> {
        Some(expected.to_vec())
    }

    /// The mouse table in the Sprint 2 input research (s2_input.md §6.7), row by row.
    #[test]
    fn mouse_table() {
        use MouseButton::{WheelDown, WheelLeft, WheelRight, WheelUp};
        let d = "default";
        // 1-3: presses and a release in normal mode.
        assert_eq!(ev("1000", d, PRESS, LEFT, ""), bytes(b"\x1b[M *%"));
        assert_eq!(ev("1000", d, RELEASE, LEFT, ""), bytes(b"\x1b[M#*%"));
        assert_eq!(ev("1000", d, PRESS, MIDDLE, ""), bytes(b"\x1b[M!*%"));
        assert_eq!(ev("1000", d, PRESS, RIGHT, ""), bytes(b"\x1b[M\"*%"));
        // 4-6: modifiers.
        assert_eq!(ev("1000", d, PRESS, LEFT, "C"), bytes(b"\x1b[M0*%"));
        assert_eq!(ev("1000", d, PRESS, LEFT, "A"), bytes(b"\x1b[M(*%"));
        assert_eq!(ev("1000", d, PRESS, LEFT, "CAS"), bytes(b"\x1b[M<*%"));
        // 7-8: wheel.
        assert_eq!(ev("1000", d, PRESS, WheelUp, ""), bytes(b"\x1b[M`*%"));
        assert_eq!(ev("1000", d, PRESS, WheelDown, ""), bytes(b"\x1b[Ma*%"));
        assert_eq!(ev("1000", d, PRESS, WheelLeft, ""), bytes(b"\x1b[Mb*%"));
        assert_eq!(ev("1000", d, PRESS, WheelRight, ""), bytes(b"\x1b[Mc*%"));
        assert_eq!(ev("1000", d, PRESS, WheelUp, "C"), bytes(b"\x1b[Mp*%"));
        // 9: no motion in normal mode.
        assert_eq!(ev("1000", d, MOVE, NONE, ""), None);
        assert_eq!(ev("1000", d, MOVE, LEFT, ""), None);
        // 10-12: button-event mode.
        assert_eq!(ev("1002", d, MOVE, LEFT, ""), bytes(b"\x1b[M@*%"));
        assert_eq!(ev("1002", d, MOVE, RIGHT, ""), bytes(b"\x1b[MB*%"));
        assert_eq!(ev("1002", d, MOVE, NONE, ""), None);
        // 13: any-event mode.
        assert_eq!(ev("1003", d, MOVE, NONE, ""), bytes(b"\x1b[MC*%"));
        // 14 (motion inside the same cell) is the caller's job: MouseAction::Move is sent only
        // when the cell changes.
        // 15-16: the default encoding's limit.
        assert_eq!(
            at("1000", d, PRESS, LEFT, "", 223, 1),
            bytes(b"\x1b[M \xff!")
        );
        assert_eq!(at("1000", d, PRESS, LEFT, "", 224, 1), None);
        assert_eq!(at("1000", d, PRESS, LEFT, "", 1, 224), None);
        // 17-19: X10.
        assert_eq!(ev("9", d, PRESS, LEFT, ""), bytes(b"\x1b[M *%"));
        assert_eq!(ev("9", d, PRESS, MIDDLE, ""), bytes(b"\x1b[M!*%"));
        assert_eq!(ev("9", d, PRESS, LEFT, "C"), bytes(b"\x1b[M *%"));
        assert_eq!(ev("9", d, RELEASE, LEFT, ""), None);
        assert_eq!(ev("9", d, PRESS, WheelUp, ""), None);
        assert_eq!(ev("9", d, MOVE, LEFT, ""), None);
        assert_eq!(ev("9", d, MOVE, NONE, ""), None);
        // 20-23: UTF-8.
        assert_eq!(ev("1000", "utf8", PRESS, LEFT, ""), bytes(b"\x1b[M *%"));
        assert_eq!(
            at("1000", "utf8", PRESS, LEFT, "", 100, 5),
            bytes(b"\x1b[M \xc2\x84%")
        );
        assert_eq!(
            at("1000", "utf8", PRESS, LEFT, "", 2015, 5),
            bytes(b"\x1b[M \xdf\xbf%")
        );
        assert_eq!(at("1000", "utf8", PRESS, LEFT, "", 2016, 5), None);
        assert_eq!(at("1000", "utf8", PRESS, LEFT, "", 5, 2016), None);
        // 24-31: SGR.
        assert_eq!(ev("1000", "sgr", PRESS, LEFT, ""), bytes(b"\x1b[<0;10;5M"));
        assert_eq!(
            ev("1000", "sgr", RELEASE, LEFT, ""),
            bytes(b"\x1b[<0;10;5m")
        );
        assert_eq!(
            ev("1000", "sgr", RELEASE, RIGHT, ""),
            bytes(b"\x1b[<2;10;5m")
        );
        assert_eq!(
            ev("1000", "sgr", PRESS, WheelUp, ""),
            bytes(b"\x1b[<64;10;5M")
        );
        assert_eq!(
            ev("1000", "sgr", PRESS, WheelDown, ""),
            bytes(b"\x1b[<65;10;5M")
        );
        assert_eq!(
            ev("1000", "sgr", PRESS, WheelLeft, ""),
            bytes(b"\x1b[<66;10;5M")
        );
        assert_eq!(
            ev("1000", "sgr", PRESS, WheelRight, ""),
            bytes(b"\x1b[<67;10;5M")
        );
        assert_eq!(
            ev("1000", "sgr", PRESS, WheelDown, "C"),
            bytes(b"\x1b[<81;10;5M")
        );
        assert_eq!(ev("1002", "sgr", MOVE, LEFT, ""), bytes(b"\x1b[<32;10;5M"));
        assert_eq!(ev("1003", "sgr", MOVE, NONE, ""), bytes(b"\x1b[<35;10;5M"));
        assert_eq!(
            at("1000", "sgr", PRESS, LEFT, "", 300, 100),
            bytes(b"\x1b[<0;300;100M")
        );
        assert_eq!(
            ev("1000", "sgr", PRESS, LEFT, "SAC"),
            bytes(b"\x1b[<28;10;5M")
        );
        // 32 (urxvt 1015) is not supported: the engine doesn't track it.
    }

    #[test]
    fn releases_and_wheel() {
        let d = "default";
        // Legacy releases carry the modifiers on code 3.
        assert_eq!(ev("1000", d, RELEASE, RIGHT, "C"), bytes(b"\x1b[M3*%"));
        assert_eq!(
            ev("1000", "sgr", RELEASE, MIDDLE, "S"),
            bytes(b"\x1b[<5;10;5m")
        );
        // Wheel "buttons" and no button have no release.
        assert_eq!(ev("1000", d, RELEASE, MouseButton::WheelUp, ""), None);
        assert_eq!(ev("1000", "sgr", RELEASE, MouseButton::WheelDown, ""), None);
        assert_eq!(ev("1000", d, RELEASE, NONE, ""), None);
        assert_eq!(ev("1000", d, PRESS, NONE, ""), None);
        // Motion never carries a wheel direction.
        assert_eq!(ev("1003", d, MOVE, MouseButton::WheelUp, ""), None);
        // Super is not part of the code.
        assert_eq!(ev("1000", "sgr", PRESS, LEFT, "M"), bytes(b"\x1b[<0;10;5M"));
        // Motion with modifiers and a held button in any-event mode.
        assert_eq!(
            ev("1003", "sgr", MOVE, MIDDLE, "A"),
            bytes(b"\x1b[<41;10;5M")
        );
        // The largest UTF-8 row, and the top-left cell.
        assert_eq!(
            at("1002", "utf8", MOVE, LEFT, "", 1, 2015),
            bytes(b"\x1b[M@!\xdf\xbf")
        );
        assert_eq!(
            at("1000", "sgr", PRESS, LEFT, "", 1, 1),
            bytes(b"\x1b[<0;1;1M")
        );
        // SGR has no limit.
        let far = MouseInput {
            action: PRESS,
            button: LEFT,
            column: u16::MAX,
            row: u16::MAX,
            mods: Modifiers::NONE,
        };
        assert_eq!(
            encode_mouse(&far, &modes("1000", "sgr")),
            bytes(b"\x1b[<0;65536;65536M")
        );
        assert_eq!(encode_mouse(&far, &modes("1000", "utf8")), None);
        assert_eq!(encode_mouse(&far, &modes("1000", d)), None);
    }

    #[test]
    fn x10_uses_the_selected_encoding() {
        assert_eq!(ev("9", "sgr", PRESS, RIGHT, "C"), bytes(b"\x1b[<2;10;5M"));
        assert_eq!(
            at("9", "utf8", PRESS, LEFT, "", 100, 5),
            bytes(b"\x1b[M \xc2\x84%")
        );
    }

    #[test]
    fn protocol_selection() {
        assert_eq!(mouse_protocol(&modes("", "default")), None);
        assert_eq!(mouse_protocol(&modes("", "sgr")), None);
        assert_eq!(
            mouse_protocol(&modes("9", "default")),
            Some(MouseProtocol::X10)
        );
        assert_eq!(
            mouse_protocol(&modes("1000", "default")),
            Some(MouseProtocol::Normal)
        );
        assert_eq!(
            mouse_protocol(&modes("1002", "default")),
            Some(MouseProtocol::ButtonEvent)
        );
        assert_eq!(
            mouse_protocol(&modes("1003", "default")),
            Some(MouseProtocol::AnyEvent)
        );
        // X10 set after 1000 wins (the engine keeps 1000).
        let mut both = modes("1000", "default");
        both.x10_mouse = true;
        assert_eq!(mouse_protocol(&both), Some(MouseProtocol::X10));
        // Without a protocol nothing is reported, whatever the encoding.
        assert_eq!(ev("", "sgr", PRESS, LEFT, ""), None);
        assert_eq!(ev("", "default", PRESS, LEFT, ""), None);
    }

    #[test]
    fn shift_bypasses_reporting() {
        for protocol in ["9", "1000", "1002", "1003"] {
            let modes = modes(protocol, "sgr");
            assert!(
                mouse_reporting_active(&modes, Modifiers::NONE),
                "{protocol}"
            );
            assert!(mouse_reporting_active(&modes, mods("CA")), "{protocol}");
            assert!(!mouse_reporting_active(&modes, mods("S")), "{protocol}");
            assert!(!mouse_reporting_active(&modes, mods("SC")), "{protocol}");
        }
        assert!(!mouse_reporting_active(&modes("", "sgr"), Modifiers::NONE));
    }

    fn alt_screen(extra: TermMode) -> InputModes {
        InputModes {
            term: TermMode::default() | TermMode::ALT_SCREEN | extra,
            x10_mouse: false,
        }
    }

    #[test]
    fn alternate_scroll_rules() {
        // One notch (3 lines) up, normal cursor keys.
        assert_eq!(
            alternate_scroll(3, &alt_screen(TermMode::empty())),
            bytes(b"\x1b[A\x1b[A\x1b[A")
        );
        // One notch down with DECCKM.
        assert_eq!(
            alternate_scroll(-3, &alt_screen(TermMode::APP_CURSOR)),
            bytes(b"\x1bOB\x1bOB\x1bOB")
        );
        assert_eq!(alternate_scroll(0, &alt_screen(TermMode::empty())), None);
        // Mouse reporting wins (the caller encodes the wheel as a mouse report instead).
        assert_eq!(
            alternate_scroll(3, &alt_screen(TermMode::MOUSE_REPORT_CLICK)),
            None
        );
        let mut x10 = alt_screen(TermMode::empty());
        x10.x10_mouse = true;
        assert_eq!(alternate_scroll(3, &x10), None);
        // Not on the alternate screen: the wheel scrolls the scrollback.
        assert_eq!(alternate_scroll(3, &InputModes::default()), None);
        // Mode 1007 reset.
        let mut reset = alt_screen(TermMode::empty());
        reset.term.remove(TermMode::ALTERNATE_SCROLL);
        assert_eq!(alternate_scroll(3, &reset), None);
        // Capped.
        let capped = alternate_scroll(i32::MIN, &alt_screen(TermMode::empty())).unwrap();
        assert_eq!(capped.len(), 3 * MAX_ALTERNATE_SCROLL_LINES);
        assert!(capped.chunks(3).all(|key| key == b"\x1b[B"));
        let capped = alternate_scroll(i32::MAX, &alt_screen(TermMode::empty())).unwrap();
        assert_eq!(capped.len(), 3 * MAX_ALTERNATE_SCROLL_LINES);
    }

    #[test]
    fn default_modes_enable_alternate_scroll() {
        // alacritty_terminal enables 1007 by default, so only the alternate screen is missing.
        assert!(
            InputModes::default()
                .term
                .contains(TermMode::ALTERNATE_SCROLL)
        );
    }
}
