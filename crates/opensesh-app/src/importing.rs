//! Importing hosts from other programs and from OpenSesh bundles, and exporting them as an
//! OpenSSH config file (Sprint 16): what the `Hosts` singleton's import and export calls do,
//! without Qt.
//!
//! Every source gives an [`Imported`]; [`apply`] adds it to `hosts.toml` under a new group, its
//! folders as groups inside it. The files are small and chosen by the user; reading them is
//! quick enough for the GUI thread, as the `~/.ssh/config` preview already does.

use std::collections::HashMap;
use std::path::Path;

use opensesh_core::hosts::{Group, Host, HostsFile, new_id};
use opensesh_import::csv::{self, Field};
use opensesh_import::{Imported, bundle, export, mobaxterm, putty, remmina};
use serde_json::{Value as Json, json};

/// The path that means PuTTY's sessions in the Windows registry.
pub const REGISTRY: &str = "registry";

/// Reads `path` as `source`: `mobaxterm`, `putty` (a `.reg` file, a sessions folder, or
/// [`REGISTRY`]), `remmina` (a profile or a folder of them), `csv` (`options.columns` maps
/// each column to a field code, `options.header` skips the first row) or `bundle`
/// (`options.identities` maps the bundle's identity ids to this computer's).
///
/// # Errors
///
/// When the source is unknown, or the file can't be read as one.
pub fn read(source: &str, path: &str, options: &Json, home: &Path) -> Result<Imported, String> {
    let path_text = path.trim();
    let file = opensesh_core::paths::expand_tilde(path_text, home);
    match source {
        "mobaxterm" => Ok(mobaxterm::load(&file)),
        "putty" if path_text == REGISTRY => Ok(putty::read_registry()),
        "putty" if file.is_dir() => Ok(putty::load_dir(&file)),
        "putty" => Ok(putty::load_reg(&file)),
        "remmina" => Ok(remmina::load(&file)),
        "csv" => {
            let table = csv::load(&file).map_err(|error| error.to_string())?;
            let columns: Vec<Field> = options
                .get("columns")
                .and_then(Json::as_array)
                .map(|codes| {
                    codes
                        .iter()
                        .map(|code| {
                            code.as_str()
                                .and_then(Field::from_code)
                                .unwrap_or(Field::Ignore)
                        })
                        .collect()
                })
                .unwrap_or_else(|| {
                    table
                        .rows
                        .first()
                        .map(|headers| csv::guess_columns(headers))
                        .unwrap_or_default()
                });
            let header = options
                .get("header")
                .and_then(Json::as_bool)
                .unwrap_or(true);
            Ok(csv::to_hosts(&table, &columns, header, &file))
        }
        "bundle" => {
            let (contents, warnings) = bundle::load(&file).map_err(|error| error.to_string())?;
            let identities: HashMap<String, String> = options
                .get("identities")
                .and_then(Json::as_object)
                .map(|map| {
                    map.iter()
                        .filter_map(|(old, new)| Some((old.clone(), new.as_str()?.to_owned())))
                        .collect()
                })
                .unwrap_or_default();
            let known = |id: &str| crate::keychain::identity_user(id).is_some();
            let mut imported = contents.to_imported(&identities, &known, &file);
            imported.warnings.extend(
                warnings
                    .into_iter()
                    .map(|message| opensesh_import::ImportWarning::new(&file, message)),
            );
            Ok(imported)
        }
        other => Err(format!("unknown source {other}")),
    }
}

/// Test runs only (the smoke test, screenshots): writes a sample of each source, from the
/// importers' fixtures, to a temporary folder: `{dir, mobaxterm, putty, remmina, csv}`.
///
/// # Errors
///
/// When the folder or a file can't be written.
pub fn write_samples() -> std::io::Result<Json> {
    macro_rules! fixture {
        ($path:literal) => {
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../opensesh-import/tests/fixtures/",
                $path
            ))
            .as_slice()
        };
    }
    let dir = std::env::temp_dir().join(format!("opensesh-import-samples-{}", std::process::id()));
    let remmina = dir.join("remmina");
    std::fs::create_dir_all(&remmina)?;
    let files: [(std::path::PathBuf, &[u8]); 7] = [
        (
            dir.join("sessions.mxtsessions"),
            fixture!("mobaxterm/sessions.mxtsessions"),
        ),
        (dir.join("sessions.reg"), fixture!("putty/sessions.reg")),
        (
            remmina.join("office.remmina"),
            fixture!("remmina/office.remmina"),
        ),
        (remmina.join("pi.remmina"), fixture!("remmina/pi.remmina")),
        (remmina.join("nas.remmina"), fixture!("remmina/nas.remmina")),
        (
            remmina.join("spice.remmina"),
            fixture!("remmina/spice.remmina"),
        ),
        (dir.join("hosts.csv"), fixture!("csv/hosts.csv")),
    ];
    for (path, bytes) in files {
        std::fs::write(path, bytes)?;
    }
    let text = |path: std::path::PathBuf| path.display().to_string();
    Ok(json!({
        "dir": text(dir.clone()),
        "mobaxterm": text(dir.join("sessions.mxtsessions")),
        "putty": text(dir.join("sessions.reg")),
        "remmina": text(remmina),
        "csv": text(dir.join("hosts.csv")),
    }))
}

