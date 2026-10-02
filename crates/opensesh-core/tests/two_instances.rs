//! Two instances on one settings folder (Sprint 16's "done when"): each keeps its hosts in
//! memory, adds its own while the other does too, saves through its own background writer and
//! now and then reads the file again (as the file watcher makes it). Nothing may be lost or
//! corrupted.

#![allow(clippy::unwrap_used, reason = "test")]

use std::path::Path;
use std::sync::{Arc, Barrier};
use std::time::Duration;

use opensesh_core::hosts::{Host, HostsFile, new_id};
use opensesh_core::sync::Baselines;
use opensesh_core::writer::FileWriter;

const EACH: usize = 40;

fn instance(path: &Path, name: &str, start: &Barrier) {
    let baselines = Arc::new(Baselines::default());
    let writer = FileWriter::spawn_with(Duration::from_millis(5), Arc::clone(&baselines)).unwrap();
    let read = |baselines: &Baselines| {
        let text = baselines.read_to_string(path).unwrap();
        HostsFile::from_toml_str(&text).unwrap().0
    };
    let mut hosts = read(&baselines);
    start.wait();
    for n in 0..EACH {
        hosts.hosts.push(Host {
            id: new_id(),
            name: format!("{name}-{n}"),
            address: format!("{name}-{n}.lan"),
            ..Host::default()
        });
        writer.write(
            path.to_path_buf(),
            hosts.to_toml_string().unwrap().into_bytes(),
            2,
            None,
        );
        if n % 3 == 0 {
            writer.flush();
        }
        // The watcher noticed the other instance's save.
        if n % 7 == 6 {
            writer.flush();
            hosts = read(&baselines);
        }
    }
    writer.flush();
}

#[test]
fn two_instances_on_one_folder_lose_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hosts.toml");
    std::fs::write(&path, "schema_version = 1\n").unwrap();
    let start = Arc::new(Barrier::new(2));
    let threads: Vec<_> = ["left", "right"]
        .into_iter()
        .map(|name| {
            let (path, start) = (path.clone(), Arc::clone(&start));
            std::thread::spawn(move || instance(&path, name, &start))
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    let text = std::fs::read_to_string(&path).unwrap();
    let (file, warnings) = HostsFile::from_toml_str(&text).unwrap();
    assert_eq!(warnings, []);
    let mut names: Vec<String> = file.hosts.iter().map(|host| host.name.clone()).collect();
    names.sort();
    let mut expected: Vec<String> = ["left", "right"]
        .iter()
        .flat_map(|name| (0..EACH).map(move |n| format!("{name}-{n}")))
        .collect();
    expected.sort();
    assert_eq!(names, expected);
    // Every write was whole: no temporary files left behind.
    let leftovers: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".tmp-"))
        .collect();
    assert_eq!(leftovers, Vec::<String>::new());
}
