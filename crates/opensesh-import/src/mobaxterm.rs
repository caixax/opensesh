//! MobaXterm's sessions (Sprint 16): `.mxtsessions` exports, single-session `.moba` files and
//! the `[Bookmarks]` sections of `MobaXterm.ini` (`%APPDATA%\MobaXterm`, or next to a portable
//! `MobaXterm.exe`).
//!
//! The format, from its public description (`Ruzgfpegk/sessionator` and its `.mxtsessions`
//! notes, MobaXterm 23.6): an INI file in Windows-1252; `[Bookmarks]`, `[Bookmarks_1]`... are
//! folders, with the folder's path in `SubRep` (`Parent\Child`); every other line is a session,
//! `name=<flag>#<icon>#<type>%<fields...>#<terminal>#<start>#<comment>#<color>`. In the fields
//! `__PTVIRG__`, `__DBLQUO__`, `__PIPE__`, `__DIEZE__` and `__PERCENT__` stand for `;`, `"`,
//! `|`, `#` and `%`, `__PIPE__` also separates the SSH gateways of a list, and `_CurrentDrive_`
//! the drive letter of key paths.
//!
//! SSH (type 0), RDP (4), VNC (5) and SFTP (7) are described field by field and imported; the
//! other types (Telnet, Serial, Mosh...) are skipped with a warning. Passwords are never read
//! (MobaXterm keeps them elsewhere, encrypted).

use std::path::Path;

use opensesh_core::hosts::{Host, Protocol};

use crate::common::{
    ImportWarning, Imported, decode_text, folder_path, is_putty_key, jump_spec, proxy_command,
    putty_key_warning,
};

/// MobaXterm's session types that are imported.
const SSH: &str = "0";
const RDP: &str = "4";
const VNC: &str = "5";
const SFTP: &str = "7";

/// Reads a `.mxtsessions`, `.moba` or `MobaXterm.ini` file.
pub fn load(path: &Path) -> Imported {
    match crate::common::read_limited(path, crate::common::MAX_FILE) {
        Ok(bytes) => parse_str(&decode_text(&bytes), path),
        Err(error) => Imported {
            warnings: vec![ImportWarning::new(
                path,
                format!("could not be read: {error}"),
            )],
            ..Imported::default()
        },
    }
}

/// Reads the sessions in `text`; `origin` names it in warnings.
#[must_use]
pub fn parse_str(text: &str, origin: &Path) -> Imported {
    let mut imported = Imported::default();
    // A `.moba` file may be a bare session line; `MobaXterm.ini` has other sections first.
    let mut in_bookmarks = true;
    let mut folder: Vec<String> = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim_end_matches('\r');
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(';') {
            continue;
        }
        if let Some(section) = trimmed
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            in_bookmarks = section == "Bookmarks"
                || section
                    .strip_prefix("Bookmarks_")
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
            folder.clear();
            continue;
        }
        if !in_bookmarks {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "SubRep" => folder = folder_path(value, '\\'),
            "ImgNum" => {}
            name => {
                let line = index + 1;
                let mut reader = Session {
                    imported: &mut imported,
                    origin,
                    line,
                    name: name.to_owned(),
                };
                if let Some(mut host) = reader.read(value) {
                    host.group = reader.imported.group(&folder);
                    imported.hosts.push(host);
                }
            }
        }
    }
    imported
}

/// One session line being read.
struct Session<'a> {
    imported: &'a mut Imported,
    origin: &'a Path,
    line: usize,
    name: String,
}

