#![no_main]

//! The terminal theme importers (iTerm2, Windows Terminal, Alacritty, Kitty, base16, OpenSesh):
//! a theme file from the web.

use libfuzzer_sys::fuzz_target;
use opensesh_core::terminal::import::{Format, import_text};

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        for format in Format::ALL {
            let _ = import_text(text, format, "fuzz");
        }
    }
});
