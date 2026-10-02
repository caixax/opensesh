//! Keyboard input for RDP (ADR 0034): Qt key events become PC scancodes (set 1, with the
//! extended flag). The server turns them into characters with its own keyboard layout, as
//! Windows' client does, so a key sends what its position means on the remote side.
//!
//! The native scan code is used where the platform gives one: Windows' is the scancode itself
//! (bit 8 is the extended flag), and X11's and Wayland's are XKB keycodes (evdev codes plus 8).
//! Elsewhere, or for a synthesized event, Qt's key is mapped to its position on a US keyboard.

/// Where a key event's native scan code comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// Windows: the scancode, with the extended flag in bit 8.
    Windows,
    /// X11 and Wayland: an XKB keycode (an evdev code plus 8).
    Xkb,
    /// No usable native scan code (macOS, synthesized events).
    Other,
}

/// A key's scancode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    /// The set 1 code.
    pub code: u8,
    /// Whether it has the `E0` prefix.
    pub extended: bool,
}

const fn key(code: u8) -> Option<Key> {
    Some(Key {
        code,
        extended: false,
    })
}

const fn extended(code: u8) -> Option<Key> {
    Some(Key {
        code,
        extended: true,
    })
}

/// The scancode of a key event: from its native scan code when the platform has one, else from
/// Qt's key (`Qt::Key`). `None` for keys without one (the event may still carry text, which can
/// go as a Unicode key).
#[must_use]
pub fn scancode(platform: Platform, native: u32, qt_key: i32) -> Option<Key> {
    let native = match platform {
        Platform::Windows => windows(native),
        Platform::Xkb => native.checked_sub(8).and_then(evdev),
        Platform::Other => None,
    };
    native.or_else(|| from_qt_key(qt_key))
}

fn windows(native: u32) -> Option<Key> {
    let code = u8::try_from(native & 0xFF).ok().filter(|code| *code != 0)?;
    Some(Key {
        code,
        extended: native & 0x100 != 0,
    })
}

/// Linux input event codes (`KEY_*`) to set 1 scancodes.
fn evdev(code: u32) -> Option<Key> {
    match code {
        // KEY_ESC (1) to KEY_KPDOT (83) are the set 1 codes themselves.
        1..=83 => u8::try_from(code).ok().and_then(key),
        86 => key(0x56),       // KEY_102ND
        87 => key(0x57),       // KEY_F11
        88 => key(0x58),       // KEY_F12
        89 => key(0x73),       // KEY_RO
        92 => key(0x79),       // KEY_HENKAN
        93 => key(0x70),       // KEY_KATAKANAHIRAGANA
        94 => key(0x7B),       // KEY_MUHENKAN
        96 => extended(0x1C),  // KEY_KPENTER
        97 => extended(0x1D),  // KEY_RIGHTCTRL
        98 => extended(0x35),  // KEY_KPSLASH
        99 => extended(0x37),  // KEY_SYSRQ (Print Screen)
        100 => extended(0x38), // KEY_RIGHTALT (AltGr)
        102 => extended(0x47), // KEY_HOME
        103 => extended(0x48), // KEY_UP
        104 => extended(0x49), // KEY_PAGEUP
        105 => extended(0x4B), // KEY_LEFT
        106 => extended(0x4D), // KEY_RIGHT
        107 => extended(0x4F), // KEY_END
        108 => extended(0x50), // KEY_DOWN
        109 => extended(0x51), // KEY_PAGEDOWN
        110 => extended(0x52), // KEY_INSERT
        111 => extended(0x53), // KEY_DELETE
        113 => extended(0x20), // KEY_MUTE
        114 => extended(0x2E), // KEY_VOLUMEDOWN
        115 => extended(0x30), // KEY_VOLUMEUP
        117 => key(0x59),      // KEY_KPEQUAL
        121 => key(0x7E),      // KEY_KPCOMMA
        124 => key(0x7D),      // KEY_YEN
        125 => extended(0x5B), // KEY_LEFTMETA
        126 => extended(0x5C), // KEY_RIGHTMETA
        127 => extended(0x5D), // KEY_COMPOSE (the menu key)
        _ => None,
    }
}

