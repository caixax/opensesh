//! The importers against their fixtures (`tests/fixtures/`). The MobaXterm, PuTTY and Remmina
//! files were written from the formats' public descriptions (see each module), not exported by
//! the programs themselves; real files replace or join them as they come.

#![allow(clippy::unwrap_used, clippy::panic, reason = "test helpers")]

use std::path::{Path, PathBuf};

use opensesh_core::hosts::{FlowControl, Host, Parity, Protocol, X11Forwarding};
use opensesh_import::csv::{self, Field};
use opensesh_import::{Imported, mobaxterm, putty, remmina};

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(path)
}

fn host<'a>(imported: &'a Imported, name: &str) -> &'a Host {
    imported
        .hosts
        .iter()
        .find(|host| host.name == name)
        .unwrap_or_else(|| panic!("no host {name} in {:?}", imported.hosts))
}

/// The group's folder names from the top.
fn folder(imported: &Imported, host: &Host) -> Vec<String> {
    let mut path = Vec::new();
    let mut at = host.group.clone();
    while let Some(id) = at {
        let group = imported.groups.iter().find(|group| group.id == id).unwrap();
        path.insert(0, group.name.clone());
        at = group.parent.clone();
    }
    path
}

fn warnings(imported: &Imported) -> Vec<String> {
    imported
        .warnings
        .iter()
        .map(|warning| warning.message.clone())
        .collect()
}

#[test]
fn mobaxterm_sessions_export() {
    let imported = mobaxterm::load(&fixture("mobaxterm/sessions.mxtsessions"));
    let names: Vec<&str> = imported.hosts.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Reference Session",
            "web-01 (€)",
            "db-01",
            "terminal-server",
            "kiosk",
            "backups"
        ]
    );

    let reference = host(&imported, "Reference Session");
    assert_eq!(reference.address, "localhost");
    assert_eq!(reference.user, None);
    assert!(folder(&imported, reference).is_empty());

    let web = host(&imported, "web-01 (€)");
    assert_eq!(web.protocol, Protocol::Ssh);
    assert_eq!(web.address, "web-01.example.com");
    assert_eq!(web.port, Some(2222));
    assert_eq!(web.user.as_deref(), Some("deploy"));
    assert_eq!(web.ssh.command.as_deref(), Some("uptime; df -h"));
    assert_eq!(
        web.jump,
        Some(vec![
            "jump@bastion.example.com".to_owned(),
            "ops@inner.example.com:2200".to_owned()
        ])
    );
    assert_eq!(web.identity_file.as_deref(), Some(r"C:\Keys\web.pem"));
    assert_eq!(
        web.ssh.proxy.as_deref(),
        Some("socks5://proxyuser@socks.example.com:1080")
    );
    assert_eq!(web.ssh.agent_forwarding, Some(true));
    assert_eq!(web.ssh.x11, None);
    assert_eq!(web.notes, "Main web server #1");
    assert_eq!(folder(&imported, web), ["Production"]);

    let db = host(&imported, "db-01");
    assert_eq!(db.identity_file, None);
    assert_eq!(
        db.ssh.proxy_command.as_deref(),
        Some("nc -X connect -x proxy:3128 %h %p")
    );
    assert_eq!(db.ssh.agent_forwarding, None);

    let ts = host(&imported, "terminal-server");
    assert_eq!(ts.protocol, Protocol::Rdp);
    assert_eq!(ts.port, None);
    assert_eq!(ts.user.as_deref(), Some(r"CORP\admin"));
    assert_eq!(ts.jump, Some(vec!["jump@bastion.example.com".to_owned()]));
    assert_eq!(ts.rdp.clipboard, Some(false));
    assert_eq!(folder(&imported, ts), ["Production", "Windows"]);

    let kiosk = host(&imported, "kiosk");
    assert_eq!(kiosk.protocol, Protocol::Vnc);
    assert_eq!(kiosk.port, Some(5901));
    assert_eq!(kiosk.vnc.read_only, Some(true));
    assert_eq!(kiosk.jump, None);

    let backups = host(&imported, "backups");
    assert_eq!(backups.protocol, Protocol::Sftp);
    assert_eq!(backups.port, Some(2022));
    assert_eq!(backups.user.as_deref(), Some("backup"));
    assert_eq!(
        backups.identity_file.as_deref(),
        Some("/home/me/.ssh/id_ed25519")
    );
    assert_eq!(folder(&imported, backups), ["Files"]);

    // Production, Production\Windows and Files.
    assert_eq!(imported.groups.len(), 3);
    let messages = warnings(&imported);
    assert_eq!(messages.len(), 3, "{messages:?}");
    assert!(messages[0].starts_with(r"db-01: the PuTTY key C:\Keys\db.ppk was left out"));
    assert_eq!(
        messages[1],
        "old-switch: Type 1 sessions aren't imported yet; skipped"
    );
    assert_eq!(messages[2], "broken: not a session line; skipped");
    assert!(imported.warnings.iter().all(|warning| warning.line > 0));
}

