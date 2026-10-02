//! OpenSesh's other terminal protocols (PLAN Sprint 12, ADR 0032): telnet, serial ports, mosh
//! and containers. This crate never depends on Qt.
//!
//! - [`telnet`]: a small NVT (option negotiation, window size, terminal type) and a terminal
//!   backend over TCP.
//! - [`serial`]: a serial port as a terminal backend, the ports this computer has.
//! - [`mosh`]: starting `mosh-server` over SSH and `mosh-client` here.
//! - [`containers`]: the `docker`, `podman` and `kubectl` commands that open a shell in a
//!   container or a pod, and the running ones.
//!
//! The backends run on the SSH client's tokio runtime ([`opensesh_ssh::runtime`]), off the GUI
//! thread. What they print themselves (connecting, why a session ended) is dim or red text in
//! the terminal, never logged with the session.

pub mod serial;
pub mod telnet;

mod output;
