//! URL detection in terminal text, and which links may be opened directly.
//!
//! [`find_urls`] finds `http`, `https`, `ftp`, `file` and `mailto` URLs in one row of text. A URL
//! runs until whitespace, a control character or a character that can't appear unencoded in a
//! URL (`<`, `>`, `"`, `` ` ``, `{`, `}`, `|`, `\`, `^`, typographic quotes and angle brackets,
//! and the ideographic and full-width punctuation of CJK text such as `、`, `。`, `，`, `（`).
//! Then the usual clean-up applies:
//!
//! - it ends before the first closing `)` or `]` that has no opening partner inside it, so
//!   `(see https://example.com)` works and `https://en.wikipedia.org/wiki/Rust_(language)`
//!   keeps its parentheses;
//! - trailing `.`, `,`, `;`, `:`, `!`, `?` and a dangling `(` or `[` are dropped;
//! - a trailing `'` is dropped when the URL is quoted (`'https://example.com'`) or has no
//!   earlier `'` to pair with.
//!
//! OSC 8 hyperlinks don't need detection; [`is_openable`] applies to both.

use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;

/// A scheme, then at least one URL character (the word-start check is done in code, because
/// the `regex` crate has no look-behind).
const URL_PATTERN: &str = concat!(
    r"(?i:https?://|ftp://|file:|mailto:)",
    // Anything but whitespace, C0, DEL, C1 and characters not allowed unencoded in a URL...
    r#"[^\s\x00-\x1F\x7F-\x9F<>"`{}|\\^"#,
    // ...typographic quotes and angle brackets: ‘ ’ “ ” ‹ › « » ⟨ ⟩...
    r"\u{2018}\u{2019}\u{201C}\u{201D}\u{2039}\u{203A}\u{AB}\u{BB}\u{27E8}\u{27E9}",
    // ...and CJK punctuation: 、 。 〈 〉 《 》 「 」 『 』 【 】 and full-width ！ （ ） ， ： ； ？
    r"\u{3001}\u{3002}\u{3008}-\u{3011}\u{FF01}\u{FF08}\u{FF09}\u{FF0C}\u{FF1A}\u{FF1B}\u{FF1F}",
    r"]+",
);

/// The compiled [`URL_PATTERN`]. `None` never happens in practice (a unit test compiles it);
/// if it did, no URL would be detected rather than the app aborting.
static URL_REGEX: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(URL_PATTERN).ok());

/// Finds the URLs in one row of terminal text. Returns their ranges as **character** indices
/// into `line` (Unicode scalar values, as `str::chars` counts them), in order, not
/// overlapping. The caller maps them to cells (a wide character is one `char` over two cells).
#[must_use]
pub fn find_urls(line: &str) -> Vec<Range<usize>> {
    let mut urls = Vec::new();
    // Cheap exit: every scheme needs a colon.
    if !line.contains(':') {
        return urls;
    }
    let Some(regex) = URL_REGEX.as_ref() else {
        return urls;
    };

    // Character index of `byte_position`, advanced as matches are found (one pass overall).
    let mut byte_position = 0;
    let mut char_position = 0;
    for found in regex.find_iter(line) {
        let before = line
            .get(..found.start())
            .unwrap_or_default()
            .chars()
            .next_back();
        // The scheme must start a word: "xhttp://" is not a URL (but "git+https://" has one).
        if before.is_some_and(|c| c.is_ascii_alphanumeric()) {
            continue;
        }
        let Some(length) = url_length(found.as_str(), before == Some('\'')) else {
            continue;
        };
        let text_before = line.get(byte_position..found.start()).unwrap_or_default();
        char_position += text_before.chars().count();
        let start = char_position;
        char_position += found
            .as_str()
            .get(..length)
            .unwrap_or_default()
            .chars()
            .count();
        byte_position = found.start() + length;
        urls.push(start..char_position);
    }
    urls
}

/// The byte length of the URL at the start of `candidate` after the clean-up rules, or `None`
/// when nothing but the scheme is left.
fn url_length(candidate: &str, quoted: bool) -> Option<usize> {
    // The scheme ("https://", "mailto:") is never trimmed.
    let colon = candidate.find(':')?;
    let scheme_end = if candidate.get(colon + 1..)?.starts_with("//") {
        colon + 3
    } else {
        colon + 1
    };

    // End before the first closing bracket without a partner.
    let mut end = candidate.len();
    let (mut parentheses, mut brackets) = (0usize, 0usize);
    for (index, c) in candidate
        .char_indices()
        .skip_while(|&(index, _)| index < scheme_end)
    {
        let depth = match c {
            '(' | ')' => &mut parentheses,
            '[' | ']' => &mut brackets,
            _ => continue,
        };
        if c == '(' || c == '[' {
            *depth += 1;
        } else if *depth == 0 {
            end = index;
            break;
        } else {
            *depth -= 1;
        }
    }

    // Drop trailing punctuation.
    let mut url = candidate.get(..end)?;
    while url.len() > scheme_end {
        let Some(last) = url.chars().next_back() else {
            break;
        };
        let drop = match last {
            '.' | ',' | ';' | ':' | '!' | '?' | '(' | '[' => true,
            '\'' => quoted || url.matches('\'').count() % 2 == 1,
            _ => false,
        };
        if !drop {
            break;
        }
        url = url.get(..url.len() - last.len_utf8())?;
    }

    let rest = url.get(scheme_end..)?;
    rest.chars().any(char::is_alphanumeric).then_some(url.len())
}

