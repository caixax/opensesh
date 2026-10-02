//! What the importers share: the result ([`Imported`]), the warnings, folders turned into
//! groups, and text in the encodings other programs write.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use opensesh_core::hosts::{Group, Host, new_id};

/// Something that was skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportWarning {
    /// The file (or the registry key).
    pub file: PathBuf,
    /// The line (from 1), 0 for the whole file.
    pub line: usize,
    /// What and why.
    pub message: String,
}

impl ImportWarning {
    /// A warning about `file` as a whole.
    pub fn new(file: &Path, message: impl Into<String>) -> Self {
        Self {
            file: file.to_path_buf(),
            line: 0,
            message: message.into(),
        }
    }
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

/// Hosts read from another program, with the folders they were in as groups. Top-level hosts
/// and groups have no group (no parent): the app puts them in a group of their own.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Imported {
    /// The folders, parents before their children.
    pub groups: Vec<Group>,
    /// The hosts, in the order they were found.
    pub hosts: Vec<Host>,
    /// What was skipped.
    pub warnings: Vec<ImportWarning>,
}

impl Imported {
    /// Adds what `other` read, with its folders merged into these by path.
    pub fn extend(&mut self, other: Self) {
        let mut folders = Folders::from_groups(&self.groups);
        let mut renamed = HashMap::new();
        for group in &other.groups {
            let path = other.path_of(group);
            if let Some(id) = folders.group(&mut self.groups, &path) {
                renamed.insert(group.id.clone(), id);
            }
        }
        for mut host in other.hosts {
            host.group = host.group.and_then(|id| renamed.get(&id).cloned());
            self.hosts.push(host);
        }
        self.warnings.extend(other.warnings);
    }

    /// The folder names from the top down to `group`.
    fn path_of(&self, group: &Group) -> Vec<String> {
        let mut path = vec![group.name.clone()];
        let mut parent = group.parent.clone();
        // Bounded: a cycle can't come from `Folders`, but stay safe.
        for _ in 0..self.groups.len() {
            let Some(id) = parent else { break };
            let Some(up) = self.groups.iter().find(|candidate| candidate.id == id) else {
                break;
            };
            path.push(up.name.clone());
            parent = up.parent.clone();
        }
        path.reverse();
        path
    }

    /// The group named by `path` (folder names from the top), made when missing.
    pub fn group(&mut self, path: &[String]) -> Option<String> {
        Folders::from_groups(&self.groups).group(&mut self.groups, path)
    }
}

/// Groups by folder path, made on demand.
#[derive(Debug, Default)]
struct Folders {
    by_path: HashMap<Vec<String>, String>,
}

impl Folders {
    fn from_groups(groups: &[Group]) -> Self {
        let mut folders = Self::default();
        let mut paths: HashMap<String, Vec<String>> = HashMap::new();
        // Parents come before their children.
        for group in groups {
            let mut path = group
                .parent
                .as_ref()
                .and_then(|parent| paths.get(parent).cloned())
                .unwrap_or_default();
            path.push(group.name.clone());
            paths.insert(group.id.clone(), path.clone());
            folders.by_path.insert(path, group.id.clone());
        }
        folders
    }

    fn group(&mut self, groups: &mut Vec<Group>, path: &[String]) -> Option<String> {
        let mut parent: Option<String> = None;
        for depth in 1..=path.len() {
            let key = path[..depth].to_vec();
            let id = if let Some(id) = self.by_path.get(&key) {
                id.clone()
            } else {
                let id = new_id();
                groups.push(Group {
                    id: id.clone(),
                    name: path[depth - 1].clone(),
                    parent: parent.clone(),
                    ..Group::default()
                });
                self.by_path.insert(key, id.clone());
                id
            };
            parent = Some(id);
        }
        parent
    }
}

