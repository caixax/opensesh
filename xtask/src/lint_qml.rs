//! `cargo xtask lint-qml`: static checks for the QML sources (PLAN §10).
//!
//! * No color literals outside `Theme.qml`: hex colors (`"#RRGGBB"`), named colors
//!   (`color: "red"`, `rect.color = "red"`, `property color accent: "red"`) and `Qt.rgba()` /
//!   `Qt.hsla()` / `Qt.hsva()` / `Qt.color()` / `Qt.lighter()` / `Qt.darker()` / `Qt.tint()` /
//!   `Qt.alpha()` calls.
//! * No user-visible string literal without `qsTr()`: Qt's text properties (`text`, `title`,
//!   `placeholderText`, `ToolTip.text`, `Accessible.name`, ...) and the `Os*` library's
//!   (`toolTip`, `helpText`, `errorText`, `subtitle`, `caption`, `message`, `category`,
//!   `acceptText`, ...) must not be bound or assigned a non-empty string literal, whether it is the
//!   whole value, a `?:` branch, a `||` fallback or part of a `+` concatenation. The same goes for
//!   the text of `Toasts.show()` and the entries of the text lists and maps `labels` and
//!   `presetNames`, even when they span several lines.
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

static COLOR_FUNCTION: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(r"\bQt\.(?:rgba|hsla|hsva|color|lighter|darker|tint|alpha)\s*\(")
});

// Bindings (`color: "red"`) and JavaScript assignments (`rect.color = "red"`). A comparison
// (`==`, `===`) never matches: its second `=` is neither whitespace nor a quote.
static NAMED_COLOR: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(r#"(?i)\b(?:[a-z]+\.)*(?:color|[a-z]*Color)\s*(?::|=)\s*["']([a-z]+)["']"#)
});

// Typed declarations such as `readonly property color accent: "red"`.
static TYPED_COLOR: LazyLock<Regex> =
    LazyLock::new(|| static_regex(r#"\bproperty\s+color\s+\w+\s*(?::|=)\s*["']((?i:[a-z]+))["']"#));

// A user-visible property followed by its binding colon or assignment `=`; the caller judges the
// value after it. Qt's own properties (`text`, `title`, `ToolTip.text`, `Accessible.name`,
// `Accessible.description`), the Os* library's (`subtitle`, `caption`, `message`, `category`,
// `accessibleName`) and every camelCase name ending in `Text`, `Title`, `Label`, `Tip`, `Caption`,
// `Message` or `Description` (`placeholderText`, `toolTip`, `helpText`, `errorText`, `acceptText`,
// `rejectText`, `actionText`, `trailingText`, `shortcutText`, ...), so new component properties
// that follow the naming are covered too. Case-sensitive, so `iconName`, `objectName`, `textRole`
// or `shortcut` (a portable key sequence) don't match.
static UI_PROPERTY: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(
        r"\b(?:text|title|subtitle|label|caption|message|description|category|tooltip|accessibleName|Accessible\.name|[a-z][A-Za-z0-9]*(?:Text|Title|Label|Tip|Caption|Message|Description))\s*(?::|=)",
    )
});

// The app's notification API: its first argument is shown to the user.
static TOAST_CALL: LazyLock<Regex> = LazyLock::new(|| static_regex(r"\bToasts\.show\s*\("));

// Properties whose value is a list or map of user-visible strings, often over several lines:
// SettingsChoice's `labels: ({ dark: qsTr("Dark") })` and OsColorPicker's
// `presetNames: [qsTr("Amber")]`.
static TEXT_COLLECTION: LazyLock<Regex> =
    LazyLock::new(|| static_regex(r"\b(?:labels|presetNames)\s*(?::|=)"));

