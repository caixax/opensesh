//! PuTTY's saved sessions (Sprint 16), from wherever PuTTY keeps them:
//!
//! - **Windows:** the registry, `HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions`, one key
//!   per session ([`read_registry`]);
//! - **a `.reg` file** exported from there with `regedit` (to bring sessions from another
//!   computer, [`load_reg`]);
//! - **Unix:** one file per session in `~/.config/putty/sessions` or `~/.putty/sessions`
//!   ([`load_dir`]), with `Key=Value` lines.
//!
//! Session names are escaped as PuTTY writes them (`%XX` for the unsafe bytes). The values read
//! are the ones PuTTY's `settings.c` writes: `HostName`, `PortNumber`, `Protocol`, `UserName`,
//! `PublicKeyFile`, `ProxyMethod` and the other `Proxy*` values, `AgentFwd`, `X11Forward`,
//! `Compression`, `RemoteCommand` and the `Serial*` values. SSH, Telnet and serial sessions are
//! imported; `Default Settings` and the other protocols (raw, rlogin, SUPDUP) are not. Passwords
//! are never read (PuTTY doesn't save them).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use opensesh_core::hosts::{FlowControl, Host, Parity, Protocol, X11Forwarding};

use crate::common::{
    ImportWarning, Imported, decode_text, is_putty_key, percent_decode, proxy_command,
    putty_key_warning,
};

/// Where PuTTY keeps its sessions under `HKEY_CURRENT_USER`.
pub const REGISTRY_KEY: &str = r"Software\SimonTatham\PuTTY\Sessions";

/// The settings new sessions start from, which isn't a session.
const DEFAULT_SETTINGS: &str = "Default Settings";

/// One saved session's values as PuTTY stores them (numbers as decimal text).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawSession {
    /// The session's name, unescaped.
    pub name: String,
    /// Its values by name.
    pub values: BTreeMap<String, String>,
    /// Where it was read (a file, or a registry key), for warnings.
    pub origin: PathBuf,
}

impl RawSession {
    fn text(&self, key: &str) -> Option<&str> {
        self.values
            .get(key)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
    }

    fn number(&self, key: &str) -> Option<u32> {
        self.text(key)?.parse().ok()
    }

    fn flag(&self, key: &str) -> bool {
        self.number(key).is_some_and(|value| value != 0)
    }
}

/// The sessions in this user's registry (Windows), as hosts.
#[cfg(windows)]
#[must_use]
pub fn read_registry() -> Imported {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::types::FromRegValue;

    let origin = PathBuf::from(format!(r"HKEY_CURRENT_USER\{REGISTRY_KEY}"));
    let Ok(sessions) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(REGISTRY_KEY) else {
        return Imported {
            warnings: vec![ImportWarning::new(
                &origin,
                "PuTTY has no saved sessions here",
            )],
            ..Imported::default()
        };
    };
    let mut raw = Vec::new();
    for escaped in sessions.enum_keys().flatten() {
        let Ok(key) = sessions.open_subkey(&escaped) else {
            continue;
        };
        let mut values = BTreeMap::new();
        for (name, value) in key.enum_values().flatten() {
            let text = String::from_reg_value(&value)
                .ok()
                .or_else(|| u32::from_reg_value(&value).ok().map(|n| n.to_string()));
            if let Some(text) = text {
                values.insert(name, text);
            }
        }
        raw.push(RawSession {
            name: percent_decode(&escaped),
            values,
            origin: origin.join(&escaped),
        });
    }
    convert(raw)
}

/// The registry only exists on Windows.
#[cfg(not(windows))]
#[must_use]
pub fn read_registry() -> Imported {
    Imported::default()
}

/// The usual sessions folder on Unix: `~/.config/putty/sessions` (or `$XDG_CONFIG_HOME`'s) when
/// it exists, else `~/.putty/sessions`.
#[must_use]
pub fn sessions_dir(home: &Path, xdg_config_home: Option<&Path>) -> PathBuf {
    let config = xdg_config_home.map_or_else(|| home.join(".config"), Path::to_path_buf);
    let xdg = config.join("putty").join("sessions");
    if xdg.is_dir() {
        xdg
    } else {
        home.join(".putty").join("sessions")
    }
}

/// Reads a sessions folder (one file per session).
#[must_use]
pub fn load_dir(dir: &Path) -> Imported {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            return Imported {
                warnings: vec![ImportWarning::new(
                    dir,
                    format!("could not be read: {error}"),
                )],
                ..Imported::default()
            };
        }
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    files.sort();
    let raw = files
        .into_iter()
        .filter_map(|path| {
            let escaped = path.file_name()?.to_string_lossy().into_owned();
            // A small file the user asked to import.
            let bytes = std::fs::read(&path).ok()?;
            Some(parse_session_file(
                &percent_decode(&escaped),
                &decode_text(&bytes),
                &path,
            ))
        })
        .collect();
    convert(raw)
}

