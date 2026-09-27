//! cxx-qt bridges between Rust and C++/QML. `build.rs` compiles every file in this folder.

pub mod app_info;
pub mod hosts;
pub mod instance;
pub mod keybindings;
pub mod keychain;
pub mod platform;
pub mod settings;
pub mod sftp_browser;
pub mod shim;
pub mod snippets;
pub mod terminal_profiles;
pub mod terminal_sessions;
pub mod terminal_view;
pub mod theme;
pub mod transfers;
pub mod tunnels;
pub mod ui_state;
pub mod updater;
pub mod workspaces;
