//! The sample shown by the live preview in Settings > Terminal (PLAN §6.2): `ls --color`, a
//! `git diff`, a powerline prompt, CJK and emoji, text styles, the 16 colors and a log line for
//! keyword highlighting. It is played through a replay backend, so the preview runs the real
//! engine and renderer with the profile being edited.

/// The sample, as a program would write it (CRLF line ends: there is no TTY to add the CR).
pub const SAMPLE: &str = concat!(
    "\x1b[1;32muser@devbox\x1b[0m:\x1b[1;34m~/src/opensesh\x1b[0m$ ls --color\r\n",
    "\x1b[1;34mcrates\x1b[0m  \x1b[1;34mdocs\x1b[0m  \x1b[1;32mrelease.sh\x1b[0m  ",
    "Cargo.toml  README.md  \x1b[1;36mlatest\x1b[0m\r\n",
    "\x1b[1;32muser@devbox\x1b[0m:\x1b[1;34m~/src/opensesh\x1b[0m$ git diff\r\n",
    "\x1b[1mdiff --git a/src/main.rs b/src/main.rs\x1b[0m\r\n",
    "\x1b[36m@@ -1,3 +1,3 @@\x1b[0m\r\n",
    " fn main() -> io::Result<()> {\r\n",
    "\x1b[31m-    println!(\"Hello\");\x1b[0m\r\n",
    "\x1b[32m+    println!(\"Hello, OpenSesh!\"); // a != b && c >= d\x1b[0m\r\n",
    " }\r\n",
    "\x1b[30;44m ~/src \x1b[34;42m\u{e0b0}\x1b[30;42m \u{e0a0} main \x1b[32;49m\u{e0b0}\x1b[0m ",
    "\x1b[2mdim\x1b[0m \x1b[1mbold\x1b[0m \x1b[3mitalic\x1b[0m \x1b[4munderline\x1b[0m ",
    "\x1b[4:3;58:5:1mcurly\x1b[0m \x1b[9mstrike\x1b[0m\r\n",
    "日本語のテキスト  한국어  中文  \u{1f680} \u{2728} \u{1f427}\r\n",
    "\x1b[40m  \x1b[41m  \x1b[42m  \x1b[43m  \x1b[44m  \x1b[45m  \x1b[46m  \x1b[47m  ",
    "\x1b[100m  \x1b[101m  \x1b[102m  \x1b[103m  \x1b[104m  \x1b[105m  \x1b[106m  \x1b[107m  \x1b[0m\r\n",
    "2026-09-26 12:00:01 ERROR connection refused from 10.0.0.12\r\n",
    "\x1b[1;32muser@devbox\x1b[0m:\x1b[1;34m~/src/opensesh\x1b[0m$ ",
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_has_what_the_plan_asks_for() {
        for part in [
            "ls --color",
            "git diff",
            "\u{e0b0}",
            "日本語",
            "\u{1f680}",
            "ERROR",
        ] {
            assert!(SAMPLE.contains(part), "{part}");
        }
        assert!(!SAMPLE.contains("\n\x1b") || SAMPLE.contains("\r\n"));
        assert!(
            SAMPLE
                .lines()
                .all(|line| line.ends_with('\r') || !line.contains('\r')),
            "CRLF line ends"
        );
    }
}