/// Checks one QML source. `path` is only used for reporting and for the `Theme.qml` exemption.
#[must_use]
pub fn lint_source(path: &Path, source: &str) -> Vec<Finding> {
    let is_theme = path.file_name().is_some_and(|name| name == THEME_FILE);
    let mut findings = Vec::new();
    let mut in_block_comment = false;
    let mut collection: Option<CollectionScan> = None;

    for (index, raw_line) in source.lines().enumerate() {
        let line = strip_comments(raw_line, &mut in_block_comment);
        if line.trim().is_empty() {
            continue;
        }
        // Scanned before the allow marker is honored, so a collection keeps its state.
        let untranslated_entry = has_untranslated_entry(&line, &mut collection);
        if raw_line.contains(ALLOW_MARKER) {
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
        if untranslated_entry || has_untranslated_literal(&line) {
            report("user-visible string literal without qsTr()".to_owned());
        }
    }
    findings
}

/// True if a user-visible property, or the text of `Toasts.show()`, gets a non-empty string
/// literal. Each match is judged on its own, so `title: ""; text: "Save"` is still reported.
fn has_untranslated_literal(line: &str) -> bool {
    // Words inside a string (`qsTr("Error message: ")`) are text, not code.
    let in_code = |found: &regex::Match<'_>| !inside_string(line, found.start());
    let value = |found: regex::Match<'_>| line.get(found.end()..).unwrap_or_default();
    let property = UI_PROPERTY
        .find_iter(line)
        .filter(in_code)
        .map(value)
        // `text == "x"` / `text === "x"` is a comparison, not an assignment.
        .any(|value| !value.starts_with('=') && shows_literal(value));
    property
        || TOAST_CALL
            .find_iter(line)
            .filter(in_code)
            .map(value)
            .any(shows_literal)
}

/// An open bracket in the value of a text collection property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bracket {
    /// A grouping parenthesis, which doesn't change what the value shows.
    Group,
    /// A call or an index (`qsTr(`, `map[`): the literals in it are not shown as they are.
    Call,
    Array,
    Object,
}

/// State of a text collection value (`labels: ({ ... })`) that goes on over several lines.
#[derive(Debug, Default)]
struct CollectionScan {
    brackets: Vec<Bracket>,
    /// Last significant character outside string literals, across lines.
    previous: Option<char>,
}

impl CollectionScan {
    /// Scans `chars`, the next piece of the value. Returns whether it shows an untranslated
    /// entry, and the index where the value ended (`None` if it goes on to the next line).
    fn feed(&mut self, chars: &[char]) -> (bool, Option<usize>) {
        let mut found = false;
        let mut index = 0;
        while let Some(&c) = chars.get(index) {
            match c {
                '"' | '\'' | '`' => {
                    let end = closing_quote(chars, index);
                    let next = chars
                        .get(end + 1..)
                        .and_then(|after| after.iter().copied().find(|c| !c.is_whitespace()));
                    found |= end > index + 1 && self.shows_entry(next);
                    self.previous = Some(c);
                    index = end + 1;
                    continue;
                }
                '(' | '[' if follows_operand(self.previous) => self.brackets.push(Bracket::Call),
                '(' => self.brackets.push(Bracket::Group),
                '[' => self.brackets.push(Bracket::Array),
                '{' => self.brackets.push(Bracket::Object),
                ')' | ']' | '}' => {
                    if self.brackets.pop().is_none() || self.brackets.is_empty() {
                        return (found, Some(index + 1));
                    }
                }
                ',' | ';' if self.brackets.is_empty() => return (found, Some(index)),
                _ => {}
            }
            if !c.is_whitespace() {
                self.previous = Some(c);
            }
            index += 1;
        }
        (found, self.brackets.is_empty().then_some(chars.len()))
    }

    /// True if a non-empty literal followed by `next` is an entry of the collection itself (not
    /// of a nested array or object, a call, an object key or a comparison), or a `?:` branch,
    /// fallback or concatenated part of one.
    fn shows_entry(&self, next: Option<char>) -> bool {
        let containers = self
            .brackets
            .iter()
            .filter(|bracket| matches!(bracket, Bracket::Array | Bracket::Object))
            .count();
        if containers != 1
            || self.brackets.contains(&Bracket::Call)
            || matches!(next, Some('=' | '!' | '<' | '>'))
        {
            return false;
        }
        let innermost = self
            .brackets
            .iter()
            .rev()
            .find(|bracket| **bracket != Bracket::Group);
        let is_value = match innermost {
            // `{ "dark": ... }` quotes a key.
            Some(Bracket::Object) => {
                matches!(self.previous, Some('(' | '?' | ':' | '+' | '|' | '&'))
            }
            _ => matches!(
                self.previous,
                Some('[' | ',' | '(' | '?' | ':' | '+' | '|' | '&')
            ),
        };
        is_value || next == Some('+')
    }
}

