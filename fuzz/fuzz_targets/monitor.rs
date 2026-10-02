#![no_main]

//! The remote monitor's and the host info's parsers: what a server prints for them.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let _ = opensesh_ssh::monitor::parse(&text);
    let _ = opensesh_ssh::monitor::parse_info(&text);
});
