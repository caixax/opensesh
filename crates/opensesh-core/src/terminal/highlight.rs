//! Keyword highlighting (PLAN §6.5): rules that style the text matching a regular expression,
//! grouped into rule sets that profiles turn on by id (`highlight_sets`).
//!
//! OpenSesh ships four sets (logs, network, status, paths and URLs); the user's own sets live in
//! `highlights.toml`:
//!
//! ```toml
//! schema_version = 1
//! [[set]]
//! id = "deploy"
//! name = "Deploy logs"
//! [[set.rule]]
//! pattern = "\\bROLLBACK\\b"
//! foreground = "bright-red"   # an ANSI color of the theme, or #RRGGBB
//! bold = true
//! ```
//!
//! Colors can name the theme's ANSI colors, so the presets look right in every theme. A pattern
//! with a capture group styles only the first group. When rules overlap, the later rule wins.
//! The engine (`opensesh-term`) runs the rules on the visible rows when it draws them.

use std::fmt;
use std::path::Path;
use std::str::FromStr;

use toml::{Table, Value};

use super::settings::valid_id;
use crate::config::Warning;
use crate::theme::{Rgba, UnknownValue};

/// Name of the rules file inside the config directory.
pub const HIGHLIGHTS_FILE: &str = "highlights.toml";

/// Current layout of the rules file.
pub const SCHEMA_VERSION: i64 = 1;

/// Longest pattern, in characters.
pub const MAX_PATTERN_CHARS: usize = 512;
/// Most rules in one set.
pub const MAX_RULES: usize = 64;
/// Most user sets.
pub const MAX_SETS: usize = 32;
/// Size limit of a compiled pattern (the `regex` crate's), so a rule can't use huge amounts of
/// memory.
pub const REGEX_SIZE_LIMIT: usize = 1 << 20;

const HEADER: &str = "# OpenSesh keyword highlighting rules. Profiles turn sets on by id\n\
                      # (highlight_sets). OpenSesh rewrites this file when the rules are changed\n\
                      # in the app: comments are not kept.\n";

/// A rule color: one of the theme's 16 ANSI colors, or a fixed one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HighlightColor {
    /// ANSI color 0-15 of the current theme.
    Ansi(u8),
    /// A fixed color.
    Rgb(Rgba),
}

/// Names of the ANSI colors in rule files, in index order.
pub const ANSI_COLOR_NAMES: [&str; 16] = [
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
    "bright-black",
    "bright-red",
    "bright-green",
    "bright-yellow",
    "bright-blue",
    "bright-magenta",
    "bright-cyan",
    "bright-white",
];

impl fmt::Display for HighlightColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ansi(index) => f.write_str(
                ANSI_COLOR_NAMES
                    .get(usize::from(*index))
                    .copied()
                    .unwrap_or("white"),
            ),
            Self::Rgb(color) => write!(f, "{color}"),
        }
    }
}

impl FromStr for HighlightColor {
    type Err = UnknownValue;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if let Some(index) = ANSI_COLOR_NAMES.iter().position(|name| *name == text) {
            // At most 15.
            return Ok(Self::Ansi(u8::try_from(index).unwrap_or(7)));
        }
        text.parse::<Rgba>()
            .map(Self::Rgb)
            .map_err(|_| UnknownValue(text.to_owned()))
    }
}

/// How matching text looks. A color left out keeps the text's own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct HighlightStyle {
    /// Text color.
    pub foreground: Option<HighlightColor>,
    /// Background color.
    pub background: Option<HighlightColor>,
    /// Bold.
    pub bold: bool,
    /// Underlined.
    pub underline: bool,
}

/// One rule.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HighlightRule {
    /// Regular expression (the `regex` crate's syntax).
    pub pattern: String,
    /// Match regardless of case.
    pub ignore_case: bool,
    /// The style of the matching text.
    pub style: HighlightStyle,
}

