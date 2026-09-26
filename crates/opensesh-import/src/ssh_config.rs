//! `~/.ssh/config` (ADR 0020): the hosts it names, for the Hosts view to import or to show
//! linked, read-only.
//!
//! What is read: `Host` blocks with plain names (each name becomes a host), and in them
//! `HostName` (`%h` is the name), `User`, `Port`, `IdentityFile` and `ProxyJump`. `Include`
//! is followed (globs, `~`, paths relative to `~/.ssh`, at most 16 levels, never the same file
//! twice in a chain). As in OpenSSH, the first value found for an option wins; a name that
//! appears in several blocks takes what each block adds.
//!
//! What is skipped with a warning: patterns with wildcards or negations (`Host *`, `*.corp`,
//! `!bastion`), `Match` blocks, options before the first `Host` line, and values that can't be
//! used. Every other keyword is left to OpenSSH, which still reads the file when it connects.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use opensesh_core::hosts::{Host, LINKED_PREFIX, SOURCE_SSH_CONFIG, Source, new_id};
use opensesh_core::paths::expand_tilde;

/// Deepest `Include` chain followed (OpenSSH's limit).
const MAX_DEPTH: usize = 16;

/// A host named in the file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SshHost {
    /// The name after `Host`.
    pub alias: String,
    /// `HostName`, with `%h` expanded.
    pub host_name: Option<String>,
    /// `User`.
    pub user: Option<String>,
    /// `Port`.
    pub port: Option<u16>,
    /// `IdentityFile` entries, in order.
    pub identity_files: Vec<String>,
    /// `ProxyJump` hops, first first (`ProxyJump none` gives none).
    pub proxy_jump: Option<Vec<String>>,
    /// The file of its first `Host` line.
    pub file: PathBuf,
    /// Its line number (from 1).
    pub line: usize,
}

/// Something that was skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportWarning {
    /// The file.
    pub file: PathBuf,
    /// The line (from 1), 0 for the whole file.
    pub line: usize,
    /// What and why.
    pub message: String,
}

impl fmt::Display for ImportWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line > 0 {
            write!(f, "{}:{}: {}", self.file.display(), self.line, self.message)
        } else {
            write!(f, "{}: {}", self.file.display(), self.message)
        }
    }
}

/// The result of reading a config file and what it includes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SshConfig {
    /// Hosts in the order they first appear.
    pub hosts: Vec<SshHost>,
    /// What was skipped.
    pub warnings: Vec<ImportWarning>,
    /// Every file read (to watch them for changes).
    pub files: Vec<PathBuf>,
}

struct Parser<'a> {
    home: &'a Path,
    config: SshConfig,
    /// Index into `config.hosts` by alias.
    index: HashMap<String, usize>,
    /// The aliases of the current `Host` block; `None` outside one (before the first `Host`, or
    /// in a `Match` block).
    current: Option<Vec<String>>,
    /// Warned once per file about options outside a usable block.
    warned_outside: bool,
}

/// Reads `path` (usually `~/.ssh/config`) and everything it includes. `home` is the user's
/// home directory (for `~` and relative includes). A missing file gives nothing and a warning.
#[must_use]
pub fn load(path: &Path, home: &Path) -> SshConfig {
    let mut parser = Parser {
        home,
        config: SshConfig::default(),
        index: HashMap::new(),
        current: None,
        warned_outside: false,
    };
    parser.read_file(path, 0, &mut Vec::new());
    parser.config
}

/// Parses text as if it were the file `origin` (includes are read from disk).
#[must_use]
pub fn parse_str(text: &str, origin: &Path, home: &Path) -> SshConfig {
    let mut parser = Parser {
        home,
        config: SshConfig::default(),
        index: HashMap::new(),
        current: None,
        warned_outside: false,
    };
    parser.config.files.push(origin.to_path_buf());
    parser.parse(text, origin, 0, &mut vec![origin.to_path_buf()]);
    parser.config
}