impl Session<'_> {
    fn warn(&mut self, message: String) {
        self.imported.warnings.push(ImportWarning {
            file: self.origin.to_path_buf(),
            line: self.line,
            message: format!("{}: {message}", self.name),
        });
    }

    fn read(&mut self, value: &str) -> Option<Host> {
        let parts: Vec<&str> = value.split('#').collect();
        if parts.len() < 3 {
            self.warn("not a session line; skipped".to_owned());
            return None;
        }
        let fields: Vec<&str> = parts[2].split('%').collect();
        let field = |index: usize| fields.get(index).map_or("", |field| field.trim());
        let kind = field(0);
        if kind.is_empty() || !kind.bytes().all(|b| b.is_ascii_digit()) {
            self.warn("not a session line; skipped".to_owned());
            return None;
        }
        let mut host = Host {
            name: self.name.clone(),
            address: unescape(field(1)),
            notes: parts.get(5).map(|c| unescape(c.trim())).unwrap_or_default(),
            ..Host::default()
        };
        match kind {
            SSH => {
                host.protocol = Protocol::Ssh;
                host.port = port(field(2), 22);
                host.user = user(field(3));
                let command = unescape(field(7));
                if !command.is_empty() {
                    host.ssh.command = Some(command);
                }
                host.jump = self.gateways(field(8), field(9), field(10), field(15));
                self.key(&mut host, field(14));
                self.proxy(
                    &mut host,
                    field(19),
                    field(20),
                    field(21),
                    field(22),
                    field(26),
                );
                if field(34) == "-1" {
                    host.ssh.agent_forwarding = Some(true);
                }
            }
            SFTP => {
                host.protocol = Protocol::Sftp;
                host.port = port(field(2), 22);
                host.user = user(field(3));
                self.key(&mut host, field(9));
                if !matches!(field(10), "" | "0") {
                    self.warn("the proxy was left out".to_owned());
                }
            }
            RDP => {
                host.protocol = Protocol::Rdp;
                host.port = port(field(2), 3389);
                host.user = user(field(3));
                host.jump = self.gateways(field(13), field(14), field(15), field(18));
                if field(19) == "0" {
                    host.rdp.clipboard = Some(false);
                }
            }
            VNC => {
                host.protocol = Protocol::Vnc;
                host.port = port(field(2), 5900);
                if field(4) == "-1" {
                    host.vnc.read_only = Some(true);
                }
                host.jump = self.gateways(field(5), field(6), field(7), field(8));
            }
            other => {
                self.warn(format!(
                    "{} sessions aren't imported yet; skipped",
                    type_name(other)
                ));
                return None;
            }
        }
        if host.address.is_empty() {
            self.warn("no remote host; skipped".to_owned());
            return None;
        }
        Some(host)
    }

    /// The SSH gateways (jump hosts), first hop first.
    fn gateways(
        &mut self,
        hosts: &str,
        ports: &str,
        users: &str,
        keys: &str,
    ) -> Option<Vec<String>> {
        let hosts = list(hosts);
        if hosts.iter().all(String::is_empty) {
            return None;
        }
        let ports = list(ports);
        let users = list(users);
        if list(keys).iter().any(|key| !key.is_empty()) {
            self.warn("the gateways' own keys were left out".to_owned());
        }
        Some(
            hosts
                .iter()
                .enumerate()
                .filter(|(_, host)| !host.is_empty())
                .map(|(at, host)| {
                    jump_spec(
                        users.get(at).and_then(|user| self::user(user)).as_deref(),
                        host,
                        ports.get(at).and_then(|port| port.parse().ok()),
                    )
                })
                .collect(),
        )
    }

    fn key(&mut self, host: &mut Host, key: &str) {
        let key = unescape(key);
        if key.is_empty() {
            return;
        }
        if is_putty_key(&key) {
            let warning = putty_key_warning(self.origin, &self.name, &key);
            self.imported.warnings.push(ImportWarning {
                line: self.line,
                ..warning
            });
        } else {
            host.identity_file = Some(key);
        }
    }

    /// The SSH proxy: SOCKS 5, HTTP or a local command; the others are left out.
    fn proxy(
        &mut self,
        host: &mut Host,
        kind: &str,
        address: &str,
        port: &str,
        login: &str,
        command: &str,
    ) {
        let scheme = match kind {
            "" | "0" => return,
            "2" => "socks5",
            "3" => "http",
            "5" => {
                let (command, password) = proxy_command(&unescape(command));
                if password {
                    self.warn("the proxy command's %pass was left as written".to_owned());
                }
                if !command.is_empty() {
                    host.ssh.proxy_command = Some(command);
                }
                return;
            }
            _ => {
                self.warn("the proxy (SOCKS 4, Telnet or SSH) was left out".to_owned());
                return;
            }
        };
        let address = unescape(address);
        if address.is_empty() {
            return;
        }
        let login = unescape(login);
        let user = if login.is_empty() {
            String::new()
        } else {
            format!("{login}@")
        };
        let port = port.parse::<u16>().unwrap_or(1080);
        host.ssh.proxy = Some(format!("{scheme}://{user}{address}:{port}"));
    }
}

/// The session types MobaXterm's notes name, for the warnings.
fn type_name(kind: &str) -> String {
    match kind {
        "11" => "Browser".to_owned(),
        other => format!("Type {other}"),
    }
}

/// A field with MobaXterm's escapes undone.
fn unescape(text: &str) -> String {
    text.replace("__PTVIRG__", ";")
        .replace("__DBLQUO__", "\"")
        .replace("__PIPE__", "|")
        .replace("__DIEZE__", "#")
        .replace("__PERCENT__", "%")
        .replace("_CurrentDrive_", "C")
}

/// A `__PIPE__`-separated list.
fn list(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    text.split("__PIPE__")
        .map(|item| unescape(item.trim()))
        .collect()
}

/// A port, unset when it is the protocol's usual one or not a number.
fn port(text: &str, usual: u16) -> Option<u16> {
    text.parse()
        .ok()
        .filter(|port| *port != usual && *port != 0)
}

/// A user name; `<default>` is MobaXterm's default login, which isn't known here.
fn user(text: &str) -> Option<String> {
    let text = unescape(text.trim());
    (!text.is_empty() && text != "<default>").then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Imported {
        parse_str(text, Path::new("test.mxtsessions"))
    }

    #[test]
    fn a_bare_session_line() {
        let imported = parse(
            "Deck=#109#0%192.168.137.40%22%deck%%0%0%%%%%0%0%0%%%-1%0%0%0%%1080%%0%0%1%#MobaFont%10#0# #-1\r\n",
        );
        assert_eq!(imported.warnings, []);
        let deck = &imported.hosts[0];
        assert_eq!(deck.name, "Deck");
        assert_eq!(deck.address, "192.168.137.40");
        assert_eq!(deck.port, None);
        assert_eq!(deck.user.as_deref(), Some("deck"));
        assert_eq!(deck.group, None);
        assert_eq!(deck.notes, "");
    }

    #[test]
    fn escapes_and_lists() {
        assert_eq!(unescape("a__PTVIRG__b__DIEZE__c__PERCENT__d"), "a;b#c%d");
        assert_eq!(unescape(r"_CurrentDrive_:\keys\a"), r"C:\keys\a");
        assert_eq!(list("a__PIPE__b__PIPE__"), ["a", "b", ""]);
        assert_eq!(list(""), Vec::<String>::new());
        assert_eq!(user("<default>"), None);
        assert_eq!(port("22", 22), None);
        assert_eq!(port("2222", 22), Some(2222));
    }
}
