//! Snippets and macros in `snippets.toml` (PLAN §4.2, Sprint 10).
//!
//! ```toml
//! schema_version = 1
//!
//! [[snippet]]
//! id = "01J..."
//! name = "Restart a service"
//! folder = "Ops/Web"             # "/" separated; empty: at the top
//! tags = ["systemd"]
//! description = "Restarts it and shows its status"
//! shortcut = "Ctrl+Alt+R"         # optional
//! text = "sudo systemctl restart {{service}}\n"
//!
//! [[snippet]]
//! id = "01K..."
//! name = "Router: enable mode"
//! [[snippet.step]]
//! send = "enable\n"
//! [[snippet.step]]
//! wait_for = "[Pp]assword:"       # a regular expression, in what the pane prints
//! timeout_ms = 10000
//! [[snippet.step]]
//! send = "{{secret:router}}\n"
//! [[snippet.step]]
//! delay_ms = 500
//! ```
//!
//! A snippet is text, or (a macro) steps: send text, wait some milliseconds, or wait for a
//! pattern in the output. In text, `{{name}}` is a value asked when the snippet runs, and
//! `{{secret:identity}}` the password of a keychain identity; neither is ever written here. A
//! newline is Enter.

use std::collections::HashMap;
use std::path::Path;

use toml::{Table, Value};

use crate::config::Warning;

/// Name of the file inside the config directory.
pub const SNIPPETS_FILE: &str = "snippets.toml";

/// Current layout of the file.
pub const SCHEMA_VERSION: i64 = 1;

/// Longest name, folder, tag, description or shortcut kept.
const MAX_SHORT: usize = 255;
/// Longest text or step text kept.
const MAX_TEXT: usize = 64 * 1024;
/// Longest wait or delay: ten minutes.
pub const MAX_WAIT_MS: u64 = 600_000;
/// The wait for a pattern when the file doesn't say.
pub const DEFAULT_TIMEOUT_MS: u64 = 10_000;

const HEADER: &str = "# OpenSesh snippets and macros. OpenSesh rewrites this file when a snippet is\n\
                      # changed in the app: comments are not kept.\n";

/// One step of a macro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Type this text (variables allowed; a newline is Enter).
    Send(String),
    /// Wait this many milliseconds.
    Delay(u64),
    /// Wait until the output matches `pattern` (a regular expression), up to `timeout_ms`.
    WaitFor {
        /// The pattern.
        pattern: String,
        /// How long to wait.
        timeout_ms: u64,
    },
}

/// A snippet or a macro.
#[derive(Debug, Clone, PartialEq)]
pub struct Snippet {
    /// Stable id (a ULID).
    pub id: String,
    /// Its name.
    pub name: String,
    /// Its folder (`Ops/Web`), or empty.
    pub folder: String,
    /// Tags.
    pub tags: Vec<String>,
    /// What it does.
    pub description: String,
    /// Its shortcut, in Qt's portable text, or empty.
    pub shortcut: String,
    /// The text (when it has no steps).
    pub text: String,
    /// The steps of a macro (empty for a text snippet).
    pub steps: Vec<Step>,
    /// Keys this version doesn't know, written back unchanged.
    pub extra: Table,
}

impl Default for Snippet {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            folder: String::new(),
            tags: Vec::new(),
            description: String::new(),
            shortcut: String::new(),
            text: String::new(),
            steps: Vec::new(),
            extra: Table::new(),
        }
    }
}

/// A piece of snippet text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    /// Text as it is.
    Text(String),
    /// `{{name}}`: a value asked when it runs.
    Variable(String),
    /// `{{secret:identity}}`: the password of a keychain identity.
    Secret(String),
}

