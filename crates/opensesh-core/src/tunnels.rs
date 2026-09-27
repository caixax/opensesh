//! Tunnels (port forwarding) in `tunnels.toml` (PLAN §4.2, Sprint 9).
//!
//! ```toml
//! schema_version = 1
//!
//! [[tunnel]]
//! id = "01J..."
//! name = "Postgres"
//! kind = "local"                  # local (-L), remote (-R) or dynamic (-D, SOCKS5)
//! host = "01H..."                 # a saved host's id, or...
//! target = "deploy@bastion:2222"  # ...quick-connect text (one of the two)
//! bind_address = "127.0.0.1"
//! bind_port = 5432
//! destination_host = "db.internal"
//! destination_port = 5432
//! tied = false                    # runs with the host's terminal sessions
//! autostart = false               # starts with OpenSesh (independent tunnels)
//! reconnect = true                # reconnects by itself (independent tunnels)
//! ```
//!
//! What the addresses mean depends on the kind:
//! - **local:** listens on this computer at `bind_*`; each connection goes, from the server, to
//!   `destination_*`.
//! - **remote:** the server listens at `bind_*`; each connection comes back and goes, from this
//!   computer, to `destination_*`.
//! - **dynamic:** listens on this computer at `bind_*` as a SOCKS5 proxy; each connection goes,
//!   from the server, where the program asked. It has no destination.
//!
//! A port of 0 lets the system (or the server) pick one. Entries that can't be used are skipped
//! with a warning; unknown keys are written back as they were.

use std::net::IpAddr;
use std::path::Path;

use toml::{Table, Value};

use crate::config::Warning;

/// Name of the file inside the config directory.
pub const TUNNELS_FILE: &str = "tunnels.toml";

/// Current layout of the file.
pub const SCHEMA_VERSION: i64 = 1;

/// Longest name, id, address or target kept.
const MAX_TEXT: usize = 255;

const HEADER: &str = "# OpenSesh tunnels (port forwarding). OpenSesh rewrites this file when a tunnel\n\
                      # is changed in the app: comments are not kept.\n";

/// The kind of forwarding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    /// `-L`: a port here reaches a destination from the server.
    #[default]
    Local,
    /// `-R`: a port on the server reaches a destination from here.
    Remote,
    /// `-D`: a SOCKS5 proxy here, connecting from the server.
    Dynamic,
}

impl Kind {
    /// The name in the file.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
            Self::Dynamic => "dynamic",
        }
    }

    /// The kind named `text`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "local" => Some(Self::Local),
            "remote" => Some(Self::Remote),
            "dynamic" => Some(Self::Dynamic),
            _ => None,
        }
    }
}

/// One tunnel.
#[derive(Debug, Clone, PartialEq)]
pub struct Tunnel {
    /// Stable id (a ULID).
    pub id: String,
    /// What the user calls it (may be empty: the route is shown instead).
    pub name: String,
    /// The kind.
    pub kind: Kind,
    /// A saved host's id, or empty.
    pub host: String,
    /// Quick-connect text (`user@host:port`), used when `host` is empty.
    pub target: String,
    /// Where it listens: here (local, dynamic) or on the server (remote).
    pub bind_address: String,
    /// Its port (0: picked when it starts).
    pub bind_port: u16,
    /// Where connections go (not for dynamic).
    pub destination_host: String,
    /// Their port.
    pub destination_port: u16,
    /// Runs with the host's terminal sessions, on their connection (needs a saved host).
    pub tied: bool,
    /// Starts with OpenSesh (independent tunnels).
    pub autostart: bool,
    /// Reconnects by itself after the connection is lost (independent tunnels).
    pub reconnect: bool,
    /// Keys this version doesn't know, written back unchanged.
    pub extra: Table,
}

impl Default for Tunnel {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            kind: Kind::Local,
            host: String::new(),
            target: String::new(),
            bind_address: "127.0.0.1".to_owned(),
            bind_port: 0,
            destination_host: String::new(),
            destination_port: 0,
            tied: false,
            autostart: false,
            reconnect: true,
            extra: Table::new(),
        }
    }
}

