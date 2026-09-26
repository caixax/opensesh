//! `TerminalSessions` QML singleton: the pane side of the terminal session registry
//! ([`crate::terminal::registry`]). A `TerminalItem` starts and attaches to the session of its
//! pane; the shell ends it here when the pane or its tab closes. Tab and pane ids come from
//! `allocateId`, unique across every window, so a tab can move to another window.

#[cxx_qt::bridge]
pub mod qobject {
    extern "RustQt" {
        /// The terminal sessions of every window, by pane id.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        type TerminalSessions = super::TerminalSessionsRust;

        /// A new id for a tab or a pane, never used before in this process.
        #[qinvokable]
        #[cxx_name = "allocateId"]
        fn allocate_id(self: &Self) -> i32;

        /// Ends the session of pane `id` (in the background). Returns whether there was one.
        #[qinvokable]
        fn close(self: &Self, id: i32) -> bool;

        /// Whether pane `id` has a session.
        #[qinvokable]
        #[cxx_name = "isOpen"]
        fn is_open(self: &Self, id: i32) -> bool;

        /// How many sessions are open.
        #[qinvokable]
        fn count(self: &Self) -> i32;
    }
}

use crate::terminal::registry;

/// Rust state behind `TerminalSessions` (none: the registry is process-wide).
#[derive(Debug, Default)]
pub struct TerminalSessionsRust;

impl qobject::TerminalSessions {
    /// See the bridge declaration.
    pub fn allocate_id(&self) -> i32 {
        registry::allocate_id()
    }

    /// See the bridge declaration.
    pub fn close(&self, id: i32) -> bool {
        registry::close(id)
    }

    /// See the bridge declaration.
    pub fn is_open(&self, id: i32) -> bool {
        registry::get(id).is_some()
    }

    /// See the bridge declaration.
    pub fn count(&self) -> i32 {
        i32::try_from(registry::count()).unwrap_or(i32::MAX)
    }
}
