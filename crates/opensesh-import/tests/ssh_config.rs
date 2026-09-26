//! `~/.ssh/config` fixtures with includes (tests/fixtures/home/.ssh).

use std::path::{Path, PathBuf};

use opensesh_import::ssh_config;

fn home() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/home")
}

#[test]
fn a_config_with_includes_is_read_like_openssh_does() {
    let home = home();
    let config = ssh_config::load(&home.join(".ssh/config"), &home);
    let names: Vec<&str> = config
        .hosts
        .iter()
        .map(|host| host.alias.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "work-db",
            "home-nas",
            "web-01",
            "loop-host",
            "bastion",
            "web-02",
            "legacy"
        ]
    );
    let host = |name: &str| config.hosts.iter().find(|host| host.alias == name).unwrap();

    // The included files come first, and the first value wins.
    assert_eq!(host("web-01").user.as_deref(), Some("overridden-later"));
    assert_eq!(host("web-02").user.as_deref(), Some("deploy"));
    assert_eq!(
        host("web-02").host_name.as_deref(),
        Some("web-02.internal.example.com")
    );
    assert_eq!(host("web-02").proxy_jump, Some(vec!["bastion".to_owned()]));
    assert_eq!(host("web-02").identity_files, vec!["~/.ssh/id_deploy"]);
    assert_eq!(host("work-db").port, Some(5022));
    assert_eq!(host("bastion").port, Some(2200));
    // An Include inside a Host block adds to that block.
    assert_eq!(host("legacy").user.as_deref(), Some("root"));
    assert_eq!(host("legacy").port, Some(2022));
    // Nothing from `Host *`, patterns, Match or files the glob doesn't match.
    assert!(
        config
            .hosts
            .iter()
            .all(|host| host.user.as_deref() != Some("nobody"))
    );
    assert!(config.hosts.iter().all(|host| host.alias != "not-included"));

    let warnings: Vec<String> = config.warnings.iter().map(ToString::to_string).collect();
    assert!(
        warnings.iter().any(|w| w.contains("includes itself")),
        "{warnings:?}"
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("missing-file") && w.contains("doesn't exist")),
        "{warnings:?}"
    );
    assert!(
        warnings.iter().any(|w| w.contains("pattern * is skipped")),
        "{warnings:?}"
    );
    assert!(
        warnings.iter().any(|w| w.contains("pattern *.lab")),
        "{warnings:?}"
    );
    assert!(warnings.iter().any(|w| w.contains("Match")), "{warnings:?}");

    // Every file read, for the watcher.
    let files: Vec<String> = config
        .files
        .iter()
        .map(|file| file.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    for expected in [
        "config",
        "10-work.conf",
        "20-personal.conf",
        "loop",
        "legacy-options",
    ] {
        assert!(files.iter().any(|file| file == expected), "{files:?}");
    }
    assert!(!files.iter().any(|file| file == "ignored.txt"));
}

#[test]
fn a_missing_config_gives_nothing_and_says_why() {
    let home = home();
    let config = ssh_config::load(&home.join(".ssh/nope"), &home);
    assert!(config.hosts.is_empty());
    assert_eq!(config.warnings.len(), 1);
    assert!(config.warnings[0].message.contains("could not read"));
}

#[test]
fn imported_hosts_connect_through_their_jump_host() {
    let home = home();
    let config = ssh_config::load(&home.join(".ssh/config"), &home);
    let file = opensesh_core::hosts::HostsFile {
        hosts: config.to_hosts(true, None),
        ..Default::default()
    };
    let web = file.find_host("web-02").unwrap();
    assert_eq!(
        file.ssh_args(web).to_args(),
        vec![
            "-i",
            "~/.ssh/id_deploy",
            "-J",
            "jump@bastion.example.com:2200",
            "-o",
            "ServerAliveInterval=30",
            "deploy@web-02.internal.example.com"
        ]
    );
}