impl HighlightRule {
    /// Whether the pattern compiles within the limits. Returns the regex error otherwise.
    ///
    /// # Errors
    ///
    /// The reason the pattern is not accepted.
    pub fn check(&self) -> Result<(), String> {
        if self.pattern.is_empty() {
            return Err("the pattern is empty".to_owned());
        }
        if self.pattern.chars().count() > MAX_PATTERN_CHARS {
            return Err(format!(
                "the pattern is longer than {MAX_PATTERN_CHARS} characters"
            ));
        }
        regex::RegexBuilder::new(&self.pattern)
            .case_insensitive(self.ignore_case)
            .size_limit(REGEX_SIZE_LIMIT)
            .build()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

/// A named group of rules.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HighlightSet {
    /// Stable id that profiles use ([`valid_id`]).
    pub id: String,
    /// Display name.
    pub name: String,
    /// The rules, applied in order.
    pub rules: Vec<HighlightRule>,
    /// Shipped with OpenSesh (can be duplicated, not changed).
    pub builtin: bool,
}

fn rule(pattern: &str, ignore_case: bool, style: HighlightStyle) -> HighlightRule {
    HighlightRule {
        pattern: pattern.to_owned(),
        ignore_case,
        style,
    }
}

const fn ansi(index: u8) -> Option<HighlightColor> {
    Some(HighlightColor::Ansi(index))
}

const fn text(color: u8, bold: bool, underline: bool) -> HighlightStyle {
    HighlightStyle {
        foreground: ansi(color),
        background: None,
        bold,
        underline,
    }
}

const RED: u8 = 1;
const GREEN: u8 = 2;
const YELLOW: u8 = 3;
const BLUE: u8 = 4;
const MAGENTA: u8 = 5;
const CYAN: u8 = 6;
const BRIGHT_BLACK: u8 = 8;
const BRIGHT_RED: u8 = 9;

/// IPv4 octet.
const OCTET: &str = r"(?:25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])";
/// IPv6 group.
const H: &str = "[0-9A-Fa-f]{1,4}";

/// The built-in rule sets.
#[must_use]
pub fn builtin_sets() -> Vec<HighlightSet> {
    let ipv4 = format!(r"\b{OCTET}(?:\.{OCTET}){{3}}(?:/[0-9]{{1,2}})?\b");
    // Full form, or compressed with `::` between groups, or ending in `::`, or `::1` and the
    // IPv4-mapped `::ffff:a.b.c.d`. A lone `::` (C++ scopes) never matches.
    let ipv6 = format!(
        r"(?:\b(?:{H}:){{7}}{H}\b|\b(?:{H}:){{1,6}}:{H}(?::{H})*\b|\b(?:{H}:){{1,7}}:(?:[^:0-9A-Fa-f]|$)|::1\b|::ffff:{OCTET}(?:\.{OCTET}){{3}}\b)(?:/[0-9]{{1,3}})?"
    );
    vec![
        HighlightSet {
            id: "logs".to_owned(),
            name: "Log levels".to_owned(),
            rules: vec![
                rule(
                    r"\b(?:FATAL|CRITICAL|CRIT|PANIC|EMERG(?:ENCY)?|ALERT)\b",
                    true,
                    text(BRIGHT_RED, true, true),
                ),
                rule(r"\b(?:ERROR|ERR)\b", true, text(RED, true, false)),
                rule(r"\bWARN(?:ING)?\b", true, text(YELLOW, true, false)),
                rule(r"\b(?:INFO|NOTICE)\b", false, text(GREEN, false, false)),
                rule(
                    r"\b(?:DEBUG|TRACE)\b",
                    false,
                    text(BRIGHT_BLACK, false, false),
                ),
            ],
            builtin: true,
        },
        HighlightSet {
            id: "network".to_owned(),
            name: "Network addresses".to_owned(),
            rules: vec![
                rule(&ipv4, false, text(CYAN, false, false)),
                rule(&ipv6, false, text(CYAN, false, false)),
                rule(
                    r"\b[0-9A-Fa-f]{2}(?:[:-][0-9A-Fa-f]{2}){5}\b",
                    false,
                    text(MAGENTA, false, false),
                ),
            ],
            builtin: true,
        },
        HighlightSet {
            id: "status".to_owned(),
            name: "Status words".to_owned(),
            rules: vec![
                rule(
                    r"\b(?:OK|SUCCESS(?:FUL)?|PASS(?:ED)?|DONE|ACTIVE|RUNNING|UP|ONLINE|ENABLED)\b",
                    false,
                    text(GREEN, true, false),
                ),
                rule(
                    r"\b(?:FAIL(?:ED|URE)?|DENIED|REFUSED|REJECTED|DOWN|OFFLINE|INACTIVE|DISABLED|TIMEOUT|UNREACHABLE|ABORT(?:ED)?)\b",
                    false,
                    text(RED, true, false),
                ),
                rule(
                    r"\b(?:permission denied|access denied|connection refused|no such file or directory|command not found)\b",
                    true,
                    text(RED, false, false),
                ),
            ],
            builtin: true,
        },
        HighlightSet {
            id: "paths".to_owned(),
            name: "Paths and URLs".to_owned(),
            rules: vec![
                rule(
                    r#"(?:^|[\s'"(=:])((?:~|\.{1,2})?/[^\s'"():;,<>|`]+)"#,
                    false,
                    text(BLUE, false, false),
                ),
                rule(
                    r#"\b[A-Za-z]:\\[^\s'"<>|:*?]*"#,
                    false,
                    text(BLUE, false, false),
                ),
                rule(
                    r#"\b(?:https?|ftp|sftp|ssh|file)://[^\s<>"'`]+"#,
                    true,
                    text(BLUE, false, true),
                ),
            ],
            builtin: true,
        },
    ]
}

/// The built-in sets and the user's.
#[derive(Debug, Clone, PartialEq)]
pub struct HighlightLibrary {
    /// Every set: built-in ones first.
    pub sets: Vec<HighlightSet>,
    /// Unknown top-level keys of the file, written back unchanged.
    pub extra: Table,
    /// The file comes from a newer OpenSesh: never overwrite it.
    pub read_only: bool,
}

impl Default for HighlightLibrary {
    fn default() -> Self {
        Self {
            sets: builtin_sets(),
            extra: Table::new(),
            read_only: false,
        }
    }
}

impl HighlightLibrary {
    /// Reads `highlights.toml` (a missing file gives the built-in sets only).
    #[must_use]
    pub fn load_file(path: &Path) -> (Self, Vec<Warning>) {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::from_toml_str(&text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                (Self::default(), Vec::new())
            }
            Err(error) => (
                Self::default(),
                vec![Warning {
                    key: HIGHLIGHTS_FILE.to_owned(),
                    message: format!("could not read the file: {error}"),
                }],
            ),
        }
    }