/// The pieces of `text`: plain text, variables and secrets. An opening `{{` without its `}}`,
/// or with nothing inside, stays text.
#[must_use]
pub fn parse(text: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut plain = String::new();
    let mut rest = text;
    while let Some(open) = rest.find("{{") {
        let after = &rest[open + 2..];
        let Some(close) = after.find("}}") else {
            break;
        };
        let inside = after[..close].trim();
        let part = match inside.split_once(':') {
            Some((kind, reference)) if kind.trim() == "secret" && !reference.trim().is_empty() => {
                Some(Part::Secret(reference.trim().to_owned()))
            }
            _ if !inside.is_empty() && !inside.contains(['{', '\n']) => {
                Some(Part::Variable(inside.to_owned()))
            }
            _ => None,
        };
        match part {
            Some(part) => {
                plain.push_str(&rest[..open]);
                if !plain.is_empty() {
                    parts.push(Part::Text(std::mem::take(&mut plain)));
                }
                parts.push(part);
            }
            None => plain.push_str(&rest[..open + 2 + close + 2]),
        }
        rest = &after[close + 2..];
    }
    plain.push_str(rest);
    if !plain.is_empty() {
        parts.push(Part::Text(plain));
    }
    parts
}

/// `text` with its variables and secrets filled in by `value` (which gets each part and returns
/// what to put there, `None` when it is missing).
///
/// # Errors
///
/// The first variable or secret `value` has nothing for.
pub fn render(text: &str, mut value: impl FnMut(&Part) -> Option<String>) -> Result<String, Part> {
    let mut out = String::with_capacity(text.len());
    for part in parse(text) {
        match &part {
            Part::Text(plain) => out.push_str(plain),
            Part::Variable(_) | Part::Secret(_) => match value(&part) {
                Some(filled) => out.push_str(&filled),
                None => return Err(part),
            },
        }
    }
    Ok(out)
}

