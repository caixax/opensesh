//! `known_hosts` files (PLAN §8): the user's `~/.ssh/known_hosts`, read only, and OpenSesh's own
//! `known_hosts` in the config folder, where OpenSesh writes. The format is OpenSSH's.
//!
//! Each line is `[@marker] hosts keytype base64-key [comment]`; hosts are comma-separated
//! patterns (`*` and `?` wildcards, `!` to exclude, `[host]:port` for ports other than 22) or one
//! hashed name (`|1|salt|hash`, the HMAC-SHA1 of the name keyed with the salt). Lines that don't
//! parse are counted, not fatal.
//!
//! [`check`] decides what a server's key means: known, new, changed (the host has a different
//! key of the same type) or revoked. OpenSesh's own file is consulted first: once a key was
//! accepted there (for example after the key changed), `~/.ssh/known_hosts` is no longer asked
//! about that host.

use std::path::{Path, PathBuf};

use base64ct::{Base64, Base64Unpadded, Encoding as _};
use hmac::{Hmac, Mac};
use opensesh_core::fsutil;
use sha1::Sha1;
use ssh_key::sha2::{Digest as _, Sha256};

/// File name of OpenSesh's own file in the config folder.
pub const KNOWN_HOSTS_FILE: &str = "known_hosts";

/// Largest file read.
const MAX_FILE: u64 = 16 * 1024 * 1024;

/// One host key line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownHost {
    /// 1-based line number.
    pub line: usize,
    /// `cert-authority` or `revoked`, when marked.
    pub marker: Option<String>,
    /// The host patterns as written (empty when hashed).
    pub hosts: String,
    /// Whether the host name is hashed.
    pub hashed: bool,
    /// The hosts field exactly as written (patterns, or the hashed name).
    pub raw_hosts: String,
    /// Key type (`ssh-ed25519`, ...).
    pub key_type: String,
    /// The key blob.
    pub key: Vec<u8>,
    /// `SHA256:...` of the key.
    pub fingerprint: String,
    /// The comment, if any.
    pub comment: String,
}

impl KnownHost {
    /// Whether this line is about `name` (`host`, or `[host]:port`).
    #[must_use]
    pub fn matches(&self, name: &str) -> bool {
        if self.hashed {
            hashed_matches(&self.raw_hosts, name)
        } else {
            patterns_match(&self.raw_hosts, name)
        }
    }
}

/// The entries of a file and the numbers of the lines that didn't parse.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KnownHosts {
    /// Host keys, in file order.
    pub entries: Vec<KnownHost>,
    /// Lines that aren't comments, blank or host keys.
    pub bad_lines: Vec<usize>,
}

/// Parses the text of a `known_hosts` file.
#[must_use]
pub fn parse(text: &str) -> KnownHosts {
    let mut out = KnownHosts::default();
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match parse_line(line, number) {
            Some(entry) => out.entries.push(entry),
            None => out.bad_lines.push(number),
        }
    }
    out
}

fn parse_line(line: &str, number: usize) -> Option<KnownHost> {
    let mut fields = line.split_whitespace();
    let mut first = fields.next()?;
    let mut marker = None;
    if let Some(name) = first.strip_prefix('@') {
        if !matches!(name, "cert-authority" | "revoked") {
            return None;
        }
        marker = Some(name.to_owned());
        first = fields.next()?;
    }
    let key_type = fields.next()?;
    let blob = Base64::decode_vec(fields.next()?).ok()?;
    // The blob starts with its own type name.
    let (len, rest) = blob.split_first_chunk::<4>()?;
    let name = rest.get(..usize::try_from(u32::from_be_bytes(*len)).ok()?)?;
    if name != key_type.as_bytes() {
        return None;
    }
    let comment = fields.collect::<Vec<_>>().join(" ");
    let hashed = first.starts_with("|1|");
    Some(KnownHost {
        line: number,
        marker,
        hosts: if hashed {
            String::new()
        } else {
            first.to_owned()
        },
        hashed,
        raw_hosts: first.to_owned(),
        key_type: key_type.to_owned(),
        fingerprint: fingerprint(&blob),
        key: blob,
        comment,
    })
}

/// `SHA256:` and the unpadded base64 of the SHA-256 of a key blob, as OpenSSH prints it.
#[must_use]
pub fn fingerprint(blob: &[u8]) -> String {
    format!(
        "SHA256:{}",
        Base64Unpadded::encode_string(&Sha256::digest(blob))
    )
}