/// Qt's key to its position on a US keyboard.
fn from_qt_key(key_value: i32) -> Option<Key> {
    // Printable keys are their (upper case) character.
    if let Ok(byte) = u8::try_from(key_value)
        && byte.is_ascii()
    {
        let row = |keys: &[u8], first: u8, byte: u8| {
            keys.iter()
                .position(|k| *k == byte)
                .and_then(|i| u8::try_from(i).ok())
                .and_then(|i| key(first + i))
        };
        return match byte {
            b'1'..=b'9' => key(0x02 + (byte - b'1')),
            b'0' => key(0x0B),
            b'Q' | b'W' | b'E' | b'R' | b'T' | b'Y' | b'U' | b'I' | b'O' | b'P' => {
                row(b"QWERTYUIOP", 0x10, byte)
            }
            b'A' | b'S' | b'D' | b'F' | b'G' | b'H' | b'J' | b'K' | b'L' => {
                row(b"ASDFGHJKL", 0x1E, byte)
            }
            b'Z' | b'X' | b'C' | b'V' | b'B' | b'N' | b'M' => row(b"ZXCVBNM", 0x2C, byte),
            b' ' => key(0x39),
            b'-' => key(0x0C),
            b'=' => key(0x0D),
            b'[' => key(0x1A),
            b']' => key(0x1B),
            b';' => key(0x27),
            b'\'' => key(0x28),
            b'`' => key(0x29),
            b'\\' => key(0x2B),
            b',' => key(0x33),
            b'.' => key(0x34),
            b'/' => key(0x35),
            _ => None,
        };
    }
    // Qt::Key_* values from 0x01000000.
    match key_value {
        0x0100_0000 => key(0x01),               // Escape
        0x0100_0001 | 0x0100_0002 => key(0x0F), // Tab, Backtab
        0x0100_0003 => key(0x0E),               // Backspace
        0x0100_0004 => key(0x1C),               // Return
        0x0100_0005 => extended(0x1C),          // Enter (keypad)
        0x0100_0006 => extended(0x52),          // Insert
        0x0100_0007 => extended(0x53),          // Delete
        0x0100_0009 => extended(0x37),          // Print
        0x0100_0010 => extended(0x47),          // Home
        0x0100_0011 => extended(0x4F),          // End
        0x0100_0012 => extended(0x4B),          // Left
        0x0100_0013 => extended(0x48),          // Up
        0x0100_0014 => extended(0x4D),          // Right
        0x0100_0015 => extended(0x50),          // Down
        0x0100_0016 => extended(0x49),          // PageUp
        0x0100_0017 => extended(0x51),          // PageDown
        0x0100_0020 => key(0x2A),               // Shift
        0x0100_0021 => key(0x1D),               // Control
        0x0100_0022 => extended(0x5B),          // Meta
        0x0100_0023 => key(0x38),               // Alt
        0x0100_0024 => key(0x3A),               // CapsLock
        0x0100_0025 => key(0x45),               // NumLock
        0x0100_0026 => key(0x46),               // ScrollLock
        0x0100_0030..=0x0100_0039 => {
            // F1 to F10
            u8::try_from(key_value - 0x0100_0030)
                .ok()
                .and_then(|n| key(0x3B + n))
        }
        0x0100_003A => key(0x57),      // F11
        0x0100_003B => key(0x58),      // F12
        0x0100_0055 => extended(0x5D), // Menu
        0x0100_1103 => extended(0x38), // AltGr
        _ => None,
    }
}

/// Left Control, Left Alt and Delete: what "Send Ctrl+Alt+Del" presses, in order.
pub const CTRL_ALT_DEL: [Key; 3] = [
    Key {
        code: 0x1D,
        extended: false,
    },
    Key {
        code: 0x38,
        extended: false,
    },
    Key {
        code: 0x53,
        extended: true,
    },
];

