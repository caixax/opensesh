//! Keyboard encoding: the bytes a key press writes to the program.
//!
//! This is the legacy xterm encoding ("PC-style function keys" in xterm's ctlseqs), which is what
//! `TERM=xterm-256color` describes and what alacritty, VTE, kitty (legacy mode) and Windows
//! Terminal send. The kitty keyboard protocol is not implemented (the engine keeps it off).
//!
//! Named keys and Ctrl combinations are encoded from the [`Key`], never from the text the
//! platform produced: Qt's `QKeyEvent::text()` differs per platform for Backspace, Ctrl+2,
//! Ctrl+Space, Ctrl+Alt+letter and more. The text is used only for printable characters (and
//! as a fallback for Ctrl combinations the key alone can't resolve).
//!
//! Summary of the encoding (CSI = `ESC [`, SS3 = `ESC O`, `m` = the modifier parameter
//! `1 + shift + 2 * alt + 4 * ctrl + 8 * meta`):
//!
//! | Key | Sends |
//! |---|---|
//! | text | its UTF-8; Alt (as Meta) prefixes `ESC` when the text is one character |
//! | Ctrl + key | the control byte of the key ([`ctrl_byte`]); other keys send their text |
//! | Enter, keypad Enter | CR, or CR LF in line feed / new line mode (LNM) |
//! | Backspace | DEL (or BS with the option); Ctrl sends the other one |
//! | Tab / Shift+Tab | HT / `CSI Z` |
//! | Escape | ESC |
//! | arrows, Home, End, Begin | `CSI X`, `SS3 X` in application cursor mode, `CSI 1 ; m X` with modifiers |
//! | Insert, Delete, PageUp, PageDown, Menu | `CSI n ~`, `CSI n ; m ~` with modifiers |
//! | F1-F4 | `SS3 P`..`SS3 S`, `CSI 1 ; m P`.. with modifiers |
//! | F5-F24 | `CSI n ~` (15, 17, 18, 19, 20, 21, 23, 24, 25, 26, 28, 29, 31, 32, 33, 34, 42-45) |
//!
//! Alt prefixes `ESC` to the C0 keys (Enter, Backspace, Tab, Escape) and to text; on the keys
//! that take a modifier parameter it is part of the parameter instead.
//!
//! Sources, verified for the Sprint 2 research: xterm patch #411 (`ctlseqs`, `input.c`),
//! alacritty 0.17.0, VTE, Windows Terminal, kitty's legacy tables, and the `xterm-256color`
//! terminfo of Debian 13, Arch, Fedora 43 and Ubuntu 22.04.

use alacritty_terminal::term::TermMode;

use crate::input::{InputModes, Modifiers};

/// The Qt 6 `Qt::Key` and `Qt::KeyboardModifier` values this module maps, from `qnamespace.h`
/// (checked against Qt 6.10.3; they have been stable since Qt 5). The crate stays Qt-free: the
/// GUI passes `QKeyEvent::key()` and `QKeyEvent::modifiers()` as plain integers.
pub mod qt {
    /// `Qt::Key_Space`. Keys below [`KEY_ESCAPE`] are Unicode code points (upper case for
    /// letters: `Qt::Key_A` is `0x41` for both `a` and `A`).
    pub const KEY_SPACE: i32 = 0x20;
    /// `Qt::Key_Asterisk`.
    pub const KEY_ASTERISK: i32 = 0x2a;
    /// `Qt::Key_Plus`.
    pub const KEY_PLUS: i32 = 0x2b;
    /// `Qt::Key_Comma`.
    pub const KEY_COMMA: i32 = 0x2c;
    /// `Qt::Key_Minus`.
    pub const KEY_MINUS: i32 = 0x2d;
    /// `Qt::Key_Period`.
    pub const KEY_PERIOD: i32 = 0x2e;
    /// `Qt::Key_Slash`.
    pub const KEY_SLASH: i32 = 0x2f;
    /// `Qt::Key_0` (`Qt::Key_1`..`Qt::Key_9` follow).
    pub const KEY_0: i32 = 0x30;
    /// `Qt::Key_9`.
    pub const KEY_9: i32 = 0x39;
    /// `Qt::Key_Equal`.
    pub const KEY_EQUAL: i32 = 0x3d;
    /// `Qt::Key_Escape`, the first of the non-character keys.
    pub const KEY_ESCAPE: i32 = 0x0100_0000;
    /// `Qt::Key_Tab`.
    pub const KEY_TAB: i32 = 0x0100_0001;
    /// `Qt::Key_Backtab` (Shift+Tab on every platform).
    pub const KEY_BACKTAB: i32 = 0x0100_0002;
    /// `Qt::Key_Backspace`.
    pub const KEY_BACKSPACE: i32 = 0x0100_0003;
    /// `Qt::Key_Return` (the main Enter key).
    pub const KEY_RETURN: i32 = 0x0100_0004;
    /// `Qt::Key_Enter` (the keypad Enter key).
    pub const KEY_ENTER: i32 = 0x0100_0005;
    /// `Qt::Key_Insert`.
    pub const KEY_INSERT: i32 = 0x0100_0006;
    /// `Qt::Key_Delete`.
    pub const KEY_DELETE: i32 = 0x0100_0007;
    /// `Qt::Key_Clear` (keypad 5 with NumLock off).
    pub const KEY_CLEAR: i32 = 0x0100_000b;
    /// `Qt::Key_Home`.
    pub const KEY_HOME: i32 = 0x0100_0010;
    /// `Qt::Key_End`.
    pub const KEY_END: i32 = 0x0100_0011;
    /// `Qt::Key_Left`.
    pub const KEY_LEFT: i32 = 0x0100_0012;
    /// `Qt::Key_Up`.
    pub const KEY_UP: i32 = 0x0100_0013;
    /// `Qt::Key_Right`.
    pub const KEY_RIGHT: i32 = 0x0100_0014;
    /// `Qt::Key_Down`.
    pub const KEY_DOWN: i32 = 0x0100_0015;
    /// `Qt::Key_PageUp`.
    pub const KEY_PAGE_UP: i32 = 0x0100_0016;
    /// `Qt::Key_PageDown`.
    pub const KEY_PAGE_DOWN: i32 = 0x0100_0017;
    /// `Qt::Key_Shift`, the first modifier key (`Control`, `Meta`, `Alt`, `CapsLock`,
    /// `NumLock` follow).
    pub const KEY_SHIFT: i32 = 0x0100_0020;
    /// `Qt::Key_ScrollLock`, the last modifier key of that block.
    pub const KEY_SCROLL_LOCK: i32 = 0x0100_0026;
    /// `Qt::Key_F1` (`Qt::Key_F2`..`Qt::Key_F35` follow).
    pub const KEY_F1: i32 = 0x0100_0030;
    /// `Qt::Key_F24`, the last function key the terminal encodes.
    pub const KEY_F24: i32 = 0x0100_0047;
    /// `Qt::Key_F35`, the last function key Qt defines.
    pub const KEY_F35: i32 = 0x0100_0052;
    /// `Qt::Key_Super_L`.
    pub const KEY_SUPER_L: i32 = 0x0100_0053;
    /// `Qt::Key_Super_R`.
    pub const KEY_SUPER_R: i32 = 0x0100_0054;
    /// `Qt::Key_Menu`.
    pub const KEY_MENU: i32 = 0x0100_0055;
    /// `Qt::Key_Hyper_L`.
    pub const KEY_HYPER_L: i32 = 0x0100_0056;
    /// `Qt::Key_Hyper_R`.
    pub const KEY_HYPER_R: i32 = 0x0100_0057;
    /// `Qt::Key_AltGr`.
    pub const KEY_ALT_GR: i32 = 0x0100_1103;
    /// `Qt::Key_Multi_key` (Compose).
    pub const KEY_MULTI_KEY: i32 = 0x0100_1120;
    /// `Qt::Key_Mode_switch`.
    pub const KEY_MODE_SWITCH: i32 = 0x0100_117e;
    /// `Qt::Key_Dead_Grave`, the first dead key.
    pub const KEY_DEAD_FIRST: i32 = 0x0100_1250;
    /// `Qt::Key_Dead_Longsolidusoverlay`, the last dead key.
    pub const KEY_DEAD_LAST: i32 = 0x0100_1293;