/// Whether `address` only listens on this machine's loopback: `localhost`, `127.x.x.x` or `::1`
/// (bracketed or not). Empty, `*` and `0.0.0.0` listen everywhere.
#[must_use]
pub fn is_loopback(address: &str) -> bool {
    let address = address.trim();
    let address = address
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(address);
    address.eq_ignore_ascii_case("localhost")
        || address.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// Whether `text` can be an address or a host name in the file: not too long, no whitespace or
/// control characters.
fn valid_address(text: &str) -> bool {
    text.len() <= MAX_TEXT && !text.chars().any(|c| c.is_whitespace() || c.is_control())
}

impl Tunnel {
    /// A new tunnel with a fresh id.
    #[must_use]
    pub fn new(kind: Kind) -> Self {
        Self {
            id: crate::hosts::new_id(),
            kind,
            ..Self::default()
        }
    }

    /// Whether it listens beyond the loopback: on this computer's network (local, dynamic) or on
    /// the server's (remote). The UI warns about it.
    #[must_use]
    pub fn exposed(&self) -> bool {
        !is_loopback(&self.bind_address)
    }

    /// Why it can't be used, if it can't (the first problem found).
    #[must_use]
    pub fn problem(&self) -> Option<&'static str> {
        if self.host.trim().is_empty() && self.target.trim().is_empty() {
            return Some("no host to go through");
        }
        if self.tied && self.host.trim().is_empty() {
            return Some("a tunnel tied to a host needs a saved host");
        }
        if !valid_address(&self.bind_address) {
            return Some("the listening address is not valid");
        }
        if self.kind != Kind::Dynamic {
            if self.destination_host.trim().is_empty() || !valid_address(&self.destination_host) {
                return Some("the destination host is not valid");
            }
            if self.destination_port == 0 {
                return Some("the destination port must be 1 to 65535");
            }
        }
        if self.name.chars().count() > MAX_TEXT || self.name.chars().any(char::is_control) {
            return Some("the name is too long or has control characters");
        }
        None
    }

    fn from_table(mut table: Table, index: usize, warnings: &mut Vec<Warning>) -> Option<Self> {
        let key = |name: &str| {
            if name.is_empty() {
                format!("tunnel[{index}]")
            } else {
                format!("tunnel[{index}].{name}")
            }
        };
        let mut warn = |name: &str, message: &str| {
            warnings.push(Warning {
                key: key(name),
                message: message.to_owned(),
            });
        };
        let text = |table: &mut Table, name: &str, warn: &mut dyn FnMut(&str, &str)| match table
            .remove(name)
        {
            None => String::new(),
            Some(Value::String(value)) if value.len() <= MAX_TEXT => value.trim().to_owned(),
            Some(Value::String(_)) => {
                warn(name, "too long; ignored");
                String::new()
            }
            Some(other) => {
                warn(
                    name,
                    &format!("expected a string, found {}", other.type_str()),
                );
                String::new()
            }
        };
        let id = text(&mut table, "id", &mut warn);
        if id.is_empty() || id.chars().any(char::is_control) {
            warn("id", "missing or not valid; the tunnel is skipped");
            return None;
        }
        let kind_text = text(&mut table, "kind", &mut warn);
        let Some(kind) = Kind::parse(&kind_text) else {
            warn(
                "kind",
                "expected local, remote or dynamic; the tunnel is skipped",
            );
            return None;
        };
        let port = |table: &mut Table, name: &str, warn: &mut dyn FnMut(&str, &str)| match table
            .remove(name)
        {
            None => 0,
            Some(Value::Integer(value)) => u16::try_from(value).unwrap_or_else(|_| {
                warn(name, "must be 0 to 65535; ignored");
                0
            }),
            Some(other) => {
                warn(
                    name,
                    &format!("expected a number, found {}", other.type_str()),
                );
                0
            }
        };
        let flag =
            |table: &mut Table, name: &str, default: bool, warn: &mut dyn FnMut(&str, &str)| {
                match table.remove(name) {
                    None => default,
                    Some(Value::Boolean(value)) => value,
                    Some(other) => {
                        warn(
                            name,
                            &format!("expected true or false, found {}", other.type_str()),
                        );
                        default
                    }
                }
            };
        let name = text(&mut table, "name", &mut warn);
        let host = text(&mut table, "host", &mut warn);
        let target = text(&mut table, "target", &mut warn);
        let bind_address = if table.contains_key("bind_address") {
            text(&mut table, "bind_address", &mut warn)
        } else {
            "127.0.0.1".to_owned()
        };
        let bind_port = port(&mut table, "bind_port", &mut warn);
        let destination_host = text(&mut table, "destination_host", &mut warn);
        let destination_port = port(&mut table, "destination_port", &mut warn);
        let tied = flag(&mut table, "tied", false, &mut warn);
        let autostart = flag(&mut table, "autostart", false, &mut warn);
        let reconnect = flag(&mut table, "reconnect", true, &mut warn);
        let tunnel = Self {
            id,
            name,
            kind,
            host,
            target,
            bind_address,
            bind_port,
            destination_host,
            destination_port,
            tied,
            autostart,
            reconnect,
            extra: table,
        };
        if let Some(problem) = tunnel.problem() {
            warn("", &format!("{problem}; the tunnel is skipped"));
            return None;
        }
        Some(tunnel)
    }

    fn to_table(&self) -> Table {
        let mut table = self.extra.clone();
        let mut put = |name: &str, value: Value| {
            table.insert(name.to_owned(), value);
        };
        put("id", Value::String(self.id.clone()));
        if !self.name.is_empty() {
            put("name", Value::String(self.name.clone()));
        }
        put("kind", Value::String(self.kind.as_str().to_owned()));
        if !self.host.is_empty() {
            put("host", Value::String(self.host.clone()));
        }
        if !self.target.is_empty() {
            put("target", Value::String(self.target.clone()));
        }
        put("bind_address", Value::String(self.bind_address.clone()));
        put("bind_port", Value::Integer(i64::from(self.bind_port)));
        if self.kind != Kind::Dynamic {
            put(
                "destination_host",
                Value::String(self.destination_host.clone()),
            );
            put(
                "destination_port",
                Value::Integer(i64::from(self.destination_port)),
            );
        }
        put("tied", Value::Boolean(self.tied));
        put("autostart", Value::Boolean(self.autostart));
        put("reconnect", Value::Boolean(self.reconnect));
        table
    }
}

