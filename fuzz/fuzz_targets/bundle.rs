#![no_main]

//! An OpenSesh bundle from someone else: read, then turned into hosts with new ids.

use std::collections::HashMap;
use std::path::Path;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data)
        && let Ok((bundle, _)) = opensesh_import::bundle::read(text)
    {
        let _ = bundle.to_imported(&HashMap::new(), &|_| false, Path::new("fuzz.opensesh"));
        let _ = opensesh_import::bundle::write(&bundle);
    }
});
