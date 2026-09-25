//! OpenSesh terminal engine (PLAN §3.3, Sprint 2).
//!
//! Qt-free: the GUI talks to it through [`session::Session`] (commands in, coalesced events and
//! render snapshots out). Output bytes come from a [`backend::TerminalBackend`]; the local PTY
//! backend lives in [`pty`], SSH and serial backends come later.
//!
//! Threads per session and the reasons for them: [ADR 0012](../../../docs/adr/0012-terminal-engine-and-session-threads.md).

pub mod backend;
pub mod input;
pub mod links;
pub mod osc;
pub mod palette;
pub mod pty;
pub mod search;
pub mod session;
pub mod shell;
pub mod snapshot;
