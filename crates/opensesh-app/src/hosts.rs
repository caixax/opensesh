//! The saved hosts in memory (Sprint 5): `hosts.toml` plus the hosts of its linked sources,
//! shared by the `Hosts` QML singleton (which loads, edits and saves them) and every
//! `TerminalItem` connected to a host (which reads its group and host terminal levels).
//!
//! Like the terminal profiles, the [`Library`] is immutable once published: a change builds a
//! new one with a higher `revision` and swaps it in.

use std::sync::{Arc, LazyLock, PoisonError, RwLock};

use opensesh_core::hosts::{HostsFile, TerminalLevel};

/// The hosts, as last loaded or edited.
#[derive(Debug, Default)]
pub struct Library {
    /// Groups, saved hosts, then the linked ones (read-only; never written to `hosts.toml`).
    pub file: HostsFile,
    /// Grows with every change.
    pub revision: u64,
}

impl Library {
    /// The terminal levels of host `id` (its groups from the outermost, then the host); none
    /// for an unknown host.
    #[must_use]
    pub fn terminal_levels(&self, id: &str) -> Vec<TerminalLevel> {
        self.file
            .host(id)
            .map(|host| self.file.terminal_levels(host))
            .unwrap_or_default()
    }
}

static LIBRARY: LazyLock<RwLock<Arc<Library>>> =
    LazyLock::new(|| RwLock::new(Arc::new(Library::default())));

/// The current library.
#[must_use]
pub fn current() -> Arc<Library> {
    Arc::clone(&LIBRARY.read().unwrap_or_else(PoisonError::into_inner))
}

/// Publishes `file` with a new revision and returns the new library.
pub fn publish(file: HostsFile) -> Arc<Library> {
    let mut slot = LIBRARY.write().unwrap_or_else(PoisonError::into_inner);
    let next = Arc::new(Library {
        file,
        revision: slot.revision + 1,
    });
    *slot = Arc::clone(&next);
    next
}
