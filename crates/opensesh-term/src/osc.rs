//! The side parser: what `alacritty_terminal` drops, read from the same bytes (ADR 0012).
//!
//! `alacritty_terminal`'s handler never sees some sequences OpenSesh needs: OSC 7 (the shell's
//! working directory) and X10 mouse reporting (`CSI ? 9 h`). A second raw
//! [`alacritty_terminal::vte::Parser`] runs over every byte the engine parses, so tokenization
//! (string terminators, chunk splits, UTF-8) is exactly the engine's. It costs a few percent of
//! the parse time and keeps only small state; OSC payloads it stores are capped at
//! [`MAX_OSC_PAYLOAD`] bytes, and [`OscLimiter`] caps every OSC string before either parser sees
//! it.

use std::sync::OnceLock;

use alacritty_terminal::vte::{Params, Parser, Perform};

/// Longest OSC 7 payload that is kept (longer ones are ignored).
pub const MAX_OSC_PAYLOAD: usize = 4096;

/// Longest OSC string payload passed on to the parsers ([`OscLimiter`]); the rest is dropped.
pub const MAX_OSC_STRING: usize = 8 * 1024;

/// Longest OSC 52 (clipboard) payload when programs may set the clipboard: 1 MiB of base64,
/// about 768 KiB of text.
pub const MAX_OSC52_STRING: usize = 1024 * 1024;

/// Caps OSC strings (`ESC ] ... BEL|ST`) before any parser sees them.
///
/// vte buffers OSC strings without a limit, and `alacritty_terminal` keeps the whole title and
/// clones it for every `CSI 22 t` (up to 4096 times): a hostile stream of a few MiB could make
/// the app allocate gigabytes. The limiter follows vte's rules for where an OSC string starts
/// (`ESC ]`) and ends (BEL, CAN, SUB or ESC) and drops payload bytes past [`MAX_OSC_STRING`], so
/// the terminators and everything outside OSC strings pass through unchanged.
#[derive(Debug, Default)]
pub struct OscLimiter {
    /// The previous byte was ESC (outside an OSC string).
    escape: bool,
    /// Payload bytes of the current OSC string, `None` outside one.
    osc: Option<usize>,
    /// The first payload bytes of the current OSC string (to spot `52;`).
    prefix: [u8; 3],
    /// Allow OSC 52 strings up to [`MAX_OSC52_STRING`].
    clipboard: bool,
}

impl OscLimiter {
    /// Lets OSC 52 (clipboard) strings through up to [`MAX_OSC52_STRING`] instead of
    /// [`MAX_OSC_STRING`], for when programs may set the clipboard.
    pub fn set_clipboard(&mut self, allowed: bool) {
        self.clipboard = allowed;
    }

    /// Filters `input`. Returns `None` when every byte passes (the common case), else the bytes
    /// to parse instead.
    pub fn filter(&mut self, input: &[u8]) -> Option<Vec<u8>> {
        let mut out: Option<Vec<u8>> = None;
        for (index, &byte) in input.iter().enumerate() {
            let keep = self.keep(byte);
            match (&mut out, keep) {
                (Some(out), true) => out.push(byte),
                (None, false) => out = Some(input[..index].to_vec()),
                _ => {}
            }
        }
        out
    }

    fn keep(&mut self, byte: u8) -> bool {
        match self.osc {
            Some(length) => match byte {
                0x07 | 0x18 | 0x1A => {
                    self.osc = None;
                    true
                }
                0x1B => {
                    self.osc = None;
                    self.escape = true;
                    true
                }
                _ => {
                    if let Some(slot) = self.prefix.get_mut(length) {
                        *slot = byte;
                    }
                    self.osc = Some(length.saturating_add(1));
                    let limit = if self.clipboard && self.prefix == *b"52;" {
                        MAX_OSC52_STRING
                    } else {
                        MAX_OSC_STRING
                    };
                    length < limit
                }
            },
            None => {
                if self.escape && byte == b']' {
                    self.osc = Some(0);
                    self.prefix = [0; 3];
                }
                self.escape = byte == 0x1B;
                true
            }
        }
    }
}

