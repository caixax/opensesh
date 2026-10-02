#![no_main]

//! A Remmina profile (`.remmina`).

use std::path::Path;

use libfuzzer_sys::fuzz_target;
use opensesh_import::common::decode_text;

fuzz_target!(|data: &[u8]| {
    let _ = opensesh_import::remmina::parse_str(&decode_text(data), Path::new("fuzz.remmina"));
});