    /// `Qt::ShiftModifier`.
    pub const SHIFT_MODIFIER: u32 = 0x0200_0000;
    /// `Qt::ControlModifier`.
    pub const CONTROL_MODIFIER: u32 = 0x0400_0000;
    /// `Qt::AltModifier`.
    pub const ALT_MODIFIER: u32 = 0x0800_0000;
    /// `Qt::MetaModifier` (Super / Windows key on Linux and Windows).
    pub const META_MODIFIER: u32 = 0x1000_0000;
    /// `Qt::KeypadModifier`: the key is on the numeric keypad.
    pub const KEYPAD_MODIFIER: u32 = 0x2000_0000;
    /// `Qt::GroupSwitchModifier`: AltGr on Windows with the `windows:altgr` platform option.
    pub const GROUP_SWITCH_MODIFIER: u32 = 0x4000_0000;
}

/// A key on the numeric keypad that produced a character (NumLock on).
///
/// With NumLock off the keypad keys arrive as the navigation keys they stand for
/// ([`Key::Home`], [`Key::Up`], [`Key::Begin`], ...) and are encoded as those.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeypadKey {
    /// A digit, `0..=9`.
    Digit(u8),
    /// `*`.
    Multiply,
    /// `+`.
    Add,
    /// `,` (the separator key).
    Separator,
    /// `-`.
    Subtract,
    /// `.` (the decimal key; the platform text may be the locale's separator instead).
    Decimal,
    /// `/`.
    Divide,
    /// `=`.
    Equal,
}

impl KeypadKey {
    /// The character the key stands for, used when the platform gave no text.
    #[must_use]
    pub fn as_char(self) -> char {
        match self {
            Self::Digit(digit) => char::from(b'0' + digit.min(9)),
            Self::Multiply => '*',
            Self::Add => '+',
            Self::Separator => ',',
            Self::Subtract => '-',
            Self::Decimal => '.',
            Self::Divide => '/',
            Self::Equal => '=',
        }
    }

    /// The final byte of the VT220 application keypad sequence `SS3 x`.
    fn vt220_final(self) -> u8 {
        match self {
            Self::Digit(digit) => b'p' + digit.min(9),
            Self::Multiply => b'j',
            Self::Add => b'k',
            Self::Separator => b'l',
            Self::Subtract => b'm',
            Self::Decimal => b'n',
            Self::Divide => b'o',
            Self::Equal => b'X',
        }
    }
}

/// A key, identified by what it is rather than by the text it produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    /// The main Enter (Return) key.
    Enter,
    /// The keypad Enter key.
    KeypadEnter,
    /// Tab.
    Tab,
    /// Shift+Tab as Qt reports it (`Qt::Key_Backtab`).
    Backtab,
    /// Backspace.
    Backspace,
    /// Escape.
    Escape,
    /// Cursor up.
    Up,
    /// Cursor down.
    Down,
    /// Cursor left.
    Left,
    /// Cursor right.
    Right,
    /// Home.
    Home,
    /// End.
    End,
    /// Begin: keypad 5 with NumLock off (`Qt::Key_Clear`).
    Begin,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// Insert.
    Insert,
    /// Delete (forward delete).
    Delete,
    /// The Menu (application) key.
    Menu,
    /// A function key, `F(1)` to `F(24)`.
    F(u8),
    /// A keypad key that produced a character (NumLock on).
    Keypad(KeypadKey),
    /// Any other key: letters, digits, punctuation, Space, and keys the terminal has no
    /// sequence for. Holds the key's character when Qt reports one (upper case for letters, as
    /// in `Qt::Key_A`), which resolves Ctrl combinations; the text itself comes from
    /// [`KeyInput::text`].
    Character(Option<char>),
    /// A key that never sends anything by itself: modifiers (Shift, Ctrl, Alt, Super, AltGr,
    /// the lock keys), dead keys and Compose.
    Ignored,
}

/// The escape codes of F5 to F24 (xterm `decfuncvalue`; F21 and up are xterm's sequential
/// extension, also used by VTE).
const FUNCTION_KEY_CODES: [u8; 20] = [
    15, 17, 18, 19, 20, 21, 23, 24, 25, 26, 28, 29, 31, 32, 33, 34, 42, 43, 44, 45,
];

impl Key {
    /// Maps a `Qt::Key` value (`QKeyEvent::key()`) to a key. `keypad` is whether
    /// `Qt::KeypadModifier` was set; it only matters for digits, operators and Enter (keypad
    /// navigation keys with NumLock off encode as the navigation keys).
    ///
    /// The constants relied on are in [`qt`].
    #[must_use]
    pub fn from_qt(qt_key: i32, keypad: bool) -> Key {
        match qt_key {
            qt::KEY_ESCAPE => Key::Escape,
            qt::KEY_TAB => Key::Tab,
            qt::KEY_BACKTAB => Key::Backtab,
            qt::KEY_BACKSPACE => Key::Backspace,
            qt::KEY_RETURN => Key::Enter,
            qt::KEY_ENTER => Key::KeypadEnter,
            qt::KEY_INSERT => Key::Insert,
            qt::KEY_DELETE => Key::Delete,
            qt::KEY_CLEAR => Key::Begin,
            qt::KEY_HOME => Key::Home,
            qt::KEY_END => Key::End,
            qt::KEY_LEFT => Key::Left,
            qt::KEY_UP => Key::Up,
            qt::KEY_RIGHT => Key::Right,
            qt::KEY_DOWN => Key::Down,
            qt::KEY_PAGE_UP => Key::PageUp,
            qt::KEY_PAGE_DOWN => Key::PageDown,
            qt::KEY_MENU => Key::Menu,
            qt::KEY_F1..=qt::KEY_F24 => {
                // The range holds 24 values, so the offset fits in a u8.
                Key::F(u8::try_from(qt_key - qt::KEY_F1 + 1).unwrap_or(0))
            }
            qt::KEY_SHIFT..=qt::KEY_SCROLL_LOCK
            | qt::KEY_SUPER_L
            | qt::KEY_SUPER_R
            | qt::KEY_HYPER_L
            | qt::KEY_HYPER_R
            | qt::KEY_ALT_GR
            | qt::KEY_MULTI_KEY
            | qt::KEY_MODE_SWITCH
            | qt::KEY_DEAD_FIRST..=qt::KEY_DEAD_LAST => Key::Ignored,
            _ if keypad => match keypad_key(qt_key) {
                Some(key) => Key::Keypad(key),
                None => Key::Character(key_char(qt_key)),
            },
            _ => Key::Character(key_char(qt_key)),
        }
    }
}

/// The keypad key for a Qt key code that carried `Qt::KeypadModifier`.
fn keypad_key(qt_key: i32) -> Option<KeypadKey> {
    Some(match qt_key {
        qt::KEY_0..=qt::KEY_9 => KeypadKey::Digit(u8::try_from(qt_key - qt::KEY_0).ok()?),
        qt::KEY_ASTERISK => KeypadKey::Multiply,
        qt::KEY_PLUS => KeypadKey::Add,
        qt::KEY_COMMA => KeypadKey::Separator,
        qt::KEY_MINUS => KeypadKey::Subtract,
        qt::KEY_PERIOD => KeypadKey::Decimal,
        qt::KEY_SLASH => KeypadKey::Divide,
        qt::KEY_EQUAL => KeypadKey::Equal,
        _ => return None,
    })
}

/// The character of a Qt key code below `Qt::Key_Escape` (Qt uses code points there).
fn key_char(qt_key: i32) -> Option<char> {
    if (qt::KEY_SPACE..qt::KEY_ESCAPE).contains(&qt_key) {
        u32::try_from(qt_key).ok().and_then(char::from_u32)
    } else {
        None
    }
}

/// Converts `Qt::KeyboardModifiers` (as an integer) to [`Modifiers`].
///
/// With `Qt::GroupSwitchModifier` (AltGr on Windows with the `windows:altgr` platform option)
/// Ctrl and Alt are dropped: the key produced an AltGr character, which is sent as text.
#[must_use]
pub fn modifiers_from_qt(qt_modifiers: u32) -> Modifiers {
    let has = |bit: u32| qt_modifiers & bit != 0;
    let alt_gr = has(qt::GROUP_SWITCH_MODIFIER);
    Modifiers {
        shift: has(qt::SHIFT_MODIFIER),
        ctrl: has(qt::CONTROL_MODIFIER) && !alt_gr,
        alt: has(qt::ALT_MODIFIER) && !alt_gr,
        meta: has(qt::META_MODIFIER),
    }
}