/// Reads and parses `path`; a missing file has no entries.
///
/// # Errors
///
/// When the file exists but can't be read, or is larger than 16 MiB.
pub fn read(path: &Path) -> std::io::Result<KnownHosts> {
    match std::fs::metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(KnownHosts::default());
        }
        Err(error) => return Err(error),
        Ok(meta) if meta.len() > MAX_FILE => {
            return Err(std::io::Error::other("the file is too large"));
        }
        Ok(_) => {}
    }
    let bytes = std::fs::read(path)?;
    Ok(parse(&String::from_utf8_lossy(&bytes)))
}

/// The name a host is written under: `host`, or `[host]:port` for any port but 22.
#[must_use]
pub fn host_name(host: &str, port: u16) -> String {
    let host = host.trim().to_ascii_lowercase();
    if port == 22 {
        host
    } else {
        format!("[{host}]:{port}")
    }
}

/// OpenSSH's `match_pattern`: `*` matches any run of characters, `?` any one, ignoring case.
fn wildcard(pattern: &[u8], name: &[u8]) -> bool {
    match pattern.split_first() {
        None => name.is_empty(),
        Some((b'*', rest)) => {
            (0..=name.len()).any(|skip| name.get(skip..).is_some_and(|tail| wildcard(rest, tail)))
        }
        Some((first, rest)) => match name.split_first() {
            Some((head, tail)) if *first == b'?' || first.eq_ignore_ascii_case(head) => {
                wildcard(rest, tail)
            }
            _ => false,
        },
    }
}

/// Whether the comma-separated `patterns` match `name`: some pattern matches and no `!` pattern
/// does.
#[must_use]
pub fn patterns_match(patterns: &str, name: &str) -> bool {
    let mut matched = false;
    for pattern in patterns.split(',').filter(|pattern| !pattern.is_empty()) {
        match pattern.strip_prefix('!') {
            Some(negated) => {
                if wildcard(negated.as_bytes(), name.as_bytes()) {
                    return false;
                }
            }
            None => matched |= wildcard(pattern.as_bytes(), name.as_bytes()),
        }
    }
    matched
}

/// Whether `|1|salt|hash` is `name` hashed.
#[must_use]
pub fn hashed_matches(field: &str, name: &str) -> bool {
    let Some(rest) = field.strip_prefix("|1|") else {
        return false;
    };
    let Some((salt, hash)) = rest.split_once('|') else {
        return false;
    };
    let (Ok(salt), Ok(hash)) = (Base64::decode_vec(salt), Base64::decode_vec(hash)) else {
        return false;
    };
    let Ok(mut mac) = <Hmac<Sha1> as Mac>::new_from_slice(&salt) else {
        return false;
    };
    mac.update(name.as_bytes());
    mac.verify_slice(&hash).is_ok()
}

/// What a server's key means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyStatus {
    /// The key is known for this host.
    Known {
        /// Where.
        file: PathBuf,
        /// Line number.
        line: usize,
    },
    /// Nothing is known about this host's key of this type (other types may be known).
    New {
        /// Fingerprints of the host's keys of other types, if any.
        other_types: Vec<String>,
    },
    /// The host has a different key of the same type: a possible attack, or a reinstalled host.
    Changed {
        /// Where the old key is.
        file: PathBuf,
        /// Line number.
        line: usize,
        /// The old key's fingerprint.
        known_fingerprint: String,
    },
    /// The key is marked `@revoked`.
    Revoked {
        /// Where.
        file: PathBuf,
        /// Line number.
        line: usize,
    },
}

/// The status of `key` (a public key blob of type `key_type`) for `host`:`port`, from `files`
/// in order of precedence (OpenSesh's own file first).
#[must_use]
pub fn check(
    files: &[(PathBuf, KnownHosts)],
    host: &str,
    port: u16,
    key_type: &str,
    key: &[u8],
) -> HostKeyStatus {
    let name = host_name(host, port);
    // A revoked key is refused whatever host it is for.
    for (file, known) in files {
        if let Some(entry) = known
            .entries
            .iter()
            .find(|entry| entry.marker.as_deref() == Some("revoked") && entry.key == key)
        {
            return HostKeyStatus::Revoked {
                file: file.clone(),
                line: entry.line,
            };
        }
    }
    let mut other_types = Vec::new();
    for (file, known) in files {
        let for_host: Vec<&KnownHost> = known
            .entries
            .iter()
            .filter(|entry| entry.marker.is_none() && entry.matches(&name))
            .collect();
        if let Some(entry) = for_host.iter().find(|entry| entry.key == key) {
            return HostKeyStatus::Known {
                file: file.clone(),
                line: entry.line,
            };
        }
        if let Some(entry) = for_host.iter().find(|entry| entry.key_type == key_type) {
            return HostKeyStatus::Changed {
                file: file.clone(),
                line: entry.line,
                known_fingerprint: entry.fingerprint.clone(),
            };
        }
        if !for_host.is_empty() {
            other_types.extend(
                for_host
                    .iter()
                    .map(|entry| format!("{} {}", entry.key_type, entry.fingerprint)),
            );
            // This file knows the host: the files after it aren't asked.
            break;
        }
    }
    HostKeyStatus::New { other_types }
}

