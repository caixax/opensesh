//! Keys as VNC sends them: X keysyms (RFC 6143 §7.5.4), from a Qt key (`Qt::Key`) and the text it
//! typed. Characters are their Latin-1 or Unicode keysyms, so the server's own layout doesn't
//! change what lands; the other keys (arrows, function keys, modifiers) have keysyms of their own.
//!
//! A key goes up with the keysym it went down with ([`Pressed`]), whatever the modifiers did
//! meanwhile.

use std::collections::HashMap;

/// `Qt::Key` values (stable across Qt 5 and 6) and their keysyms.
const SPECIAL: &[(i32, u32)] = &[
    (0x0100_0000, 0xff1b), // Escape
    (0x0100_0001, 0xff09), // Tab
    (0x0100_0002, 0xfe20), // Backtab: ISO_Left_Tab
    (0x0100_0003, 0xff08), // Backspace
    (0x0100_0004, 0xff0d), // Return
    (0x0100_0005, 0xff8d), // Enter: KP_Enter
    (0x0100_0006, 0xff63), // Insert
    (0x0100_0007, 0xffff), // Delete
    (0x0100_0008, 0xff13), // Pause
    (0x0100_0009, 0xff61), // Print
    (0x0100_000a, 0xff15), // SysReq
    (0x0100_000b, 0xff0b), // Clear
    (0x0100_0010, 0xff50), // Home
    (0x0100_0011, 0xff57), // End
    (0x0100_0012, 0xff51), // Left
    (0x0100_0013, 0xff52), // Up
    (0x0100_0014, 0xff53), // Right
    (0x0100_0015, 0xff54), // Down
    (0x0100_0016, 0xff55), // PageUp
    (0x0100_0017, 0xff56), // PageDown
    (0x0100_0020, 0xffe1), // Shift: Shift_L
    (0x0100_0021, 0xffe3), // Control: Control_L
    (0x0100_0022, 0xffe7), // Meta: Meta_L
    (0x0100_0023, 0xffe9), // Alt: Alt_L
    (0x0100_0024, 0xffe5), // CapsLock
    (0x0100_0025, 0xff7f), // NumLock
    (0x0100_0026, 0xff14), // ScrollLock
    (0x0100_0053, 0xffeb), // Super_L
    (0x0100_0054, 0xffec), // Super_R
    (0x0100_0055, 0xff67), // Menu
    (0x0100_1103, 0xfe03), // AltGr: ISO_Level3_Shift
];

/// `Qt::Key_F1`; F1 to F35 follow it, as their keysyms follow 0xffbe.
const QT_F1: i32 = 0x0100_0030;
/// `Qt::Key_F35`.
const QT_F35: i32 = 0x0100_0052;

/// A character's keysym: Latin-1 as itself, the rest as a Unicode keysym.
#[must_use]
pub fn of_char(character: char) -> u32 {
    let code = u32::from(character);
    match code {
        0x20..=0x7e | 0xa0..=0xff => code,
        _ => 0x0100_0000 + code,
    }
}

/// The keysym for a Qt key that typed `text`, if it has one.
#[must_use]
pub fn keysym(qt_key: i32, text: &str) -> Option<u32> {
    if let Some((_, keysym)) = SPECIAL.iter().find(|(key, _)| *key == qt_key) {
        return Some(*keysym);
    }
    if (QT_F1..=QT_F35).contains(&qt_key) {
        return u32::try_from(qt_key - QT_F1).ok().map(|n| 0xffbe + n);
    }
    // A key that typed one printable character: that character.
    let mut chars = text.chars();
    if let (Some(character), None) = (chars.next(), chars.next())
        && !character.is_control()
    {
        return Some(of_char(character));
    }
    // With Ctrl the text is a control character: the key itself (letters in lower case, as an
    // unshifted key sends them).
    match u32::try_from(qt_key) {
        Ok(code @ 0x41..=0x5a) => Some(code + 0x20),
        Ok(code @ 0x20..=0x7e) => Some(code),
        Ok(code @ 0xa0..=0xff) => Some(code),
        _ => None,
    }
}

/// The keys held down, by Qt key and native scan code, with the keysym each went down with.
#[derive(Debug, Default)]
pub struct Pressed {
    down: HashMap<(i32, u32), u32>,
}

impl Pressed {
    /// A key went down: its keysym, remembered for its release.
    pub fn press(&mut self, qt_key: i32, native: u32, text: &str) -> Option<u32> {
        let keysym = keysym(qt_key, text)?;
        self.down.insert((qt_key, native), keysym);
        Some(keysym)
    }

    /// A key went up: the keysym it went down with.
    pub fn release(&mut self, qt_key: i32, native: u32, text: &str) -> Option<u32> {
        self.down
            .remove(&(qt_key, native))
            .or_else(|| keysym(qt_key, text))
    }

    /// Every key still down (the focus went away): their keysyms, forgotten.
    pub fn release_all(&mut self) -> Vec<u32> {
        self.down.drain().map(|(_, keysym)| keysym).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_characters() {
        assert_eq!(keysym(0x0100_0000, ""), Some(0xff1b));
        assert_eq!(keysym(0x0100_0012, ""), Some(0xff51));
        assert_eq!(keysym(QT_F1, ""), Some(0xffbe));
        assert_eq!(keysym(QT_F1 + 11, ""), Some(0xffc9));
        assert_eq!(keysym(0x41, "a"), Some(0x61));
        assert_eq!(keysym(0x41, "A"), Some(0x41));
        // Ctrl+C types "\x03": the key, unshifted.
        assert_eq!(keysym(0x43, "\u{3}"), Some(0x63));
        assert_eq!(keysym(0xd1, "ñ"), Some(0xf1));
        assert_eq!(keysym(0x20ac, "€"), Some(0x0100_20ac));
        assert_eq!(keysym(0x0100_ffff, ""), None);
    }

    #[test]
    fn a_key_goes_up_as_it_went_down() {
        let mut pressed = Pressed::default();
        // Shift went up before the key: the release still sends "A".
        assert_eq!(pressed.press(0x41, 30, "A"), Some(0x41));
        assert_eq!(pressed.release(0x41, 30, "a"), Some(0x41));
        pressed.press(0x0100_0021, 29, "");
        assert_eq!(pressed.release_all(), vec![0xffe3]);
        assert!(pressed.release_all().is_empty());
    }
}
