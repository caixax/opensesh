#![no_main]

//! Quick connect's parser (`user@host:port -J jump`, `ssh://`, `rdp://`...): any text typed or
//! pasted into the quick connect field.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = opensesh_core::hosts::target::parse(text);
    }
});