/// Adds `host`:`port` with the key to OpenSesh's file, replacing the host's keys of the same
/// type there (after the user accepted a changed key). Written atomically.
///
/// # Errors
///
/// When the file can't be read or written.
pub fn remember(
    path: &Path,
    host: &str,
    port: u16,
    key_type: &str,
    key: &[u8],
) -> std::io::Result<()> {
    let name = host_name(host, port);
    let current = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    let known = parse(&current);
    let replaced: Vec<usize> = known
        .entries
        .iter()
        .filter(|entry| {
            entry.marker.is_none() && entry.key_type == key_type && entry.matches(&name)
        })
        .map(|entry| entry.line)
        .collect();
    let mut text: String = current
        .lines()
        .enumerate()
        .filter(|(index, _)| !replaced.contains(&(index + 1)))
        .map(|(_, line)| format!("{line}\n"))
        .collect();
    text.push_str(&format!(
        "{name} {key_type} {}\n",
        Base64::encode_string(key)
    ));
    fsutil::atomic_write(path, text.as_bytes(), fsutil::DEFAULT_BACKUPS).map(|_| ())
}

/// Removes the given lines (1-based) from OpenSesh's file.
///
/// # Errors
///
/// When the file can't be read or written.
pub fn forget(path: &Path, lines: &[usize]) -> std::io::Result<()> {
    let current = std::fs::read_to_string(path)?;
    let text: String = current
        .lines()
        .enumerate()
        .filter(|(index, _)| !lines.contains(&(index + 1)))
        .map(|(_, line)| format!("{line}\n"))
        .collect();
    fsutil::atomic_write(path, text.as_bytes(), fsutil::DEFAULT_BACKUPS).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIC6tmVU1VE59P7TYx6UJYcZkhy7FRiLjhH6gdK9Sayyd";
    const OTHER: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl";

    fn blob(base64: &str) -> Vec<u8> {
        Base64::decode_vec(base64).unwrap()
    }

    #[test]
    fn lines() {
        let text = format!(
            "# comment\n\
             \n\
             github.com,140.82.121.3 ssh-ed25519 {KEY}\n\
             |1|F1E1KeoE/eEWhi10WpGv4OdiO6Y=|3988QV0VE8wmZL7suNrYQLITLCg= ssh-ed25519 {KEY} note here\n\
             @cert-authority *.example.com ssh-ed25519 {KEY}\n\
             @bogus host ssh-ed25519 {KEY}\n\
             host ssh-rsa {KEY}\n\
             host ssh-ed25519 not-base64!\n\
             just-one-field\n"
        );
        let parsed = parse(&text);
        assert_eq!(parsed.entries.len(), 3);
        assert_eq!(parsed.bad_lines, [6, 7, 8, 9]);
        let first = &parsed.entries[0];
        assert_eq!(first.line, 3);
        assert_eq!(first.hosts, "github.com,140.82.121.3");
        assert_eq!(first.key_type, "ssh-ed25519");
        assert!(first.fingerprint.starts_with("SHA256:"));
        assert!(!first.fingerprint.ends_with('='));
        let hashed = &parsed.entries[1];
        assert!(hashed.hashed);
        assert_eq!(hashed.hosts, "");
        assert_eq!(hashed.comment, "note here");
        assert_eq!(hashed.fingerprint, first.fingerprint);
        assert_eq!(parsed.entries[2].marker.as_deref(), Some("cert-authority"));
    }

    #[test]
    fn fingerprint_matches_ssh_key() {
        let key = ssh_key::PublicKey::from_openssh(&format!("ssh-ed25519 {KEY}")).unwrap();
        let expected = key.fingerprint(ssh_key::HashAlg::Sha256).to_string();
        let parsed = parse(&format!("h ssh-ed25519 {KEY}"));
        assert_eq!(parsed.entries[0].fingerprint, expected);
    }

    #[test]
    fn missing_file() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            read(&dir.path().join("none")).unwrap(),
            KnownHosts::default()
        );
    }

    #[test]
    fn patterns() {
        assert!(patterns_match("web,db", "db"));
        assert!(patterns_match("*.example.com", "a.b.example.com"));
        assert!(!patterns_match("*.example.com", "example.com"));
        assert!(patterns_match("web-?", "WEB-1"));
        assert!(!patterns_match("web-?", "web-10"));
        assert!(patterns_match(
            "*.example.com,!secret.example.com",
            "a.example.com"
        ));
        assert!(!patterns_match(
            "*.example.com,!secret.example.com",
            "secret.example.com"
        ));
        assert!(patterns_match("[web]:2222", "[web]:2222"));
        assert!(!patterns_match("web", "[web]:2222"));
        assert!(patterns_match("[*.lab]:*", "[a.lab]:2200"));
        assert_eq!(host_name("Web", 22), "web");
        assert_eq!(host_name("::1", 2222), "[::1]:2222");
    }

    #[test]
    fn hashed_names() {
        // Hashed by `ssh-keygen -H` (OpenSSH 10.0): "[web]:2222", then "github.com".
        let web = "|1|zq3FkAVa9/JEGoiSDJwn3o3jnuo=|kjkTViyT6vM7SLzFahZzITHEPlA=";
        let github = "|1|74vQjlIMVPtfl8n+4Ij3+3Yva4U=|re0Tt8FBztY2GmguCzsirYtaF7g=";
        assert!(hashed_matches(web, "[web]:2222"));
        assert!(!hashed_matches(web, "web"));
        assert!(hashed_matches(github, "github.com"));
        assert!(!hashed_matches(github, "[web]:2222"));
        // And the same computation built here.
        let salt = [7_u8; 20];
        let mut mac = <Hmac<Sha1> as Mac>::new_from_slice(&salt).unwrap();
        mac.update(b"[web]:2222");
        let hash = mac.finalize().into_bytes();
        let field = format!(
            "|1|{}|{}",
            Base64::encode_string(&salt),
            Base64::encode_string(&hash)
        );
        assert!(hashed_matches(&field, "[web]:2222"));
        assert!(!hashed_matches(&field, "web"));
        assert!(!hashed_matches("|1|bad|bad", "web"));
    }

    #[test]
    fn statuses() {
        let own = PathBuf::from("own");
        let user = PathBuf::from("user");
        let files = vec![
            (
                own.clone(),
                parse(&format!("changed.example ssh-ed25519 {OTHER}\n")),
            ),
            (
                user.clone(),
                parse(&format!(
                    "known.example ssh-ed25519 {KEY}\n\
                     changed.example ssh-ed25519 {KEY}\n\
                     rsa.example ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAAAAQDVX0OlbE8=\n\
                     @revoked * ssh-ed25519 {OTHER}\n"
                )),
            ),
        ];
        let key = blob(KEY);
        assert_eq!(
            check(&files, "known.example", 22, "ssh-ed25519", &key),
            HostKeyStatus::Known {
                file: user.clone(),
                line: 1
            }
        );
        assert!(matches!(
            check(&files, "new.example", 22, "ssh-ed25519", &key),
            HostKeyStatus::New { ref other_types } if other_types.is_empty()
        ));
        // Our own file wins: it has another key for this host.
        assert!(matches!(
            check(&files, "changed.example", 22, "ssh-ed25519", &key),
            HostKeyStatus::Changed { ref file, line: 1, .. } if *file == own
        ));
        // The same name on another port is another host.
        assert!(matches!(
            check(&files, "known.example", 2222, "ssh-ed25519", &key),
            HostKeyStatus::New { .. }
        ));
        assert!(matches!(
            check(&files, "anything", 22, "ssh-ed25519", &blob(OTHER)),
            HostKeyStatus::Revoked { line: 4, .. }
        ));
    }

    #[test]
    fn remembering_and_forgetting() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(KNOWN_HOSTS_FILE);
        remember(&path, "web", 2222, "ssh-ed25519", &blob(KEY)).unwrap();
        remember(&path, "db", 22, "ssh-ed25519", &blob(KEY)).unwrap();
        // A new key of the same type replaces the old one for that host only.
        remember(&path, "web", 2222, "ssh-ed25519", &blob(OTHER)).unwrap();
        let known = read(&path).unwrap();
        assert_eq!(known.entries.len(), 2);
        assert_eq!(known.entries[0].raw_hosts, "db");
        assert_eq!(known.entries[1].raw_hosts, "[web]:2222");
        assert_eq!(known.entries[1].key, blob(OTHER));
        forget(&path, &[1]).unwrap();
        assert_eq!(read(&path).unwrap().entries.len(), 1);
    }
}