/// What a bundle holds besides hosts: `{snippets, profiles, themes, keychain}`.
#[must_use]
pub fn bundle_contents(path: &str, home: &Path) -> Json {
    let file = opensesh_core::paths::expand_tilde(path.trim(), home);
    match bundle::load(&file) {
        Ok((contents, _)) => {
            let count = |folder: &str| {
                contents
                    .files
                    .iter()
                    .filter(|(path, _)| path.starts_with(&format!("{folder}/")))
                    .count()
            };
            json!({
                "snippets": contents.snippets.snippets.len(),
                "profiles": count("profiles"),
                "themes": count("themes"),
                "keychain": contents.keychain.is_some(),
                "runs": profile_shells(&contents.files),
            })
        }
        Err(_) => json!({}),
    }
}

/// Where `source` usually keeps its sessions here: a file, a folder, [`REGISTRY`], or empty.
#[must_use]
pub fn default_path(source: &str, home: &Path) -> String {
    let env_dir = |name: &str| {
        std::env::var_os(name)
            .map(std::path::PathBuf::from)
            .filter(|dir| dir.is_absolute())
    };
    match source {
        "mobaxterm" if cfg!(windows) => env_dir("APPDATA")
            .map(|appdata| appdata.join("MobaXterm").join("MobaXterm.ini"))
            .filter(|path| path.is_file())
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        "putty" if cfg!(windows) => REGISTRY.to_owned(),
        "putty" => putty::sessions_dir(home, env_dir("XDG_CONFIG_HOME").as_deref())
            .display()
            .to_string(),
        "remmina" => remmina::profiles_dir(home, env_dir("XDG_DATA_HOME").as_deref())
            .display()
            .to_string(),
        _ => String::new(),
    }
}

/// A CSV file's first rows and the guessed fields, for the column mapping:
/// `{delimiter, rows, fields, error}`.
#[must_use]
pub fn csv_table(path: &str, home: &Path) -> Json {
    let file = opensesh_core::paths::expand_tilde(path.trim(), home);
    match csv::load(&file) {
        Ok(table) => {
            let width = table.columns();
            let rows: Vec<Vec<String>> = table
                .rows
                .iter()
                .take(6)
                .map(|row| {
                    (0..width)
                        .map(|at| row.get(at).cloned().unwrap_or_default())
                        .collect()
                })
                .collect();
            let mut fields: Vec<&str> = table
                .rows
                .first()
                .map(|headers| csv::guess_columns(headers))
                .unwrap_or_default()
                .into_iter()
                .map(Field::code)
                .collect();
            fields.resize(width, Field::Ignore.code());
            json!({
                "delimiter": table.delimiter.to_string(),
                "rows": rows,
                "count": table.rows.len(),
                "fields": fields,
                "error": "",
            })
        }
        Err(error) => json!({ "rows": [], "fields": [], "count": 0, "error": error.to_string() }),
    }
}

/// The programs an import would run on this computer, so the user sees them before importing a
/// file from someone else: `[{name, kind, command}]`, `kind` `proxy` (a host's or group's
/// ProxyCommand, run to connect) or `shell` (the shell of a local terminal).
#[must_use]
pub fn runs_here(imported: &Imported) -> Vec<Json> {
    let shell = |table: &toml::Table| {
        table
            .get("shell")
            .and_then(toml::Value::as_str)
            .map(str::trim)
            .filter(|shell| !shell.is_empty())
            .map(str::to_owned)
    };
    let mut out = Vec::new();
    let mut add = |name: &str, kind: &str, command: Option<String>| {
        if let Some(command) = command.filter(|command| !command.trim().is_empty()) {
            out.push(json!({ "name": name, "kind": kind, "command": command }));
        }
    };
    for group in &imported.groups {
        add(
            &group.name,
            "proxy",
            group.defaults.ssh.proxy_command.clone(),
        );
        add(&group.name, "shell", shell(&group.defaults.terminal));
    }
    for host in &imported.hosts {
        add(&host.name, "proxy", host.ssh.proxy_command.clone());
        add(&host.name, "shell", shell(&host.terminal));
    }
    out
}

