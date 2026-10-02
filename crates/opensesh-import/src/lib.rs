//! OpenSesh importers (PLAN Sprint 5, Sprint 16): other programs' host lists turned into
//! OpenSesh hosts.
//!
//! - [`ssh_config`]: `~/.ssh/config` (Sprint 5, ADR 0020), imported or linked;
//! - [`mobaxterm`], [`putty`], [`remmina`]: those programs' saved sessions;
//! - [`csv`]: any spreadsheet, with the columns mapped by the user;
//! - [`bundle`]: OpenSesh's own bundles (hosts, profiles, themes, snippets, the keychain);
//! - [`export`]: hosts written as an OpenSSH config file.
//!
//! Every importer gives an [`Imported`]: hosts, the folders they were in as groups, and
//! warnings for what was left out. None of them reads a password.

pub mod bundle;
pub mod common;
pub mod csv;
pub mod export;
pub mod mobaxterm;
pub mod putty;
pub mod remmina;
pub mod ssh_config;

pub use common::{ImportWarning, Imported};