#[test]
fn mobaxterm_ini_and_single_sessions() {
    let imported = mobaxterm::load(&fixture("mobaxterm/MobaXterm.ini"));
    assert_eq!(warnings(&imported), Vec::<String>::new());
    let names: Vec<&str> = imported.hosts.iter().map(|h| h.name.as_str()).collect();
    // The [Misc] and [Passwords] sections are not sessions.
    assert_eq!(names, ["lab", "team-box"]);
    assert_eq!(folder(&imported, host(&imported, "team-box")), ["Team"]);
    assert_eq!(host(&imported, "team-box").user, None);

    let imported = mobaxterm::load(&fixture("mobaxterm/single.moba"));
    assert_eq!(imported.hosts.len(), 1);
    assert_eq!(imported.hosts[0].address, "solo.example.com");

    let missing = mobaxterm::load(&fixture("mobaxterm/missing.mxtsessions"));
    assert!(missing.hosts.is_empty());
    assert!(missing.warnings[0].message.starts_with("could not be read"));
}

#[test]
fn putty_registry_export() {
    let imported = putty::load_reg(&fixture("putty/sessions.reg"));
    let names: Vec<&str> = imported.hosts.iter().map(|h| h.name.as_str()).collect();
    // Default Settings is not a session; the raw one is skipped; host keys aren't sessions.
    assert_eq!(
        names,
        [
            "router (lab)",
            "via command",
            "Switch console",
            "COM3 console"
        ]
    );

    let router = host(&imported, "router (lab)");
    assert_eq!(router.address, "192.168.1.1");
    assert_eq!(router.user.as_deref(), Some("admin"));
    assert_eq!(router.port, None);
    assert_eq!(router.identity_file, None);
    assert_eq!(
        router.ssh.proxy.as_deref(),
        Some("socks5://me@socks.lab:1080")
    );
    assert_eq!(router.ssh.proxy_command, None);
    assert_eq!(router.ssh.agent_forwarding, Some(true));
    assert_eq!(router.ssh.compression, Some(true));
    assert_eq!(router.ssh.x11, Some(X11Forwarding::Untrusted));
    assert_eq!(router.ssh.command, None);

    let command = host(&imported, "via command");
    assert_eq!(command.port, Some(2222));
    assert_eq!(command.user.as_deref(), Some("ops"));
    assert_eq!(
        command.identity_file.as_deref(),
        Some(r"C:\Users\me\.ssh\id_ed25519")
    );
    assert_eq!(
        command.ssh.proxy_command.as_deref(),
        Some("ssh -W %h:%p jump.lab")
    );
    assert_eq!(command.ssh.command.as_deref(), Some("tmux attach"));

    let switch = host(&imported, "Switch console");
    assert_eq!(switch.protocol, Protocol::Telnet);
    assert_eq!(switch.port, Some(2323));

    let com3 = host(&imported, "COM3 console");
    assert_eq!(com3.protocol, Protocol::Serial);
    assert_eq!(com3.address, "COM3");
    assert_eq!(com3.port, None);
    assert_eq!(com3.serial.baud, Some(115_200));
    assert_eq!(com3.serial.data_bits, Some(7));
    assert_eq!(com3.serial.stop_bits, Some(2));
    assert_eq!(com3.serial.parity, Some(Parity::Even));
    assert_eq!(com3.serial.flow_control, Some(FlowControl::Hardware));

    let messages = warnings(&imported);
    assert_eq!(messages.len(), 3, "{messages:?}");
    assert!(messages[0].starts_with(r"router (lab): the PuTTY key C:\Users\me\keys\router.ppk"));
    assert_eq!(
        messages[1],
        "router (lab): its port forwardings were left out; add them as tunnels"
    );
    assert_eq!(messages[2], "café: raw sessions aren't supported; skipped");
}

#[test]
fn putty_sessions_folder() {
    let imported = putty::load_dir(&fixture("putty/sessions"));
    assert_eq!(warnings(&imported), Vec::<String>::new());
    let names: Vec<&str> = imported.hosts.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["ttyUSB0", "web server"]);
    let web = host(&imported, "web server");
    assert_eq!(web.address, "web.example");
    assert_eq!(web.user.as_deref(), Some("www"));
    assert_eq!(web.identity_file.as_deref(), Some("/home/me/.ssh/id_rsa"));
    assert_eq!(web.ssh.x11, None);
    let tty = host(&imported, "ttyUSB0");
    assert_eq!(tty.address, "/dev/ttyUSB0");
    assert_eq!(tty.serial.baud, Some(9600));
    assert_eq!(tty.serial.stop_bits, None);
    assert_eq!(tty.serial.flow_control, Some(FlowControl::Software));
}

