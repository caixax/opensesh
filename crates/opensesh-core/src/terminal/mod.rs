//! Terminal customization (PLAN §6.2 to §6.5): the options and their profiles, color themes and
//! their importers, and keyword highlighting rules. The engine (`opensesh-term`) and the app use
//! these; nothing here depends on either.

pub mod highlight;
pub mod import;
pub mod profile;
pub mod settings;
pub mod theme;
