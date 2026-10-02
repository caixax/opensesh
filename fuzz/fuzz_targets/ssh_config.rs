#![no_main]

//! `~/.ssh/config`. Includes are followed, so the fuzzer may name files on this computer: they
//! are only read (regular files, bounded), never written.

use std::path::Path;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let config = opensesh_import::ssh_config::parse_str(
            text,
            Path::new("/nonexistent-fuzz-home/.ssh/config"),
            Path::new("/nonexistent-fuzz-home"),
        );
        let _ = config.to_hosts(false, None);
    }
});