    /// Parses the rules file. Invalid rules and sets are skipped with a warning; a file that
    /// isn't valid TOML gives the built-in sets and one warning.
    #[must_use]
    pub fn from_toml_str(text: &str) -> (Self, Vec<Warning>) {
        let mut warnings = Vec::new();
        let mut warn = |key: String, message: String| warnings.push(Warning { key, message });
        let mut root: Table = match text.parse() {
            Ok(root) => root,
            Err(error) => {
                let error: toml::de::Error = error;
                warn(
                    HIGHLIGHTS_FILE.to_owned(),
                    format!("not valid TOML: {error}"),
                );
                // Never overwrite a file we couldn't read.
                let library = Self {
                    read_only: true,
                    ..Self::default()
                };
                return (library, warnings);
            }
        };
        let version = root
            .remove("schema_version")
            .and_then(|value| value.as_integer())
            .unwrap_or(SCHEMA_VERSION);
        let mut library = Self {
            read_only: version > SCHEMA_VERSION,
            ..Self::default()
        };
        let sets = match root.remove("set") {
            Some(Value::Array(sets)) => sets,
            Some(_) => {
                warn(
                    "set".to_owned(),
                    "expected a list of [[set]] tables".to_owned(),
                );
                Vec::new()
            }
            None => Vec::new(),
        };
        for (index, value) in sets.into_iter().enumerate() {
            let key = format!("set[{index}]");
            let Value::Table(table) = value else {
                warn(key, "expected a table".to_owned());
                continue;
            };
            match parse_set(&table, &key, &mut warn) {
                Some(set) if library.sets.iter().any(|other| other.id == set.id) => warn(
                    format!("{key}.id"),
                    format!("`{}` is already used; set skipped", set.id),
                ),
                Some(_) if library.user_sets().count() >= MAX_SETS => {
                    warn(key, format!("more than {MAX_SETS} sets; skipped"));
                }
                Some(set) => library.sets.push(set),
                None => {}
            }
        }
        library.extra = root;
        (library, warnings)
    }