/// The keyboard layout id (a Windows locale id) for a locale name as Qt writes it (`es_ES`,
/// `en-GB`), told to the server at connection time; US English when unknown.
#[must_use]
pub fn layout_id(locale: &str) -> u32 {
    let name = locale.replace('-', "_");
    let (language, territory) = name.split_once('_').unwrap_or((name.as_str(), ""));
    match (language, territory) {
        ("en", "GB") => 0x0809,
        ("en", _) => 0x0409,
        ("es", "MX" | "AR" | "CO" | "CL" | "PE" | "VE") => 0x080A,
        ("es", _) => 0x040A,
        ("ca", _) => 0x0403,
        ("gl", _) => 0x0456,
        ("eu", _) => 0x042D,
        ("pt", "BR") => 0x0416,
        ("pt", _) => 0x0816,
        ("fr", "CA") => 0x0C0C,
        ("fr", "BE") => 0x080C,
        ("fr", "CH") => 0x100C,
        ("fr", _) => 0x040C,
        ("de", "CH") => 0x0807,
        ("de", _) => 0x0407,
        ("it", _) => 0x0410,
        ("nl", "BE") => 0x0813,
        ("nl", _) => 0x0413,
        ("sv", _) => 0x041D,
        ("nb" | "no", _) => 0x0414,
        ("da", _) => 0x0406,
        ("fi", _) => 0x040B,
        ("pl", _) => 0x0415,
        ("cs", _) => 0x0405,
        ("sk", _) => 0x041B,
        ("hu", _) => 0x040E,
        ("ro", _) => 0x0418,
        ("tr", _) => 0x041F,
        ("el", _) => 0x0408,
        ("ru", _) => 0x0419,
        ("uk", _) => 0x0422,
        ("ja", _) => 0x0411,
        ("ko", _) => 0x0412,
        ("zh", "TW" | "HK") => 0x0404,
        ("zh", _) => 0x0804,
        _ => 0x0409,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_scan_codes() {
        // 'A', and Delete with its extended bit.
        assert_eq!(scancode(Platform::Windows, 0x1E, 0x41), key(0x1E));
        assert_eq!(scancode(Platform::Windows, 0x153, 0), extended(0x53));
        // A synthesized event (no scan code) falls back to the key.
        assert_eq!(scancode(Platform::Windows, 0, 0x5A), key(0x2C));
    }

    #[test]
    fn xkb_keycodes() {
        // 'A' is evdev 30, XKB 38; Right Alt (AltGr) is evdev 100; the arrows are extended.
        assert_eq!(scancode(Platform::Xkb, 38, 0), key(0x1E));
        assert_eq!(scancode(Platform::Xkb, 108, 0), extended(0x38));
        assert_eq!(scancode(Platform::Xkb, 111 + 8, 0), extended(0x53));
        assert_eq!(scancode(Platform::Xkb, 9, 0), key(0x01));
        assert_eq!(scancode(Platform::Xkb, 0, 0), None);
    }

    #[test]
    fn qt_keys_on_a_us_keyboard() {
        assert_eq!(scancode(Platform::Other, 0, 0x51), key(0x10)); // Q
        assert_eq!(scancode(Platform::Other, 0, 0x4C), key(0x26)); // L
        assert_eq!(scancode(Platform::Other, 0, 0x4D), key(0x32)); // M
        assert_eq!(scancode(Platform::Other, 0, 0x31), key(0x02)); // 1
        assert_eq!(scancode(Platform::Other, 0, 0x30), key(0x0B)); // 0
        assert_eq!(scancode(Platform::Other, 0, 0x0100_0013), extended(0x48)); // Up
        assert_eq!(scancode(Platform::Other, 0, 0x0100_0039), key(0x44)); // F10
        assert_eq!(scancode(Platform::Other, 0, 0x0100_003B), key(0x58)); // F12
        assert_eq!(scancode(Platform::Other, 0, 0xE9), None); // é: sent as Unicode
    }

    #[test]
    fn layouts() {
        assert_eq!(layout_id("es_ES"), 0x040A);
        assert_eq!(layout_id("en-GB"), 0x0809);
        assert_eq!(layout_id("pt_BR"), 0x0416);
        assert_eq!(layout_id("C"), 0x0409);
    }
}
