//! Waiting after wrong master passwords (PLAN §8), kept in `vault-attempts.toml` in the data
//! folder so that restarting the app doesn't reset it.
//!
//! The first [`FREE_ATTEMPTS`] wrong passwords cost nothing. After that, each one makes the next
//! attempt wait [`FIRST_DELAY_SECS`] seconds, doubling up to [`MAX_DELAY_SECS`]. A right password
//! clears it. This slows down guessing through the app; guessing against a copy of `vault.bin`
//! is slowed by Argon2id instead (see `docs/threat-model.md`).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use opensesh_core::fsutil;

/// File name in the data folder.
pub const ATTEMPTS_FILE: &str = "vault-attempts.toml";
/// Wrong passwords allowed before any wait.
pub const FREE_ATTEMPTS: u32 = 3;
/// The first wait, in seconds.
pub const FIRST_DELAY_SECS: u64 = 5;
/// The longest wait, in seconds.
pub const MAX_DELAY_SECS: u64 = 300;

#[derive(Debug, Default, Serialize, Deserialize)]
struct AttemptsFile {
    #[serde(default)]
    schema_version: i64,
    #[serde(default)]
    failures: u32,
    /// Seconds since the Unix epoch.
    #[serde(default)]
    last_failure: u64,
}

/// Wrong master passwords in a row, and when the last one was.
#[derive(Debug, Default)]
pub struct Backoff {
    /// `None`: kept in memory only (test runs).
    path: Option<PathBuf>,
    failures: u32,
    last_failure: u64,
}

impl Backoff {
    /// The wait after `failures` wrong passwords in a row, in seconds.
    #[must_use]
    pub fn delay_after(failures: u32) -> u64 {
        if failures < FREE_ATTEMPTS {
            return 0;
        }
        let doublings = (failures - FREE_ATTEMPTS).min(16);
        (FIRST_DELAY_SECS << doublings).min(MAX_DELAY_SECS)
    }

    /// The state saved in `path` (none when it's missing or can't be read), saving there.
    #[must_use]
    pub fn load(path: Option<PathBuf>) -> Self {
        let saved = path
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| toml::from_str::<AttemptsFile>(&text).ok())
            .unwrap_or_default();
        Self {
            path,
            failures: saved.failures,
            last_failure: saved.last_failure,
        }
    }

    /// Wrong passwords in a row.
    #[must_use]
    pub fn failures(&self) -> u32 {
        self.failures
    }

    /// Seconds to wait before the next attempt at `now` (seconds since the Unix epoch).
    ///
    /// A last failure in the future means the clock went back: the wait restarts from `now`
    /// instead of lasting until the clock catches up.
    pub fn wait(&mut self, now: u64) -> u64 {
        let delay = Self::delay_after(self.failures);
        if delay == 0 {
            return 0;
        }
        if self.last_failure > now {
            self.last_failure = now;
            self.save();
        }
        delay.saturating_sub(now - self.last_failure)
    }

    /// A wrong password at `now`.
    pub fn failed(&mut self, now: u64) {
        self.failures = self.failures.saturating_add(1);
        self.last_failure = now;
        self.save();
    }

    /// A right password: no more waiting.
    pub fn succeeded(&mut self) {
        if self.failures == 0 {
            return;
        }
        self.failures = 0;
        self.last_failure = 0;
        if let Some(path) = &self.path
            && let Err(error) = std::fs::remove_file(path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(path = %path.display(), "could not clear the password attempts: {error}");
        }
    }

    fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let file = AttemptsFile {
            schema_version: 1,
            failures: self.failures,
            last_failure: self.last_failure,
        };
        let text = match toml::to_string(&file) {
            Ok(text) => format!("# Wrong master passwords in a row (the wait after them).\n{text}"),
            Err(error) => {
                tracing::warn!("could not encode the password attempts: {error}");
                return;
            }
        };
        if let Err(error) = fsutil::atomic_write(path, text.as_bytes(), 0) {
            tracing::warn!(path = %path.display(), "could not save the password attempts: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delays_grow_and_stop_growing() {
        let delays: Vec<u64> = (0..12).map(Backoff::delay_after).collect();
        assert_eq!(delays, [0, 0, 0, 5, 10, 20, 40, 80, 160, 300, 300, 300]);
        assert_eq!(Backoff::delay_after(u32::MAX), MAX_DELAY_SECS);
    }

    #[test]
    fn waits_count_down_and_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(ATTEMPTS_FILE);
        let mut backoff = Backoff::load(Some(path.clone()));
        let start = 1_000_000;
        for attempt in 0..3 {
            assert_eq!(backoff.wait(start + attempt), 0);
            backoff.failed(start + attempt);
        }
        assert_eq!(backoff.wait(start + 2), 5);
        assert_eq!(backoff.wait(start + 5), 2);
        // A restart remembers.
        let mut again = Backoff::load(Some(path.clone()));
        assert_eq!(again.failures(), 3);
        assert_eq!(again.wait(start + 5), 2);
        assert_eq!(again.wait(start + 7), 0);
        again.failed(start + 7);
        assert_eq!(again.wait(start + 7), 10);
        // The clock went back an hour: wait the delay from now, not an hour more.
        assert_eq!(again.wait(start + 7 - 3600), 10);
        assert_eq!(again.wait(start + 7 - 3600 + 10), 0);
        // A right password clears it, and the file.
        again.succeeded();
        assert_eq!(again.wait(start + 8), 0);
        assert!(!path.exists());
        assert_eq!(Backoff::load(Some(path)).failures(), 0);
    }

    #[test]
    fn a_damaged_file_starts_over() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(ATTEMPTS_FILE);
        std::fs::write(&path, "failures = \"many\"").unwrap();
        assert_eq!(Backoff::load(Some(path)).failures(), 0);
        let mut memory = Backoff::load(None);
        for _ in 0..4 {
            memory.failed(10);
        }
        assert_eq!(memory.wait(10), 10);
    }
}