/// Whether a link (detected or from OSC 8) may be opened without asking: `http`, `https` and
/// `ftp` URLs with a host, and `mailto` URLs. Everything else returns false: `file:`, custom
/// schemes such as `opensesh:` (PLAN §8: they always need a confirmation, which comes in a later
/// sprint), `javascript:`, `data:`, and anything with whitespace or control characters.
#[must_use]
pub fn is_openable(url: &str) -> bool {
    if url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return false;
    }
    let Some((scheme, rest)) = url.split_once(':') else {
        return false;
    };
    let is = |name: &str| scheme.eq_ignore_ascii_case(name);
    if is("mailto") {
        return !rest.is_empty();
    }
    if is("http") || is("https") || is("ftp") {
        return rest
            .strip_prefix("//")
            .and_then(|authority| authority.split(['/', '?', '#']).next())
            .is_some_and(|host| !host.is_empty());
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The URLs `find_urls` finds in `line`, as strings.
    fn urls(line: &str) -> Vec<String> {
        let chars: Vec<char> = line.chars().collect();
        find_urls(line)
            .into_iter()
            .map(|range| chars[range].iter().collect())
            .collect()
    }

    /// Asserts that `line` holds exactly the URL `expected`.
    #[track_caller]
    fn one(line: &str, expected: &str) {
        assert_eq!(urls(line), [expected], "in {line:?}");
    }

    #[track_caller]
    fn none(line: &str) {
        assert_eq!(urls(line), Vec::<String>::new(), "in {line:?}");
    }

    #[test]
    fn pattern_compiles() {
        assert!(Regex::new(URL_PATTERN).is_ok());
        assert!(URL_REGEX.is_some());
    }

    #[test]
    fn schemes() {
        one("https://www.example.org", "https://www.example.org");
        one("http://example.org", "http://example.org");
        one(
            "ftp://ftp.example.org/pub/file.tar.gz",
            "ftp://ftp.example.org/pub/file.tar.gz",
        );
        one("file:///C:/Windows/", "file:///C:/Windows/");
        one("file:/home/user/whatever", "file:/home/user/whatever");
        one("mailto:someone@example.org", "mailto:someone@example.org");
        one("HTTPS://EXAMPLE.ORG/A", "HTTPS://EXAMPLE.ORG/A");
        one("MailTo:x@y.z", "MailTo:x@y.z");
        none("ssh://host");
        none("gopher://example.org");
        none("javascript:alert(1)");
        none("opensesh://connect/1");
    }

    #[test]
    fn not_urls() {
        none("");
        none("no links here");
        none("http::trace::on_request::log_parameters");
        none("http//www.example.org");
        none("/user:example.org");
        none("mailto: example@example.org");
        none("mailto:");
        none("https://");
        none("https:// example.org");
        none("https://.");
        none("https://...");
        none("(https://)");
        none("xhttps://example.org");
        none("1http://example.org");
    }

    #[test]
    fn surrounding_text() {
        one("see https://example.org.", "https://example.org");
        one("see https://example.org...", "https://example.org");
        one("at https://example.org, then", "https://example.org");
        one("go to https://example.org!", "https://example.org");
        one("really https://example.org?", "https://example.org");
        one("https://example.org: it works", "https://example.org");
        one("https://example.org;", "https://example.org");
        one("<https://example.org/a>", "https://example.org/a");
        one("\"https://example.org/a\"", "https://example.org/a");
        one("`https://example.org/a`", "https://example.org/a");
        one("{https://example.org/a}", "https://example.org/a");
        one(
            "\u{201c}https://example.org/a\u{201d}",
            "https://example.org/a",
        );
        one("\u{ab}https://example.org/a\u{bb}", "https://example.org/a");
        one(
            "curl https://example.org/i.sh|sh",
            "https://example.org/i.sh",
        );
        one("url=https://example.org/x&y=1", "https://example.org/x&y=1");
        one(
            "git+https://github.com/a/b.git",
            "https://github.com/a/b.git",
        );
        one("clone:https://example.org", "https://example.org");
        one("\thttps://example.org\t", "https://example.org");
        one("https://example.org\u{a0}next", "https://example.org");
    }

    #[test]
    fn brackets() {
        one(
            "https://en.wikipedia.org/wiki/Rust_(programming_language)",
            "https://en.wikipedia.org/wiki/Rust_(programming_language)",
        );
        one(
            "(see https://en.wikipedia.org/wiki/Rust_(programming_language))",
            "https://en.wikipedia.org/wiki/Rust_(programming_language)",
        );
        one(
            "https://en.wikipedia.org/wiki/Rust_(programming_language).",
            "https://en.wikipedia.org/wiki/Rust_(programming_language)",
        );
        one("(https://example.org)", "https://example.org");
        one("(https://example.org).", "https://example.org");
        one("(https://example.org/path/)", "https://example.org/path/");
        one("[https://example.org]", "https://example.org");
        one("[link](https://example.org/a)", "https://example.org/a");
        one("http://[::1]:8080/x", "http://[::1]:8080/x");
        one("[http://[::1]:8080/]", "http://[::1]:8080/");
        one("https://example.org/a[1]/b", "https://example.org/a[1]/b");
        one("https://example.org/a)b", "https://example.org/a");
        one("https://example.org/f(", "https://example.org/f");
        one("https://example.org/f?q=(a", "https://example.org/f?q=(a");
        one("https://example.org/((a))", "https://example.org/((a))");
        one("https://example.org/((a)))", "https://example.org/((a))");
    }

    #[test]
    fn quotes() {
        one("'https://example.org'", "https://example.org");
        one("'https://example.org/'.", "https://example.org/");
        one("url='https://example.org/x';", "https://example.org/x");
        one(
            "'https://example.org/O'Brien'",
            "https://example.org/O'Brien",
        );
        one("https://example.org/it's", "https://example.org/it's");
        one("https://example.org/it's.", "https://example.org/it's");
        one("https://example.org/x'", "https://example.org/x");
        one("https://example.org/'a'", "https://example.org/'a'");
    }

    #[test]
    fn several_urls_and_character_indices() {
        let line = "a https://one.example b (http://two.example) mailto:c@d.e";
        assert_eq!(
            urls(line),
            ["https://one.example", "http://two.example", "mailto:c@d.e"]
        );
        assert_eq!(find_urls(line), [2..21, 25..43, 45..57]);

        // Character indices, not bytes: "日本語 " is 4 characters and 10 bytes.
        let line = "\u{65e5}\u{672c}\u{8a9e} https://example.org/\u{65e5} \u{e9} ftp://f.example";
        assert_eq!(find_urls(line), [4..25, 28..43]);
        assert_eq!(
            urls(line),
            ["https://example.org/\u{65e5}", "ftp://f.example"]
        );

        // CJK text without a space before the URL still has a word start, and CJK punctuation
        // ends it.
        one(
            "\u{8bbf}\u{95ee}https://example.org\u{3002}",
            "https://example.org",
        );
        one(
            "\u{89c1}https://example.org/a\u{ff0c}\u{7136}\u{540e}",
            "https://example.org/a",
        );
        one(
            "\u{ff08}https://example.org/a\u{ff09}",
            "https://example.org/a",
        );
        one(
            "\u{300c}https://example.org/a\u{300d}",
            "https://example.org/a",
        );
        one("https://example.org/a\u{ff1f}", "https://example.org/a");
        one(
            "https://ja.wikipedia.org/wiki/\u{65e5}\u{672c}",
            "https://ja.wikipedia.org/wiki/\u{65e5}\u{672c}",
        );

        // A rejected candidate doesn't shift the indices of the next one.
        let line = "xhttp://no \u{e9} https://yes.example";
        assert_eq!(find_urls(line), vec![Range { start: 13, end: 32 }]);
    }

    #[test]
    fn long_lines() {
        let path = "a/".repeat(2000);
        let line = format!("{} https://example.org/{path} end", "\u{2500}".repeat(300));
        let found = find_urls(&line);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].start, 301);
        assert_eq!(found[0].len(), "https://example.org/".len() + path.len());
    }

    #[test]
    fn openable() {
        for url in [
            "http://example.org",
            "https://example.org/a?b=c#d",
            "HTTPS://EXAMPLE.ORG",
            "https://user@example.org:8443/",
            "http://[::1]:8080/",
            "ftp://ftp.example.org/pub",
            "mailto:someone@example.org",
            "mailto:?subject=hi",
            "https://example.org/\u{65e5}",
        ] {
            assert!(is_openable(url), "{url}");
        }
        for url in [
            "",
            "example.org",
            "file:///etc/passwd",
            "file:/home/user",
            "opensesh://connect/1",
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "vbscript:msgbox",
            "ssh://host",
            "smb://server/share",
            "mailto:",
            "https:",
            "https:/example.org",
            "https:example.org",
            "https://",
            "https:///etc/passwd",
            "https://?q",
            "https://#f",
            " https://example.org",
            "https://example.org/a b",
            "https://example.org/\n",
            "https://example.org/\u{1b}[31m",
            "https://example.org/\u{9b}",
            "https://example.org/\u{7f}",
        ] {
            assert!(!is_openable(url), "{url:?}");
        }
    }

    #[test]
    fn detected_urls_of_openable_schemes_are_openable() {
        let line = "https://a.example/x http://b.example ftp://c.example mailto:d@e.f file:///g";
        let chars: Vec<char> = line.chars().collect();
        let openable: Vec<bool> = find_urls(line)
            .into_iter()
            .map(|range| is_openable(&chars[range].iter().collect::<String>()))
            .collect();
        assert_eq!(openable, [true, true, true, true, false]);
    }
}