/// Extracts OSC 7 and tracks X10 mouse mode from a terminal byte stream.
#[derive(Default)]
pub struct SideParser {
    parser: Parser,
    state: SideState,
}

impl std::fmt::Debug for SideParser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SideParser")
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
struct SideState {
    /// X10 mouse reporting (`CSI ? 9 h`) is the active mouse protocol.
    x10_mouse: bool,
    /// The engine's mouse modes (1000/1002/1003) are stale: mode 9 replaced them, or turned
    /// reporting off, after they were set.
    engine_mouse_hidden: bool,
    /// Inside a synchronized update (`CSI ? 2026 h` ... `CSI ? 2026 l`).
    synchronized: bool,
    /// Latest OSC 7 directory that was not taken yet.
    working_directory: Option<String>,
    /// ENQ (Ctrl+E) bytes received since the last [`SideParser::take_enquiries`].
    enquiries: usize,
}

impl SideParser {
    /// A parser in the ground state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds bytes (any chunking works: sequences may be split across calls).
    pub fn advance(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.state, bytes);
    }

    /// Whether X10 mouse reporting is the active mouse protocol: `CSI ? 9 h` was the last mouse
    /// protocol the program enabled (a later `?1000h`, `?1002h` or `?1003h` wins over it), and
    /// neither a mouse-mode reset nor RIS (`ESC c`) came after it.
    #[must_use]
    pub fn x10_mouse(&self) -> bool {
        self.state.x10_mouse
    }

    /// Whether the engine's mouse modes must be ignored: `CSI ? 9 h` or `CSI ? 9 l` came after
    /// them (xterm keeps a single mouse protocol, so resetting 9 turns reporting off).
    #[must_use]
    pub fn engine_mouse_hidden(&self) -> bool {
        self.state.engine_mouse_hidden
    }

    /// Whether a synchronized update (DEC mode 2026) is open: the screen should not be shown
    /// until it ends.
    #[must_use]
    pub fn synchronized(&self) -> bool {
        self.state.synchronized
    }

    /// Ends a synchronized update that timed out (the program never closed it).
    pub fn end_synchronized(&mut self) {
        self.state.synchronized = false;
    }

    /// The working directory from the latest local OSC 7 since the previous call, if any.
    pub fn take_working_directory(&mut self) -> Option<String> {
        self.state.working_directory.take()
    }

    /// How many ENQ (0x05) control bytes arrived since the previous call: each one asks for the
    /// answerback message.
    pub fn take_enquiries(&mut self) -> usize {
        std::mem::take(&mut self.state.enquiries)
    }
}

