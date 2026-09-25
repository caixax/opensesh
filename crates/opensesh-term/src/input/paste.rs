//! Paste and focus reporting.
//!
//! Pasted text is always sanitised, bracketed or not (Windows Terminal's rule): CR LF and lone
//! LF become CR (what Enter sends), and every C0 control except HT and CR, DEL and every C1
//! control are removed. Without ESC and without C1 (8-bit CSI, U+009B) the pasted text can't
//! forge the end-of-paste marker `ESC [ 201 ~`, and it can't smuggle other control sequences
//! into the program. Removing ETX also covers shells that end a bracketed paste on Ctrl+C.
//!
//! The warnings and editing of the safe-paste dialog (PLAN §8) come in a later sprint and run
//! before this.

use alacritty_terminal::term::TermMode;

use crate::input::InputModes;

/// Start of a bracketed paste, `CSI 200 ~`.
const PASTE_START: &[u8] = b"\x1b[200~";
/// End of a bracketed paste, `CSI 201 ~`.
const PASTE_END: &[u8] = b"\x1b[201~";

/// Encodes clipboard text for the program: sanitised (see the module docs) and wrapped in
/// `CSI 200 ~` / `CSI 201 ~` when the program enabled bracketed paste (DECSET 2004).
///
/// Returns an empty vector when nothing is left to send.
#[must_use]
pub fn encode_paste(text: &str, modes: &InputModes) -> Vec<u8> {
    let bracketed = modes.term.contains(TermMode::BRACKETED_PASTE);
    let markers = if bracketed {
        PASTE_START.len() + PASTE_END.len()
    } else {
        0
    };
    let mut out = Vec::with_capacity(text.len() + markers);
    if bracketed {
        out.extend_from_slice(PASTE_START);
    }
    let body_start = out.len();

    // Copy runs of unchanged text in one go; only line breaks and controls interrupt them.
    let mut run_start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((index, c)) = chars.next() {
        if c == '\t' || !c.is_control() {
            continue;
        }
        out.extend_from_slice(text.get(run_start..index).unwrap_or_default().as_bytes());
        run_start = index + c.len_utf8();
        match c {
            '\r' => {
                out.push(b'\r');
                if let Some(&(next, '\n')) = chars.peek() {
                    chars.next();
                    run_start = next + 1;
                }
            }
            '\n' => out.push(b'\r'),
            // Every other C0 control, DEL and C1 control is dropped.
            _ => {}
        }
    }
    out.extend_from_slice(text.get(run_start..).unwrap_or_default().as_bytes());

    if out.len() == body_start {
        return Vec::new();
    }
    if bracketed {
        out.extend_from_slice(PASTE_END);
    }
    out
}

/// Encodes a focus change for focus reporting (DECSET 1004): `CSI I` when the terminal gains
/// the focus, `CSI O` when it loses it, or `None` when the program didn't ask for them.
///
/// The caller sends only transitions (remembering the last state sent), with "focused" meaning
/// the window is active and the terminal item has the active focus.
#[must_use]
pub fn encode_focus(focused: bool, modes: &InputModes) -> Option<&'static [u8]> {
    if !modes.term.contains(TermMode::FOCUS_IN_OUT) {
        return None;
    }
    Some(if focused { b"\x1b[I" } else { b"\x1b[O" })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modes(bracketed: bool) -> InputModes {
        let mut modes = InputModes::default();
        modes.term.set(TermMode::BRACKETED_PASTE, bracketed);
        modes
    }

    /// The paste table in the Sprint 2 input research (s2_input.md §7.4).
    #[test]
    fn paste_table() {
        #[rustfmt::skip]
        let cases: &[(u32, &str, bool, &[u8])] = &[
            (1, "ls -l", false, b"ls -l"),
            (2, "a\r\nb\nc\rd", false, b"a\rb\rc\rd"),
            (3, "echo hi\n", true, b"\x1b[200~echo hi\r\x1b[201~"),
            (4, "x\x1b[201~rm -rf ~\n", true, b"\x1b[200~x[201~rm -rf ~\r\x1b[201~"),
            (5, "a\x03b\x7fc\td\x00e", false, b"abc\tde"),
            (6, "a\u{9b}31mb", true, b"\x1b[200~a31mb\x1b[201~"),
            (7, "\r\n\r\n", true, b"\x1b[200~\r\r\x1b[201~"),
            (8, "\u{e9}\u{20ac}\u{1f600}", true, b"\x1b[200~\xc3\xa9\xe2\x82\xac\xf0\x9f\x98\x80\x1b[201~"),
            (9, "", true, b""),
            (9, "\x1b", true, b""),
            (9, "", false, b""),
            (9, "\x1b\x00\u{85}", false, b""),
        ];
        for &(row, text, bracketed, expected) in cases {
            assert_eq!(
                encode_paste(text, &modes(bracketed)),
                expected,
                "row {row}: {text:?}"
            );
        }
    }

    #[test]
    fn nested_end_markers_cannot_be_rebuilt() {
        // Removing only the literal marker once would turn this into ESC [ 201 ~.
        let pasted = encode_paste("a\x1b[20\x1b[201~1~b", &modes(true));
        assert_eq!(pasted, b"\x1b[200~a[20[201~1~b\x1b[201~");
        let body = &pasted[6..pasted.len() - 6];
        assert!(!body.contains(&0x1b));
    }

    #[test]
    fn every_control_is_filtered() {
        let mut text = String::new();
        for code in (0x00..=0x1f).chain(0x7f..=0x9f) {
            text.push(char::from_u32(code).unwrap());
        }
        // Only HT stays, and CR (from CR, LF and the CR LF pair).
        assert_eq!(encode_paste(&text, &modes(false)), b"\t\r\r");
        // Line breaks at the edges and in sequence.
        assert_eq!(encode_paste("\n\r\n\r", &modes(false)), b"\r\r\r");
        assert_eq!(encode_paste("\n\n", &modes(false)), b"\r\r");
        assert_eq!(encode_paste("a\r", &modes(false)), b"a\r");
        assert_eq!(encode_paste("\ra", &modes(false)), b"\ra");
    }

    #[test]
    fn unicode_is_kept() {
        // Non-control characters outside ASCII, including the line and paragraph separators,
        // zero-width characters and characters right after the C1 block.
        let text = "\u{a0}\u{2028}\u{200b}\u{e9}\u{65e5}\u{1f600}";
        assert_eq!(encode_paste(text, &modes(false)), text.as_bytes());
    }

    #[test]
    fn focus_reports() {
        let mut modes = InputModes::default();
        assert_eq!(encode_focus(true, &modes), None);
        assert_eq!(encode_focus(false, &modes), None);
        modes.term.insert(TermMode::FOCUS_IN_OUT);
        assert_eq!(encode_focus(true, &modes), Some(&b"\x1b[I"[..]));
        assert_eq!(encode_focus(false, &modes), Some(&b"\x1b[O"[..]));
    }
}
