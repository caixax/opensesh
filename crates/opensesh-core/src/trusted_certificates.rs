//! Server certificates trusted on first use (ADR 0034), as SSH host keys: remote desktop servers
//! mostly present self-signed certificates, so the first one is shown with its fingerprint and
//! the user decides; a remembered one that changes is a warning.
//!
//! Remembered certificates live in `trusted_certificates.toml` in the config folder: one entry
//! per host and port with the certificate's SHA-256 fingerprint, written atomically. The file
//! holds no secrets.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The file's name in the config folder.
pub const FILE: &str = "trusted_certificates.toml";

/// What is known about a server's certificate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trust {
    /// It is the remembered one.
    Known,
    /// Nothing is remembered for this host and port.
    New,
    /// Another certificate is remembered: its fingerprint.
    Changed {
        /// The remembered fingerprint.
        known: String,
    },
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Entries {
    #[serde(default, rename = "certificate")]
    certificates: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    host: String,
    port: u16,
    sha256: String,
}

/// The remembered certificates.
#[derive(Debug, Clone)]
pub struct TrustedCertificates {
    path: PathBuf,
}

impl TrustedCertificates {
    /// The store in `path` (created on the first remembered certificate).
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Where it is.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn entries(&self) -> std::io::Result<Entries> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => toml::from_str(&text).map_err(|error| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, error.message().to_owned())
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Entries::default()),
            Err(error) => Err(error),
        }
    }

    /// What is known about `fingerprint` for `host` and `port` (hosts compared without case).
    ///
    /// # Errors
    ///
    /// When the file can't be read or isn't valid.
    pub fn check(&self, host: &str, port: u16, fingerprint: &str) -> std::io::Result<Trust> {
        let entries = self.entries()?;
        Ok(
            match entries
                .certificates
                .iter()
                .find(|entry| entry.port == port && entry.host.eq_ignore_ascii_case(host))
            {
                None => Trust::New,
                Some(entry) if entry.sha256 == fingerprint => Trust::Known,
                Some(entry) => Trust::Changed {
                    known: entry.sha256.clone(),
                },
            },
        )
    }

    /// Remembers `fingerprint` for `host` and `port`, replacing what was there.
    ///
    /// # Errors
    ///
    /// When the file can't be read or written.
    pub fn remember(&self, host: &str, port: u16, fingerprint: &str) -> std::io::Result<()> {
        let mut entries = self.entries()?;
        entries
            .certificates
            .retain(|entry| !(entry.port == port && entry.host.eq_ignore_ascii_case(host)));
        entries.certificates.push(Entry {
            host: host.to_owned(),
            port,
            sha256: fingerprint.to_owned(),
        });
        entries
            .certificates
            .sort_by(|a, b| (a.host.to_lowercase(), a.port).cmp(&(b.host.to_lowercase(), b.port)));
        let body =
            toml::to_string(&entries).map_err(|error| std::io::Error::other(error.to_string()))?;
        let text = format!(
            "# Server certificates OpenSesh trusts (remote desktop): host, port and the SHA-256 of\n\
             # the certificate. Remove an entry to be asked again.\n\n{body}"
        );
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        crate::fsutil::atomic_write(&self.path, text.as_bytes(), 0).map(drop)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests")]

    use super::*;

    #[test]
    fn remembered_new_and_changed() {
        let dir = tempfile::tempdir().unwrap();
        let store = TrustedCertificates::new(dir.path().join("sub").join(FILE));
        assert_eq!(store.check("win11", 3389, "SHA256:a").unwrap(), Trust::New);
        store.remember("win11", 3389, "SHA256:a").unwrap();
        store.remember("other", 3389, "SHA256:b").unwrap();
        assert_eq!(
            store.check("WIN11", 3389, "SHA256:a").unwrap(),
            Trust::Known
        );
        assert_eq!(store.check("win11", 3390, "SHA256:a").unwrap(), Trust::New);
        assert_eq!(
            store.check("win11", 3389, "SHA256:c").unwrap(),
            Trust::Changed {
                known: "SHA256:a".to_owned()
            }
        );
        // Replaced, not added.
        store.remember("win11", 3389, "SHA256:c").unwrap();
        assert_eq!(
            store.check("win11", 3389, "SHA256:c").unwrap(),
            Trust::Known
        );
        let text = std::fs::read_to_string(store.path()).unwrap();
        assert_eq!(text.matches("[[certificate]]").count(), 2, "{text}");
        // A file that isn't valid says so.
        std::fs::write(store.path(), "certificate = 3").unwrap();
        assert!(store.check("win11", 3389, "SHA256:c").is_err());
    }
}