/// One session file's `Key=Value` lines.
#[must_use]
pub fn parse_session_file(name: &str, text: &str, origin: &Path) -> RawSession {
    let values = text
        .lines()
        .filter_map(|line| {
            let (key, value) = line.trim_end_matches('\r').split_once('=')?;
            Some((key.to_owned(), value.to_owned()))
        })
        .collect();
    RawSession {
        name: name.to_owned(),
        values,
        origin: origin.to_path_buf(),
    }
}

/// Reads a `.reg` file exported with `regedit`.
#[must_use]
pub fn load_reg(path: &Path) -> Imported {
    match std::fs::read(path) {
        Ok(bytes) => convert(parse_reg(&decode_text(&bytes), path)),
        Err(error) => Imported {
            warnings: vec![ImportWarning::new(
                path,
                format!("could not be read: {error}"),
            )],
            ..Imported::default()
        },
    }
}

/// The PuTTY sessions in a `.reg` file's text (`regedit`'s format: `[key]` lines, then
/// `"Name"="text"` and `"Name"=dword:0000abcd` values).
#[must_use]
pub fn parse_reg(text: &str, origin: &Path) -> Vec<RawSession> {
    let mut sessions: Vec<RawSession> = Vec::new();
    let mut current: Option<RawSession> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(key) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            sessions.extend(current.take());
            current = session_key(key).map(|escaped| RawSession {
                name: percent_decode(escaped),
                values: BTreeMap::new(),
                origin: origin.to_path_buf(),
            });
            continue;
        }
        let Some(session) = current.as_mut() else {
            continue;
        };
        if let Some((name, value)) = reg_value(line) {
            session.values.insert(name, value);
        }
    }
    sessions.extend(current);
    sessions
}

/// The escaped session name of a `[...\SimonTatham\PuTTY\Sessions\<name>]` key; `None` for
/// other keys, removals (`[-...]`) and keys below a session.
fn session_key(key: &str) -> Option<&str> {
    if key.starts_with('-') {
        return None;
    }
    let marker = r"\simontatham\putty\sessions\";
    let at = key.to_ascii_lowercase().find(marker)?;
    let name = &key[at + marker.len()..];
    (!name.is_empty() && !name.contains('\\')).then_some(name)
}

/// A `"Name"="text"` or `"Name"=dword:...` line.
fn reg_value(line: &str) -> Option<(String, String)> {
    let (name, rest) = quoted(line)?;
    let rest = rest.trim_start().strip_prefix('=')?.trim_start();
    if let Some(hex) = rest.strip_prefix("dword:") {
        let number = u32::from_str_radix(hex.trim(), 16).ok()?;
        return Some((name, number.to_string()));
    }
    let (value, _) = quoted(rest)?;
    Some((name, value))
}

/// A `"..."` string with `\\` and `\"` escapes at the start of `text`, and what follows it.
fn quoted(text: &str) -> Option<(String, &str)> {
    let mut chars = text.strip_prefix('"')?.char_indices();
    let mut out = String::new();
    while let Some((at, c)) = chars.next() {
        match c {
            '"' => return Some((out, &text[at + 2..])),
            '\\' => out.push(chars.next().map_or('\\', |(_, next)| next)),
            other => out.push(other),
        }
    }
    None
}

/// The sessions as hosts.
#[must_use]
pub fn convert(sessions: Vec<RawSession>) -> Imported {
    let mut imported = Imported::default();
    for session in sessions {
        if session.name == DEFAULT_SETTINGS {
            continue;
        }
        if let Some(host) = convert_one(&session, &mut imported.warnings) {
            imported.hosts.push(host);
        }
    }
    imported
}

fn convert_one(session: &RawSession, warnings: &mut Vec<ImportWarning>) -> Option<Host> {
    let name = &session.name;
    let mut warn = |message: String| {
        warnings.push(ImportWarning::new(
            &session.origin,
            format!("{name}: {message}"),
        ));
    };
    // PuTTY's default protocol is SSH.
    let protocol = match session.text("Protocol").unwrap_or("ssh") {
        "ssh" => Protocol::Ssh,
        "telnet" => Protocol::Telnet,
        "serial" => Protocol::Serial,
        other => {
            warn(format!("{other} sessions aren't supported; skipped"));
            return None;
        }
    };
    let mut host = Host {
        name: name.clone(),
        protocol,
        ..Host::default()
    };
    if protocol == Protocol::Serial {
        let Some(line) = session.text("SerialLine") else {
            warn("no serial line; skipped".to_owned());
            return None;
        };
        host.address = line.to_owned();
        serial(session, &mut host, &mut warn);
        return Some(host);
    }
    let Some(address) = session.text("HostName") else {
        warn("no host name; skipped".to_owned());
        return None;
    };
    // PuTTY takes `user@host` in the host name.
    match address.rsplit_once('@') {
        Some((user, address)) => {
            host.address = address.to_owned();
            host.user = Some(user.to_owned());
        }
        None => host.address = address.to_owned(),
    }
    if let Some(user) = session.text("UserName") {
        host.user = Some(user.to_owned());
    }
    host.port = session
        .number("PortNumber")
        .and_then(|port| u16::try_from(port).ok())
        .filter(|port| Some(*port) != protocol.default_port() && *port != 0);
    if protocol != Protocol::Ssh {
        return Some(host);
    }
    if let Some(key) = session.text("PublicKeyFile") {
        if is_putty_key(key) {
            warnings.push(putty_key_warning(&session.origin, name, key));
        } else {
            host.identity_file = Some(key.to_owned());
        }
    }
    let mut warn = |message: String| {
        warnings.push(ImportWarning::new(
            &session.origin,
            format!("{name}: {message}"),
        ));
    };
    proxy(session, &mut host, &mut warn);
    if session.flag("AgentFwd") {
        host.ssh.agent_forwarding = Some(true);
    }
    if session.flag("X11Forward") {
        // PuTTY's forwarding gives the display's own cookie; untrusted is the safer start.
        host.ssh.x11 = Some(X11Forwarding::Untrusted);
    }
    if session.flag("Compression") {
        host.ssh.compression = Some(true);
    }
    if let Some(command) = session.text("RemoteCommand") {
        host.ssh.command = Some(command.to_owned());
    }
    if session.text("PortForwardings").is_some() {
        warn("its port forwardings were left out; add them as tunnels".to_owned());
    }
    Some(host)
}

