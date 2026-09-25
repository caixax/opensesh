//! OpenSesh core: domain model, configuration and platform-independent logic.
//!
//! This crate never depends on Qt, so everything here can be unit-tested headless.

pub mod config;
pub mod desktop;
pub mod fsutil;
pub mod identity;
pub mod paths;
pub mod state;
pub mod theme;
pub mod watch;
pub mod writer;

pub use paths::{AppPaths, PathsError};