/// Feeds `line` to the text collection value being scanned, and starts scanning the value of
/// every `labels:` / `presetNames:` binding the line opens. True if the line gives one of them
/// an untranslated entry.
fn has_untranslated_entry(line: &str, collection: &mut Option<CollectionScan>) -> bool {
    let mut rest: Vec<char> = line.chars().collect();
    let mut found = false;
    loop {
        if collection.is_none() {
            let text: String = rest.iter().collect();
            let Some(start) = TEXT_COLLECTION
                .find_iter(&text)
                .find(|start| !inside_string(&text, start.start()))
            else {
                return found;
            };
            let value = text.get(start.end()..).unwrap_or_default();
            rest = value.chars().collect();
            // `labels === other` is a comparison.
            if value.starts_with('=') {
                continue;
            }
            *collection = Some(CollectionScan::default());
        }
        let Some(scan) = collection.as_mut() else {
            return found;
        };
        let (entry, end) = scan.feed(&rest);
        found |= entry;
        let Some(end) = end else {
            return found;
        };
        *collection = None;
        rest = rest.get(end..).unwrap_or_default().to_vec();
    }
}

/// True if a bracket right after `previous` belongs to a call or an index (`fn(`, `map[`) rather
/// than opening a group, an array or an object.
fn follows_operand(previous: Option<char>) -> bool {
    previous.is_some_and(|p| p.is_alphanumeric() || matches!(p, '_' | '$' | ')' | ']'))
}

/// True if byte offset `pos` of `line` falls inside a string literal.
fn inside_string(line: &str, pos: usize) -> bool {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, c) in line.char_indices() {
        if index >= pos {
            break;
        }
        match quote {
            Some(_) if escaped => escaped = false,
            Some(_) if c == '\\' => escaped = true,
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if matches!(c, '"' | '\'' | '`') => quote = Some(c),
            None => {}
        }
    }
    quote.is_some()
}

/// True if the expression at the start of `expr` can evaluate to a non-empty string literal that
/// it spells out: the literal is the whole value (`"Save"`), a `?:` branch (`busy ? "Wait" : x`),
/// a `||` / `??` / `&&` operand (`name || "Untitled"`) or part of a concatenation
/// (`n + " hosts"`), also inside grouping parentheses. Literals inside calls, arrays and objects
/// (`qsTr("Save")`, `fn("id")`, `{ kind: "info" }`) and in comparisons (`kind === "dir" ? a : b`)
/// are not judged. The expression ends at a top-level `,` or `;`, or at a bracket it didn't open.
fn shows_literal(expr: &str) -> bool {
    let chars: Vec<char> = expr.chars().collect();
    // One entry per open bracket: true for a grouping parenthesis, whose value is the
    // expression's value; false for a call, an array or an object.
    let mut brackets: Vec<bool> = Vec::new();
    // Last significant character outside string literals; `None` at the start.
    let mut previous: Option<char> = None;
    let mut index = 0;

    while let Some(&c) = chars.get(index) {
        match c {
            '"' | '\'' | '`' => {
                let end = closing_quote(&chars, index);
                let judged = brackets.iter().all(|grouping| *grouping);
                let empty = end == index + 1;
                let next = chars
                    .get(end + 1..)
                    .and_then(|after| after.iter().find(|c| !c.is_whitespace()));
                // `"dir" === kind` compares the literal instead of showing it.
                let compared = matches!(next, Some('=' | '!' | '<' | '>'));
                let is_value = matches!(previous, None | Some('(' | '?' | ':' | '+' | '|' | '&'));
                if judged && !empty && !compared && (is_value || next == Some(&'+')) {
                    return true;
                }
                previous = Some(c);
                index = end + 1;
                continue;
            }
            '(' => brackets.push(!follows_operand(previous)),
            '[' | '{' => brackets.push(false),
            ')' | ']' | '}' => {
                if brackets.pop().is_none() {
                    return false;
                }
            }
            ',' | ';' if brackets.is_empty() => return false,
            _ => {}
        }
        if !c.is_whitespace() {
            previous = Some(c);
        }
        index += 1;
    }
    false
}