    /// The file's text: the user's sets only.
    #[must_use]
    pub fn to_toml_string(&self) -> String {
        let mut root = self.extra.clone();
        root.remove("schema_version");
        root.remove("set");
        root.insert("schema_version".into(), Value::Integer(SCHEMA_VERSION));
        let sets: Vec<Value> = self
            .user_sets()
            .map(|set| {
                let mut table = Table::new();
                table.insert("id".into(), Value::String(set.id.clone()));
                table.insert("name".into(), Value::String(set.name.clone()));
                let rules = set
                    .rules
                    .iter()
                    .map(rule_to_table)
                    .map(Value::Table)
                    .collect();
                table.insert("rule".into(), Value::Array(rules));
                Value::Table(table)
            })
            .collect();
        if !sets.is_empty() {
            root.insert("set".into(), Value::Array(sets));
        }
        format!("{HEADER}\n{root}")
    }

    /// The user's sets.
    pub fn user_sets(&self) -> impl Iterator<Item = &HighlightSet> {
        self.sets.iter().filter(|set| !set.builtin)
    }

    /// The set with `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&HighlightSet> {
        self.sets.iter().find(|set| set.id == id)
    }

    /// Adds a user set or replaces the user set with the same id. Built-in sets can't be
    /// replaced; returns whether the set was stored.
    pub fn put(&mut self, set: HighlightSet) -> bool {
        match self.sets.iter_mut().find(|other| other.id == set.id) {
            Some(existing) if existing.builtin => false,
            Some(existing) => {
                *existing = HighlightSet {
                    builtin: false,
                    ..set
                };
                true
            }
            None => {
                self.sets.push(HighlightSet {
                    builtin: false,
                    ..set
                });
                true
            }
        }
    }

    /// Removes a user set. Returns whether it existed.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.sets.len();
        self.sets.retain(|set| set.builtin || set.id != id);
        self.sets.len() != before
    }

    /// A new set id for `name` that no set uses yet.
    #[must_use]
    pub fn unique_id(&self, name: &str) -> String {
        let base = super::profile::slug(name);
        let base = if base.is_empty() {
            "rules".to_owned()
        } else {
            base
        };
        let mut id = base.clone();
        let mut n = 2;
        while self.get(&id).is_some() {
            id = format!("{base}-{n}");
            n += 1;
        }
        id
    }
}

