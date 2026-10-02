#![no_main]

//! The sync merge and the conflict splitter: three versions of a settings file (one input cut
//! in three at NUL bytes), and a file with Git's conflict markers.

use libfuzzer_sys::fuzz_target;
use opensesh_core::sync;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    if let Some(sides) = sync::split_git_conflict(text) {
        let _ = sync::merge3(sides.base.as_deref().unwrap_or(""), &sides.here, &sides.there);
    }
    let mut parts = text.splitn(3, '\0');
    let (base, ours, theirs) = (
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
    );
    if let Ok(merged) = sync::merge3(base, ours, theirs) {
        // Merging the result again with itself changes nothing.
        let again = sync::merge3(&merged.text, &merged.text, &merged.text);
        assert!(again.is_ok_and(|again| again.conflicts.is_empty()));
    }
    if let (Ok(here), Ok(there)) = (ours.parse::<toml::Table>(), theirs.parse::<toml::Table>()) {
        let _ = sync::differences(&here, &there);
        let _ = sync::resolve(&here, &there, &std::collections::HashMap::new());
    }
});
