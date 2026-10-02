//! OpenSesh's VNC client (Sprint 14, ADR 0035): RFB 3.3 to 3.8 (RFC 6143), without Qt.
//!
//! - [`client`]: connecting (the version and security handshake) and the session.
//! - [`canvas`]: the desktop's pixels, which the decoders draw into.
//! - [`decode`]: Raw, CopyRect, Hextile, ZRLE and Tight, the cursor.
//! - [`drive`]: a session as the app drives a remote desktop (the RDP helper's messages).
//! - [`keysym`]: Qt keys and the text they type as X keysyms.
//! - [`messages`]: the version, the server's init, and the client's messages.
//! - [`security`]: none, VNC authentication and VeNCrypt's X509 subtypes.
//! - [`tls`]: VeNCrypt's TLS, with the server's certificate kept for the app to decide on.
//! - [`testing`]: an in-process RFB server for tests, the smoke test and screenshots.
//!
//! Passwords and clipboard text are never logged.

pub mod canvas;
pub mod client;
pub mod decode;
pub mod drive;
pub mod keysym;
pub mod messages;
pub mod security;
pub mod testing;
pub mod tls;

/// Why a VNC connection failed or ended.
#[derive(Debug, thiserror::Error)]
pub enum VncError {
    /// The connection broke.
    #[error("the connection was lost: {0}")]
    Io(#[from] std::io::Error),
    /// The server said something this client can't follow.
    #[error("the server sent something unexpected: {0}")]
    Protocol(String),
    /// The server wants something this client doesn't do.
    #[error("{0}")]
    Unsupported(String),
    /// The server refused the password (or the user name and password).
    #[error("{0}")]
    Auth(String),
    /// The server refused the connection, or its certificate wasn't accepted.
    #[error("{0}")]
    Refused(String),
    /// Connecting took too long.
    #[error("the server didn't answer in time")]
    Timeout,
}