/// Index of the quote that closes the string literal opening at `start`, skipping escaped
/// characters; the last index if the literal doesn't end on this line.
fn closing_quote(chars: &[char], start: usize) -> usize {
    let quote = chars.get(start).copied();
    let mut index = start + 1;
    while let Some(&c) = chars.get(index) {
        if c == '\\' {
            index += 2;
            continue;
        }
        if Some(c) == quote {
            return index;
        }
        index += 1;
    }
    chars.len().saturating_sub(1).max(start)
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
        assert_eq!(lint("color: Qt.alpha(Theme.text, 0.4)").len(), 1);
        assert_eq!(lint("border.color: Qt.alpha (Theme.accent, 0.5)").len(), 1);
        assert!(lint("opacity: control.alpha(0.4)").is_empty());
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
    fn component_text_properties_are_checked() {
        for property in [
            "toolTip",
            "helpText",
            "errorText",
            "subtitle",
            "caption",
            "message",
            "category",
            "acceptText",
            "rejectText",
            "actionText",
            "trailingText",
            "shortcutText",
            "toastActionText",
            "accessibleName",
            "Accessible.description",
            "displayText",
            "informativeText",
            "detailedText",
            "windowTitle",
            "emptyLabel",
            "hintMessage",
            "tooltip",
        ] {
            assert_eq!(
                lint(&format!(r#"{property}: "Close""#)).len(),
                1,
                "{property}"
            );
            assert_eq!(
                lint(&format!("row.{property} = 'Close'")).len(),
                1,
                "{property}"
            );
            assert!(
                lint(&format!(r#"{property}: qsTr("Close")"#)).is_empty(),
                "{property}"
            );
            assert!(lint(&format!(r#"{property}: """#)).is_empty(), "{property}");
        }
        assert_eq!(
            lint(r#"OsIconButton { iconName: "x"; toolTip: "Close" }"#),
            vec!["1: user-visible string literal without qsTr()"]
        );
        assert_eq!(lint(r#"property string acceptText: "OK""#).len(), 1);
        assert!(lint(r#"property string acceptText: qsTr("OK")"#).is_empty());
    }

    #[test]
    fn non_text_properties_are_not_checked() {
        for line in [
            r#"iconName: "door-open""#,
            r#"objectName: "knockButton""#,
            r#"name: "chevron-down""#,
            r#"shortcut: "Ctrl+Shift+P""#,
            r#"textRole: "label""#,
            r#"kind: "warning""#,
            r#"variant: "primary""#,
            r#"actionId: "app.quit""#,
            r#"ToolTip.visible: hovered && toolTip.length > 0"#,
        ] {
            assert!(lint(line).is_empty(), "{line}");
        }
    }

    #[test]
    fn literals_in_ternaries_fallbacks_and_concatenations_are_rejected() {
        for line in [
            r#"text: busy ? "Wait" : qsTr("Go")"#,
            r#"text: busy ? qsTr("Wait") : "Go""#,
            r#"text: count + " hosts""#,
            r#"text: qsTr("Hosts") + ": " + count"#,
            r#"text: name || "Untitled""#,
            r#"text: name ?? "Untitled""#,
            r#"toolTip: (checked ? "Hide" : qsTr("Show"))"#,
            r#"onClicked: status.text = saved ? "Saved" : qsTr("Failed")"#,
            r#"helpText: `${count} hosts`"#,
        ] {
            assert_eq!(lint(line).len(), 1, "{line}");
        }
        for line in [
            r#"text: busy ? qsTr("Wait") : qsTr("Go")"#,
            r#"text: kind === "dir" ? qsTr("Folder") : qsTr("File")"#,
            r#"text: "dir" !== kind ? qsTr("File") : qsTr("Folder")"#,
            r#"text: qsTr("%1 hosts").arg(count)"#,
            r#"text: Platform.keySequenceText("Ctrl+C")"#,
            r#"text: labels[value] ?? value"#,
            r#"text: map["key"] || qsTr("None")"#,
            r#"text: fn(busy ? "a" : "b")"#,
            r#"text: toast.text || """#,
            r#"title: qsTr("Save"); objectName: "saveDialog""#,
            r#"{ text: qsTr("Dark"), value: "dark" }"#,
            r#"OsText { text: qsTr("x") } Item { objectName: "y" }"#,
            r#"text: qsTr("Error message: ") + qsTr("none")"#,
            r#"onClicked: console.warn("text: ", value)"#,
            r#"helpText: qsTr("Type \"label: \" first")"#,
        ] {
            assert!(lint(line).is_empty(), "{line}");
        }
    }

    #[test]
    fn text_collections_are_checked_over_several_lines() {
        let source = r#"
SettingsChoice {
    values: ["system", "dark", "light"]
    labels: ({
            system: qsTr("System"),
            dark: "Dark",
            light: isMac
                ? "Light" : qsTr("Light")
        })
    icons: ({ system: "sun-moon", dark: "moon" })
    value: "dark"
}
OsColorPicker {
    presetNames: [qsTr("Sesame"), "Amber",
        qsTr("Rose")]
}"#;
        assert_eq!(
            lint(source),
            vec![
                "6: user-visible string literal without qsTr()",
                "8: user-visible string literal without qsTr()",
                "14: user-visible string literal without qsTr()",
            ]
        );
        for line in [
            r#"labels: ({ dark: "Dark" })"#,
            r#"presetNames: ["Amber"]"#,
            r#"labels: ({ dark: qsTr("Dark") }); presetNames: ["Amber"]"#,
            r#"page.labels = { dark: busy ? "Wait" : qsTr("Dark") }"#,
        ] {
            assert_eq!(lint(line).len(), 1, "{line}");
        }
        for line in [
            r#"labels: ({ "keep_window": qsTr("Keep the window open") })"#,
            r#"labels: ({ dark: names["dark"], light: fn("light") })"#,
            r#"labels: ({ dark: kind === "x" ? qsTr("A") : qsTr("B") })"#,
            r#"labels: ({ nested: { value: "dark" } })"#,
            r#"property var labels: ({})"#,
            r#"visible: labels === other && kind == "dark""#,
            r#"text: qsTr("labels: \"x\"")"#,
            "showLabels: true",
        ] {
            assert!(lint(line).is_empty(), "{line}");
        }
    }

    #[test]
    fn a_text_collection_ends_with_its_value() {
        // The value is a plain expression or closes on its line: the next lines are not entries.
        assert!(lint("labels: page.labels\nicons: ({ dark: \"moon\" })").is_empty());
        assert!(lint("labels: ({})\nicons: ({\n    dark: \"moon\"\n})").is_empty());
        // An allowed line still counts for the brackets.
        assert_eq!(
            lint("labels: ({\n    a: \"A\", // lint-qml: allow\n    b: \"B\"\n})\nicons: [\"x\"]"),
            vec!["3: user-visible string literal without qsTr()"]
        );
    }

    #[test]
    fn toast_text_is_checked() {
        assert_eq!(
            lint(r#"onClicked: Toasts.show("Saved", "success")"#).len(),
            1
        );
        assert_eq!(lint("Toasts.show(`Saved`)").len(), 1);
        assert!(lint(r#"onClicked: Toasts.show(qsTr("Saved"), "success")"#).is_empty());
        assert!(lint(r#"Toasts.show(message, "danger")"#).is_empty());
        assert!(
            lint(r#"Toasts.show(qsTr("Lost."), "danger", qsTr("Retry"), "app.retry")"#).is_empty()
        );
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
