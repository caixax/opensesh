//! Remmina's connection profiles (Sprint 16): `.remmina` files, kept in
//! `$XDG_DATA_HOME/remmina` (`~/.local/share/remmina`), or `~/.remmina` in old versions.
//!
//! Each file is an INI file with a `[remmina]` section. The keys read are the ones Remmina's own
//! source reads: `name`, `group` (folders separated by `/`), `protocol`, `server` (`host[:port]`,
//! IPv6 in brackets; for VNC a number under 100 is a display, port 5900 + n), `username`,
//! `domain`, `ssh_privatekey`, `ssh_tunnel_enabled`, `ssh_tunnel_server` and
//! `ssh_tunnel_username` (the tunnel becomes a jump host), `viewonly`, `disableclipboard`,
//! `labels` (comma-separated, as tags) and `notes_text` (`%XX`-escaped). RDP, VNC, SSH and SFTP
//! profiles are imported. Passwords are never read (Remmina keeps them encrypted, or in the
//! system keyring).

use std::path::{Path, PathBuf};

use opensesh_core::hosts::{Host, Protocol};

use crate::common::{
    ImportWarning, Imported, decode_text, folder_path, is_putty_key, jump_spec, percent_decode,
    putty_key_warning, split_host_port,
};

/// Where Remmina keeps its profiles: `$XDG_DATA_HOME/remmina` (else `~/.local/share/remmina`)
/// when it exists, else the old `~/.remmina`.
#[must_use]
pub fn profiles_dir(home: &Path, xdg_data_home: Option<&Path>) -> PathBuf {
    let data = xdg_data_home.map_or_else(|| home.join(".local").join("share"), Path::to_path_buf);
    let current = data.join("remmina");
    if current.is_dir() {
        current
    } else {
        home.join(".remmina")
    }
}

/// Reads every `.remmina` file in `dir`, or one file.
#[must_use]
pub fn load(path: &Path) -> Imported {
    if path.is_dir() {
        let mut files: Vec<PathBuf> = match std::fs::read_dir(path) {
            Ok(entries) => entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|file| {
                    file.extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("remmina"))
                })
                .collect(),
            Err(error) => {
                return Imported {
                    warnings: vec![ImportWarning::new(
                        path,
                        format!("could not be read: {error}"),
                    )],
                    ..Imported::default()
                };
            }
        };
        files.sort();
        let mut imported = Imported::default();
        for file in files {
            imported.extend(load_file(&file));
        }
        return imported;
    }
    load_file(path)
}

fn load_file(path: &Path) -> Imported {
    // A small file the user asked to import.
    match std::fs::read(path) {
        Ok(bytes) => parse_str(&decode_text(&bytes), path),
        Err(error) => Imported {
            warnings: vec![ImportWarning::new(
                path,
                format!("could not be read: {error}"),
            )],
            ..Imported::default()
        },
    }
}

/// One profile's text; `origin` is its file (its name stands in for a missing `name`).
#[must_use]
pub fn parse_str(text: &str, origin: &Path) -> Imported {
    let mut imported = Imported::default();
    let mut in_section = false;
    let mut get = std::collections::HashMap::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(['#', ';']) {
            continue;
        }
        if let Some(section) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            in_section = section == "remmina";
            continue;
        }
        if in_section && let Some((key, value)) = line.split_once('=') {
            get.insert(key.trim().to_owned(), value.trim().to_owned());
        }
    }
    let value = |key: &str| get.get(key).map(String::as_str).filter(|v| !v.is_empty());
    let name = value("name").map_or_else(
        || {
            origin
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default()
        },
        str::to_owned,
    );
    let mut warn = |message: String| {
        imported
            .warnings
            .push(ImportWarning::new(origin, format!("{name}: {message}")));
    };
    let Some(kind) = value("protocol") else {
        warn("not a Remmina profile; skipped".to_owned());
        return imported;
    };
    let (protocol, usual) = match kind {
        "RDP" => (Protocol::Rdp, 3389),
        "VNC" => (Protocol::Vnc, 5900),
        "SSH" => (Protocol::Ssh, 22),
        "SFTP" => (Protocol::Sftp, 22),
        other => {
            warn(format!("{other} profiles aren't supported; skipped"));
            return imported;
        }
    };
    let Some(server) = value("server") else {
        warn("no server; skipped".to_owned());
        return imported;
    };
    if server.starts_with("unix://") {
        warn("a VNC server on a local socket isn't supported; skipped".to_owned());
        return imported;
    }
    let (address, mut port) = split_host_port(server);
    if protocol == Protocol::Vnc {
        // Remmina, like libvncclient, takes small numbers as display numbers.
        port = port.map(|port| if port < 100 { port + 5900 } else { port });
    }
    let mut host = Host {
        name: name.clone(),
        protocol,
        address,
        port: port.filter(|port| *port != usual),
        user: value("username").map(str::to_owned),
        tags: value("labels")
            .map(|labels| {
                labels
                    .split(',')
                    .map(str::trim)
                    .filter(|label| !label.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        notes: value("notes_text").map(percent_decode).unwrap_or_default(),
        ..Host::default()
    };
    match protocol {
        Protocol::Rdp => {
            host.rdp.domain = value("domain").map(str::to_owned);
        }
        Protocol::Vnc => {
            if value("viewonly") == Some("1") {
                host.vnc.read_only = Some(true);
            }
        }
        _ => {
            if let Some(key) = value("ssh_privatekey") {
                if is_putty_key(key) {
                    imported
                        .warnings
                        .push(putty_key_warning(origin, &name, key));
                } else {
                    host.identity_file = Some(key.to_owned());
                }
            }
        }
    }
    if value("disableclipboard") == Some("1") {
        match protocol {
            Protocol::Rdp => host.rdp.clipboard = Some(false),
            Protocol::Vnc => host.vnc.clipboard = Some(false),
            _ => {}
        }
    }
    if value("ssh_tunnel_enabled") == Some("1") {
        match value("ssh_tunnel_server") {
            Some(tunnel) => {
                let (tunnel_host, tunnel_port) = split_host_port(tunnel);
                host.jump = Some(vec![jump_spec(
                    value("ssh_tunnel_username"),
                    &tunnel_host,
                    tunnel_port,
                )]);
            }
            None => imported.warnings.push(ImportWarning::new(
                origin,
                format!(
                    "{name}: the SSH tunnel to the server itself was left out; set the host's jump host"
                ),
            )),
        }
    }
    host.group = value("group")
        .map(|group| folder_path(group, '/'))
        .and_then(|path| imported.group(&path));
    imported.hosts.push(host);
    imported
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_profiles_folder() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(
            profiles_dir(home.path(), None),
            home.path().join(".remmina")
        );
        let current = home.path().join(".local").join("share").join("remmina");
        std::fs::create_dir_all(&current).unwrap();
        assert_eq!(profiles_dir(home.path(), None), current);
    }

    #[test]
    fn vnc_displays_and_a_missing_name() {
        let imported = parse_str(
            "[remmina]\nprotocol=VNC\nserver=desk.lan:1\n",
            Path::new("/p/1700000000.remmina"),
        );
        assert_eq!(imported.warnings, []);
        let desk = &imported.hosts[0];
        assert_eq!(desk.name, "1700000000");
        assert_eq!(desk.port, Some(5901));
        let imported = parse_str(
            "[remmina]\nname=Other\nprotocol=SPICE\nserver=a\n",
            Path::new("o.remmina"),
        );
        assert!(imported.hosts.is_empty());
        assert_eq!(imported.warnings.len(), 1);
    }
}
