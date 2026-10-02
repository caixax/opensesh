#![no_main]

//! The paste analyzer: whatever is on the clipboard when the user pastes into a terminal.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = opensesh_core::paste::analyze(text, false);
        let _ = opensesh_core::paste::analyze(text, true);
    }
});
