//! Hosts written as an OpenSSH config file (Sprint 16), for `ssh` and the tools that read
//! `~/.ssh/config`.
//!
//! Each SSH, SFTP or Mosh host becomes a `Host` block with what it resolves to (its group's
//! settings included): `HostName`, `User`, `Port`, `IdentityFile`, `ProxyJump`,
//! `ProxyCommand`, `ForwardAgent`, `ForwardX11`, `Compression`. Keychain identities can't be
//! written (their keys stay in the vault): the block says so in a comment.

use std::collections::HashSet;
use std::fmt::Write as _;

use opensesh_core::hosts::{Host, HostsFile, Protocol, X11Forwarding};

/// What the export left out, by host name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Exported {
    /// The file's text.
    pub text: String,
    /// How many hosts were written.
    pub count: usize,
    /// Hosts left out (other protocols), and settings OpenSSH can't take.
    pub warnings: Vec<String>,
}

/// `hosts` (from `file`) as an OpenSSH config file.
#[must_use]
pub fn ssh_config(file: &HostsFile, hosts: &[&Host]) -> Exported {
    let mut out = Exported {
        text: "# Exported from OpenSesh.\n".to_owned(),
        ..Exported::default()
    };
    let mut taken = HashSet::new();
    for host in hosts {
        if !matches!(
            host.protocol,
            Protocol::Ssh | Protocol::Sftp | Protocol::Mosh
        ) {
            out.warnings
                .push(format!("{}: not an SSH host; left out", host.name));
            continue;
        }
        if host.address.trim().is_empty() {
            continue;
        }
        let alias = unique(alias(&host.name), &mut taken);
        let resolved = file.resolve(host);
        let mut block = String::new();
        if alias != host.name {
            let _ = writeln!(block, "# {}", host.name.replace('\n', " "));
        }
        let _ = writeln!(block, "Host {alias}");
        let _ = writeln!(block, "    HostName {}", host.address.trim());
        if let Some(user) = resolved.user() {
            let _ = writeln!(block, "    User {}", quote(user));
        }
        if let Some(port) = resolved.port().filter(|port| *port != 22) {
            let _ = writeln!(block, "    Port {port}");
        }
        if let Some(path) = resolved.string("identity_file") {
            let _ = writeln!(block, "    IdentityFile {}", quote(path));
        }
        if resolved.identity().is_some() {
            let _ = writeln!(
                block,
                "    # Its keychain identity stays in OpenSesh (export the key from the Keychain view)."
            );
        }
        let jumps: Vec<String> = resolved
            .jump()
            .iter()
            .map(|jump| file.jump_spec(jump))
            .collect();
        if !jumps.is_empty() {
            let _ = writeln!(block, "    ProxyJump {}", jumps.join(","));
        }
        if let Some(command) = resolved.string("ssh.proxy_command") {
            // OpenSesh's %h, %p and %r are OpenSSH's.
            let _ = writeln!(block, "    ProxyCommand {command}");
        } else if resolved.string("ssh.proxy").is_some() {
            out.warnings.push(format!(
                "{}: its proxy has no OpenSSH setting; left out",
                host.name
            ));
        }
        if resolved.flag("ssh.agent_forwarding") {
            let _ = writeln!(block, "    ForwardAgent yes");
        }
        match resolved.x11() {
            X11Forwarding::Off => {}
            X11Forwarding::Untrusted => {
                let _ = writeln!(block, "    ForwardX11 yes");
            }
            X11Forwarding::Trusted => {
                let _ = writeln!(block, "    ForwardX11 yes\n    ForwardX11Trusted yes");
            }
        }
        if resolved.flag("ssh.compression") {
            let _ = writeln!(block, "    Compression yes");
        }
        out.text.push('\n');
        out.text.push_str(&block);
        out.count += 1;
    }
    out
}

/// A `Host` pattern from a name: spaces become `-`, OpenSSH's pattern characters go.
fn alias(name: &str) -> String {
    let alias: String = name
        .trim()
        .chars()
        .filter_map(|c| match c {
            c if c.is_whitespace() => Some('-'),
            '*' | '?' | '!' | ',' | '"' | '#' => None,
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect();
    if alias.is_empty() {
        "host".to_owned()
    } else {
        alias
    }
}

fn unique(alias: String, taken: &mut HashSet<String>) -> String {
    let mut candidate = alias.clone();
    let mut n = 2;
    while !taken.insert(candidate.to_ascii_lowercase()) {
        candidate = format!("{alias}-{n}");
        n += 1;
    }
    candidate
}

/// A value with spaces in double quotes, as OpenSSH reads them.
fn quote(value: &str) -> String {
    if value.contains(char::is_whitespace) {
        format!("\"{value}\"")
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use opensesh_core::hosts::{Group, HostDefaults};

    use super::*;

    #[test]
    fn hosts_as_blocks() {
        let mut file = HostsFile::default();
        file.groups.push(Group {
            id: "G".to_owned(),
            name: "Prod".to_owned(),
            defaults: HostDefaults {
                user: Some("deploy".to_owned()),
                ..HostDefaults::default()
            },
            ..Group::default()
        });
        let mut bastion = Host {
            id: "B".to_owned(),
            name: "bastion".to_owned(),
            address: "bastion.example".to_owned(),
            port: Some(2222),
            ..Host::default()
        };
        bastion.ssh.agent_forwarding = Some(true);
        let mut web = Host {
            id: "W".to_owned(),
            name: "web server".to_owned(),
            address: "10.0.0.5".to_owned(),
            group: Some("G".to_owned()),
            identity_file: Some("~/.ssh/my key".to_owned()),
            jump: Some(vec!["B".to_owned()]),
            ..Host::default()
        };
        web.ssh.x11 = Some(X11Forwarding::Trusted);
        web.ssh.proxy_command = Some("nc -X 5 -x proxy:1080 %h %p".to_owned());
        let desk = Host {
            id: "D".to_owned(),
            name: "desk".to_owned(),
            protocol: Protocol::Rdp,
            address: "desk".to_owned(),
            ..Host::default()
        };
        let twin = Host {
            id: "T".to_owned(),
            name: "Web Server".to_owned(),
            address: "10.0.0.6".to_owned(),
            ..Host::default()
        };
        file.hosts = vec![bastion, web, desk, twin];
        let refs: Vec<&Host> = file.hosts.iter().collect();
        let exported = ssh_config(&file, &refs);
        assert_eq!(exported.count, 3);
        assert_eq!(exported.warnings, ["desk: not an SSH host; left out"]);
        assert_eq!(
            exported.text,
            "# Exported from OpenSesh.\n\
             \n\
             Host bastion\n    HostName bastion.example\n    Port 2222\n    ForwardAgent yes\n\
             \n\
             # web server\n\
             Host web-server\n    HostName 10.0.0.5\n    User deploy\n    IdentityFile \"~/.ssh/my key\"\n    ProxyJump bastion.example:2222\n    ProxyCommand nc -X 5 -x proxy:1080 %h %p\n    ForwardX11 yes\n    ForwardX11Trusted yes\n\
             \n\
             # Web Server\n\
             Host Web-Server-2\n    HostName 10.0.0.6\n"
        );
        // What it wrote reads back.
        let back = crate::ssh_config::parse_str(
            &exported.text,
            std::path::Path::new("/home/me/.ssh/config"),
            std::path::Path::new("/home/me"),
        );
        assert_eq!(back.hosts.len(), 3);
        assert_eq!(back.hosts[1].user.as_deref(), Some("deploy"));
        assert_eq!(back.warnings, []);
    }
}