/// Splits the arguments of a line: whitespace separates, double quotes group.
fn split_args(text: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut in_arg = false;
    for c in text.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                in_arg = true;
            }
            c if c.is_whitespace() && !quoted => {
                if in_arg {
                    args.push(std::mem::take(&mut current));
                    in_arg = false;
                }
            }
            c => {
                current.push(c);
                in_arg = true;
            }
        }
    }
    if quoted {
        return Err("a quote is not closed".to_owned());
    }
    if in_arg {
        args.push(current);
    }
    Ok(args)
}

/// `Keyword args` or `Keyword=args`.
fn split_keyword(line: &str) -> Option<(String, &str)> {
    let line = line.trim();
    let end = line
        .find(|c: char| c.is_whitespace() || c == '=')
        .unwrap_or(line.len());
    let keyword = &line[..end];
    if keyword.is_empty() {
        return None;
    }
    let mut rest = line[end..].trim_start();
    if let Some(after) = rest.strip_prefix('=') {
        rest = after.trim_start();
    }
    Some((keyword.to_ascii_lowercase(), rest))
}

fn has_wildcard(pattern: &str) -> bool {
    pattern.contains(['*', '?', '!'])
}

/// `*` and `?` against one path component.
fn wildcard_match(pattern: &[char], text: &[char]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some(('*', rest)) => (0..=text.len()).any(|skip| wildcard_match(rest, &text[skip..])),
        Some(('?', rest)) => !text.is_empty() && wildcard_match(rest, &text[1..]),
        Some((c, rest)) => text.first() == Some(c) && wildcard_match(rest, &text[1..]),
    }
}

/// The files a glob names, sorted (as glob(3) does); a path without wildcards is returned as
/// it is.
fn expand_glob(pattern: &Path) -> Vec<PathBuf> {
    let mut found = vec![PathBuf::new()];
    for component in pattern.components() {
        let text = component.as_os_str().to_string_lossy().into_owned();
        if !text.contains(['*', '?']) {
            for path in &mut found {
                path.push(component.as_os_str());
            }
            continue;
        }
        let wanted: Vec<char> = text.chars().collect();
        let mut next = Vec::new();
        for dir in &found {
            let Ok(entries) = std::fs::read_dir(if dir.as_os_str().is_empty() {
                Path::new(".")
            } else {
                dir
            }) else {
                continue;
            };
            let mut names: Vec<String> = entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                // Like glob(3): a leading dot must be matched explicitly.
                .filter(|name| !name.starts_with('.') || text.starts_with('.'))
                .filter(|name| wildcard_match(&wanted, &name.chars().collect::<Vec<_>>()))
                .collect();
            names.sort();
            next.extend(names.into_iter().map(|name| dir.join(name)));
        }
        found = next;
    }
    found
}