/// Splits a folder path on `separator`, dropping empty parts.
pub(crate) fn folder_path(text: &str, separator: char) -> Vec<String> {
    text.split(separator)
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Largest file an importer reads (session lists are kilobytes; a CSV of 100,000 hosts is a few
/// megabytes).
pub const MAX_FILE: u64 = 16 * 1024 * 1024;

/// Reads `path` when it is a regular file of at most `max` bytes: a device or a FIFO (`Include
/// /dev/zero`, a pipe named `x.remmina`) would never end, and a huge file isn't a session list.
///
/// # Errors
///
/// When it can't be read, isn't a regular file, or is larger than `max`.
pub fn read_limited(path: &Path, max: u64) -> std::io::Result<Vec<u8>> {
    use std::io::{Error, ErrorKind, Read};

    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(Error::new(ErrorKind::InvalidInput, "not a regular file"));
    }
    if metadata.len() > max {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("larger than {} MiB", max / (1024 * 1024)),
        ));
    }
    // The file may grow between the check and the read: never more than `max`, plus one byte
    // to tell.
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(max + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("larger than {} MiB", max / (1024 * 1024)),
        ));
    }
    Ok(bytes)
}

/// Text from a file another program wrote: UTF-16 with a byte order mark (`regedit`'s `.reg`
/// files), UTF-8 (with or without a mark), else Windows-1252 (MobaXterm, older PuTTY).
#[must_use]
pub fn decode_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let (text, _) = encoding_rs::UTF_16LE.decode_without_bom_handling(rest);
        return text.into_owned();
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let (text, _) = encoding_rs::UTF_16BE.decode_without_bom_handling(rest);
        return text.into_owned();
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => {
            let (text, _) = encoding_rs::WINDOWS_1252.decode_without_bom_handling(bytes);
            text.into_owned()
        }
    }
}

/// `%XX` escapes decoded (PuTTY's session names, Remmina's notes); anything else is kept.
#[must_use]
pub fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%'
            && let (Some(high), Some(low)) = (
                bytes.get(at + 1).and_then(|b| (*b as char).to_digit(16)),
                bytes.get(at + 2).and_then(|b| (*b as char).to_digit(16)),
            )
        {
            // Two hex digits make a byte.
            out.push(u8::try_from(high * 16 + low).unwrap_or(b'?'));
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    decode_text(&out)
}

/// `host`, `host:port`, `[v6]` or `[v6]:port`; a bare IPv6 address (more than one `:`) has no
/// port. A port that isn't a number gives `None` for it.
#[must_use]
pub fn split_host_port(text: &str) -> (String, Option<u16>) {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix('[')
        && let Some((host, after)) = rest.split_once(']')
    {
        let port = after.strip_prefix(':').and_then(|port| port.parse().ok());
        return (host.to_owned(), port);
    }
    match text.split_once(':') {
        Some((host, port)) if !port.contains(':') => (host.to_owned(), port.trim().parse().ok()),
        _ => (text.to_owned(), None),
    }
}

/// A `[user@]host[:port]` jump reference (IPv6 in brackets), the port left out when it is 22.
#[must_use]
pub fn jump_spec(user: Option<&str>, host: &str, port: Option<u16>) -> String {
    let mut spec = String::new();
    if let Some(user) = user.filter(|user| !user.is_empty()) {
        spec.push_str(user);
        spec.push('@');
    }
    if host.contains(':') {
        spec.push('[');
        spec.push_str(host);
        spec.push(']');
    } else {
        spec.push_str(host);
    }
    if let Some(port) = port.filter(|port| *port != 22) {
        spec.push(':');
        spec.push_str(&port.to_string());
    }
    spec
}

/// A local proxy command from MobaXterm or PuTTY, with their `%host`, `%port` and `%user` as
/// OpenSesh's `%h`, `%p` and `%r` (PuTTY's `\n` at the end dropped); `true` when it uses
/// `%pass`, which has no equivalent.
#[must_use]
pub fn proxy_command(text: &str) -> (String, bool) {
    let mut out = String::new();
    let mut rest = text.trim();
    while let Some(at) = rest.find(['%', '\\']) {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let (replacement, used) = [
            ("%host", "%h"),
            ("%port", "%p"),
            ("%user", "%r"),
            ("%%", "%%"),
            ("\\n", ""),
            ("\\r", ""),
            ("\\\\", "\\"),
        ]
        .iter()
        .find(|(from, _)| tail.starts_with(from))
        .map_or_else(|| (&tail[..1], 1), |(from, to)| (*to, from.len()));
        out.push_str(replacement);
        rest = &tail[used..];
    }
    out.push_str(rest);
    let password = text.contains("%pass");
    (out.trim().to_owned(), password)
}

/// Whether a key file is in PuTTY's format, which the Keychain converts but SSH can't use as a
/// file.
#[must_use]
pub fn is_putty_key(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ppk"))
}