#[test]
fn remmina_profiles() {
    let imported = remmina::load(&fixture("remmina"));
    let names: Vec<&str> = imported.hosts.iter().map(|h| h.name.as_str()).collect();
    // Sorted by file name; the SPICE profile and the .txt file are left out.
    assert_eq!(names, ["NAS", "Office PC", "Raspberry Pi"]);

    let office = host(&imported, "Office PC");
    assert_eq!(office.protocol, Protocol::Rdp);
    assert_eq!(office.address, "office.example.com");
    assert_eq!(office.port, Some(3390));
    assert_eq!(office.user.as_deref(), Some("jdoe"));
    assert_eq!(office.rdp.domain.as_deref(), Some("CORP"));
    assert_eq!(office.rdp.clipboard, Some(false));
    assert_eq!(
        office.jump,
        Some(vec!["jdoe@gw.example.com:2222".to_owned()])
    );
    assert_eq!(office.tags, ["work", "windows"]);
    assert_eq!(office.notes, "Ask IT first");
    assert_eq!(folder(&imported, office), ["Work", "Windows"]);

    let pi = host(&imported, "Raspberry Pi");
    assert_eq!(pi.protocol, Protocol::Vnc);
    assert_eq!(pi.port, Some(5901));
    assert_eq!(pi.vnc.read_only, Some(true));
    assert_eq!(pi.jump, None);

    let nas = host(&imported, "NAS");
    assert_eq!(nas.protocol, Protocol::Ssh);
    assert_eq!(nas.address, "2001:db8::5");
    assert_eq!(nas.port, Some(2200));
    assert_eq!(
        nas.identity_file.as_deref(),
        Some("/home/me/.ssh/id_ed25519")
    );
    // Both Home profiles share one group.
    assert_eq!(nas.group, pi.group);
    assert_eq!(imported.groups.len(), 3);

    assert_eq!(
        warnings(&imported),
        ["VM console: SPICE profiles aren't supported; skipped"]
    );
}

#[test]
fn csv_with_mapped_columns() {
    let table = csv::load(&fixture("csv/hosts.csv")).unwrap();
    assert_eq!(table.delimiter, ',');
    let mapping = csv::guess_columns(&table.rows[0]);
    assert_eq!(
        mapping,
        [
            Field::Name,
            Field::Address,
            Field::Port,
            Field::User,
            Field::Protocol,
            Field::Group,
            Field::Tags,
            Field::Notes
        ]
    );
    let imported = csv::to_hosts(&table, &mapping, true, &fixture("csv/hosts.csv"));
    let names: Vec<&str> = imported.hosts.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["web", "db", "desk"]);

    let web = host(&imported, "web");
    assert_eq!(web.port, None);
    assert_eq!(web.user.as_deref(), Some("deploy"));
    assert_eq!(web.tags, ["web", "linux"]);
    assert_eq!(web.notes, "Front end");
    assert_eq!(folder(&imported, web), ["Prod", "Web"]);

    let db = host(&imported, "db");
    assert_eq!(db.address, "db.example.com");
    assert_eq!(db.user.as_deref(), Some("admin"));
    assert_eq!(db.port, Some(2222));
    assert_eq!(db.notes, r#"Primary database, "main""#);
    assert_eq!(folder(&imported, db), ["Prod"]);

    let desk = host(&imported, "desk");
    assert_eq!(desk.protocol, Protocol::Rdp);
    assert_eq!(desk.port, None);
    assert_eq!(desk.user.as_deref(), Some(r"CORP\me"));

    let lines: Vec<usize> = imported.warnings.iter().map(|w| w.line).collect();
    assert_eq!(lines, [5, 6]);
    assert_eq!(
        warnings(&imported),
        ["no address; skipped", "unknown protocol gopher; skipped"]
    );
}

#[test]
fn csv_without_names_or_a_header() {
    let table = csv::load(&fixture("csv/semicolons.csv")).unwrap();
    assert_eq!(table.delimiter, ';');
    let mapping = csv::guess_columns(&table.rows[0]);
    assert_eq!(mapping, [Field::Address, Field::User]);
    let imported = csv::to_hosts(&table, &mapping, true, Path::new("s.csv"));
    let names: Vec<&str> = imported.hosts.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["alpha.lan", "beta.lan"]);
    assert_eq!(imported.hosts[0].user.as_deref(), Some("root"));
    assert_eq!(imported.hosts[1].user, None);
    // Without skipping the first row, the header is a host too.
    let imported = csv::to_hosts(&table, &mapping, false, Path::new("s.csv"));
    assert_eq!(imported.hosts.len(), 3);
}

#[test]
fn several_sources_share_folders() {
    let mut all = remmina::load(&fixture("remmina"));
    all.extend(csv::to_hosts(
        &csv::parse("name,host,folder\nextra,extra.lan,Home\n"),
        &[Field::Name, Field::Address, Field::Group],
        true,
        Path::new("x.csv"),
    ));
    let extra = host(&all, "extra");
    assert_eq!(extra.group, host(&all, "NAS").group);
    assert_eq!(all.groups.len(), 3);
}