/// One key press, as the terminal item received it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInput {
    /// The key.
    pub key: Key,
    /// The modifiers held.
    pub mods: Modifiers,
    /// What the platform produced (`QKeyEvent::text()`): used for printable characters only.
    pub text: String,
}

impl KeyInput {
    /// Builds the input from a `QKeyEvent`: `key()`, `modifiers()` as an integer and `text()`.
    ///
    /// Besides [`Key::from_qt`] and [`modifiers_from_qt`], on Windows it works around two Qt
    /// behaviours (read in Qt 6.10.3 `qwindowskeymapper.cpp`):
    ///
    /// - Without the `windows:altgr` platform option, AltGr arrives as Ctrl+Alt with the AltGr
    ///   character as the key and the text. When Ctrl and Alt are both held and the text is one
    ///   printable character that is not an ASCII letter or digit, it is treated as AltGr text
    ///   (the Windows Terminal heuristic). A real Ctrl+Alt+letter gives an upper-case ASCII
    ///   letter as text there, so it still encodes as `ESC` + the control byte.
    /// - Qt lower-cases letters typed with Alt (`WM_SYSCHAR`), so Alt+Shift+A would send
    ///   `ESC a`. The case of a letter typed with Alt follows Shift instead (Caps Lock is not
    ///   visible here).
    #[must_use]
    pub fn from_qt(qt_key: i32, qt_modifiers: u32, text: &str) -> Self {
        translate_qt(qt_key, qt_modifiers, text, cfg!(windows))
    }
}

/// Whether a Qt key event is one digit of a Windows Alt code (Alt held, a numeric keypad
/// digit: Alt+0233 types "é"). Windows sends the composed character by itself when Alt is
/// released, so the digits must neither reach the program (as Alt+digit, which shells bind to
/// digit arguments) nor trigger the app's Alt+1..9 shortcuts.
#[must_use]
pub fn is_windows_alt_code(qt_key: i32, qt_modifiers: u32) -> bool {
    alt_code_digit(qt_key, qt_modifiers, cfg!(windows))
}

fn alt_code_digit(qt_key: i32, qt_modifiers: u32, windows: bool) -> bool {
    windows
        && (qt::KEY_0..=qt::KEY_9).contains(&qt_key)
        && qt_modifiers & qt::KEYPAD_MODIFIER != 0
        && qt_modifiers & qt::ALT_MODIFIER != 0
        && qt_modifiers & (qt::CONTROL_MODIFIER | qt::GROUP_SWITCH_MODIFIER) == 0
}

/// [`KeyInput::from_qt`] with the platform as a parameter, so both paths are unit-tested.
fn translate_qt(qt_key: i32, qt_modifiers: u32, text: &str, windows: bool) -> KeyInput {
    if alt_code_digit(qt_key, qt_modifiers, windows) {
        return KeyInput {
            key: Key::Ignored,
            mods: modifiers_from_qt(qt_modifiers),
            text: String::new(),
        };
    }
    let key = Key::from_qt(qt_key, qt_modifiers & qt::KEYPAD_MODIFIER != 0);
    let mut mods = modifiers_from_qt(qt_modifiers);
    let mut text = text.to_owned();
    if windows {
        let character_key = matches!(key, Key::Character(_) | Key::Keypad(_));
        if character_key && mods.ctrl && mods.alt && is_alt_gr_text(&text) {
            mods.ctrl = false;
            mods.alt = false;
        }
        if mods.alt && !mods.ctrl {
            if let (Key::Character(Some(letter)), Some(typed)) = (key, single_char(&text)) {
                if letter.is_ascii_alphabetic() && typed.eq_ignore_ascii_case(&letter) {
                    text = if mods.shift {
                        typed.to_ascii_uppercase()
                    } else {
                        typed.to_ascii_lowercase()
                    }
                    .to_string();
                }
            }
        }
    }
    KeyInput { key, mods, text }
}

/// Whether `text` looks like a character typed with AltGr: exactly one printable character
/// that is not an ASCII letter or digit.
fn is_alt_gr_text(text: &str) -> bool {
    single_char(text).is_some_and(|c| !c.is_control() && c != ' ' && !c.is_ascii_alphanumeric())
}

/// The only character of `text`, if it has exactly one.
fn single_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => Some(c),
        _ => None,
    }
}

/// User options that change the encoding (PLAN §6.2; settings UI in Sprint 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyOptions {
    /// Backspace sends BS (`^H`, 0x08) instead of DEL (`^?`, 0x7f). Ctrl+Backspace sends the
    /// other one. Default: false (terminfo `kbs=^?`).
    pub backspace_sends_ctrl_h: bool,
    /// Alt acts as Meta: it prefixes `ESC` to text and to the C0 keys. Default: true.
    pub alt_sends_escape: bool,
    /// Delete without modifiers sends DEL (`^?`, 0x7f) instead of the VT220 `ESC [ 3 ~`.
    /// Default: false.
    pub delete_sends_del: bool,
    /// In application keypad mode (DECKPAM), keypad keys send the VT220 sequences (`SS3 p`..
    /// `SS3 y`, `SS3 j`..`SS3 o`, `SS3 X`, `SS3 M`) instead of their characters. Default: false,
    /// like alacritty, kitty and Windows Terminal (curses programs enable DECKPAM at start, and
    /// digit entry must keep working).
    pub vt220_keypad: bool,
}

impl Default for KeyOptions {
    fn default() -> Self {
        Self {
            backspace_sends_ctrl_h: false,
            alt_sends_escape: true,
            delete_sends_del: false,
            vt220_keypad: false,
        }
    }
}

/// The control byte Ctrl+`c` sends, for the key's character `c` (upper or lower case letters,
/// `@`, Space, `` ` ``, `2`..`8`, `[ \ ] ^ _ / ?` and `{ | } ~`), or `None` when Ctrl doesn't
/// change the key (digits `0`, `1`, `9` and other punctuation send themselves).
#[must_use]
pub fn ctrl_byte(c: char) -> Option<u8> {
    Some(match c {
        'a'..='z' | 'A'..='Z' => u8::try_from(c).ok()? & 0x1f,
        '@' | ' ' | '2' | '`' => 0x00,
        '[' | '3' | '{' => 0x1b,
        '\\' | '4' | '|' => 0x1c,
        ']' | '5' | '}' => 0x1d,
        '^' | '6' | '~' => 0x1e,
        '_' | '7' | '/' => 0x1f,
        '8' | '?' => 0x7f,
        _ => return None,
    })
}

/// Encodes a key press. Returns `None` when the terminal has nothing to send for it (a modifier
/// or dead key alone, a key without text or sequence), so the caller can let the event go.
///
/// The caller decides first which keys are app shortcuts (see [`terminal_wants_shortcut`]),
/// and handles app-level bindings such as Shift+PageUp scrolling the scrollback.
#[must_use]
pub fn encode_key(input: &KeyInput, modes: &InputModes, options: &KeyOptions) -> Option<Vec<u8>> {
    let mods = input.mods;
    let term = modes.term;
    let esc = mods.alt && options.alt_sends_escape;
    Some(match input.key {
        Key::Ignored => return None,
        Key::Enter => with_esc(esc, newline(term)),
        Key::KeypadEnter if vt220_keypad(term, options) => ss3(b'M'),
        Key::KeypadEnter => with_esc(esc, newline(term)),
        Key::Tab if mods.shift => with_esc(esc, b"\x1b[Z"),
        Key::Tab => with_esc(esc, b"\t"),
        Key::Backtab => with_esc(esc, b"\x1b[Z"),
        Key::Backspace => {
            let bs = options.backspace_sends_ctrl_h != mods.ctrl;
            with_esc(esc, if bs { b"\x08" } else { b"\x7f" })
        }
        Key::Escape => with_esc(esc, b"\x1b"),
        Key::Up => cursor_key(b'A', mods, term),
        Key::Down => cursor_key(b'B', mods, term),
        Key::Right => cursor_key(b'C', mods, term),
        Key::Left => cursor_key(b'D', mods, term),
        Key::Home => cursor_key(b'H', mods, term),
        Key::End => cursor_key(b'F', mods, term),
        Key::Begin => cursor_key(b'E', mods, term),
        Key::Insert => tilde_key(2, mods),
        Key::Delete if options.delete_sends_del && mods == Modifiers::NONE => b"\x7f".to_vec(),
        Key::Delete => tilde_key(3, mods),
        Key::PageUp => tilde_key(5, mods),
        Key::PageDown => tilde_key(6, mods),
        Key::Menu => tilde_key(29, mods),
        Key::F(number) => function_key(number, mods)?,
        Key::Keypad(key) if vt220_keypad(term, options) => ss3(key.vt220_final()),
        Key::Keypad(key) => {
            let c = key.as_char();
            let mut buffer = [0; 4];
            let text: &str = if input.text.is_empty() {
                c.encode_utf8(&mut buffer)
            } else {
                input.text.as_str()
            };
            character(Some(c), text, mods, options)?
        }
        Key::Character(c) => character(c, &input.text, mods, options)?,
    })
}

