#![no_main]

//! A CSV file, read with its guessed columns.

use std::path::Path;

use libfuzzer_sys::fuzz_target;
use opensesh_import::common::decode_text;
use opensesh_import::csv;

fuzz_target!(|data: &[u8]| {
    let table = csv::parse(&decode_text(data));
    let columns = table
        .rows
        .first()
        .map(|headers| csv::guess_columns(headers))
        .unwrap_or_default();
    let _ = csv::to_hosts(&table, &columns, true, Path::new("fuzz.csv"));
    let _ = csv::to_hosts(&table, &csv::Field::ALL, false, Path::new("fuzz.csv"));
});