/// The warning for a PuTTY key file that was left out.
pub(crate) fn putty_key_warning(file: &Path, name: &str, key: &str) -> ImportWarning {
    ImportWarning::new(
        file,
        format!(
            "{name}: the PuTTY key {key} was left out; import it in the Keychain, which converts it, then pick it for the host"
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_in_other_encodings() {
        assert_eq!(decode_text(b"caf\xc3\xa9"), "café");
        assert_eq!(decode_text(b"\xef\xbb\xbfcaf\xc3\xa9"), "café");
        assert_eq!(decode_text(b"caf\xe9 \x80"), "café €");
        assert_eq!(decode_text(&[0xFF, 0xFE, b'h', 0, b'i', 0]), "hi");
        assert_eq!(percent_decode("My%20Server%2Fone%"), "My Server/one%");
        assert_eq!(percent_decode("caf%C3%A9"), "café");
        assert_eq!(percent_decode("caf%E9"), "café");
    }

    #[test]
    fn only_regular_files_of_a_bounded_size_are_read() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.csv");
        std::fs::write(&file, "x".repeat(2048)).unwrap();
        assert_eq!(read_limited(&file, 4096).unwrap().len(), 2048);
        assert!(read_limited(&file, 1024).is_err());
        assert!(read_limited(dir.path(), MAX_FILE).is_err());
        assert!(read_limited(&dir.path().join("missing"), MAX_FILE).is_err());
        #[cfg(unix)]
        assert_eq!(
            read_limited(Path::new("/dev/zero"), MAX_FILE).map_err(|e| e.kind()),
            Err(std::io::ErrorKind::InvalidInput)
        );
    }

    #[test]
    fn hosts_and_ports() {
        assert_eq!(split_host_port("a.example"), ("a.example".to_owned(), None));
        assert_eq!(
            split_host_port("a.example:2222"),
            ("a.example".to_owned(), Some(2222))
        );
        assert_eq!(
            split_host_port("[2001:db8::1]:3390"),
            ("2001:db8::1".to_owned(), Some(3390))
        );
        assert_eq!(
            split_host_port("2001:db8::1"),
            ("2001:db8::1".to_owned(), None)
        );
        assert_eq!(jump_spec(Some("me"), "bastion", Some(22)), "me@bastion");
        assert_eq!(
            jump_spec(None, "2001:db8::1", Some(2200)),
            "[2001:db8::1]:2200"
        );
        assert_eq!(
            proxy_command(r"nc -X 5 -x proxy:1080 %host %port %% %user\n"),
            ("nc -X 5 -x proxy:1080 %h %p %% %r".to_owned(), false)
        );
        assert_eq!(
            proxy_command("connect %host %port %pass"),
            ("connect %h %p %pass".to_owned(), true)
        );
        assert!(is_putty_key(r"C:\keys\id.PPK"));
        assert!(!is_putty_key("~/.ssh/id_ed25519"));
    }

    #[test]
    fn folders_become_nested_groups() {
        let mut imported = Imported::default();
        let path =
            |parts: &[&str]| -> Vec<String> { parts.iter().map(|p| (*p).to_owned()).collect() };
        let web = imported.group(&path(&["Prod", "Web"]));
        let prod = imported.group(&path(&["Prod"]));
        assert_eq!(imported.groups.len(), 2);
        assert_eq!(imported.groups[1].parent, prod);
        assert_eq!(imported.group(&path(&["Prod", "Web"])), web);
        assert_eq!(imported.group(&[]), None);

        // Another file's folders join these by path.
        let mut other = Imported::default();
        let other_web = other.group(&path(&["Prod", "Web"]));
        let db = other.group(&path(&["Prod", "Db"]));
        other.hosts.push(Host {
            name: "a".to_owned(),
            group: other_web,
            ..Host::default()
        });
        other.hosts.push(Host {
            name: "b".to_owned(),
            group: db,
            ..Host::default()
        });
        imported.extend(other);
        assert_eq!(imported.groups.len(), 3);
        assert_eq!(imported.hosts[0].group, web);
        assert_eq!(imported.groups[2].parent, prod);
        assert_eq!(
            imported.hosts[1].group.as_deref(),
            Some(imported.groups[2].id.as_str())
        );
    }
}