impl Parser<'_> {
    fn warn(&mut self, file: &Path, line: usize, message: impl Into<String>) {
        self.config.warnings.push(ImportWarning {
            file: file.to_path_buf(),
            line,
            message: message.into(),
        });
    }

    fn read_file(&mut self, path: &Path, depth: usize, chain: &mut Vec<PathBuf>) {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => {
                self.warn(path, 0, format!("could not read: {error}"));
                return;
            }
        };
        if !self.config.files.iter().any(|seen| seen == path) {
            self.config.files.push(path.to_path_buf());
        }
        chain.push(path.to_path_buf());
        self.parse(&text, path, depth, chain);
        chain.pop();
    }

    fn expand_home(&self, path: &str) -> PathBuf {
        let path = path.replace("%d", &self.home.to_string_lossy());
        if path == "~" {
            return self.home.to_path_buf();
        }
        if let Some(rest) = path.strip_prefix("~/") {
            return self.home.join(rest);
        }
        let path = PathBuf::from(path);
        if path.is_absolute() {
            path
        } else {
            // OpenSSH: relative includes in a user config are in ~/.ssh.
            self.home.join(".ssh").join(path)
        }
    }

    fn parse(&mut self, text: &str, file: &Path, depth: usize, chain: &mut Vec<PathBuf>) {
        self.warned_outside = false;
        for (number, raw) in text.lines().enumerate() {
            let line = number + 1;
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let Some((keyword, rest)) = split_keyword(trimmed) else {
                continue;
            };
            let args = match split_args(rest) {
                Ok(args) => args,
                Err(message) => {
                    self.warn(file, line, message);
                    continue;
                }
            };
            match keyword.as_str() {
                "host" => self.start_host(&args, file, line),
                "match" => {
                    // Its options are skipped with the block, without more warnings.
                    self.current = Some(Vec::new());
                    self.warn(
                        file,
                        line,
                        "Match blocks are skipped (OpenSSH still applies them)",
                    );
                }
                "include" => self.include(&args, file, line, depth, chain),
                "hostname" | "user" | "port" | "identityfile" | "proxyjump" => {
                    let Some(aliases) = self.current.clone() else {
                        if !self.warned_outside {
                            self.warned_outside = true;
                            self.warn(
                                file,
                                line,
                                "options outside a Host block with plain names are skipped",
                            );
                        }
                        continue;
                    };
                    for alias in aliases {
                        self.apply(&alias, &keyword, &args, file, line);
                    }
                }
                _ => {}
            }
        }
    }

    fn start_host(&mut self, patterns: &[String], file: &Path, line: usize) {
        let mut aliases = Vec::new();
        for pattern in patterns {
            if has_wildcard(pattern) {
                self.warn(
                    file,
                    line,
                    format!("pattern {pattern} is skipped (only plain host names are imported)"),
                );
                continue;
            }
            if pattern.starts_with('-') {
                self.warn(
                    file,
                    line,
                    format!("{pattern} can't be a host name; skipped"),
                );
                continue;
            }
            if !self.index.contains_key(pattern) {
                self.index.insert(pattern.clone(), self.config.hosts.len());
                self.config.hosts.push(SshHost {
                    alias: pattern.clone(),
                    file: file.to_path_buf(),
                    line,
                    ..SshHost::default()
                });
            }
            aliases.push(pattern.clone());
        }
        self.current = Some(aliases);
    }

    fn include(
        &mut self,
        args: &[String],
        file: &Path,
        line: usize,
        depth: usize,
        chain: &mut Vec<PathBuf>,
    ) {
        if depth + 1 >= MAX_DEPTH {
            self.warn(file, line, "Include is nested too deeply; skipped");
            return;
        }
        for arg in args {
            let pattern = self.expand_home(arg);
            if !arg.contains(['*', '?']) && !pattern.exists() {
                self.warn(file, line, format!("{} doesn't exist", pattern.display()));
                continue;
            }
            let matches = expand_glob(&pattern);
            for path in matches {
                if chain.iter().any(|seen| seen == &path) {
                    self.warn(
                        file,
                        line,
                        format!("{} includes itself; skipped", path.display()),
                    );
                    continue;
                }
                if path.is_dir() {
                    continue;
                }
                // The included file's own Host lines start new blocks; lines before them belong
                // to the block the Include is in, as in OpenSSH.
                let current = self.current.clone();
                let warned = self.warned_outside;
                self.read_file(&path, depth + 1, chain);
                self.current = current;
                self.warned_outside = warned;
            }
        }
    }

    fn apply(&mut self, alias: &str, keyword: &str, args: &[String], file: &Path, line: usize) {
        let Some(&index) = self.index.get(alias) else {
            return;
        };
        let first = args.first().cloned().unwrap_or_default();
        let mut warning = None;
        {
            let host = &mut self.config.hosts[index];
            match keyword {
                "hostname" if host.host_name.is_none() && !first.is_empty() => {
                    let name = first.replace("%h", alias).replace("%%", "%");
                    if name.starts_with('-') {
                        warning = Some(format!("HostName {name} can't be used"));
                    } else {
                        host.host_name = Some(name);
                    }
                }
                "user" if host.user.is_none() && !first.is_empty() => {
                    if first.starts_with('-') {
                        warning = Some(format!("User {first} can't be used"));
                    } else {
                        host.user = Some(first);
                    }
                }
                "port" if host.port.is_none() => match first.parse::<u16>() {
                    Ok(port) if port > 0 => host.port = Some(port),
                    _ => warning = Some(format!("Port {first} is not a port")),
                },
                "identityfile" if !first.is_empty() && !first.eq_ignore_ascii_case("none") => {
                    if !host.identity_files.contains(&first) {
                        host.identity_files.push(first);
                    }
                }
                "proxyjump" if host.proxy_jump.is_none() && !first.is_empty() => {
                    host.proxy_jump = Some(if first.eq_ignore_ascii_case("none") {
                        Vec::new()
                    } else {
                        first
                            .split(',')
                            .map(str::trim)
                            .filter(|hop| !hop.is_empty())
                            .map(str::to_owned)
                            .collect()
                    });
                }
                _ => {}
            }
        }
        if let Some(message) = warning {
            self.warn(file, line, message);
        }
    }
}

