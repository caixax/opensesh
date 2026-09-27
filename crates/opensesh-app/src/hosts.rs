//! The saved hosts in memory (Sprint 5): `hosts.toml` plus the hosts of its linked sources,
//! shared by the `Hosts` QML singleton (which loads, edits and saves them) and every
//! `TerminalItem` connected to a host (which reads its group and host terminal levels).
//!
//! Like the terminal profiles, the [`Library`] is immutable once published: a change builds a
//! new one with a higher `revision` and swaps it in. The app's SSH defaults (Settings > SSH) go
//! into every published file as its `base` ([`set_defaults`]).

use std::sync::{Arc, LazyLock, PoisonError, RwLock};

use opensesh_core::hosts::{HostsFile, TerminalLevel};
use toml::Table;

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

/// The app's defaults for every host (see [`HostsFile::base`]).
static DEFAULTS: LazyLock<RwLock<Table>> = LazyLock::new(|| RwLock::new(Table::new()));

/// The current library.
#[must_use]
pub fn current() -> Arc<Library> {
    Arc::clone(&LIBRARY.read().unwrap_or_else(PoisonError::into_inner))
}

/// Publishes `file` (with the app's defaults) with a new revision and returns the new library.
pub fn publish(mut file: HostsFile) -> Arc<Library> {
    file.base = DEFAULTS
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let mut slot = LIBRARY.write().unwrap_or_else(PoisonError::into_inner);
    let next = Arc::new(Library {
        file,
        revision: slot.revision + 1,
    });
    *slot = Arc::clone(&next);
    next
}

/// Sets the app's defaults for every host (a host table, see [`HostsFile::base`]) and publishes
/// the hosts again when they changed.
pub fn set_defaults(defaults: Table) {
    {
        let mut slot = DEFAULTS.write().unwrap_or_else(PoisonError::into_inner);
        if *slot == defaults {
            return;
        }
        *slot = defaults;
    }
    let file = current().file.clone();
    publish(file);
}
