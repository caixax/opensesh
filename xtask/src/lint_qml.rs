//! `cargo xtask lint-qml`: static checks for the QML sources (PLAN §10).
//!
//! * No color literals outside `Theme.qml`: hex colors (`"#RRGGBB"`), named colors
//!   (`color: "red"`, `rect.color = "red"`, `property color accent: "red"`) and `Qt.rgba()` /
//!   `Qt.hsla()` / `Qt.hsva()` / `Qt.color()` calls.
//! * No user-visible string literal without `qsTr()`: properties such as `text`, `title` or
//!   `Accessible.name` must not be bound or assigned a bare, non-empty string literal.
//!
//! The checks are line-based heuristics. A line can opt out with a trailing
//! `// lint-qml: allow` comment, which must be justified in review.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{Context, Result};
use regex::Regex;

/// Directory scanned by default: `<workspace>/crates/opensesh-app/qml`.
#[must_use]
pub fn default_qml_root(workspace: &Path) -> PathBuf {
    ["crates", "opensesh-app", "qml"]
        .iter()
        .fold(workspace.to_path_buf(), |path, part| path.join(part))
}

/// File allowed to define colors.
const THEME_FILE: &str = "Theme.qml";

/// Marker that silences the linter on one line.
const ALLOW_MARKER: &str = "lint-qml: allow";

/// A single lint violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// File that contains the violation.
    pub path: PathBuf,
    /// 1-based line number.
    pub line: usize,
    /// Human-readable description.
    pub message: String,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.path.display(), self.line, self.message)
    }
}

/// Compiles a hardcoded pattern. Every pattern is exercised by the unit tests below, so a typo
/// fails `cargo test` instead of the linter at run time.
#[allow(clippy::expect_used)]
fn static_regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("hardcoded regex must compile")
}

static HEX_COLOR: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(r#"["']#(?:[0-9A-Fa-f]{3}|[0-9A-Fa-f]{4}|[0-9A-Fa-f]{6}|[0-9A-Fa-f]{8})["']"#)
});

static COLOR_FUNCTION: LazyLock<Regex> =
    LazyLock::new(|| static_regex(r"\bQt\.(?:rgba|hsla|hsva|color|lighter|darker|tint)\s*\("));

// Bindings (`color: "red"`) and JavaScript assignments (`rect.color = "red"`). A comparison
// (`==`, `===`) never matches: its second `=` is neither whitespace nor a quote.
static NAMED_COLOR: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(r#"(?i)\b(?:[a-z]+\.)*(?:color|[a-z]*Color)\s*(?::|=)\s*["']([a-z]+)["']"#)
});

// Typed declarations such as `readonly property color accent: "red"`.
static TYPED_COLOR: LazyLock<Regex> =
    LazyLock::new(|| static_regex(r#"\bproperty\s+color\s+\w+\s*(?::|=)\s*["']((?i:[a-z]+))["']"#));

// Ends with the opening quote of the literal, so the caller can tell `""` apart.
static UNTRANSLATED: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(
        r#"\b(?:text|title|placeholderText|label|description|tooltip|ToolTip\.text|Accessible\.name|Accessible\.description|displayText|informativeText|detailedText)\s*(?::|=)\s*["'`]"#,
    )
});

/// Checks one QML source. `path` is only used for reporting and for the `Theme.qml` exemption.
#[must_use]
pub fn lint_source(path: &Path, source: &str) -> Vec<Finding> {
    let is_theme = path.file_name().is_some_and(|name| name == THEME_FILE);
    let mut findings = Vec::new();
    let mut in_block_comment = false;

    for (index, raw_line) in source.lines().enumerate() {
        let line = strip_comments(raw_line, &mut in_block_comment);
        if line.trim().is_empty() || raw_line.contains(ALLOW_MARKER) {
            continue;
        }
        let mut report = |message: String| {
            findings.push(Finding {
                path: path.to_path_buf(),
                line: index + 1,
                message,
            });
        };

        if !is_theme {
            if HEX_COLOR.is_match(&line) {
                report("hardcoded hex color; use a Theme token".to_owned());
            }
            if COLOR_FUNCTION.is_match(&line) {
                report("color computed in QML; expose it as a Theme token".to_owned());
            }
            if let Some(name) = NAMED_COLOR
                .captures_iter(&line)
                .chain(TYPED_COLOR.captures_iter(&line))
                .filter_map(|c| c.get(1))
                .find(|name| name.as_str() != "transparent")
            {
                report(format!(
                    "hardcoded named color \"{}\"; use a Theme token",
                    name.as_str()
                ));
            }
        }
        if has_untranslated_literal(&line) {
            report("user-visible string literal without qsTr()".to_owned());
        }
    }
    findings
}