/// Hosts of the linked sources of `hosts.toml` (read-only), with what was skipped and the
/// files to watch.
#[must_use]
pub fn load_sources(sources: &[Source], home: &Path) -> SshConfig {
    let mut all = SshConfig::default();
    for source in sources {
        if source.kind != SOURCE_SSH_CONFIG {
            all.warnings.push(ImportWarning {
                file: PathBuf::from(&source.path),
                line: 0,
                message: format!(
                    "linked sources of kind {} are not supported; skipped",
                    source.kind
                ),
            });
            continue;
        }
        let config = load(&expand_tilde(&source.path, home), home);
        for host in config.hosts {
            // A name linked twice keeps its first entry.
            if !all.hosts.iter().any(|seen| seen.alias == host.alias) {
                all.hosts.push(host);
            }
        }
        all.warnings.extend(config.warnings);
        for file in config.files {
            if !all.files.contains(&file) {
                all.files.push(file);
            }
        }
    }
    all
}

impl SshConfig {
    /// The hosts as OpenSesh hosts. Linked ones get `ssh_config:<name>` ids (stable across
    /// reloads, read-only); copies get new ids, in `group` when given.
    #[must_use]
    pub fn to_hosts(&self, linked: bool, group: Option<&str>) -> Vec<Host> {
        self.hosts
            .iter()
            .map(|entry| Host {
                id: if linked {
                    format!("{LINKED_PREFIX}{}", entry.alias)
                } else {
                    new_id()
                },
                name: entry.alias.clone(),
                group: if linked {
                    None
                } else {
                    group.map(str::to_owned)
                },
                address: entry
                    .host_name
                    .clone()
                    .unwrap_or_else(|| entry.alias.clone()),
                port: entry.port,
                user: entry.user.clone(),
                identity_file: entry.identity_files.first().cloned(),
                jump: entry.proxy_jump.clone(),
                ..Host::default()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> SshConfig {
        parse_str(
            text,
            Path::new("/home/me/.ssh/config"),
            Path::new("/home/me"),
        )
    }

    #[test]
    fn keywords_quotes_and_first_value_wins() {
        let config = parse(
            "# comment\n\
             Host web\n\
             \tUser a\n\
             Host web db\n\
             \tUSER b\n\
             \tPort=2222\n\
             \tIdentityFile \"~/.ssh/id deploy\"\n\
             \tIdentityFile ~/.ssh/id_ed25519\n\
             Host bastion\n\
             \tHostName %h.example.com\n\
             \tProxyJump none\n\
             Host internal\n\
             \tProxyJump ops@bastion:2200,hop2\n",
        );
        assert!(config.warnings.is_empty(), "{:?}", config.warnings);
        let names: Vec<&str> = config
            .hosts
            .iter()
            .map(|host| host.alias.as_str())
            .collect();
        assert_eq!(names, vec!["web", "db", "bastion", "internal"]);
        let web = &config.hosts[0];
        assert_eq!((web.user.as_deref(), web.port), (Some("a"), Some(2222)));
        assert_eq!(
            web.identity_files,
            vec!["~/.ssh/id deploy", "~/.ssh/id_ed25519"]
        );
        assert_eq!(config.hosts[1].user.as_deref(), Some("b"));
        assert_eq!(
            config.hosts[2].host_name.as_deref(),
            Some("bastion.example.com")
        );
        assert_eq!(config.hosts[2].proxy_jump, Some(Vec::new()));
        assert_eq!(
            config.hosts[3].proxy_jump,
            Some(vec!["ops@bastion:2200".to_owned(), "hop2".to_owned()])
        );
        assert_eq!(config.hosts[3].line, 12);
    }

    #[test]
    fn wildcards_match_blocks_and_bad_values_are_warned() {
        let config = parse(
            "User everyone\n\
             Host *\n\
             \tServerAliveInterval 30\n\
             Host *.corp !skip real\n\
             \tPort nope\n\
             Match host x\n\
             \tUser y\n\
             Host -oProxyCommand=evil\n\
             Host q\n\
             \tHostName \"unclosed\n",
        );
        assert_eq!(config.hosts.len(), 2);
        assert_eq!(config.hosts[0].alias, "real");
        assert_eq!(config.hosts[0].port, None);
        let text: Vec<String> = config.warnings.iter().map(ToString::to_string).collect();
        assert!(
            text.iter().any(|w| w.contains("outside a Host block")),
            "{text:?}"
        );
        assert!(
            text.iter().any(|w| w.contains("pattern * is skipped")),
            "{text:?}"
        );
        assert!(
            text.iter().any(|w| w.contains("pattern *.corp")),
            "{text:?}"
        );
        assert!(text.iter().any(|w| w.contains("!skip")), "{text:?}");
        assert!(text.iter().any(|w| w.contains("Port nope")), "{text:?}");
        assert!(text.iter().any(|w| w.contains("Match")), "{text:?}");
        assert!(
            text.iter().any(|w| w.contains("can't be a host name")),
            "{text:?}"
        );
        assert!(text.iter().any(|w| w.contains("quote")), "{text:?}");
        assert!(
            text.iter()
                .any(|w| w.starts_with("/home/me/.ssh/config:5:")),
            "{text:?}"
        );
    }

    #[test]
    fn hosts_become_opensesh_hosts() {
        let config = parse(
            "Host web\n\tHostName 10.0.0.5\n\tUser deploy\n\tProxyJump bastion\nHost bastion\n",
        );
        let linked = config.to_hosts(true, None);
        assert_eq!(linked[0].id, "ssh_config:web");
        assert!(linked[0].is_linked());
        assert_eq!(linked[0].address, "10.0.0.5");
        assert_eq!(linked[0].jump, Some(vec!["bastion".to_owned()]));
        assert_eq!(linked[1].address, "bastion");
        let copies = config.to_hosts(false, Some("G"));
        assert_eq!(copies[0].id.len(), 26);
        assert_eq!(copies[0].group.as_deref(), Some("G"));
        assert!(!copies[0].is_linked());
    }

    #[test]
    fn globs() {
        let pattern: Vec<char> = "*.conf".chars().collect();
        assert!(wildcard_match(
            &pattern,
            &"work.conf".chars().collect::<Vec<_>>()
        ));
        assert!(!wildcard_match(
            &pattern,
            &"work.txt".chars().collect::<Vec<_>>()
        ));
        let pattern: Vec<char> = "h?st".chars().collect();
        assert!(wildcard_match(
            &pattern,
            &"host".chars().collect::<Vec<_>>()
        ));
    }
}
