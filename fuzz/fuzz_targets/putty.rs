#![no_main]

//! PuTTY's sessions: a `.reg` export and a session file of the Unix sessions folder.

use std::path::Path;

use libfuzzer_sys::fuzz_target;
use opensesh_import::common::{decode_text, percent_decode};
use opensesh_import::putty;

fuzz_target!(|data: &[u8]| {
    let text = decode_text(data);
    let _ = putty::convert(putty::parse_reg(&text, Path::new("fuzz.reg")));
    let name = percent_decode(text.lines().next().unwrap_or_default());
    let _ = putty::convert(vec![putty::parse_session_file(&name, &text, Path::new("fuzz"))]);
});