/// True if a user-visible property is set to a non-empty string literal. Each match is judged on
/// its own, so `title: ""; text: "Save"` is still reported.
fn has_untranslated_literal(line: &str) -> bool {
    UNTRANSLATED.find_iter(line).any(|found| {
        // The match ends with the opening quote; the same quote right after it means `""`.
        let quote = found.as_str().chars().next_back();
        let rest = line.get(found.end()..).unwrap_or_default();
        quote.is_some_and(|quote| !rest.starts_with(quote))
    })
}

/// Removes `//` and `/* */` comments from a line, keeping track of multi-line block comments.
/// String literals are respected, so `"http://example.org"` is not treated as a comment.
fn strip_comments(line: &str, in_block_comment: &mut bool) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    let mut quote: Option<char> = None;

    while let Some(c) = chars.next() {
        if *in_block_comment {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                *in_block_comment = false;
            }
            continue;
        }
        match quote {
            Some(q) => {
                out.push(c);
                if c == '\\' {
                    if let Some(escaped) = chars.next() {
                        out.push(escaped);
                    }
                } else if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' | '`' => {
                    quote = Some(c);
                    out.push(c);
                }
                '/' if chars.peek() == Some(&'/') => break,
                '/' if chars.peek() == Some(&'*') => {
                    chars.next();
                    *in_block_comment = true;
                }
                _ => out.push(c),
            },
        }
    }
    out
}

/// Lints every `.qml` file under `root` (recursively).
///
/// # Errors
///
/// Fails if a directory or file can't be read.
pub fn lint_dir(root: &Path) -> Result<Vec<Finding>> {
    let mut files = Vec::new();
    collect_qml_files(root, &mut files)?;
    files.sort();

    let mut findings = Vec::new();
    for file in files {
        let source = std::fs::read_to_string(&file)
            .with_context(|| format!("reading {}", file.display()))?;
        findings.extend(lint_source(&file, &source));
    }
    Ok(findings)
}

fn collect_qml_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries =
        std::fs::read_dir(dir).with_context(|| format!("reading directory {}", dir.display()))?;
    for entry in entries {
        let path = entry
            .with_context(|| format!("reading directory {}", dir.display()))?
            .path();
        if path.is_dir() {
            collect_qml_files(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "qml") {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lint(source: &str) -> Vec<String> {
        lint_source(Path::new("View.qml"), source)
            .into_iter()
            .map(|f| format!("{}: {}", f.line, f.message))
            .collect()
    }

    #[test]
    fn clean_file_has_no_findings() {
        let source = r##"
import QtQuick
import QtQuick.Controls

Button {
    text: qsTr("Open sesame")
    Accessible.name: qsTr("Knock on the door")
    color: Theme.accent
    title: ""
    objectName: "knockButton"
    source: "qrc:/icons/door-open.svg"
    // text: "commented out"
    /* color: "#ff0000" */
    readonly property url docs: "https://example.org/#fff"
    background.color: "transparent"
}
"##;
        assert_eq!(lint(source), Vec::<String>::new());
    }

    #[test]
    fn hex_colors_are_rejected() {
        assert_eq!(lint(r##"color: "#E6B450""##).len(), 1);
        assert_eq!(lint("color: '#fff'").len(), 1);
        assert_eq!(lint(r##"border.color: "#80FFFFFF""##).len(), 1);
    }

    #[test]
    fn named_colors_and_color_functions_are_rejected() {
        assert_eq!(lint(r#"color: "red""#).len(), 1);
        assert_eq!(lint(r#"selectionColor: "Blue""#).len(), 1);
        assert_eq!(lint("color: Qt.rgba(1, 0, 0, 1)").len(), 1);
        assert_eq!(lint("color: Qt.darker(Theme.accent, 1.2)").len(), 1);
    }

    #[test]
    fn typed_color_declarations_are_rejected() {
        assert_eq!(
            lint(r#"property color accent: "red""#),
            vec![r#"1: hardcoded named color "red"; use a Theme token"#]
        );
        assert_eq!(lint(r#"readonly property color bg: "white""#).len(), 1);
        assert_eq!(lint(r#"property   color  bg :  'Navy'"#).len(), 1);
        // A name that already ends in `Color` is reported once, not once per pattern.
        assert_eq!(lint(r#"property color accentColor: "red""#).len(), 1);
        assert!(lint(r#"property color overlay: "transparent""#).is_empty());
        assert!(lint("property color accent: Theme.accent").is_empty());
        assert!(lint(r#"property string label: qsTr("red")"#).is_empty());
    }

    #[test]
    fn color_assignments_are_rejected_but_comparisons_are_not() {
        assert_eq!(lint(r#"onClicked: rect.color = "red""#).len(), 1);
        assert_eq!(lint(r#"onClicked: rect.color='red'"#).len(), 1);
        assert_eq!(lint(r#"onClicked: border.color = "Blue""#).len(), 1);
        assert!(lint(r#"visible: rect.color == "red""#).is_empty());
        assert!(lint(r#"visible: rect.color === "red""#).is_empty());
        assert!(lint(r#"visible: rect.color=="red""#).is_empty());
        assert!(lint(r#"visible: rect.color != "red""#).is_empty());
        assert!(lint(r#"onClicked: rect.color = "transparent""#).is_empty());
    }

    #[test]
    fn every_named_color_on_a_line_is_checked() {
        assert!(lint(r#"color: "transparent""#).is_empty());
        assert_eq!(
            lint(r#"color: "transparent"; border.color: "red""#),
            vec![r#"1: hardcoded named color "red"; use a Theme token"#]
        );
    }

    #[test]
    fn untranslated_strings_are_rejected() {
        assert_eq!(lint(r#"text: "Hello""#).len(), 1);
        assert_eq!(lint(r#"text: "Count: " + count"#).len(), 1);
        assert_eq!(lint(r#"Accessible.name: "Close""#).len(), 1);
        assert_eq!(lint(r#"ToolTip.text: 'Copy'"#).len(), 1);
        assert_eq!(lint(r#"placeholderText: `Search`"#).len(), 1);
    }

    #[test]
    fn untranslated_assignments_are_rejected_but_comparisons_are_not() {
        assert_eq!(lint(r#"onClicked: status.text = "Saved""#).len(), 1);
        assert_eq!(lint(r#"onClicked: dialog.title='Error'"#).len(), 1);
        assert_eq!(
            lint(r#"onClicked: field.placeholderText = `Search`"#).len(),
            1
        );
        assert!(lint(r#"visible: status.text == "Saved""#).is_empty());
        assert!(lint(r#"visible: status.text === "Saved""#).is_empty());
        assert!(lint(r#"visible: status.text==="Saved""#).is_empty());
        assert!(lint(r#"visible: status.text !== "Saved""#).is_empty());
        assert!(lint(r#"onClicked: status.text = qsTr("Saved")"#).is_empty());
    }

    #[test]
    fn empty_strings_are_judged_per_property() {
        assert!(lint(r#"title: """#).is_empty());
        assert!(lint("title: ''").is_empty());
        assert!(lint("title: ``").is_empty());
        assert!(lint(r#"onClicked: status.text = """#).is_empty());
        assert!(lint(r#"title: ""; text: qsTr("Save")"#).is_empty());
        assert_eq!(
            lint(r#"title: ""; text: "Save""#),
            vec!["1: user-visible string literal without qsTr()"]
        );
        assert_eq!(lint(r#"title: ''; text: 'Save'"#).len(), 1);
        assert_eq!(lint(r#"text: "Save"; title: """#).len(), 1);
    }

    #[test]
    fn allow_marker_silences_a_line() {
        assert!(lint(r#"text: "OpenSesh" // lint-qml: allow (brand name)"#).is_empty());
    }

    #[test]
    fn theme_file_may_define_colors_but_not_untranslated_text() {
        let source = "readonly property color accent: \"#E6B450\"\ntext: \"x\"";
        let findings = lint_source(Path::new("qml/Theme.qml"), source);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 2);
    }

    #[test]
    fn multi_line_block_comments_are_ignored() {
        let source = "/*\ncolor: \"#fff\"\ntext: \"x\"\n*/\ntext: qsTr(\"ok\")";
        assert!(lint(source).is_empty());
    }

    #[test]
    fn line_numbers_are_one_based() {
        let findings = lint("\n\ntext: \"x\"");
        assert_eq!(
            findings,
            vec!["3: user-visible string literal without qsTr()"]
        );
    }
}