/// Whether the terminal takes a key that is also an app shortcut (it accepts Qt's
/// `ShortcutOverride` event for it), per ADR 0011: function keys F1-F24 without Ctrl or Alt,
/// except F11 (full screen). Everything else stays with the app shortcuts; Ctrl+F6 and
/// Ctrl+Shift+F6 are the keyboard way out of the terminal.
#[must_use]
pub fn terminal_wants_shortcut(key: &Key, mods: Modifiers) -> bool {
    matches!(*key, Key::F(number) if (1..=24).contains(&number) && number != 11)
        && !mods.ctrl
        && !mods.alt
}

/// Encodes a character key: the Ctrl table, the platform text, and the Alt (Meta) prefix.
fn character(
    key: Option<char>,
    text: &str,
    mods: Modifiers,
    options: &KeyOptions,
) -> Option<Vec<u8>> {
    let esc = mods.alt && options.alt_sends_escape;
    if mods.ctrl {
        if let Some(byte) = key.and_then(ctrl_byte).or_else(|| lone_control(text)) {
            return Some(with_esc(esc, &[byte]));
        }
    }
    let mut buffer = [0; 4];
    let text = match key {
        // Ctrl+1 and similar: the platform may produce no text, but the key sends itself.
        Some(c) if text.is_empty() && mods.ctrl && c.is_ascii_graphic() => {
            &*c.encode_utf8(&mut buffer)
        }
        _ if text.is_empty() => return None,
        _ => text,
    };
    Some(with_esc(
        esc && single_char(text).is_some(),
        text.as_bytes(),
    ))
}

/// The byte of `text` when it is a single C0 control or DEL (what xkb and Windows produce for
/// Ctrl combinations on layouts the key table doesn't cover).
fn lone_control(text: &str) -> Option<u8> {
    single_char(text)
        .filter(|&c| c < ' ' || c == '\x7f')
        .and_then(|c| u8::try_from(c).ok())
}

/// Whether keypad keys use the VT220 application sequences.
fn vt220_keypad(term: TermMode, options: &KeyOptions) -> bool {
    options.vt220_keypad && term.contains(TermMode::APP_KEYPAD)
}

/// What Enter sends: CR, or CR LF in line feed / new line mode.
fn newline(term: TermMode) -> &'static [u8] {
    if term.contains(TermMode::LINE_FEED_NEW_LINE) {
        b"\r\n"
    } else {
        b"\r"
    }
}

/// The xterm modifier parameter: `1 + shift + 2 * alt + 4 * ctrl + 8 * meta`.
fn modifier_parameter(mods: Modifiers) -> u8 {
    1 + u8::from(mods.shift)
        + 2 * u8::from(mods.alt)
        + 4 * u8::from(mods.ctrl)
        + 8 * u8::from(mods.meta)
}

/// `bytes`, prefixed with `ESC` when `esc` is set.
fn with_esc(esc: bool, bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 1);
    if esc {
        out.push(0x1b);
    }
    out.extend_from_slice(bytes);
    out
}

/// `SS3 final`.
fn ss3(final_byte: u8) -> Vec<u8> {
    vec![0x1b, b'O', final_byte]
}

/// `CSI 1 ; m final`.
fn csi_modified(final_byte: u8, parameter: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(8);
    out.extend_from_slice(b"\x1b[1;");
    push_decimal(&mut out, u32::from(parameter));
    out.push(final_byte);
    out
}

/// A cursor key (arrows, Home, End, Begin): `CSI X`, `SS3 X` with DECCKM, `CSI 1 ; m X` with
/// modifiers in both modes.
fn cursor_key(final_byte: u8, mods: Modifiers, term: TermMode) -> Vec<u8> {
    let parameter = modifier_parameter(mods);
    if parameter > 1 {
        csi_modified(final_byte, parameter)
    } else if term.contains(TermMode::APP_CURSOR) {
        ss3(final_byte)
    } else {
        vec![0x1b, b'[', final_byte]
    }
}

/// An editing or function key of the form `CSI n ~` / `CSI n ; m ~`.
fn tilde_key(code: u8, mods: Modifiers) -> Vec<u8> {
    let parameter = modifier_parameter(mods);
    let mut out = Vec::with_capacity(8);
    out.extend_from_slice(b"\x1b[");
    push_decimal(&mut out, u32::from(code));
    if parameter > 1 {
        out.push(b';');
        push_decimal(&mut out, u32::from(parameter));
    }
    out.push(b'~');
    out
}

/// F1-F24; `None` for other numbers.
fn function_key(number: u8, mods: Modifiers) -> Option<Vec<u8>> {
    match number {
        1..=4 => {
            let final_byte = b'P' + (number - 1);
            let parameter = modifier_parameter(mods);
            Some(if parameter > 1 {
                csi_modified(final_byte, parameter)
            } else {
                ss3(final_byte)
            })
        }
        5..=24 => {
            let code = FUNCTION_KEY_CODES.get(usize::from(number - 5))?;
            Some(tilde_key(*code, mods))
        }
        _ => None,
    }
}