/// The contents of `tunnels.toml`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TunnelsFile {
    /// The tunnels, in the order of the file.
    pub tunnels: Vec<Tunnel>,
    /// Unknown top-level keys, written back unchanged.
    pub extra: Table,
    /// The file comes from a newer OpenSesh, or couldn't be read or parsed: never overwrite it.
    pub read_only: bool,
}

impl TunnelsFile {
    /// Reads the file (a missing file means no tunnels).
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
                    key: TUNNELS_FILE.to_owned(),
                    message: format!("could not read the file: {error}"),
                }],
            ),
        }
    }

    /// Parses the file's text; tunnels that can't be used are skipped with a warning.
    #[must_use]
    pub fn from_toml_str(text: &str) -> (Self, Vec<Warning>) {
        let mut warnings = Vec::new();
        let mut root: Table = match text.parse() {
            Ok(root) => root,
            Err(error) => {
                let error: toml::de::Error = error;
                warnings.push(Warning {
                    key: TUNNELS_FILE.to_owned(),
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
        let mut tunnels: Vec<Tunnel> = Vec::new();
        match root.remove("tunnel") {
            None => {}
            Some(Value::Array(entries)) => {
                for (index, entry) in entries.into_iter().enumerate() {
                    match entry {
                        Value::Table(table) => {
                            if let Some(tunnel) = Tunnel::from_table(table, index, &mut warnings) {
                                if tunnels.iter().any(|seen| seen.id == tunnel.id) {
                                    warnings.push(Warning {
                                        key: format!("tunnel[{index}].id"),
                                        message: "used twice; the second is skipped".to_owned(),
                                    });
                                } else {
                                    tunnels.push(tunnel);
                                }
                            }
                        }
                        other => warnings.push(Warning {
                            key: format!("tunnel[{index}]"),
                            message: format!("expected a table, found {}", other.type_str()),
                        }),
                    }
                }
            }
            Some(other) => warnings.push(Warning {
                key: "tunnel".to_owned(),
                message: format!("expected an array of tables, found {}", other.type_str()),
            }),
        }
        (
            Self {
                tunnels,
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
            "tunnel".into(),
            Value::Array(
                self.tunnels
                    .iter()
                    .map(|tunnel| Value::Table(tunnel.to_table()))
                    .collect(),
            ),
        );
        format!("{HEADER}\n{root}")
    }

    /// The tunnel with `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Tunnel> {
        self.tunnels.iter().find(|tunnel| tunnel.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_addresses() {
        for address in [
            "127.0.0.1",
            "127.1.2.3",
            "localhost",
            "LOCALHOST",
            "::1",
            "[::1]",
        ] {
            assert!(is_loopback(address), "{address}");
        }
        for address in [
            "",
            "*",
            "0.0.0.0",
            "::",
            "192.168.1.10",
            "example.com",
            "[::]",
        ] {
            assert!(!is_loopback(address), "{address}");
        }
    }

    #[test]
    fn round_trip() {
        let text = r#"
schema_version = 1
future = "kept"

[[tunnel]]
id = "A"
name = "Postgres"
kind = "local"
host = "H1"
bind_port = 5432
destination_host = "db.internal"
destination_port = 5432
color = "kept too"

[[tunnel]]
id = "B"
kind = "dynamic"
target = "me@bastion:2222"
bind_address = "0.0.0.0"
bind_port = 1080
autostart = true
reconnect = false

[[tunnel]]
id = "C"
kind = "remote"
host = "H2"
tied = true
bind_port = 0
destination_host = "localhost"
destination_port = 3000
"#;
        let (file, warnings) = TunnelsFile::from_toml_str(text);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(!file.read_only);
        assert_eq!(file.tunnels.len(), 3);
        let postgres = file.get("A").unwrap();
        assert_eq!(postgres.bind_address, "127.0.0.1");
        assert!(!postgres.exposed() && postgres.reconnect && !postgres.autostart);
        assert_eq!(
            postgres.extra.get("color"),
            Some(&Value::String("kept too".into()))
        );
        let socks = file.get("B").unwrap();
        assert_eq!(socks.kind, Kind::Dynamic);
        assert!(socks.exposed() && socks.autostart && !socks.reconnect);
        let remote = file.get("C").unwrap();
        assert!(remote.tied && remote.kind == Kind::Remote && remote.bind_port == 0);
        let (again, warnings) = TunnelsFile::from_toml_str(&file.to_toml_string());
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(again, file);
        assert!(file.to_toml_string().contains("future = \"kept\""));
    }

    #[test]
    fn bad_entries_are_skipped() {
        let text = r#"
[[tunnel]]
kind = "local"
target = "x"

[[tunnel]]
id = "no-kind"
target = "x"

[[tunnel]]
id = "no-destination"
kind = "local"
target = "x"
bind_port = 8080

[[tunnel]]
id = "tied-without-host"
kind = "dynamic"
target = "x"
tied = true

[[tunnel]]
id = "big-port"
kind = "dynamic"
target = "x"
bind_port = 70000

[[tunnel]]
id = "big-port"
kind = "dynamic"
target = "y"
"#;
        let (file, warnings) = TunnelsFile::from_toml_str(text);
        let ids: Vec<&str> = file
            .tunnels
            .iter()
            .map(|tunnel| tunnel.id.as_str())
            .collect();
        assert_eq!(ids, ["big-port"]);
        assert_eq!(file.tunnels[0].bind_port, 0);
        assert!(warnings.iter().any(|w| w.key == "tunnel[0].id"));
        assert!(warnings.iter().any(|w| w.key == "tunnel[1].kind"));
        assert!(
            warnings
                .iter()
                .any(|w| w.message.contains("destination host"))
        );
        assert!(warnings.iter().any(|w| w.message.contains("saved host")));
        assert!(warnings.iter().any(|w| w.key == "tunnel[4].bind_port"));
        assert!(warnings.iter().any(|w| w.message.contains("used twice")));
    }

    #[test]
    fn newer_or_broken_files_are_read_only() {
        assert!(TunnelsFile::from_toml_str("schema_version = 2").0.read_only);
        assert!(TunnelsFile::from_toml_str("this is not toml").0.read_only);
        let (empty, warnings) = TunnelsFile::from_toml_str("");
        assert!(empty.tunnels.is_empty() && warnings.is_empty() && !empty.read_only);
    }

    #[test]
    fn new_tunnels_have_ids_and_defaults() {
        let tunnel = Tunnel::new(Kind::Remote);
        assert_eq!(tunnel.id.len(), 26);
        assert_eq!(tunnel.bind_address, "127.0.0.1");
        assert_eq!(tunnel.problem(), Some("no host to go through"));
    }
}