/// The shells a bundle's profiles start in local terminals: `[{name, kind: "shell", command}]`.
fn profile_shells(files: &[(String, String)]) -> Vec<Json> {
    files
        .iter()
        .filter(|(path, _)| path.starts_with("profiles/"))
        .filter_map(|(path, text)| {
            let table: toml::Table = text.parse().ok()?;
            let shell = table
                .get("terminal")
                .and_then(|terminal| terminal.get("shell"))
                .or_else(|| table.get("shell"))
                .and_then(toml::Value::as_str)
                .map(str::trim)
                .filter(|shell| !shell.is_empty())?;
            let name = table
                .get("name")
                .and_then(toml::Value::as_str)
                .unwrap_or(path.as_str());
            Some(json!({ "name": name, "kind": "shell", "command": shell }))
        })
        .collect()
}

/// The hosts and warnings to show before importing.
#[must_use]
pub fn preview(imported: &Imported, library: &HostsFile, target: impl Fn(&Host) -> String) -> Json {
    let paths = folder_paths(&imported.groups);
    let hosts: Vec<Json> = imported
        .hosts
        .iter()
        .map(|host| {
            json!({
                "name": host.name,
                "protocol": host.protocol.as_str(),
                "target": target(host),
                "folder": host.group.as_ref().and_then(|id| paths.get(id)).cloned().unwrap_or_default(),
                "saved": is_saved(library, host),
            })
        })
        .collect();
    // The file's name, not its whole path: the user just chose it.
    let warnings: Vec<String> = imported
        .warnings
        .iter()
        .map(|warning| {
            let file = warning
                .file
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            match (file.is_empty(), warning.line) {
                (true, _) => warning.message.clone(),
                (false, 0) => format!("{file}: {}", warning.message),
                (false, line) => format!("{file}:{line}: {}", warning.message),
            }
        })
        .collect();
    json!({
        "hosts": hosts,
        "groups": imported.groups.len(),
        "warnings": warnings,
        "runs": runs_here(imported),
        "error": "",
    })
}

/// Each group's folder path (`Prod / Web`).
fn folder_paths(groups: &[Group]) -> HashMap<String, String> {
    let mut paths: HashMap<String, String> = HashMap::new();
    for group in groups {
        let path = match group.parent.as_ref().and_then(|parent| paths.get(parent)) {
            Some(parent) => format!("{parent} / {}", group.name),
            None => group.name.clone(),
        };
        paths.insert(group.id.clone(), path);
    }
    paths
}

/// Whether the library has this host already: same name, protocol and address.
fn is_saved(library: &HostsFile, host: &Host) -> bool {
    library.hosts.iter().any(|saved| {
        !saved.is_linked()
            && saved.name == host.name
            && saved.protocol == host.protocol
            && saved.address.eq_ignore_ascii_case(&host.address)
    })
}

/// What [`apply`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Applied {
    /// Hosts added.
    pub added: usize,
    /// Hosts left out: saved already, or not valid.
    pub skipped: usize,
    /// The new group's id (empty when nothing was added).
    pub group: String,
}

/// Adds `imported` to `file` in a new group named `group_name`: its folders become groups
/// inside it. Hosts saved already, and hosts that aren't valid, are skipped.
pub fn apply(file: &mut HostsFile, imported: Imported, group_name: &str) -> Applied {
    let root = Group {
        id: new_id(),
        name: group_name.trim().to_owned(),
        ..Group::default()
    };
    let mut fresh = Vec::new();
    let mut skipped = 0;
    for mut host in imported.hosts {
        if host.id.is_empty() {
            host.id = new_id();
        }
        if host.group.is_none() {
            host.group = Some(root.id.clone());
        }
        // Checked against the file with the new groups in it, so their ids are known.
        if is_saved(file, &host) {
            skipped += 1;
            continue;
        }
        fresh.push(host);
    }
    let mut groups = imported.groups;
    for group in &mut groups {
        if group.parent.is_none() {
            group.parent = Some(root.id.clone());
        }
    }
    let mut check = file.clone();
    check.groups.push(root.clone());
    check.groups.extend(groups.iter().cloned());
    let (valid, invalid): (Vec<Host>, Vec<Host>) = fresh
        .into_iter()
        .partition(|host| check.validate_host(host).is_empty());
    skipped += invalid.len();
    if valid.is_empty() {
        return Applied {
            skipped,
            ..Applied::default()
        };
    }
    let group = root.id.clone();
    file.groups.push(root);
    file.groups.extend(groups);
    let at = file
        .hosts
        .iter()
        .position(Host::is_linked)
        .unwrap_or(file.hosts.len());
    let added = valid.len();
    file.hosts.splice(at..at, valid);
    Applied {
        added,
        skipped,
        group,
    }
}