fn parse_set(
    table: &Table,
    key: &str,
    warn: &mut impl FnMut(String, String),
) -> Option<HighlightSet> {
    let id = table.get("id").and_then(Value::as_str).unwrap_or_default();
    if !valid_id(id) {
        warn(
            format!("{key}.id"),
            "expected lowercase letters, digits, - _ and .; set skipped".to_owned(),
        );
        return None;
    }
    let name = table
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map_or_else(|| super::theme::title_from_id(id), str::to_owned);
    let mut rules = Vec::new();
    if let Some(Value::Array(items)) = table.get("rule") {
        for (index, item) in items.iter().enumerate() {
            let rule_key = format!("{key}.rule[{index}]");
            let Some(item) = item.as_table() else {
                warn(rule_key, "expected a table".to_owned());
                continue;
            };
            if rules.len() >= MAX_RULES {
                warn(rule_key, format!("more than {MAX_RULES} rules; skipped"));
                continue;
            }
            match parse_rule(item) {
                Ok(rule) => rules.push(rule),
                Err(message) => warn(rule_key, format!("{message}; rule skipped")),
            }
        }
    }
    Some(HighlightSet {
        id: id.to_owned(),
        name,
        rules,
        builtin: false,
    })
}

fn parse_rule(table: &Table) -> Result<HighlightRule, String> {
    let pattern = table
        .get("pattern")
        .and_then(Value::as_str)
        .ok_or("no pattern")?
        .to_owned();
    let flag = |name: &str| -> Result<bool, String> {
        match table.get(name) {
            None => Ok(false),
            Some(Value::Boolean(value)) => Ok(*value),
            Some(_) => Err(format!("{name}: expected true or false")),
        }
    };
    let color = |name: &str| -> Result<Option<HighlightColor>, String> {
        match table.get(name) {
            None => Ok(None),
            Some(Value::String(text)) => text
                .parse()
                .map(Some)
                .map_err(|_| format!("{name}: `{text}` is not an ANSI color name or #RRGGBB")),
            Some(_) => Err(format!("{name}: expected a string")),
        }
    };
    let rule = HighlightRule {
        pattern,
        ignore_case: flag("ignore_case")?,
        style: HighlightStyle {
            foreground: color("foreground")?,
            background: color("background")?,
            bold: flag("bold")?,
            underline: flag("underline")?,
        },
    };
    rule.check()?;
    Ok(rule)
}

