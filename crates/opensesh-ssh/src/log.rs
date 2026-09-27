//! Session logs (PLAN Sprint 7): what a session printed, appended to a file, either raw (escape
//! sequences and all) or as clean text (sequences removed, carriage returns folded).
//!
//! Writing happens on its own thread so a slow disk never slows the session. Only what the
//! server printed is logged, never what was typed (passwords typed at a remote `sudo` prompt are
//! not echoed by the server, so they don't reach the log either).

use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::Path;

use crossbeam_channel::Sender;

/// Removes terminal escape sequences from a byte stream, keeping text, tabs and newlines.
#[derive(Debug, Default)]
pub struct Cleaner {
    state: State,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum State {
    #[default]
    Text,
    /// After ESC.
    Escape,
    /// Inside `ESC [` (CSI) until a final byte.
    Csi,
    /// Inside `ESC ]`, `ESC P`, `ESC _`, `ESC ^` or `ESC X` (strings) until BEL or ST.
    String,
    /// ESC seen inside a string (maybe the start of ST).
    StringEscape,
    /// One more byte to skip (`ESC (`, `ESC )` and similar take one argument).
    SkipOne,
}

impl Cleaner {
    /// The text in `bytes`, continuing sequences split across calls.
    pub fn clean(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(bytes.len());
        for &byte in bytes {
            self.state = match (self.state, byte) {
                (State::Text, 0x1b) => State::Escape,
                (State::Text, b'\r') => State::Text,
                (State::Text, b'\n' | b'\t') => {
                    out.push(byte);
                    State::Text
                }
                (State::Text, 0x08) => {
                    // Backspace erases what was just printed.
                    out.pop();
                    State::Text
                }
                (State::Text, byte) if byte < 0x20 || byte == 0x7f => State::Text,
                (State::Text, byte) => {
                    out.push(byte);
                    State::Text
                }
                (State::Escape, b'[') => State::Csi,
                (State::Escape, b']' | b'P' | b'_' | b'^' | b'X') => State::String,
                (State::Escape, b'(' | b')' | b'*' | b'+' | b'#' | b'%') => State::SkipOne,
                (State::Escape, _) => State::Text,
                (State::Csi, 0x40..=0x7e) => State::Text,
                (State::Csi, _) => State::Csi,
                (State::String, 0x07) => State::Text,
                (State::String, 0x1b) => State::StringEscape,
                (State::String, _) => State::String,
                (State::StringEscape, b'\\') => State::Text,
                (State::StringEscape, _) => State::String,
                (State::SkipOne, _) => State::Text,
            };
        }
        out
    }
}

/// An open session log. Dropping it finishes writing in the background.
#[derive(Debug)]
pub struct SessionLog {
    sender: Sender<Vec<u8>>,
}

impl SessionLog {
    /// Opens `path` for appending (creating folders) and starts the writer thread.
    ///
    /// # Errors
    ///
    /// When the file can't be opened or the thread can't start.
    pub fn open(path: &Path, raw: bool) -> std::io::Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        let (sender, receiver) = crossbeam_channel::unbounded::<Vec<u8>>();
        std::thread::Builder::new()
            .name("session-log".to_owned())
            .spawn(move || {
                let mut cleaner = Cleaner::default();
                for chunk in receiver {
                    let data = if raw { chunk } else { cleaner.clean(&chunk) };
                    if file.write_all(&data).is_err() {
                        break;
                    }
                }
                let _ = file.flush();
            })?;
        Ok(Self { sender })
    }

    /// Appends what the session printed.
    pub fn write(&self, bytes: &[u8]) {
        let _ = self.sender.send(bytes.to_vec());
    }
}

/// `YYYY-MM-DD_HH-MM-SS` (UTC) for `seconds` since the Unix epoch, for log file names.
#[must_use]
pub fn timestamp(seconds: u64) -> String {
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    let rest = seconds % 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}_{:02}-{:02}-{:02}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_sequences_go() {
        let mut cleaner = Cleaner::default();
        let text = cleaner.clean(
            b"\x1b[1;32muser@host\x1b[0m:\x1b]0;title\x07~$ ls\r\n\x1b[?2004hfile\ta\x08b\r\n",
        );
        assert_eq!(
            String::from_utf8(text).unwrap(),
            "user@host:~$ ls\nfile\tb\n"
        );
        // A sequence split across reads.
        let mut cleaner = Cleaner::default();
        let mut text = cleaner.clean(b"x\x1b[3");
        text.extend(cleaner.clean(b"1mred\x1b]8;;http://a\x1b\\link"));
        assert_eq!(String::from_utf8(text).unwrap(), "xredlink");
        // UTF-8 passes through.
        assert_eq!(Cleaner::default().clean("ñ €".as_bytes()), "ñ €".as_bytes());
    }

    #[test]
    fn timestamps() {
        assert_eq!(timestamp(0), "1970-01-01_00-00-00");
        assert_eq!(timestamp(1_727_398_800), "2024-09-27_01-00-00");
        assert_eq!(timestamp(951_782_400), "2000-02-29_00-00-00");
    }

    #[test]
    fn logs_append() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("logs").join("session.log");
        let log = SessionLog::open(&path, false).unwrap();
        log.write(b"\x1b[1mhello\x1b[0m\r\n");
        drop(log);
        // The writer finishes on its own thread.
        for _ in 0..100 {
            if std::fs::read_to_string(&path).is_ok_and(|text| text == "hello\n") {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!(
            "the log was not written: {:?}",
            std::fs::read_to_string(&path)
        );
    }
}