impl Perform for SideState {
    fn execute(&mut self, byte: u8) {
        if byte == 0x05 {
            self.enquiries = self.enquiries.saturating_add(1);
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        let Some((&first, rest)) = params.split_first() else {
            return;
        };
        if first != b"7" || rest.is_empty() {
            return;
        }
        // vte splits the payload on ';', which is legal inside a URL: join it back.
        let length = rest.iter().map(|part| part.len() + 1).sum::<usize>();
        if length > MAX_OSC_PAYLOAD + 1 {
            return;
        }
        let payload = rest.join(&b';');
        if let Some(path) = parse_osc7(&payload, local_hostname()) {
            self.working_directory = Some(path);
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        if ignore || intermediates != b"?" || !matches!(action, 'h' | 'l') {
            return;
        }
        let set = action == 'h';
        for param in params {
            match param.first().copied() {
                // Setting or resetting X10 replaces whatever mode the engine still has.
                Some(9) => {
                    self.x10_mouse = set;
                    self.engine_mouse_hidden = true;
                }
                // xterm keeps one mouse protocol: enabling another one replaces X10, and
                // resetting any of them turns mouse reporting off (the engine tracks those).
                Some(1000 | 1002 | 1003) => {
                    self.x10_mouse = false;
                    self.engine_mouse_hidden = false;
                }
                Some(2026) => self.synchronized = set,
                _ => {}
            }
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        // RIS (full reset) turns every mouse mode off.
        if intermediates.is_empty() && byte == b'c' {
            self.x10_mouse = false;
            self.engine_mouse_hidden = false;
            self.synchronized = false;
        }
    }
}

/// Parses an OSC 7 payload, `file://HOST/PATH` with a percent-encoded path, and returns the path
/// when the host is this machine: empty, `localhost`, or `hostname` (case-insensitive; either
/// side may be the short name). On Windows, `/C:/dir` becomes `C:/dir`.
///
/// Returns `None` for other schemes, remote hosts, invalid percent-encoding or a path that is
/// not UTF-8.
#[must_use]
pub fn parse_osc7(payload: &[u8], hostname: Option<&str>) -> Option<String> {
    let rest = payload.strip_prefix(b"file://")?;
    let slash = rest.iter().position(|&byte| byte == b'/')?;
    let (host, path) = rest.split_at(slash);
    let host = std::str::from_utf8(host).ok()?;
    if !is_local_host(host, hostname) {
        return None;
    }
    let path = String::from_utf8(percent_decode(path)?).ok()?;
    if path.chars().any(char::is_control) {
        return None;
    }
    Some(windows_drive_path(path))
}

/// `/C:/dir` is how Windows shells write a drive path in a `file://` URL.
fn windows_drive_path(path: String) -> String {
    if cfg!(windows) {
        let bytes = path.as_bytes();
        let is_drive = bytes.len() >= 3
            && bytes[0] == b'/'
            && bytes[1].is_ascii_alphabetic()
            && bytes[2] == b':';
        if is_drive {
            return path[1..].to_owned();
        }
    }
    path
}

fn is_local_host(host: &str, hostname: Option<&str>) -> bool {
    if host.is_empty() || host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let Some(local) = hostname else {
        return false;
    };
    let short = |name: &str| name.split('.').next().unwrap_or(name).to_owned();
    host.eq_ignore_ascii_case(local)
        || (!local.contains('.') && short(host).eq_ignore_ascii_case(local))
        || (!host.contains('.') && host.eq_ignore_ascii_case(&short(local)))
}

fn percent_decode(input: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len());
    let mut bytes = input.iter();
    while let Some(&byte) = bytes.next() {
        if byte == b'%' {
            let high = hex_value(*bytes.next()?)?;
            let low = hex_value(*bytes.next()?)?;
            out.push(high << 4 | low);
        } else {
            out.push(byte);
        }
    }
    Some(out)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// This machine's host name, read once. Windows: `COMPUTERNAME`. Unix: the kernel's name
/// (`/proc/sys/kernel/hostname`), `/etc/hostname`, then `HOSTNAME`.
fn local_hostname() -> Option<&'static str> {
    static HOSTNAME: OnceLock<Option<String>> = OnceLock::new();
    HOSTNAME
        .get_or_init(|| {
            let from_file = |path: &str| {
                std::fs::read_to_string(path)
                    .ok()
                    .map(|name| name.trim().to_owned())
            };
            let candidates = if cfg!(windows) {
                vec![std::env::var("COMPUTERNAME").ok()]
            } else {
                vec![
                    from_file("/proc/sys/kernel/hostname"),
                    from_file("/etc/hostname"),
                    std::env::var("HOSTNAME").ok(),
                ]
            };
            candidates
                .into_iter()
                .flatten()
                .find(|name| !name.is_empty())
        })
        .as_deref()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limit(chunks: &[&[u8]]) -> Vec<u8> {
        let mut limiter = OscLimiter::default();
        let mut out = Vec::new();
        for chunk in chunks {
            match limiter.filter(chunk) {
                Some(filtered) => out.extend(filtered),
                None => out.extend_from_slice(chunk),
            }
        }
        out
    }

    #[test]
    fn short_osc_strings_and_other_bytes_pass_unchanged() {
        let text: &[u8] = b"abc\x1b]0;title\x07\x1b[1mbold\x1b]8;;https://x\x1b\\link";
        let mut limiter = OscLimiter::default();
        assert!(limiter.filter(text).is_none());
    }

    #[test]
    fn long_osc_payloads_are_cut_but_keep_their_terminator() {
        let mut stream = b"\x1b]2;".to_vec();
        stream.extend(std::iter::repeat_n(b'A', MAX_OSC_STRING * 3));
        stream.extend_from_slice(b"\x07after");
        let out = limit(&[&stream]);
        assert_eq!(out.len(), 2 + MAX_OSC_STRING + 1 + 5);
        assert!(out.ends_with(b"\x07after"));
    }

    #[test]
    fn the_cap_holds_across_chunk_splits() {
        let mut body = vec![b'A'; MAX_OSC_STRING + 100];
        body.push(0x07);
        let (first, second) = body.split_at(MAX_OSC_STRING - 10);
        let out = limit(&[b"\x1b", b"]0;", first, second, b"x"]);
        assert_eq!(out.len(), 1 + 3 + MAX_OSC_STRING - 2 + 1 + 1);
        assert!(out.ends_with(b"\x07x"));
    }

    #[test]
    fn can_sub_and_escape_end_the_osc_string() {
        for terminator in [0x18_u8, 0x1A, 0x1B] {
            let mut stream = b"\x1b]0;".to_vec();
            stream.extend(std::iter::repeat_n(b'A', MAX_OSC_STRING + 5));
            stream.push(terminator);
            stream.extend(std::iter::repeat_n(b'B', MAX_OSC_STRING + 5));
            let out = limit(&[&stream]);
            let bs = out.iter().filter(|&&byte| byte == b'B').count();
            assert_eq!(bs, MAX_OSC_STRING + 5, "terminator {terminator:#x}");
        }
    }

    #[test]
    fn osc7_local_paths() {
        assert_eq!(
            parse_osc7(b"file:///home/u/dir%20x", None).as_deref(),
            Some("/home/u/dir x")
        );
        assert_eq!(
            parse_osc7(b"file://localhost/tmp", None).as_deref(),
            Some("/tmp")
        );
        assert_eq!(
            parse_osc7(b"file://MyBox/srv/a%2Fb", Some("mybox")).as_deref(),
            Some("/srv/a/b")
        );
        // Short and fully qualified names match each other.
        assert_eq!(
            parse_osc7(b"file://mybox.example.org/x", Some("mybox")).as_deref(),
            Some("/x")
        );
        assert_eq!(
            parse_osc7(b"file://mybox/x", Some("mybox.example.org")).as_deref(),
            Some("/x")
        );
        assert_eq!(
            parse_osc7("file:///caf%C3%A9".as_bytes(), None).as_deref(),
            Some("/café")
        );
    }

    #[test]
    fn osc7_rejects_remote_and_malformed() {
        assert_eq!(parse_osc7(b"file://server/home", Some("mybox")), None);
        assert_eq!(parse_osc7(b"file://server/home", None), None);
        assert_eq!(parse_osc7(b"kitty-shell-cwd:///home", None), None);
        assert_eq!(parse_osc7(b"file://", None), None);
        assert_eq!(parse_osc7(b"file:///bad%zz", None), None);
        assert_eq!(parse_osc7(b"file:///truncated%4", None), None);
        assert_eq!(parse_osc7(b"file:///not-utf8%FF", None), None);
        assert_eq!(parse_osc7(b"file:///new%0Aline", None), None);
    }

    #[test]
    fn osc7_windows_drive() {
        let path = parse_osc7(b"file://localhost/C:/Users/me", None);
        if cfg!(windows) {
            assert_eq!(path.as_deref(), Some("C:/Users/me"));
        } else {
            assert_eq!(path.as_deref(), Some("/C:/Users/me"));
        }
    }

    #[test]
    fn side_parser_extracts_osc7_across_chunks_and_terminators() {
        let first = b"prompt\x1b]7;file:///home/u/a;b\x07more";
        let second = b"\x1b]7;file:///srv\x1b\\";
        for chunk_size in [1, 2, 3, 5, 64] {
            let mut side = SideParser::new();
            for chunk in first.chunks(chunk_size) {
                side.advance(chunk);
            }
            assert_eq!(
                side.take_working_directory().as_deref(),
                Some("/home/u/a;b"),
                "BEL-terminated, chunk size {chunk_size}"
            );
            assert_eq!(side.take_working_directory(), None);
            for chunk in second.chunks(chunk_size) {
                side.advance(chunk);
            }
            assert_eq!(
                side.take_working_directory().as_deref(),
                Some("/srv"),
                "ST-terminated, chunk size {chunk_size}"
            );
        }
        // Only the latest directory of a burst is kept.
        let mut side = SideParser::new();
        side.advance(b"\x1b]7;file:///one\x07\x1b]7;file:///two\x07");
        assert_eq!(side.take_working_directory().as_deref(), Some("/two"));
    }

    #[test]
    fn side_parser_ignores_other_oscs_and_long_payloads() {
        let mut side = SideParser::new();
        side.advance(b"\x1b]0;title\x07\x1b]8;;http://x\x07\x1b]7\x07");
        assert_eq!(side.take_working_directory(), None);
        let mut long = b"\x1b]7;file:///".to_vec();
        long.extend(std::iter::repeat_n(b'a', MAX_OSC_PAYLOAD + 10));
        long.push(0x07);
        side.advance(&long);
        assert_eq!(side.take_working_directory(), None);
        // A payload at the cap is still accepted.
        let mut fits = b"\x1b]7;file:///".to_vec();
        fits.extend(std::iter::repeat_n(
            b'a',
            MAX_OSC_PAYLOAD - "file:///".len(),
        ));
        fits.push(0x07);
        side.advance(&fits);
        assert!(side.take_working_directory().is_some());
    }

    #[test]
    fn x10_mouse_mode_tracking() {
        let mut side = SideParser::new();
        assert!(!side.x10_mouse());
        side.advance(b"\x1b[?9h");
        assert!(side.x10_mouse());
        side.advance(b"\x1b[?9l");
        assert!(!side.x10_mouse());
        // Last protocol set wins.
        side.advance(b"\x1b[?1000h\x1b[?9h");
        assert!(side.x10_mouse());
        side.advance(b"\x1b[?1002h");
        assert!(!side.x10_mouse());
        side.advance(b"\x1b[?9;1006h");
        assert!(side.x10_mouse());
        side.advance(b"\x1b[?1003l");
        assert!(!side.x10_mouse());
        // RIS resets it; other private modes and non-private 9 don't touch it.
        side.advance(b"\x1b[?9h\x1b[?25l\x1b[9h\x1b[9l");
        assert!(side.x10_mouse());
        side.advance(b"\x1bc");
        assert!(!side.x10_mouse());
        // Split sequence.
        side.advance(b"\x1b[");
        side.advance(b"?");
        side.advance(b"9h");
        assert!(side.x10_mouse());
    }

    #[test]
    fn enquiries_are_counted_outside_strings_only() {
        let mut side = SideParser::new();
        side.advance(b"a\x05b\x05");
        side.advance(b"\x1b]0;title\x05still title\x07");
        assert_eq!(side.take_enquiries(), 2);
        assert_eq!(side.take_enquiries(), 0);
    }

    #[test]
    fn clipboard_strings_may_be_longer_when_allowed() {
        let body = vec![b'A'; MAX_OSC_STRING * 2];
        let mut stream = b"\x1b]52;c;".to_vec();
        stream.extend_from_slice(&body);
        stream.push(0x07);

        let mut limiter = OscLimiter::default();
        let cut = limiter.filter(&stream).unwrap();
        assert_eq!(cut.len(), 2 + MAX_OSC_STRING + 1);

        let mut limiter = OscLimiter::default();
        limiter.set_clipboard(true);
        assert!(limiter.filter(&stream).is_none(), "passes whole");

        // Other OSC strings keep the normal cap.
        let mut title = b"\x1b]2;".to_vec();
        title.extend_from_slice(&body);
        title.push(0x07);
        assert!(limiter.filter(&title).is_some());
    }
}
