//! `known_hosts` files, read-only for now (Sprint 6): the user's `~/.ssh/known_hosts` and
//! OpenSesh's own `known_hosts` in the config folder. Checking and adding host keys come with the
//! SSH client (Sprint 7).
//!
//! Each line is `[@marker] hosts keytype base64-key [comment]`; hosts are comma-separated
//! patterns or one hashed name (`|1|salt|hash`). Lines that don't parse are counted, not fatal.

use std::path::Path;

use base64ct::{Base64, Base64Unpadded, Encoding as _};
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
    /// Key type (`ssh-ed25519`, ...).
    pub key_type: String,
    /// `SHA256:...` of the key.
    pub fingerprint: String,
    /// The comment, if any.
    pub comment: String,
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
    let digest = Sha256::digest(&blob);
    Some(KnownHost {
        line: number,
        marker,
        hosts: if hashed {
            String::new()
        } else {
            first.to_owned()
        },
        hashed,
        key_type: key_type.to_owned(),
        fingerprint: format!("SHA256:{}", Base64Unpadded::encode_string(&digest)),
        comment,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIC6tmVU1VE59P7TYx6UJYcZkhy7FRiLjhH6gdK9Sayyd";

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
}