fn rule_to_table(rule: &HighlightRule) -> Table {
    let mut table = Table::new();
    table.insert("pattern".into(), Value::String(rule.pattern.clone()));
    if rule.ignore_case {
        table.insert("ignore_case".into(), Value::Boolean(true));
    }
    if let Some(color) = rule.style.foreground {
        table.insert("foreground".into(), Value::String(color.to_string()));
    }
    if let Some(color) = rule.style.background {
        table.insert("background".into(), Value::String(color.to_string()));
    }
    if rule.style.bold {
        table.insert("bold".into(), Value::Boolean(true));
    }
    if rule.style.underline {
        table.insert("underline".into(), Value::Boolean(true));
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(set: &str, text: &str) -> Vec<String> {
        let library = HighlightLibrary::default();
        let mut found = Vec::new();
        for rule in &library.get(set).unwrap().rules {
            let regex = regex::RegexBuilder::new(&rule.pattern)
                .case_insensitive(rule.ignore_case)
                .build()
                .unwrap();
            for captures in regex.captures_iter(text) {
                let m = captures.get(1).or_else(|| captures.get(0)).unwrap();
                found.push(m.as_str().to_owned());
            }
        }
        found
    }

    #[test]
    fn every_builtin_rule_compiles() {
        for set in builtin_sets() {
            assert!(valid_id(&set.id));
            for rule in &set.rules {
                rule.check()
                    .unwrap_or_else(|e| panic!("{}: {e}", rule.pattern));
            }
        }
    }

    #[test]
    fn log_levels() {
        assert_eq!(
            matches(
                "logs",
                "2026-09-26 ERROR boom, warning: x FATAL INFO DEBUG info"
            ),
            ["FATAL", "ERROR", "warning", "INFO", "DEBUG"]
        );
        assert!(matches("logs", "errors and terrors").is_empty());
    }

    #[test]
    fn network_addresses() {
        let found = matches(
            "network",
            "from 192.168.1.20 and 10.0.0.0/8, not 999.1.1.1; fe80::1 2001:db8::ff00:42:8329 ::1 \
             std::vector 12:30:45 aa:bb:cc:dd:ee:ff 01-23-45-67-89-AB",
        );
        for expected in [
            "192.168.1.20",
            "10.0.0.0/8",
            "fe80::1",
            "2001:db8::ff00:42:8329",
            "::1",
            "aa:bb:cc:dd:ee:ff",
            "01-23-45-67-89-AB",
        ] {
            assert!(found.iter().any(|m| m == expected), "{expected}: {found:?}");
        }
        for unexpected in ["999.1.1.1", "::", "12:30:45"] {
            assert!(
                !found.iter().any(|m| m == unexpected),
                "{unexpected}: {found:?}"
            );
        }
    }

    #[test]
    fn status_words_and_paths() {
        assert_eq!(
            matches(
                "status",
                "[ OK ] started; FAILED; Permission denied; up to date"
            ),
            ["OK", "FAILED", "Permission denied"]
        );
        let found = matches(
            "paths",
            "cd ~/src/app && ls ./x /etc/hosts C:\\Users\\me see https://example.com/a?b=1 a/b",
        );
        for expected in [
            "~/src/app",
            "./x",
            "/etc/hosts",
            "C:\\Users\\me",
            "https://example.com/a?b=1",
        ] {
            assert!(found.iter().any(|m| m == expected), "{expected}: {found:?}");
        }
        assert!(!found.iter().any(|m| m == "a/b"), "{found:?}");
    }

    #[test]
    fn user_sets_round_trip_and_bad_rules_are_skipped() {
        let (library, warnings) = HighlightLibrary::from_toml_str(
            r##"
            schema_version = 1
            future = 1
            [[set]]
            id = "deploy"
            name = "Deploy"
            [[set.rule]]
            pattern = "\\bROLLBACK\\b"
            foreground = "bright-red"
            background = "#202020"
            bold = true
            [[set.rule]]
            pattern = "(unclosed"
            [[set.rule]]
            pattern = "x"
            foreground = "chartreuse"
            [[set]]
            id = "logs"
            [[set]]
            id = "Bad Id"
            "##,
        );
        let joined: Vec<String> = warnings.iter().map(ToString::to_string).collect();
        assert_eq!(warnings.len(), 4, "{joined:?}");
        let deploy = library.get("deploy").unwrap();
        assert_eq!(deploy.rules.len(), 1);
        assert_eq!(
            deploy.rules[0].style.foreground,
            Some(HighlightColor::Ansi(9))
        );
        assert_eq!(
            deploy.rules[0].style.background,
            Some(HighlightColor::Rgb(Rgba::rgb(0x20, 0x20, 0x20)))
        );
        assert!(
            library.get("logs").unwrap().builtin,
            "a user set can't replace a preset"
        );

        let text = library.to_toml_string();
        assert!(!text.contains("Log levels"), "presets are not written");
        let (again, warnings) = HighlightLibrary::from_toml_str(&text);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(again, library);
    }

    #[test]
    fn put_remove_and_ids() {
        let mut library = HighlightLibrary::default();
        let mut logs = library.get("logs").unwrap().clone();
        assert!(!library.put(logs.clone()), "presets stay");
        logs.id = library.unique_id("Log levels");
        assert_eq!(logs.id, "log-levels");
        assert!(library.put(logs.clone()));
        assert!(!library.get("log-levels").unwrap().builtin);
        assert!(library.remove("log-levels"));
        assert!(!library.remove("logs"));
        assert_eq!(library.unique_id("logs"), "logs-2");
        assert!("#12".parse::<HighlightColor>().is_err());
        assert_eq!(HighlightColor::Ansi(12).to_string(), "bright-blue");
    }

    #[test]
    fn a_broken_file_is_never_overwritten() {
        let (library, warnings) = HighlightLibrary::from_toml_str("[[set]\n");
        assert!(library.read_only);
        assert_eq!(warnings.len(), 1);
        assert_eq!(library.sets, builtin_sets());
    }
}