impl Snippet {
    /// A new snippet with a fresh id.
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            id: crate::hosts::new_id(),
            name: name.to_owned(),
            ..Self::default()
        }
    }

    /// Whether it is a macro (steps rather than text).
    #[must_use]
    pub fn is_macro(&self) -> bool {
        !self.steps.is_empty()
    }

    /// Every text it types: its text, or the text of its send steps.
    fn texts(&self) -> Vec<&str> {
        if self.is_macro() {
            self.steps
                .iter()
                .filter_map(|step| match step {
                    Step::Send(text) => Some(text.as_str()),
                    _ => None,
                })
                .collect()
        } else {
            vec![self.text.as_str()]
        }
    }

    /// The variables it asks for, each once, in order.
    #[must_use]
    pub fn variables(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for text in self.texts() {
            for part in parse(text) {
                if let Part::Variable(name) = part
                    && !out.contains(&name)
                {
                    out.push(name);
                }
            }
        }
        out
    }

    /// The identities whose passwords it types, each once, in order.
    #[must_use]
    pub fn secrets(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for text in self.texts() {
            for part in parse(text) {
                if let Part::Secret(name) = part
                    && !out.contains(&name)
                {
                    out.push(name);
                }
            }
        }
        out
    }

    /// Why it can't be used, if it can't (the first problem found).
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        if self.name.trim().is_empty() {
            return Some("a name is needed".to_owned());
        }
        for (what, value) in [
            ("the name", &self.name),
            ("the folder", &self.folder),
            ("the description", &self.description),
            ("the shortcut", &self.shortcut),
        ] {
            if value.chars().count() > MAX_SHORT
                || value.chars().any(|c| c.is_control() && c != '\n')
            {
                return Some(format!("{what} is too long or has control characters"));
            }
        }
        if !self.is_macro() && self.text.is_empty() {
            return Some("there is nothing to type".to_owned());
        }
        if self.text.len() > MAX_TEXT {
            return Some("the text is too long".to_owned());
        }
        for (index, step) in self.steps.iter().enumerate() {
            let number = index + 1;
            match step {
                Step::Send(text) if text.is_empty() || text.len() > MAX_TEXT => {
                    return Some(format!("step {number} has nothing to type, or too much"));
                }
                Step::Delay(ms) if *ms == 0 || *ms > MAX_WAIT_MS => {
                    return Some(format!("step {number}: a pause of 1 ms to 10 minutes"));
                }
                Step::WaitFor {
                    pattern,
                    timeout_ms,
                } => {
                    if let Err(error) = regex::Regex::new(pattern) {
                        return Some(format!("step {number}: the pattern is not valid ({error})"));
                    }
                    if pattern.is_empty() {
                        return Some(format!("step {number}: the pattern is empty"));
                    }
                    if *timeout_ms == 0 || *timeout_ms > MAX_WAIT_MS {
                        return Some(format!("step {number}: a timeout of 1 ms to 10 minutes"));
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn from_table(mut table: Table, index: usize, warnings: &mut Vec<Warning>) -> Option<Self> {
        let key = |name: &str| format!("snippet[{index}].{name}");
        let mut warn = |name: &str, message: &str| {
            warnings.push(Warning {
                key: if name.is_empty() {
                    format!("snippet[{index}]")
                } else {
                    key(name)
                },
                message: message.to_owned(),
            });
        };
        let text = |table: &mut Table, name: &str, warn: &mut dyn FnMut(&str, &str)| match table
            .remove(name)
        {
            None => String::new(),
            Some(Value::String(value)) => value,
            Some(other) => {
                warn(
                    name,
                    &format!("expected a string, found {}", other.type_str()),
                );
                String::new()
            }
        };
        let id = text(&mut table, "id", &mut warn).trim().to_owned();
        if id.is_empty() || id.chars().any(char::is_control) {
            warn("id", "missing or not valid; the snippet is skipped");
            return None;
        }
        let name = text(&mut table, "name", &mut warn).trim().to_owned();
        let folder = normalize_folder(&text(&mut table, "folder", &mut warn));
        let description = text(&mut table, "description", &mut warn);
        let shortcut = text(&mut table, "shortcut", &mut warn).trim().to_owned();
        let body = text(&mut table, "text", &mut warn);
        let tags = match table.remove("tags") {
            None => Vec::new(),
            Some(Value::Array(values)) => values
                .into_iter()
                .filter_map(|value| value.as_str().map(|tag| tag.trim().to_owned()))
                .filter(|tag| !tag.is_empty())
                .collect(),
            Some(other) => {
                warn(
                    "tags",
                    &format!("expected a list, found {}", other.type_str()),
                );
                Vec::new()
            }
        };
        let mut steps = Vec::new();
        match table.remove("step") {
            None => {}
            Some(Value::Array(values)) => {
                for (number, value) in values.into_iter().enumerate() {
                    match step_from(value) {
                        Ok(step) => steps.push(step),
                        Err(message) => {
                            warn(
                                &format!("step[{number}]"),
                                &format!("{message}; the snippet is skipped"),
                            );
                            return None;
                        }
                    }
                }
            }
            Some(other) => {
                warn(
                    "step",
                    &format!("expected an array of tables, found {}", other.type_str()),
                );
                return None;
            }
        }
        let snippet = Self {
            id,
            name,
            folder,
            tags,
            description,
            shortcut,
            text: body,
            steps,
            extra: table,
        };
        if let Some(problem) = snippet.problem() {
            warn("", &format!("{problem}; the snippet is skipped"));
            return None;
        }
        Some(snippet)
    }

    fn to_table(&self) -> Table {
        let mut table = self.extra.clone();
        let mut put = |name: &str, value: Value| {
            table.insert(name.to_owned(), value);
        };
        put("id", Value::String(self.id.clone()));
        put("name", Value::String(self.name.clone()));
        if !self.folder.is_empty() {
            put("folder", Value::String(self.folder.clone()));
        }
        if !self.tags.is_empty() {
            put(
                "tags",
                Value::Array(self.tags.iter().cloned().map(Value::String).collect()),
            );
        }
        if !self.description.is_empty() {
            put("description", Value::String(self.description.clone()));
        }
        if !self.shortcut.is_empty() {
            put("shortcut", Value::String(self.shortcut.clone()));
        }
        if self.is_macro() {
            put(
                "step",
                Value::Array(
                    self.steps
                        .iter()
                        .map(|step| Value::Table(step_table(step)))
                        .collect(),
                ),
            );
        } else {
            put("text", Value::String(self.text.clone()));
        }
        table
    }
}

fn millis(value: Option<Value>, name: &str) -> Result<Option<u64>, String> {
    match value {
        None => Ok(None),
        Some(Value::Integer(ms)) => u64::try_from(ms)
            .map(Some)
            .map_err(|_| format!("{name} can't be negative")),
        Some(other) => Err(format!(
            "{name}: expected a number, found {}",
            other.type_str()
        )),
    }
}

fn step_from(value: Value) -> Result<Step, String> {
    let Value::Table(mut table) = value else {
        return Err("a step is a table".to_owned());
    };
    if let Some(send) = table.remove("send") {
        return match send {
            Value::String(text) => Ok(Step::Send(text)),
            other => Err(format!(
                "send: expected a string, found {}",
                other.type_str()
            )),
        };
    }
    if let Some(pattern) = table.remove("wait_for") {
        let Value::String(pattern) = pattern else {
            return Err("wait_for: expected a string".to_owned());
        };
        let timeout_ms =
            millis(table.remove("timeout_ms"), "timeout_ms")?.unwrap_or(DEFAULT_TIMEOUT_MS);
        return Ok(Step::WaitFor {
            pattern,
            timeout_ms,
        });
    }
    match millis(table.remove("delay_ms"), "delay_ms")? {
        Some(ms) => Ok(Step::Delay(ms)),
        None => Err("a step has send, wait_for or delay_ms".to_owned()),
    }
}

fn step_table(step: &Step) -> Table {
    let mut table = Table::new();
    match step {
        Step::Send(text) => {
            table.insert("send".into(), Value::String(text.clone()));
        }
        Step::Delay(ms) => {
            table.insert(
                "delay_ms".into(),
                Value::Integer(i64::try_from(*ms).unwrap_or(i64::MAX)),
            );
        }
        Step::WaitFor {
            pattern,
            timeout_ms,
        } => {
            table.insert("wait_for".into(), Value::String(pattern.clone()));
            table.insert(
                "timeout_ms".into(),
                Value::Integer(i64::try_from(*timeout_ms).unwrap_or(i64::MAX)),
            );
        }
    }
    table
}

/// `/Ops//Web/ ` becomes `Ops/Web`.
#[must_use]
pub fn normalize_folder(folder: &str) -> String {
    folder
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// The contents of `snippets.toml`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SnippetsFile {
    /// The snippets, in the order of the file.
    pub snippets: Vec<Snippet>,
    /// Unknown top-level keys, written back unchanged.
    pub extra: Table,
    /// The file comes from a newer OpenSesh, or couldn't be read or parsed: never overwrite it.
    pub read_only: bool,
}

impl SnippetsFile {
    /// Reads the file (a missing file means no snippets).
    #[must_use]
    pub fn load_file(path: &Path) -> (Self, Vec<Warning>) {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::from_toml_str(&text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                (Self::default(), Vec::new())
            }
            Err(error) => (
                Self {
                    read_only: true,
                    ..Self::default()
                },
                vec![Warning {
                    key: SNIPPETS_FILE.to_owned(),
                    message: format!("could not read the file: {error}"),
                }],
            ),
        }
    }

    /// Parses the file's text; snippets that can't be used are skipped with a warning.
    #[must_use]
    pub fn from_toml_str(text: &str) -> (Self, Vec<Warning>) {
        let mut warnings = Vec::new();
        let mut root: Table = match text.parse() {
            Ok(root) => root,
            Err(error) => {
                let error: toml::de::Error = error;
                warnings.push(Warning {
                    key: SNIPPETS_FILE.to_owned(),
                    message: format!("not valid TOML: {error}"),
                });
                return (
                    Self {
                        read_only: true,
                        ..Self::default()
                    },
                    warnings,
                );
            }
        };
        let version = root
            .remove("schema_version")
            .and_then(|value| value.as_integer())
            .unwrap_or(SCHEMA_VERSION);
        let mut snippets: Vec<Snippet> = Vec::new();
        match root.remove("snippet") {
            None => {}
            Some(Value::Array(entries)) => {
                for (index, entry) in entries.into_iter().enumerate() {
                    let Value::Table(table) = entry else {
                        warnings.push(Warning {
                            key: format!("snippet[{index}]"),
                            message: "expected a table".to_owned(),
                        });
                        continue;
                    };
                    if let Some(snippet) = Snippet::from_table(table, index, &mut warnings) {
                        if snippets.iter().any(|seen| seen.id == snippet.id) {
                            warnings.push(Warning {
                                key: format!("snippet[{index}].id"),
                                message: "used twice; the second is skipped".to_owned(),
                            });
                        } else {
                            snippets.push(snippet);
                        }
                    }
                }
            }
            Some(other) => warnings.push(Warning {
                key: "snippet".to_owned(),
                message: format!("expected an array of tables, found {}", other.type_str()),
            }),
        }
        (
            Self {
                snippets,
                extra: root,
                read_only: version > SCHEMA_VERSION,
            },
            warnings,
        )
    }

    /// The file's text.
    #[must_use]
    pub fn to_toml_string(&self) -> String {
        let mut root = self.extra.clone();
        root.insert("schema_version".into(), Value::Integer(SCHEMA_VERSION));
        root.insert(
            "snippet".into(),
            Value::Array(
                self.snippets
                    .iter()
                    .map(|snippet| Value::Table(snippet.to_table()))
                    .collect(),
            ),
        );
        format!("{HEADER}\n{root}")
    }

    /// The snippet with `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Snippet> {
        self.snippets.iter().find(|snippet| snippet.id == id)
    }

    /// Every folder (and the folders above them), sorted.
    #[must_use]
    pub fn folders(&self) -> Vec<String> {
        let mut folders: Vec<String> = Vec::new();
        for snippet in &self.snippets {
            let mut path = String::new();
            for part in snippet.folder.split('/').filter(|part| !part.is_empty()) {
                if !path.is_empty() {
                    path.push('/');
                }
                path.push_str(part);
                if !folders.contains(&path) {
                    folders.push(path.clone());
                }
            }
        }
        folders.sort_by_key(|folder| folder.to_lowercase());
        folders
    }

    /// Every tag with how many snippets have it, sorted by name.
    #[must_use]
    pub fn tags(&self) -> Vec<(String, usize)> {
        let mut counts: HashMap<String, (String, usize)> = HashMap::new();
        for tag in self.snippets.iter().flat_map(|snippet| snippet.tags.iter()) {
            counts
                .entry(tag.to_lowercase())
                .or_insert_with(|| (tag.clone(), 0))
                .1 += 1;
        }
        let mut tags: Vec<(String, usize)> = counts.into_values().collect();
        tags.sort_by_key(|(tag, _)| tag.to_lowercase());
        tags
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates() {
        assert_eq!(
            parse("ssh {{user}}@{{ host }} -p {{secret:db admin}}!"),
            [
                Part::Text("ssh ".into()),
                Part::Variable("user".into()),
                Part::Text("@".into()),
                Part::Variable("host".into()),
                Part::Text(" -p ".into()),
                Part::Secret("db admin".into()),
                Part::Text("!".into()),
            ]
        );
        // Not variables: empty, unclosed, a secret without a name, docker's own braces.
        assert_eq!(parse("{{}} x"), [Part::Text("{{}} x".into())]);
        assert_eq!(parse("a {{b"), [Part::Text("a {{b".into())]);
        assert_eq!(parse("{{secret:}}"), [Part::Variable("secret:".into())]);
        assert_eq!(
            parse("{{ {.Names} }}"),
            [Part::Text("{{ {.Names} }}".into())]
        );
        assert_eq!(parse(""), []);
        let rendered = render("hi {{name}}, {{secret:vault}}", |part| match part {
            Part::Variable(name) if name == "name" => Some("Ana".into()),
            Part::Secret(name) if name == "vault" => Some("s3cret".into()),
            _ => None,
        });
        assert_eq!(rendered, Ok("hi Ana, s3cret".to_owned()));
        assert_eq!(
            render("{{missing}}", |_| None),
            Err(Part::Variable("missing".into()))
        );
    }

    #[test]
    fn round_trip() {
        let text = r#"
schema_version = 1
future = true

[[snippet]]
id = "A"
name = "Restart"
folder = "/Ops//Web/"
tags = ["systemd", " prod ", ""]
shortcut = "Ctrl+Alt+R"
text = "sudo systemctl restart {{service}}\n"
color = "kept"

[[snippet]]
id = "B"
name = "Router enable"
[[snippet.step]]
send = "enable\n"
[[snippet.step]]
wait_for = "[Pp]assword:"
[[snippet.step]]
send = "{{secret:router}}\n"
[[snippet.step]]
delay_ms = 500
"#;
        let (file, warnings) = SnippetsFile::from_toml_str(text);
        assert!(warnings.is_empty(), "{warnings:?}");
        let restart = file.get("A").unwrap();
        assert_eq!(restart.folder, "Ops/Web");
        assert_eq!(restart.tags, ["systemd", "prod"]);
        assert_eq!(restart.variables(), ["service"]);
        assert!(!restart.is_macro());
        let router = file.get("B").unwrap();
        assert!(router.is_macro());
        assert_eq!(router.steps.len(), 4);
        assert_eq!(
            router.steps[1],
            Step::WaitFor {
                pattern: "[Pp]assword:".into(),
                timeout_ms: DEFAULT_TIMEOUT_MS
            }
        );
        assert_eq!(router.secrets(), ["router"]);
        assert!(router.variables().is_empty());
        let (again, warnings) = SnippetsFile::from_toml_str(&file.to_toml_string());
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(again, file);
        let written = file.to_toml_string();
        assert!(written.contains("future = true") && written.contains("color = \"kept\""));
        assert_eq!(file.folders(), ["Ops", "Ops/Web"]);
        assert_eq!(
            file.tags(),
            [("prod".to_owned(), 1), ("systemd".to_owned(), 1)]
        );
    }

    #[test]
    fn bad_snippets_are_skipped() {
        let text = r#"
[[snippet]]
name = "no id"
text = "x"

[[snippet]]
id = "no-name"
text = "x"

[[snippet]]
id = "empty"
name = "Empty"

[[snippet]]
id = "bad-pattern"
name = "Bad"
[[snippet.step]]
wait_for = "("

[[snippet]]
id = "bad-step"
name = "Bad step"
[[snippet.step]]
jump = 1

[[snippet]]
id = "long-wait"
name = "Long"
[[snippet.step]]
delay_ms = 3600000

[[snippet]]
id = "ok"
name = "OK"
text = "ls\n"

[[snippet]]
id = "ok"
name = "Again"
text = "pwd\n"
"#;
        let (file, warnings) = SnippetsFile::from_toml_str(text);
        let ids: Vec<&str> = file
            .snippets
            .iter()
            .map(|snippet| snippet.id.as_str())
            .collect();
        assert_eq!(ids, ["ok"]);
        for expected in [
            "missing or not valid",
            "a name is needed",
            "nothing to type",
            "not valid (",
            "send, wait_for or delay_ms",
            "10 minutes",
            "used twice",
        ] {
            assert!(
                warnings
                    .iter()
                    .any(|warning| warning.message.contains(expected)),
                "{expected}: {warnings:?}"
            );
        }
    }

    #[test]
    fn newer_or_broken_files_are_read_only() {
        assert!(
            SnippetsFile::from_toml_str("schema_version = 2")
                .0
                .read_only
        );
        assert!(SnippetsFile::from_toml_str("[[snippet]").0.read_only);
        assert!(!SnippetsFile::from_toml_str("").0.read_only);
    }
}