/// Writes the hosts `ids` (every saved SSH host when empty) to `path` as an OpenSSH config
/// file: `{count, warnings}`, or `{error}`.
#[must_use]
pub fn export_ssh_config(file: &HostsFile, ids: &[String], path: &Path) -> Json {
    let hosts: Vec<&Host> = file
        .hosts
        .iter()
        .filter(|host| ids.is_empty() || ids.contains(&host.id))
        .collect();
    let exported = export::ssh_config(file, &hosts);
    match opensesh_core::fsutil::atomic_write(path, exported.text.as_bytes(), 0) {
        Ok(_) => json!({ "count": exported.count, "warnings": exported.warnings }),
        Err(error) => json!({ "error": error.to_string() }),
    }
}

#[cfg(test)]
mod tests {
    use opensesh_core::hosts::Protocol;

    use super::*;

    fn host(name: &str, address: &str) -> Host {
        Host {
            name: name.to_owned(),
            address: address.to_owned(),
            ..Host::default()
        }
    }

    #[test]
    fn imports_go_into_a_new_group() {
        let mut file = HostsFile::default();
        file.hosts.push(Host {
            id: new_id(),
            ..host("old", "old.lan")
        });
        let mut imported = Imported::default();
        let web = imported.group(&["Web".to_owned()]);
        imported.hosts.push(host("old", "old.lan"));
        imported.hosts.push(Host {
            group: web.clone(),
            ..host("new", "new.lan")
        });
        imported.hosts.push(host("broken", "bad host name"));
        imported.hosts.push(Host {
            protocol: Protocol::Rdp,
            ..host("old", "old.lan")
        });
        let applied = apply(&mut file, imported, "From MobaXterm");
        assert_eq!((applied.added, applied.skipped), (2, 2));
        let root = file.group(&applied.group).unwrap();
        assert_eq!(root.name, "From MobaXterm");
        let web_group = file.groups.iter().find(|g| g.name == "Web").unwrap();
        assert_eq!(web_group.parent.as_deref(), Some(applied.group.as_str()));
        let new = file.hosts.iter().find(|h| h.name == "new").unwrap();
        assert_eq!(new.group.as_deref(), Some(web_group.id.as_str()));
        assert!(!new.id.is_empty());
        let rdp = file
            .hosts
            .iter()
            .find(|h| h.protocol == Protocol::Rdp)
            .unwrap();
        assert_eq!(rdp.group.as_deref(), Some(applied.group.as_str()));

        // Nothing new: no group is made.
        let mut again = Imported::default();
        again.hosts.push(host("new", "new.lan"));
        let groups = file.groups.len();
        let applied = apply(&mut file, again, "Again");
        assert_eq!((applied.added, applied.skipped), (0, 1));
        assert_eq!(file.groups.len(), groups);
    }

    #[test]
    fn what_would_run_here_is_listed() {
        let mut imported = Imported::default();
        let mut proxied = host("p", "p.lan");
        proxied.ssh.proxy_command = Some("nc proxy %h %p".to_owned());
        let mut local = Host {
            protocol: Protocol::Local,
            ..host("l", "")
        };
        local.terminal.insert(
            "shell".to_owned(),
            toml::Value::String("evil.sh".to_owned()),
        );
        imported.hosts = vec![proxied, local, host("plain", "x.lan")];
        let runs = runs_here(&imported);
        assert_eq!(
            runs,
            [
                json!({ "name": "p", "kind": "proxy", "command": "nc proxy %h %p" }),
                json!({ "name": "l", "kind": "shell", "command": "evil.sh" }),
            ]
        );
        let shells = profile_shells(&[
            (
                "profiles/x.toml".to_owned(),
                "name = \"X\"\n[terminal]\nshell = \"zsh -l\"\n".to_owned(),
            ),
            ("profiles/y.toml".to_owned(), "name = \"Y\"\n".to_owned()),
            ("themes/t.toml".to_owned(), "shell = \"no\"\n".to_owned()),
        ]);
        assert_eq!(
            shells,
            [json!({ "name": "X", "kind": "shell", "command": "zsh -l" })]
        );
    }

    #[test]
    fn previews_name_the_folders() {
        let mut imported = Imported::default();
        let web = imported.group(&["Prod".to_owned(), "Web".to_owned()]);
        imported.hosts.push(Host {
            group: web,
            ..host("a", "a.lan")
        });
        let shown = preview(&imported, &HostsFile::default(), |h| h.address.clone());
        assert_eq!(shown["hosts"][0]["folder"], "Prod / Web");
        assert_eq!(shown["hosts"][0]["target"], "a.lan");
        assert_eq!(shown["hosts"][0]["saved"], false);
        assert_eq!(shown["groups"], 2);
    }
}