/// Appends `value` in decimal ASCII.
pub(crate) fn push_decimal(out: &mut Vec<u8>, value: u32) {
    let mut digits = [0u8; 10];
    let mut len = 0;
    let mut rest = value;
    loop {
        // `rest % 10` is below 10, so the cast can't truncate.
        digits[len] = b'0' + (rest % 10) as u8;
        len += 1;
        rest /= 10;
        if rest == 0 {
            break;
        }
    }
    out.extend(digits[..len].iter().rev());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses a modifier list such as `"C+S"` (S = Shift, A = Alt, C = Ctrl, M = Super/Meta).
    fn mods(spec: &str) -> Modifiers {
        let mut mods = Modifiers::NONE;
        for part in spec.split('+').filter(|part| !part.is_empty()) {
            match part {
                "S" => mods.shift = true,
                "A" => mods.alt = true,
                "C" => mods.ctrl = true,
                "M" => mods.meta = true,
                other => panic!("unknown modifier {other}"),
            }
        }
        mods
    }

    /// Parses a mode list such as `"DECCKM+LNM"` into engine modes and options.
    fn modes(spec: &str) -> (InputModes, KeyOptions) {
        let mut modes = InputModes::default();
        let mut options = KeyOptions::default();
        for part in spec.split('+').filter(|part| !part.is_empty()) {
            match part {
                "DECCKM" => modes.term.insert(TermMode::APP_CURSOR),
                "DECKPAM" => modes.term.insert(TermMode::APP_KEYPAD),
                "LNM" => modes.term.insert(TermMode::LINE_FEED_NEW_LINE),
                "BS" => options.backspace_sends_ctrl_h = true,
                "DEL" => options.delete_sends_del = true,
                "NOMETA" => options.alt_sends_escape = false,
                "VT220" => options.vt220_keypad = true,
                other => panic!("unknown mode {other}"),
            }
        }
        (modes, options)
    }

    fn encode(key: Key, mod_spec: &str, text: &str, mode_spec: &str) -> Option<Vec<u8>> {
        let (modes, options) = modes(mode_spec);
        let input = KeyInput {
            key,
            mods: mods(mod_spec),
            text: text.to_owned(),
        };
        encode_key(&input, &modes, &options)
    }

    const fn ch(c: char) -> Key {
        Key::Character(Some(c))
    }

    const fn kp(key: KeypadKey) -> Key {
        Key::Keypad(key)
    }

    /// One row of the keyboard table in the Sprint 2 input research (s2_input.md §3.9), plus
    /// the platform text Qt gives for it where that matters.
    struct Row {
        row: u32,
        key: Key,
        mods: &'static str,
        text: &'static str,
        modes: &'static str,
        expected: Option<&'static [u8]>,
    }

    const fn row(
        row: u32,
        key: Key,
        mods: &'static str,
        text: &'static str,
        modes: &'static str,
        expected: &'static [u8],
    ) -> Row {
        Row {
            row,
            key,
            mods,
            text,
            modes,
            expected: Some(expected),
        }
    }

    const fn nothing(row: u32, key: Key, mods: &'static str, text: &'static str) -> Row {
        Row {
            row,
            key,
            mods,
            text,
            modes: "",
            expected: None,
        }
    }

    #[rustfmt::skip]
    const TABLE: &[Row] = &[
        // Printable text.
        row(1, ch('A'), "", "a", "", b"a"),
        row(2, ch('A'), "S", "A", "", b"A"),
        row(3, ch('\u{c9}'), "", "\u{e9}", "", b"\xc3\xa9"),
        row(4, ch('E'), "", "\u{20ac}", "", b"\xe2\x82\xac"),
        // Alt as Meta.
        row(5, ch('A'), "A", "a", "", b"\x1ba"),
        row(6, ch('A'), "S+A", "A", "", b"\x1bA"),
        row(7, ch('A'), "A", "a", "NOMETA", b"a"),
        row(8, ch('\u{c9}'), "A", "\u{e9}", "", b"\x1b\xc3\xa9"),
        // Ctrl table (xkb text, then Windows text where it differs).
        row(9, ch('A'), "C", "\x01", "", b"\x01"),
        row(9, ch('A'), "C", "a", "", b"\x01"),
        row(10, ch('A'), "C+S", "\x01", "", b"\x01"),
        row(11, ch('Z'), "C", "\x1a", "", b"\x1a"),
        row(12, ch('A'), "C+A", "\x01", "", b"\x1b\x01"),
        row(12, ch('A'), "C+A", "A", "", b"\x1b\x01"),
        row(13, ch(' '), "C", "\0", "", b"\x00"),
        row(13, ch(' '), "C", " ", "", b"\x00"),
        row(14, ch(' '), "C+A", "\0", "", b"\x1b\x00"),
        row(15, ch('@'), "C", "\0", "", b"\x00"),
        row(15, ch('@'), "C+S", "2", "", b"\x00"),
        row(16, ch('2'), "C", "\0", "", b"\x00"),
        row(16, ch('2'), "C", "2", "", b"\x00"),
        row(17, ch('['), "C", "\x1b", "", b"\x1b"),
        row(17, ch('3'), "C", "\x1b", "", b"\x1b"),
        row(17, ch('3'), "C", "3", "", b"\x1b"),
        row(18, ch('\\'), "C", "\x1c", "", b"\x1c"),
        row(18, ch('4'), "C", "4", "", b"\x1c"),
        row(19, ch(']'), "C", "\x1d", "", b"\x1d"),
        row(19, ch('5'), "C", "5", "", b"\x1d"),
        row(20, ch('^'), "C", "\x1e", "", b"\x1e"),
        row(20, ch('6'), "C", "6", "", b"\x1e"),
        row(20, ch('~'), "C", "\x1e", "", b"\x1e"),
        row(21, ch('_'), "C", "\x1f", "", b"\x1f"),
        row(21, ch('7'), "C", "7", "", b"\x1f"),
        row(21, ch('/'), "C", "\x1f", "", b"\x1f"),
        row(22, ch('8'), "C", "\x7f", "", b"\x7f"),
        row(22, ch('?'), "C", "?", "", b"\x7f"),
        row(23, ch('`'), "C", "\0", "", b"\x00"),
        row(24, ch('1'), "C", "1", "", b"1"),
        row(24, ch('1'), "C", "", "", b"1"),
        row(24, ch('0'), "C", "0", "", b"0"),
        row(24, ch('9'), "C", "9", "", b"9"),
        row(24, ch(';'), "C", ";", "", b";"),
        row(24, ch('.'), "C", ".", "", b"."),
        // Space.
        row(25, ch(' '), "", " ", "", b" "),
        row(25, ch(' '), "S", " ", "", b" "),
        row(26, ch(' '), "A", " ", "", b"\x1b "),
        // Enter (Linux text "\r", Windows Ctrl+Enter text "\n": the text is ignored).
        row(27, Key::Enter, "", "\r", "", b"\r"),
        row(27, Key::Enter, "S", "\r", "", b"\r"),
        row(27, Key::Enter, "C", "\n", "", b"\r"),
        row(28, Key::Enter, "", "\r", "LNM", b"\r\n"),
        row(29, Key::Enter, "A", "\r", "", b"\x1b\r"),
        row(30, Key::Enter, "A", "\r", "LNM", b"\x1b\r\n"),
        row(31, Key::KeypadEnter, "", "\r", "", b"\r"),
        row(32, Key::KeypadEnter, "", "\r", "DECKPAM", b"\r"),
        row(33, Key::KeypadEnter, "", "\r", "DECKPAM+VT220", b"\x1bOM"),
        // Backspace (xkb text "\b", Windows Ctrl+Backspace text "\x7f": ignored).
        row(34, Key::Backspace, "", "\x08", "", b"\x7f"),
        row(34, Key::Backspace, "S", "\x08", "", b"\x7f"),
        row(35, Key::Backspace, "C", "\x7f", "", b"\x08"),
        row(36, Key::Backspace, "A", "\x08", "", b"\x1b\x7f"),
        row(37, Key::Backspace, "C+A", "", "", b"\x1b\x08"),
        row(38, Key::Backspace, "", "\x08", "BS", b"\x08"),
        row(39, Key::Backspace, "C", "\x08", "BS", b"\x7f"),
        // Tab.
        row(40, Key::Tab, "", "\t", "", b"\t"),
        row(40, Key::Tab, "C", "\t", "", b"\t"),
        row(41, Key::Tab, "S", "", "", b"\x1b[Z"),
        row(41, Key::Backtab, "S", "", "", b"\x1b[Z"),
        row(42, Key::Tab, "A", "\t", "", b"\x1b\t"),
        row(43, Key::Tab, "S+A", "", "", b"\x1b\x1b[Z"),
        row(43, Key::Backtab, "S+A", "", "", b"\x1b\x1b[Z"),
        // Escape.
        row(44, Key::Escape, "", "\x1b", "", b"\x1b"),
        row(44, Key::Escape, "S", "\x1b", "", b"\x1b"),
        row(44, Key::Escape, "C", "\x1b", "", b"\x1b"),
        row(45, Key::Escape, "A", "\x1b", "", b"\x1b\x1b"),
        // Cursor keys.
        row(46, Key::Up, "", "", "", b"\x1b[A"),
        row(47, Key::Up, "", "", "DECCKM", b"\x1bOA"),
        row(48, Key::Up, "S", "", "", b"\x1b[1;2A"),
        row(48, Key::Up, "S", "", "DECCKM", b"\x1b[1;2A"),
        row(49, Key::Up, "A", "", "", b"\x1b[1;3A"),
        row(50, Key::Up, "S+A", "", "", b"\x1b[1;4A"),
        row(51, Key::Up, "C", "", "DECCKM", b"\x1b[1;5A"),
        row(52, Key::Up, "C+S", "", "", b"\x1b[1;6A"),
        row(53, Key::Up, "C+A", "", "", b"\x1b[1;7A"),
        row(54, Key::Up, "C+A+S", "", "", b"\x1b[1;8A"),
        row(55, Key::Up, "M", "", "", b"\x1b[1;9A"),
        row(56, Key::Down, "", "", "", b"\x1b[B"),
        row(56, Key::Right, "", "", "", b"\x1b[C"),
        row(56, Key::Left, "", "", "", b"\x1b[D"),
        row(57, Key::Down, "", "", "DECCKM", b"\x1bOB"),
        row(57, Key::Right, "", "", "DECCKM", b"\x1bOC"),
        row(57, Key::Left, "", "", "DECCKM", b"\x1bOD"),
        row(58, Key::Left, "C", "", "", b"\x1b[1;5D"),
        row(59, Key::Home, "", "", "", b"\x1b[H"),
        row(59, Key::End, "", "", "", b"\x1b[F"),
        row(60, Key::Home, "", "", "DECCKM", b"\x1bOH"),
        row(60, Key::End, "", "", "DECCKM", b"\x1bOF"),
        row(61, Key::Home, "C", "", "", b"\x1b[1;5H"),
        row(62, Key::End, "S", "", "DECCKM", b"\x1b[1;2F"),
        row(63, Key::Begin, "", "", "", b"\x1b[E"),
        row(63, Key::Begin, "", "", "DECCKM", b"\x1bOE"),
        // Editing keys.
        row(64, Key::Insert, "", "", "", b"\x1b[2~"),
        row(64, Key::Delete, "", "\x7f", "", b"\x1b[3~"),
        row(64, Key::Delete, "", "\x7f", "DEL", b"\x7f"),
        row(64, Key::Delete, "S", "", "DEL", b"\x1b[3;2~"),
        row(65, Key::PageUp, "", "", "", b"\x1b[5~"),
        row(65, Key::PageDown, "", "", "", b"\x1b[6~"),
        row(66, Key::Delete, "C", "", "", b"\x1b[3;5~"),
        row(67, Key::Insert, "S", "", "", b"\x1b[2;2~"),
        row(68, Key::PageUp, "C", "", "", b"\x1b[5;5~"),
        row(69, Key::PageDown, "S+A", "", "", b"\x1b[6;4~"),
        // Function keys.
        row(70, Key::F(1), "", "", "", b"\x1bOP"),
        row(70, Key::F(2), "", "", "", b"\x1bOQ"),
        row(70, Key::F(3), "", "", "", b"\x1bOR"),
        row(70, Key::F(4), "", "", "", b"\x1bOS"),
        row(70, Key::F(1), "", "", "DECCKM", b"\x1bOP"),
        row(70, Key::F(4), "", "", "DECCKM", b"\x1bOS"),
        row(71, Key::F(1), "S", "", "", b"\x1b[1;2P"),
        row(72, Key::F(2), "A", "", "", b"\x1b[1;3Q"),
        row(73, Key::F(3), "C", "", "", b"\x1b[1;5R"),
        row(74, Key::F(4), "C+S", "", "", b"\x1b[1;6S"),
        row(75, Key::F(5), "", "", "", b"\x1b[15~"),
        row(75, Key::F(6), "", "", "", b"\x1b[17~"),
        row(75, Key::F(7), "", "", "", b"\x1b[18~"),
        row(75, Key::F(8), "", "", "", b"\x1b[19~"),
        row(75, Key::F(9), "", "", "", b"\x1b[20~"),
        row(75, Key::F(10), "", "", "", b"\x1b[21~"),
        row(75, Key::F(11), "", "", "", b"\x1b[23~"),
        row(75, Key::F(12), "", "", "", b"\x1b[24~"),
        row(76, Key::F(5), "S", "", "", b"\x1b[15;2~"),
        row(77, Key::F(12), "C", "", "", b"\x1b[24;5~"),
        row(78, Key::F(10), "C+A+S", "", "", b"\x1b[21;8~"),
        row(79, Key::F(13), "", "", "", b"\x1b[25~"),
        row(79, Key::F(14), "", "", "", b"\x1b[26~"),
        row(79, Key::F(15), "", "", "", b"\x1b[28~"),
        row(79, Key::F(16), "", "", "", b"\x1b[29~"),
        row(79, Key::F(17), "", "", "", b"\x1b[31~"),
        row(79, Key::F(18), "", "", "", b"\x1b[32~"),
        row(79, Key::F(19), "", "", "", b"\x1b[33~"),
        row(79, Key::F(20), "", "", "", b"\x1b[34~"),
        row(80, Key::F(21), "", "", "", b"\x1b[42~"),
        row(80, Key::F(22), "", "", "", b"\x1b[43~"),
        row(80, Key::F(23), "", "", "", b"\x1b[44~"),
        row(80, Key::F(24), "", "", "", b"\x1b[45~"),
        row(81, Key::F(13), "S", "", "", b"\x1b[25;2~"),
        row(82, Key::Menu, "", "", "", b"\x1b[29~"),
        // Keypad.
        row(83, kp(KeypadKey::Digit(1)), "", "1", "", b"1"),
        row(83, kp(KeypadKey::Digit(1)), "", "1", "DECKPAM", b"1"),
        row(84, kp(KeypadKey::Digit(1)), "", "1", "DECKPAM+VT220", b"\x1bOq"),
        row(85, kp(KeypadKey::Digit(0)), "", "0", "DECKPAM+VT220", b"\x1bOp"),
        row(85, kp(KeypadKey::Digit(9)), "", "9", "DECKPAM+VT220", b"\x1bOy"),
        row(86, kp(KeypadKey::Multiply), "", "*", "DECKPAM+VT220", b"\x1bOj"),
        row(86, kp(KeypadKey::Add), "", "+", "DECKPAM+VT220", b"\x1bOk"),
        row(86, kp(KeypadKey::Separator), "", ",", "DECKPAM+VT220", b"\x1bOl"),
        row(86, kp(KeypadKey::Subtract), "", "-", "DECKPAM+VT220", b"\x1bOm"),
        row(86, kp(KeypadKey::Decimal), "", ".", "DECKPAM+VT220", b"\x1bOn"),
        row(86, kp(KeypadKey::Divide), "", "/", "DECKPAM+VT220", b"\x1bOo"),
        row(86, kp(KeypadKey::Equal), "", "=", "DECKPAM+VT220", b"\x1bOX"),
        row(87, kp(KeypadKey::Add), "", "+", "DECKPAM", b"+"),
        row(88, Key::End, "", "", "", b"\x1b[F"),
        row(88, Key::End, "", "", "DECCKM", b"\x1bOF"),
        row(89, Key::Insert, "", "", "DECKPAM", b"\x1b[2~"),
        // Keys that send nothing by themselves.
        nothing(90, Key::Ignored, "S", ""),
        nothing(90, Key::Ignored, "C", ""),
        nothing(90, Key::Ignored, "", "\u{b4}"),
        nothing(90, Key::Character(None), "", ""),
        nothing(90, Key::Character(Some('A')), "", ""),
    ];

    #[test]
    fn keyboard_table() {
        let mut failures = Vec::new();
        for case in TABLE {
            let got = encode(case.key, case.mods, case.text, case.modes);
            if got.as_deref() != case.expected {
                failures.push(format!(
                    "row {}: {:?} mods {:?} text {:?} modes {:?}: expected {:?}, got {:?}",
                    case.row, case.key, case.mods, case.text, case.modes, case.expected, got
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn keyboard_table_covers_every_research_row() {
        for number in 1..=90 {
            assert!(
                TABLE.iter().any(|case| case.row == number),
                "row {number} has no case"
            );
        }
    }

    #[test]
    fn qt_key_mapping() {
        use qt::*;
        let named = [
            (KEY_ESCAPE, Key::Escape),
            (KEY_TAB, Key::Tab),
            (KEY_BACKTAB, Key::Backtab),
            (KEY_BACKSPACE, Key::Backspace),
            (KEY_RETURN, Key::Enter),
            (KEY_ENTER, Key::KeypadEnter),
            (KEY_INSERT, Key::Insert),
            (KEY_DELETE, Key::Delete),
            (KEY_CLEAR, Key::Begin),
            (KEY_HOME, Key::Home),
            (KEY_END, Key::End),
            (KEY_LEFT, Key::Left),
            (KEY_UP, Key::Up),
            (KEY_RIGHT, Key::Right),
            (KEY_DOWN, Key::Down),
            (KEY_PAGE_UP, Key::PageUp),
            (KEY_PAGE_DOWN, Key::PageDown),
            (KEY_MENU, Key::Menu),
            (KEY_F1, Key::F(1)),
            (0x0100_003b, Key::F(12)),
            (0x0100_003c, Key::F(13)),
            (KEY_F24, Key::F(24)),
        ];
        for (qt_key, key) in named {
            assert_eq!(Key::from_qt(qt_key, false), key, "{qt_key:#x}");
            // Keypad navigation keys (NumLock off) are the navigation keys.
            assert_eq!(Key::from_qt(qt_key, true), key, "{qt_key:#x} on the keypad");
        }
        // F25 and up exist in Qt (X11 only) but have no terminal sequence.
        assert_eq!(Key::from_qt(0x0100_0048, false), Key::Character(None));
        assert_eq!(Key::from_qt(KEY_F35, false), Key::Character(None));
        let ignored = [
            KEY_SHIFT,
            0x0100_0021, // Control
            0x0100_0022, // Meta
            0x0100_0023, // Alt
            0x0100_0024, // CapsLock
            0x0100_0025, // NumLock
            KEY_SCROLL_LOCK,
            KEY_SUPER_L,
            KEY_SUPER_R,
            KEY_HYPER_L,
            KEY_HYPER_R,
            KEY_ALT_GR,
            KEY_MULTI_KEY,
            KEY_MODE_SWITCH,
            KEY_DEAD_FIRST,
            0x0100_1251, // Dead_Acute
            KEY_DEAD_LAST,
        ];
        for qt_key in ignored {
            assert_eq!(Key::from_qt(qt_key, false), Key::Ignored, "{qt_key:#x}");
        }
        assert_eq!(Key::from_qt(0x41, false), Key::Character(Some('A')));
        assert_eq!(Key::from_qt(KEY_SPACE, false), Key::Character(Some(' ')));
        assert_eq!(Key::from_qt(0xc4, false), Key::Character(Some('\u{c4}')));
        assert_eq!(Key::from_qt(0x0100_0009, false), Key::Character(None)); // Print
        assert_eq!(Key::from_qt(0x01ff_ffff, false), Key::Character(None)); // unknown
        assert_eq!(Key::from_qt(0, false), Key::Character(None));
        assert_eq!(Key::from_qt(-1, false), Key::Character(None));
        assert_eq!(Key::from_qt(0xd800, false), Key::Character(None)); // surrogate
    }

    #[test]
    fn qt_keypad_mapping() {
        use qt::*;
        for digit in 0..=9u8 {
            assert_eq!(
                Key::from_qt(KEY_0 + i32::from(digit), true),
                Key::Keypad(KeypadKey::Digit(digit))
            );
            assert_eq!(
                Key::from_qt(KEY_0 + i32::from(digit), false),
                Key::Character(Some(char::from(b'0' + digit)))
            );
        }
        let operators = [
            (KEY_ASTERISK, KeypadKey::Multiply),
            (KEY_PLUS, KeypadKey::Add),
            (KEY_COMMA, KeypadKey::Separator),
            (KEY_MINUS, KeypadKey::Subtract),
            (KEY_PERIOD, KeypadKey::Decimal),
            (KEY_SLASH, KeypadKey::Divide),
            (KEY_EQUAL, KeypadKey::Equal),
        ];
        for (qt_key, key) in operators {
            assert_eq!(Key::from_qt(qt_key, true), Key::Keypad(key));
            assert_eq!(key.as_char(), key_char(qt_key).unwrap());
        }
        // A keypad-flagged key that is neither a digit nor an operator stays a character.
        assert_eq!(Key::from_qt(0x41, true), Key::Character(Some('A')));
    }

    #[test]
    fn qt_modifiers() {
        use qt::*;
        assert_eq!(modifiers_from_qt(0), Modifiers::NONE);
        assert_eq!(
            modifiers_from_qt(SHIFT_MODIFIER | CONTROL_MODIFIER | ALT_MODIFIER | META_MODIFIER),
            Modifiers {
                shift: true,
                ctrl: true,
                alt: true,
                meta: true
            }
        );
        // AltGr with the windows:altgr option: Ctrl and Alt are dropped.
        assert_eq!(
            modifiers_from_qt(GROUP_SWITCH_MODIFIER | CONTROL_MODIFIER | ALT_MODIFIER),
            Modifiers::NONE
        );
        // Keypad is not a modifier for the encoder.
        assert_eq!(modifiers_from_qt(KEYPAD_MODIFIER), Modifiers::NONE);
    }

    fn qt_encode(qt_key: i32, qt_modifiers: u32, text: &str, windows: bool) -> Option<Vec<u8>> {
        let input = translate_qt(qt_key, qt_modifiers, text, windows);
        encode_key(&input, &InputModes::default(), &KeyOptions::default())
    }

    #[test]
    fn qt_events_on_linux_and_windows() {
        use qt::*;
        const C: u32 = CONTROL_MODIFIER;
        const A: u32 = ALT_MODIFIER;
        const S: u32 = SHIFT_MODIFIER;
        const KP: u32 = KEYPAD_MODIFIER;
        // (key, modifiers, xkb text, Windows text, expected) from s2_input.md §4.3.
        #[rustfmt::skip]
        let cases: &[(i32, u32, &str, &str, &[u8])] = &[
            (KEY_BACKSPACE, 0, "\x08", "\x08", b"\x7f"),
            (KEY_BACKSPACE, C, "\x08", "\x7f", b"\x08"),
            (KEY_RETURN, C, "\r", "\n", b"\r"),
            (KEY_SPACE, C, "\0", " ", b"\x00"),
            (0x32, C, "\0", "2", b"\x00"),               // Ctrl+2
            (0x40, C | S, "\0", "2", b"\x00"),           // Ctrl+Shift+2 (Ctrl+@)
            (0x5b, C, "\x1b", "\x1b", b"\x1b"),          // Ctrl+[
            (0x3f, C | S, "?", "?", b"\x7f"),            // Ctrl+?
            (0x41, C, "\x01", "\x01", b"\x01"),          // Ctrl+A
            (0x41, C | A, "\x01", "A", b"\x1b\x01"),     // Ctrl+Alt+A
            (0x41, A, "a", "a", b"\x1ba"),               // Alt+A
            (KEY_DELETE, 0, "\x7f", "\x7f", b"\x1b[3~"),
            (KEY_ESCAPE, 0, "\x1b", "\x1b", b"\x1b"),
            (KEY_0 + 5, KP, "5", "5", b"5"),             // keypad 5, NumLock on
            (KEY_CLEAR, KP, "", "", b"\x1b[E"),          // keypad 5, NumLock off
            (KEY_ENTER, KP, "\r", "\r", b"\r"),
            (KEY_SHIFT, 0, "", "", b""),
            (KEY_DEAD_FIRST + 1, 0, "", "", b""),
        ];
        for &(qt_key, qt_modifiers, xkb_text, windows_text, expected) in cases {
            let expected = if expected.is_empty() {
                None
            } else {
                Some(expected.to_vec())
            };
            assert_eq!(
                qt_encode(qt_key, qt_modifiers, xkb_text, false),
                expected,
                "xkb {qt_key:#x} {qt_modifiers:#x} {xkb_text:?}"
            );
            assert_eq!(
                qt_encode(qt_key, qt_modifiers, windows_text, true),
                expected,
                "windows {qt_key:#x} {qt_modifiers:#x} {windows_text:?}"
            );
        }
    }

    #[test]
    fn windows_alt_gr_without_the_platform_option() {
        use qt::*;
        let ctrl_alt = CONTROL_MODIFIER | ALT_MODIFIER;
        // German AltGr+Q: Qt reports Key_At with Ctrl+Alt and the text "@".
        assert_eq!(qt_encode(0x40, ctrl_alt, "@", true), Some(b"@".to_vec()));
        // AltGr+E: the euro sign; AltGr+2 on a German layout: superscript two.
        assert_eq!(
            qt_encode(0x20ac, ctrl_alt, "\u{20ac}", true),
            Some("\u{20ac}".into())
        );
        assert_eq!(
            qt_encode(0xb2, ctrl_alt, "\u{b2}", true),
            Some("\u{b2}".into())
        );
        // Polish AltGr+A.
        assert_eq!(
            qt_encode(0x104, ctrl_alt, "\u{105}", true),
            Some("\u{105}".into())
        );
        // A real Ctrl+Alt+letter still encodes as ESC + the control byte.
        assert_eq!(
            qt_encode(0x41, ctrl_alt, "A", true),
            Some(b"\x1b\x01".to_vec())
        );
        // With the windows:altgr option Qt reports GroupSwitch and no Ctrl.
        assert_eq!(
            qt_encode(0x40, GROUP_SWITCH_MODIFIER, "@", true),
            Some(b"@".to_vec())
        );
        // The heuristic is Windows-only: on Linux Ctrl+Alt+? is ESC DEL.
        assert_eq!(
            qt_encode(0x3f, ctrl_alt | SHIFT_MODIFIER, "?", false),
            Some(b"\x1b\x7f".to_vec())
        );
        // It never applies to named keys.
        assert_eq!(
            qt_encode(KEY_UP, ctrl_alt, "", true),
            Some(b"\x1b[1;7A".to_vec())
        );
    }

    #[test]
    fn windows_alt_letter_case_follows_shift() {
        use qt::*;
        // Qt lower-cases letters typed with Alt on Windows.
        assert_eq!(
            qt_encode(0x41, ALT_MODIFIER | SHIFT_MODIFIER, "a", true),
            Some(b"\x1bA".to_vec())
        );
        assert_eq!(
            qt_encode(0x41, ALT_MODIFIER, "a", true),
            Some(b"\x1ba".to_vec())
        );
        // Linux text is already right, and is left alone (Caps Lock gives "A").
        assert_eq!(
            qt_encode(0x41, ALT_MODIFIER, "A", false),
            Some(b"\x1bA".to_vec())
        );
        // Non-letters are untouched.
        assert_eq!(
            qt_encode(0x31, ALT_MODIFIER, "1", true),
            Some(b"\x1b1".to_vec())
        );
    }

    #[test]
    fn options_default() {
        assert_eq!(
            KeyOptions::default(),
            KeyOptions {
                backspace_sends_ctrl_h: false,
                alt_sends_escape: true,
                delete_sends_del: false,
                vt220_keypad: false
            }
        );
    }

    #[test]
    fn no_meta_prefix_when_alt_is_not_meta() {
        assert_eq!(
            encode(Key::Enter, "A", "\r", "NOMETA"),
            Some(b"\r".to_vec())
        );
        assert_eq!(
            encode(Key::Backspace, "A", "", "NOMETA"),
            Some(b"\x7f".to_vec())
        );
        assert_eq!(encode(ch('A'), "C+A", "", "NOMETA"), Some(b"\x01".to_vec()));
        // Keys with a modifier parameter keep Alt in the parameter.
        assert_eq!(
            encode(Key::Up, "A", "", "NOMETA"),
            Some(b"\x1b[1;3A".to_vec())
        );
    }

    #[test]
    fn text_edge_cases() {
        // Several characters (compressed auto-repeat, IME): sent as is, no Meta prefix.
        assert_eq!(encode(ch('A'), "A", "aa", ""), Some(b"aa".to_vec()));
        // Super + text sends the text.
        assert_eq!(encode(ch('A'), "M", "a", ""), Some(b"a".to_vec()));
        // A key Qt couldn't name but with text (Key_unknown).
        assert_eq!(
            encode(Key::Character(None), "", "\u{1f600}", ""),
            Some("\u{1f600}".into())
        );
        // Ctrl on a key outside the table whose platform text is a control character.
        assert_eq!(
            encode(Key::Character(None), "C", "\x03", ""),
            Some(b"\x03".to_vec())
        );
        assert_eq!(
            encode(Key::Character(Some('\u{441}')), "C", "\x03", ""),
            Some(b"\x03".to_vec())
        );
        // Ctrl on a key outside the table without text sends nothing.
        assert_eq!(encode(Key::Character(Some('\u{e9}')), "C", "", ""), None);
        // Keypad keys without text fall back to their character; Alt and Ctrl apply.
        assert_eq!(
            encode(kp(KeypadKey::Digit(7)), "", "", ""),
            Some(b"7".to_vec())
        );
        assert_eq!(
            encode(kp(KeypadKey::Digit(7)), "A", "7", ""),
            Some(b"\x1b7".to_vec())
        );
        assert_eq!(
            encode(kp(KeypadKey::Digit(3)), "C", "3", ""),
            Some(b"\x1b".to_vec())
        );
        // The locale's decimal separator is kept.
        assert_eq!(
            encode(kp(KeypadKey::Decimal), "", ",", ""),
            Some(b",".to_vec())
        );
        // Keypad VT220 codes ignore modifiers.
        assert_eq!(
            encode(kp(KeypadKey::Digit(1)), "C+A", "1", "DECKPAM+VT220"),
            Some(b"\x1bOq".to_vec())
        );
        // Keypad Enter in LNM, and in VT220 mode without DECKPAM.
        assert_eq!(
            encode(Key::KeypadEnter, "", "", "LNM"),
            Some(b"\r\n".to_vec())
        );
        assert_eq!(
            encode(Key::KeypadEnter, "", "", "VT220"),
            Some(b"\r".to_vec())
        );
        // Out-of-range function keys.
        assert_eq!(encode(Key::F(0), "", "", ""), None);
        assert_eq!(encode(Key::F(25), "", "", ""), None);
        // Super adds 8 to the parameter.
        assert_eq!(
            encode(Key::F(5), "M+C", "", ""),
            Some(b"\x1b[15;13~".to_vec())
        );
        assert_eq!(
            encode(Key::F(1), "M+S+A+C", "", ""),
            Some(b"\x1b[1;16P".to_vec())
        );
    }

    #[test]
    fn ctrl_table() {
        for letter in 'a'..='z' {
            let expected = letter as u8 - b'a' + 1;
            assert_eq!(ctrl_byte(letter), Some(expected));
            assert_eq!(ctrl_byte(letter.to_ascii_uppercase()), Some(expected));
        }
        for c in ['0', '1', '9', '-', '=', ';', '\'', ',', '.', '!', '\u{e9}'] {
            assert_eq!(ctrl_byte(c), None, "{c:?}");
        }
        assert_eq!(ctrl_byte('{'), Some(0x1b));
        assert_eq!(ctrl_byte('|'), Some(0x1c));
        assert_eq!(ctrl_byte('}'), Some(0x1d));
    }

    #[test]
    fn shortcut_override_follows_adr_0011() {
        let none = Modifiers::NONE;
        let shift = mods("S");
        for number in 1..=24 {
            let expected = number != 11;
            assert_eq!(
                terminal_wants_shortcut(&Key::F(number), none),
                expected,
                "F{number}"
            );
            assert_eq!(
                terminal_wants_shortcut(&Key::F(number), shift),
                expected,
                "Shift+F{number}"
            );
            assert!(
                !terminal_wants_shortcut(&Key::F(number), mods("C")),
                "Ctrl+F{number}"
            );
            assert!(
                !terminal_wants_shortcut(&Key::F(number), mods("C+S")),
                "Ctrl+Shift+F{number}"
            );
            assert!(
                !terminal_wants_shortcut(&Key::F(number), mods("A")),
                "Alt+F{number}"
            );
        }
        assert!(terminal_wants_shortcut(&Key::F(6), mods("M")));
        assert!(!terminal_wants_shortcut(&Key::F(0), none));
        assert!(!terminal_wants_shortcut(&Key::F(25), none));
        for key in [
            Key::Tab,
            Key::Enter,
            Key::PageUp,
            ch('C'),
            Key::Keypad(KeypadKey::Add),
        ] {
            assert!(!terminal_wants_shortcut(&key, none), "{key:?}");
            assert!(!terminal_wants_shortcut(&key, mods("C+S")), "{key:?}");
        }
    }

    #[test]
    fn decimal() {
        for (value, text) in [
            (0, "0"),
            (7, "7"),
            (10, "10"),
            (2015, "2015"),
            (u32::MAX, "4294967295"),
        ] {
            let mut out = Vec::new();
            push_decimal(&mut out, value);
            assert_eq!(out, text.as_bytes());
        }
    }
    #[test]
    fn windows_alt_codes_send_nothing_until_the_composed_character() {
        let alt_keypad = qt::ALT_MODIFIER | qt::KEYPAD_MODIFIER;
        let modes = InputModes::default();
        let options = KeyOptions::default();
        // Windows: the digits of Alt+0233 are swallowed; the "é" arrives afterwards as text.
        for digit in 0..10 {
            let input = translate_qt(qt::KEY_0 + digit, alt_keypad, "", true);
            assert_eq!(input.key, Key::Ignored);
            assert_eq!(encode_key(&input, &modes, &options), None);
            assert!(alt_code_digit(qt::KEY_0 + digit, alt_keypad, true));
        }
        // Main-row Alt+digit, Ctrl+Alt and other platforms keep their meaning.
        assert!(!alt_code_digit(qt::KEY_0 + 2, qt::ALT_MODIFIER, true));
        assert!(!alt_code_digit(
            qt::KEY_0 + 2,
            alt_keypad | qt::CONTROL_MODIFIER,
            true
        ));
        assert!(!alt_code_digit(qt::KEY_0 + 2, alt_keypad, false));
        let linux = translate_qt(qt::KEY_0 + 2, alt_keypad, "", false);
        assert_eq!(
            encode_key(&linux, &modes, &options).as_deref(),
            Some(&b"\x1b2"[..])
        );
    }
}
