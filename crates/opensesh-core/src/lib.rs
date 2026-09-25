//! OpenSesh core: domain model, configuration and platform-independent logic.
//!
//! This crate never depends on Qt, so everything here can be unit-tested headless.
//! Sprint 0 only ships the application identity and the directory layout (PLAN §4.1).

pub mod identity;
pub mod paths;

pub use paths::{AppPaths, PathsError};
