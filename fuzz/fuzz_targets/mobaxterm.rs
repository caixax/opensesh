#![no_main]

//! MobaXterm's session files (`.mxtsessions`, `.moba`, `MobaXterm.ini`), as bytes in any
//! encoding.

use std::path::Path;

use libfuzzer_sys::fuzz_target;
use opensesh_import::common::decode_text;

fuzz_target!(|data: &[u8]| {
    let _ = opensesh_import::mobaxterm::parse_str(&decode_text(data), Path::new("fuzz.mxtsessions"));
});