/// `ProxyMethod`: 0 none, 1 SOCKS 4, 2 SOCKS 5, 3 HTTP, 4 Telnet, 5 a local command.
fn proxy(session: &RawSession, host: &mut Host, warn: &mut impl FnMut(String)) {
    let scheme = match session.number("ProxyMethod").unwrap_or(0) {
        0 => return,
        2 => "socks5",
        3 => "http",
        5 => {
            if let Some(command) = session.text("ProxyTelnetCommand") {
                let (command, password) = proxy_command(command);
                if password {
                    warn("the proxy command's %pass was left as written".to_owned());
                }
                host.ssh.proxy_command = Some(command);
            }
            return;
        }
        _ => {
            warn("the proxy (SOCKS 4 or Telnet) was left out".to_owned());
            return;
        }
    };
    let Some(address) = session.text("ProxyHost") else {
        return;
    };
    let user = session
        .text("ProxyUsername")
        .map(|user| format!("{user}@"))
        .unwrap_or_default();
    let port = session.number("ProxyPort").unwrap_or(1080);
    host.ssh.proxy = Some(format!("{scheme}://{user}{address}:{port}"));
}

/// The serial line's settings, in PuTTY's numbers.
fn serial(session: &RawSession, host: &mut Host, warn: &mut impl FnMut(String)) {
    host.serial.baud = session.number("SerialSpeed").filter(|speed| *speed > 0);
    host.serial.data_bits = session
        .number("SerialDataBits")
        .and_then(|bits| u8::try_from(bits).ok())
        .filter(|bits| (5..=8).contains(bits));
    // In half bits: 2 is one stop bit, 3 one and a half, 4 two.
    match session.number("SerialStopHalfbits") {
        Some(2) | None => {}
        Some(4) => host.serial.stop_bits = Some(2),
        Some(_) => warn("1.5 stop bits aren't supported; 1 is used".to_owned()),
    }
    match session.number("SerialParity") {
        Some(0) | None => {}
        Some(1) => host.serial.parity = Some(Parity::Odd),
        Some(2) => host.serial.parity = Some(Parity::Even),
        Some(_) => warn("mark and space parity aren't supported; none is used".to_owned()),
    }
    // PuTTY's default is XON/XOFF.
    match session.number("SerialFlowControl").unwrap_or(1) {
        0 => host.serial.flow_control = Some(FlowControl::None),
        1 => host.serial.flow_control = Some(FlowControl::Software),
        2 => host.serial.flow_control = Some(FlowControl::Hardware),
        _ => warn("DSR/DTR flow control isn't supported; none is used".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reg_lines() {
        assert_eq!(
            reg_value(r#""HostName"="db.example""#),
            Some(("HostName".to_owned(), "db.example".to_owned()))
        );
        assert_eq!(
            reg_value(r#""PublicKeyFile"="C:\\Keys\\a \"b\".ppk""#),
            Some((
                "PublicKeyFile".to_owned(),
                r#"C:\Keys\a "b".ppk"#.to_owned()
            ))
        );
        assert_eq!(
            reg_value(r#""PortNumber"=dword:00000016"#),
            Some(("PortNumber".to_owned(), "22".to_owned()))
        );
        assert_eq!(reg_value(r#""Colour0"=hex:01,02"#), None);
        assert_eq!(
            session_key(r"HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\My%20Server"),
            Some("My%20Server")
        );
        assert_eq!(
            session_key(r"HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions"),
            None
        );
        assert_eq!(
            session_key(r"-HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\x"),
            None
        );
    }

    #[test]
    fn the_sessions_folder() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(
            sessions_dir(home.path(), None),
            home.path().join(".putty").join("sessions")
        );
        let xdg = home.path().join(".config").join("putty").join("sessions");
        std::fs::create_dir_all(&xdg).unwrap();
        assert_eq!(sessions_dir(home.path(), None), xdg);
    }
}
