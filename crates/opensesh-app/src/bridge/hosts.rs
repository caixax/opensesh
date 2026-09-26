//! `Hosts` QML singleton (Sprint 5): the saved hosts and groups of `hosts.toml`, the hosts linked
//! from `~/.ssh/config`, fuzzy search, editing, the recent connections, quick-connect parsing
//! and the OpenSSH command of each host.
//!
//! Lists go to QML as JSON text. Each host's summary JSON is built once per change, so a
//! search is the match plus a join. Files are read at startup and again when they change on
//! disk (after our own saves settle); saves go through the background writer. A `hosts.toml`
//! that can't be parsed, or that a newer OpenSesh wrote, is shown but never overwritten. Test
//! runs read the files but never write them, and can show a generated list instead.
//!
//! Field problems from `validateHost` and `validateGroup` are codes (`required`, `invalid`,
//! `unknown`) that QML words.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt string type from cxx-qt-lib.
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qstringlist.h");
        /// Qt string list type from cxx-qt-lib.
        type QStringList = cxx_qt_lib::QStringList;
    }

    extern "RustQt" {
        /// Saved hosts, groups, search and quick connect.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(i32, revision, READ, NOTIFY = changed)]
        #[qproperty(i32, count, READ, NOTIFY = changed)]
        #[qproperty(i32, favorite_count, cxx_name = "favoriteCount", READ, NOTIFY = changed)]
        #[qproperty(i32, recent_count, cxx_name = "recentCount", READ, NOTIFY = changed)]
        #[qproperty(i32, ungrouped_count, cxx_name = "ungroupedCount", READ, NOTIFY = changed)]
        #[qproperty(i32, linked_count, cxx_name = "linkedCount", READ, NOTIFY = changed)]
        #[qproperty(QString, groups, READ, NOTIFY = changed)]
        #[qproperty(QString, tags, READ, NOTIFY = changed)]
        #[qproperty(QString, sources, READ, NOTIFY = changed)]
        #[qproperty(QString, recent_targets, cxx_name = "recentTargets", READ, NOTIFY = changed)]
        #[qproperty(QString, problems, READ, NOTIFY = changed)]
        #[qproperty(bool, read_only, cxx_name = "readOnly", READ, NOTIFY = changed)]
        #[qproperty(QString, file_path, cxx_name = "filePath", READ, NOTIFY = changed)]
        type Hosts = super::HostsRust;

        /// Anything listed changed.
        #[qsignal]
        fn changed(self: Pin<&mut Self>);

        /// A save failed, or a change was refused (`read-only`): kind and technical detail.
        #[qsignal]
        fn problem(self: Pin<&mut Self>, kind: QString, detail: QString);

        /// Hosts matching `text` in `scope` (`all`, `favorites`, `recent`, `ungrouped`,
        /// `linked` or a group id), optionally one `protocol` and one `tag`, sorted by `sort` (`name`,
        /// `address`, `recent`, `group`) when there is no text: a JSON list of summaries
        /// (`id`, `name`, `protocol`, `address`, `target`, `tags`, `favorite`, `color`, `icon`,
        /// `group`, `groupPath`, `linked`, `sprint`).
        #[qinvokable]
        fn search(
            self: Pin<&mut Self>,
            text: &QString,
            scope: &QString,
            protocol: &QString,
            tag: &QString,
            sort: &QString,
        ) -> QString;

        /// Host `id` for the editor (every field as saved, `linked`, and `inherited`: what its
        /// group gives each field, see `inherited`); empty when unknown.
        #[qinvokable]
        #[cxx_name = "hostJson"]
        fn host_json(self: &Self, id: &QString) -> QString;

        /// What a host of `protocol` in group `group` inherits: `{key: {value, origin
        /// ("default" or "group"), group, groupName}}` for each inherited key.
        #[qinvokable]
        fn inherited(self: &Self, group: &QString, protocol: &QString) -> QString;

        /// The id of the host with id or name `text` (ignoring case), empty when none.
        #[qinvokable]
        #[cxx_name = "findHost"]
        fn find_host(self: &Self, text: &QString) -> QString;

        /// Group `id` for the editor, with `path` and `inherited` (from its parents).
        #[qinvokable]
        #[cxx_name = "groupJson"]
        fn group_json(self: &Self, id: &QString) -> QString;

        /// Field problems of a host (JSON as from `hostJson`): `{field: code}`.
        #[qinvokable]
        #[cxx_name = "validateHost"]
        fn validate_host(self: &Self, host: &QString) -> QString;

        /// Field problems of a group: `{field: code}`.
        #[qinvokable]
        #[cxx_name = "validateGroup"]
        fn validate_group(self: &Self, group: &QString) -> QString;

        /// Saves a host (a new one when its id is empty). Returns its id, empty when it isn't
        /// valid or can't be saved.
        #[qinvokable]
        #[cxx_name = "saveHost"]
        fn save_host(self: Pin<&mut Self>, host: &QString) -> QString;

        /// Saves a group (a new one when its id is empty). Returns its id, empty on failure.
        #[qinvokable]
        #[cxx_name = "saveGroup"]
        fn save_group(self: Pin<&mut Self>, group: &QString) -> QString;

        /// Deletes hosts (a JSON list of ids; linked ones are skipped). Returns how many.
        #[qinvokable]
        #[cxx_name = "deleteHosts"]
        fn delete_hosts(self: Pin<&mut Self>, ids: &QString) -> i32;

        /// Deletes a group; its hosts and subgroups move to its parent.
        #[qinvokable]
        #[cxx_name = "deleteGroup"]
        fn delete_group(self: Pin<&mut Self>, id: &QString) -> bool;

        /// Moves hosts (a JSON list of ids) into `group` (empty: no group). Returns how many.
        #[qinvokable]
        #[cxx_name = "moveHosts"]
        fn move_hosts(self: Pin<&mut Self>, ids: &QString, group: &QString) -> i32;

        /// Moves a group into `parent` (empty: the top). Refused into itself or a subgroup.
        #[qinvokable]
        #[cxx_name = "moveGroup"]
        fn move_group(self: Pin<&mut Self>, id: &QString, parent: &QString) -> bool;

        /// A copy of host `id` (a linked host becomes an editable one). Returns the new id.
        #[qinvokable]
        #[cxx_name = "duplicateHost"]
        fn duplicate_host(self: Pin<&mut Self>, id: &QString) -> QString;

        /// Marks hosts (a JSON list of ids) as favorites or not.
        #[qinvokable]
        #[cxx_name = "setFavorite"]
        fn set_favorite(self: Pin<&mut Self>, ids: &QString, favorite: bool) -> i32;

        /// The `ssh` command line of host `id` (to copy); empty for other protocols.
        #[qinvokable]
        #[cxx_name = "sshCommand"]
        fn ssh_command(self: &Self, id: &QString) -> QString;

        /// The program and arguments that connect to host `id` (OpenSSH for SSH hosts); empty
        /// when the protocol can't connect yet or the host can't be used on a command line.
        #[qinvokable]
        #[cxx_name = "connectCommand"]
        fn connect_command(self: &Self, id: &QString) -> QStringList;

        /// Quick-connect text parsed: `{ok, error, protocol, user, host, port, jump, text
        /// (canonical), sprint (0 when it connects today)}`.
        #[qinvokable]
        #[cxx_name = "parseTarget"]
        fn parse_target(self: &Self, text: &QString) -> QString;

        /// The program and arguments for quick-connect text (SSH only for now).
        #[qinvokable]
        #[cxx_name = "targetCommand"]
        fn target_command(self: &Self, text: &QString) -> QStringList;

        /// Suggestions for quick connect: saved hosts that match, then recent targets.
        #[qinvokable]
        fn suggest(self: Pin<&mut Self>, text: &QString) -> QString;

        /// Records a connection to saved host `id` (Recent).
        #[qinvokable]
        #[cxx_name = "recordHost"]
        fn record_host(self: Pin<&mut Self>, id: &QString);

        /// Records a quick-connect target (its canonical text).
        #[qinvokable]
        #[cxx_name = "recordTarget"]
        fn record_target(self: Pin<&mut Self>, text: &QString);

        /// The usual `~/.ssh/config` path.
        #[qinvokable]
        #[cxx_name = "defaultSshConfig"]
        fn default_ssh_config(self: &Self) -> QString;

        /// What importing `path` would bring: `{path, hosts: [{alias, target, jump}], warnings,
        /// linked}`.
        #[qinvokable]
        #[cxx_name = "previewSshConfig"]
        fn preview_ssh_config(self: &Self, path: &QString) -> QString;

        /// Imports `path`: `mode` `link` follows it read-only, `copy` adds its hosts to a new
        /// group called `group_name`. Returns `{added, skipped, group}`, empty on failure.
        #[qinvokable]
        #[cxx_name = "importSshConfig"]
        fn import_ssh_config(
            self: Pin<&mut Self>,
            path: &QString,
            mode: &QString,
            group_name: &QString,
        ) -> QString;

        /// Stops following a linked file.
        #[qinvokable]
        #[cxx_name = "unlinkSource"]
        fn unlink_source(self: Pin<&mut Self>, path: &QString) -> bool;

        /// Test runs only: shows `count` generated hosts instead of the saved ones.
        #[qinvokable]
        #[cxx_name = "loadFixture"]
        fn load_fixture(self: Pin<&mut Self>, count: i32);
    }

    impl cxx_qt::Initialize for Hosts {}
    impl cxx_qt::Threading for Hosts {}
}

use core::pin::Pin;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};
use opensesh_core::fsutil;
use opensesh_core::hosts::recent::{RECENT_FILE, RecentList};
use opensesh_core::hosts::search::{self, Query, Scope, Searcher, Sort, natural_cmp, sample_hosts};
use opensesh_core::hosts::target::{self, SshArgs};
use opensesh_core::hosts::{
    Group, HOSTS_FILE, Host, HostsFile, Origin, Protocol, SOURCE_SSH_CONFIG, Source, new_id,
};
use opensesh_core::watch::FileWatcher;
use opensesh_import::ssh_config::{self, SshConfig};
use serde_json::{Value as Json, json};

use crate::bridge::app_info::is_test_run;
use crate::hosts as library;
use crate::saves::SaveTracker;
use crate::services;

/// Rust state behind `Hosts`.
#[derive(Default)]
pub struct HostsRust {
    revision: i32,
    count: i32,
    favorite_count: i32,
    recent_count: i32,
    ungrouped_count: i32,
    linked_count: i32,
    groups: QString,
    tags: QString,
    sources: QString,
    recent_targets: QString,
    problems: QString,
    read_only: bool,
    file_path: QString,
    config_dir: Option<PathBuf>,
    data_dir: Option<PathBuf>,
    home: PathBuf,
    recent: RecentList,
    searcher: Searcher,
    /// Summary JSON of each host, in the library's order.
    summaries: Vec<String>,
    saves: SaveTracker,
    watchers: Vec<FileWatcher>,
    watched: Vec<PathBuf>,
    /// The file couldn't be parsed or is newer: never saved over.
    locked: bool,
    file_problems: Vec<String>,
    linked_problems: Vec<String>,
}

/// What was read from disk.
struct Disk {
    file: HostsFile,
    locked: bool,
    problems: Vec<String>,
    linked: SshConfig,
}

fn load_disk(config_dir: &Path, home: &Path) -> Disk {
    let path = config_dir.join(HOSTS_FILE);
    let (mut file, locked, problems) = match HostsFile::load(&path) {
        Ok((file, warnings)) => {
            let locked = file.read_only;
            (
                file,
                locked,
                warnings.iter().map(ToString::to_string).collect(),
            )
        }
        Err(error) => (HostsFile::default(), true, vec![error.to_string()]),
    };
    let linked = ssh_config::load_sources(&file.sources, home);
    file.hosts.extend(linked.to_hosts(true, None));
    Disk {
        file,
        locked,
        problems,
        linked,
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

fn count(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

fn ids_from(json: &QString) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(&json.to_string()).unwrap_or_default()
}

/// `user@host:port` of a host, with what it inherits.
fn target_text(file: &HostsFile, host: &Host) -> String {
    if !host.protocol.needs_address() {
        return String::new();
    }
    let resolved = file.resolve(host);
    let mut text = String::new();
    if host.protocol.is_network() {
        if let Some(user) = resolved.user() {
            text.push_str(user);
            text.push('@');
        }
    }
    text.push_str(&target::bracket_ipv6(&host.address));
    if let Some(port) = resolved
        .port()
        .filter(|port| host.protocol.is_network() && Some(*port) != host.protocol.default_port())
    {
        text.push(':');
        text.push_str(&port.to_string());
    }
    text
}

fn summary(file: &HostsFile, host: &Host) -> String {
    json!({
        "id": host.id,
        "name": host.name,
        "protocol": host.protocol.as_str(),
        "address": host.address,
        "target": target_text(file, host),
        "tags": host.tags,
        "favorite": host.favorite,
        "color": if host.color.is_empty() {
            file.group_chain(host.group.as_deref())
                .iter()
                .find(|group| !group.color.is_empty())
                .map(|group| group.color.clone())
                .unwrap_or_default()
        } else {
            host.color.clone()
        },
        "icon": host.icon,
        "group": host.group.clone().unwrap_or_default(),
        "groupPath": host.group.as_deref().map(|id| file.group_path(id)).unwrap_or_default(),
        "linked": host.is_linked(),
        "sprint": host.protocol.available_in().unwrap_or(0),
    })
    .to_string()
}

/// The groups as a tree, depth first with children by name.
fn groups_json(file: &HostsFile) -> Json {
    let counts = file.counts();
    let mut out = Vec::new();
    fn walk(
        file: &HostsFile,
        parent: Option<&str>,
        depth: usize,
        counts: &std::collections::BTreeMap<Option<String>, usize>,
        out: &mut Vec<Json>,
    ) {
        let mut children: Vec<&Group> = file
            .groups
            .iter()
            .filter(|group| group.parent.as_deref() == parent)
            .collect();
        children.sort_by(|a, b| natural_cmp(&a.name, &b.name));
        for group in children {
            let subtree = file.subtree(&group.id);
            let total: usize = subtree
                .iter()
                .map(|id| counts.get(&Some(id.clone())).copied().unwrap_or(0))
                .sum();
            let has_children = file
                .groups
                .iter()
                .any(|other| other.parent.as_deref() == Some(group.id.as_str()));
            out.push(json!({
                "id": group.id,
                "name": group.name,
                "parent": group.parent.clone().unwrap_or_default(),
                "color": group.color,
                "depth": depth,
                "path": file.group_path(&group.id),
                "count": counts.get(&Some(group.id.clone())).copied().unwrap_or(0),
                "total": total,
                "hasChildren": has_children,
            }));
            walk(file, Some(&group.id), depth + 1, counts, out);
        }
    }
    walk(file, None, 0, &counts, &mut out);
    Json::Array(out)
}

fn inherited_json(file: &HostsFile, group: Option<&str>, protocol: Protocol) -> Json {
    let fields = file.inherited(group, protocol);
    let mut out = serde_json::Map::new();
    for (key, field) in fields {
        let (origin, group_id, group_name) = match &field.origin {
            Origin::Default => ("default", String::new(), String::new()),
            Origin::Group(id) => (
                "group",
                id.clone(),
                file.group(id)
                    .map(|group| group.name.clone())
                    .unwrap_or_default(),
            ),
            Origin::Host => ("host", String::new(), String::new()),
        };
        out.insert(
            key.to_owned(),
            json!({
                "value": field.value.as_ref().and_then(|value| serde_json::to_value(value).ok()).unwrap_or(Json::Null),
                "origin": origin,
                "group": group_id,
                "groupName": group_name,
            }),
        );
    }
    Json::Object(out)
}

fn problems_json(problems: Vec<(&'static str, &'static str)>) -> QString {
    let map: serde_json::Map<String, Json> = problems
        .into_iter()
        .map(|(field, code)| (field.to_owned(), Json::String(code.to_owned())))
        .collect();
    QString::from(&Json::Object(map).to_string())
}

/// A host from the editor's JSON. Values the model can't hold (a port out of range) are
/// reported as field problems instead of failing.
fn host_from_json(text: &str) -> Result<Host, Vec<(&'static str, &'static str)>> {
    let mut value: Json = serde_json::from_str(text).map_err(|_| vec![("host", "invalid")])?;
    let mut problems = Vec::new();
    if let Some(object) = value.as_object_mut() {
        // Empty strings and nulls mean "not set" (inherited).
        object.retain(|_, field| !field.is_null() && field.as_str() != Some(""));
        for section in ["ssh", "sftp", "serial"] {
            if let Some(Json::Object(inner)) = object.get_mut(section) {
                inner.retain(|_, field| !field.is_null() && field.as_str() != Some(""));
            }
        }
        if let Some(port) = object.get("port") {
            if port
                .as_u64()
                .and_then(|port| u16::try_from(port).ok())
                .is_none_or(|port| port == 0)
            {
                problems.push(("port", "invalid"));
                object.remove("port");
            }
        }
        // "id" and "name" must stay strings even when empty.
        object.entry("id").or_insert(Json::String(String::new()));
        object.entry("name").or_insert(Json::String(String::new()));
    }
    let mut host: Host = serde_json::from_value(value).map_err(|_| vec![("host", "invalid")])?;
    host.name = host.name.trim().to_owned();
    host.address = host.address.trim().to_owned();
    host.user = host
        .user
        .map(|user| user.trim().to_owned())
        .filter(|user| !user.is_empty());
    host.tags = opensesh_core::hosts::clean_tags(&host.tags);
    if let Some(jump) = &mut host.jump {
        *jump = jump
            .iter()
            .map(|hop| hop.trim().to_owned())
            .filter(|hop| !hop.is_empty())
            .collect();
    }
    if problems.is_empty() {
        Ok(host)
    } else {
        Err(problems)
    }
}

fn group_from_json(text: &str) -> Result<Group, Vec<(&'static str, &'static str)>> {
    let mut value: Json = serde_json::from_str(text).map_err(|_| vec![("group", "invalid")])?;
    let mut problems = Vec::new();
    if let Some(object) = value.as_object_mut() {
        object.retain(|_, field| !field.is_null() && field.as_str() != Some(""));
        object.entry("id").or_insert(Json::String(String::new()));
        object.entry("name").or_insert(Json::String(String::new()));
        if let Some(Json::Object(defaults)) = object.get_mut("defaults") {
            defaults.retain(|_, field| !field.is_null() && field.as_str() != Some(""));
            for section in ["ssh", "sftp"] {
                if let Some(Json::Object(inner)) = defaults.get_mut(section) {
                    inner.retain(|_, field| !field.is_null() && field.as_str() != Some(""));
                }
            }
            if let Some(port) = defaults.get("port") {
                if port
                    .as_u64()
                    .and_then(|port| u16::try_from(port).ok())
                    .is_none_or(|port| port == 0)
                {
                    problems.push(("port", "invalid"));
                    defaults.remove("port");
                }
            }
        }
    }
    let mut group: Group = serde_json::from_value(value).map_err(|_| vec![("group", "invalid")])?;
    group.name = group.name.trim().to_owned();
    if problems.is_empty() {
        Ok(group)
    } else {
        Err(problems)
    }
}

impl qobject::Hosts {
    fn file(&self) -> std::sync::Arc<library::Library> {
        library::current()
    }

    /// Publishes `file` and refreshes everything QML reads.
    fn publish(mut self: Pin<&mut Self>, file: HostsFile) {
        let published = library::publish(file);
        let file = &published.file;
        let summaries: Vec<String> = file.hosts.iter().map(|host| summary(file, host)).collect();
        let tags: Vec<Json> = search::tags(file)
            .into_iter()
            .map(|(tag, count)| json!({ "tag": tag, "count": count }))
            .collect();
        let sources: Vec<Json> = file
            .sources
            .iter()
            .map(|source| json!({ "kind": source.kind, "path": source.path }))
            .collect();
        {
            let mut state = self.as_mut().rust_mut();
            state.summaries = summaries;
            state.count = count(file.hosts.len());
            state.favorite_count = count(file.hosts.iter().filter(|host| host.favorite).count());
            state.ungrouped_count = count(
                file.hosts
                    .iter()
                    .filter(|host| host.group.is_none() && !host.is_linked())
                    .count(),
            );
            state.linked_count = count(file.hosts.iter().filter(|host| host.is_linked()).count());
            state.groups = QString::from(&groups_json(file).to_string());
            state.tags = QString::from(&Json::Array(tags).to_string());
            state.sources = QString::from(&Json::Array(sources).to_string());
            state.revision = i32::try_from(published.revision).unwrap_or(i32::MAX);
        }
        self.as_mut().refresh_recent();
        self.as_mut().refresh_problems();
        self.changed();
    }

    fn refresh_recent(mut self: Pin<&mut Self>) {
        let library = self.file();
        let existing: HashSet<&str> = library
            .file
            .hosts
            .iter()
            .map(|host| host.id.as_str())
            .collect();
        let recent_hosts = self
            .recent
            .entries()
            .iter()
            .filter(|entry| existing.contains(entry.host.as_str()))
            .count();
        let targets: Vec<&str> = self
            .recent
            .entries()
            .iter()
            .filter(|entry| entry.host.is_empty())
            .map(|entry| entry.target.as_str())
            .collect();
        let text = QString::from(&json!(targets).to_string());
        let mut state = self.as_mut().rust_mut();
        state.recent_count = count(recent_hosts);
        state.recent_targets = text;
    }

    fn refresh_problems(mut self: Pin<&mut Self>) {
        let mut list = self.file_problems.clone();
        list.extend(self.linked_problems.iter().cloned());
        let locked = self.locked;
        let mut state = self.as_mut().rust_mut();
        state.problems = QString::from(&json!(list).to_string());
        state.read_only = locked;
    }

    /// Applies `change` to a copy of the hosts, publishes and saves it. Refused (with a
    /// `read-only` problem) while the file is locked.
    fn modify<R>(mut self: Pin<&mut Self>, change: impl FnOnce(&mut HostsFile) -> R) -> Option<R> {
        if self.locked {
            let path = self.file_path.clone();
            self.as_mut().problem(QString::from("read-only"), path);
            return None;
        }
        let mut file = self.file().file.clone();
        let result = change(&mut file);
        self.as_mut().publish(file);
        self.as_mut().save();
        Some(result)
    }

    /// Queues `hosts.toml` for writing (not in test runs).
    fn save(mut self: Pin<&mut Self>) {
        let (Some(dir), Some(services)) = (self.config_dir.clone(), services::get()) else {
            return;
        };
        if is_test_run() || self.locked {
            return;
        }
        let text = match self.file().file.to_toml_string() {
            Ok(text) => text,
            Err(error) => {
                tracing::warn!("could not write the hosts: {error}");
                return;
            }
        };
        let seq = self.as_mut().rust_mut().saves.queue();
        let qt_thread = self.qt_thread();
        services.writer.write(
            dir.join(HOSTS_FILE),
            text.into_bytes(),
            fsutil::DEFAULT_BACKUPS,
            Some(Box::new(move |path, result| {
                let failure = result
                    .err()
                    .map(|error| format!("{}: {error}", path.display()));
                let _ = qt_thread.queue(move |object| object.save_done(seq, failure));
            })),
        );
    }

    fn save_done(mut self: Pin<&mut Self>, seq: u64, failure: Option<String>) {
        self.as_mut().rust_mut().saves.finished(seq);
        if let Some(detail) = failure {
            tracing::warn!("could not save the hosts: {detail}");
            self.as_mut()
                .problem(QString::from("save-failed"), QString::from(&detail));
        }
        if self.as_mut().rust_mut().saves.take_reload() {
            self.reload_in_background();
        }
    }

    fn save_recent(self: Pin<&mut Self>) {
        let (Some(dir), Some(services)) = (self.data_dir.clone(), services::get()) else {
            return;
        };
        if is_test_run() {
            return;
        }
        services.writer.write(
            dir.join(RECENT_FILE),
            self.recent.to_toml_string().into_bytes(),
            0,
            None,
        );
    }

    fn reload_in_background(self: Pin<&mut Self>) {
        let Some(dir) = self.config_dir.clone() else {
            return;
        };
        let home = self.home.clone();
        let qt_thread = self.qt_thread();
        let spawned = std::thread::Builder::new()
            .name("opensesh-hosts".to_owned())
            .spawn(move || {
                let disk = load_disk(&dir, &home);
                let _ = qt_thread.queue(move |object| object.apply_disk(disk));
            });
        if let Err(error) = spawned {
            tracing::warn!("could not reload the hosts: {error}");
        }
    }

    fn apply_disk(mut self: Pin<&mut Self>, disk: Disk) {
        if self.saves.pending() {
            self.as_mut().rust_mut().saves.reload_wanted = true;
            return;
        }
        let changed = disk.file != self.file().file;
        let files = disk.linked.files.clone();
        {
            let mut state = self.as_mut().rust_mut();
            state.locked = disk.locked;
            state.file_problems = disk.problems;
            state.linked_problems = disk
                .linked
                .warnings
                .iter()
                .map(ToString::to_string)
                .collect();
        }
        if changed {
            tracing::info!("hosts changed on disk; applied");
            self.as_mut().publish(disk.file);
        } else {
            self.as_mut().refresh_problems();
            self.as_mut().changed();
        }
        self.watch(files);
    }

    /// Watches `hosts.toml` and the linked files (again when the set of files changed).
    fn watch(mut self: Pin<&mut Self>, linked: Vec<PathBuf>) {
        if is_test_run() {
            return;
        }
        let Some(dir) = self.config_dir.clone() else {
            return;
        };
        let mut wanted = vec![dir.join(HOSTS_FILE)];
        wanted.extend(linked);
        if wanted == self.watched {
            return;
        }
        let mut watchers = Vec::new();
        for path in &wanted {
            let qt_thread = self.qt_thread();
            match FileWatcher::spawn(path, Duration::from_millis(300), move || {
                let _ = qt_thread.queue(|object| object.reload_in_background());
            }) {
                Ok(watcher) => watchers.push(watcher),
                Err(error) => tracing::warn!("{} won't reload by itself: {error}", path.display()),
            }
        }
        let mut state = self.as_mut().rust_mut();
        state.watchers = watchers;
        state.watched = wanted;
    }

    /// See the bridge declaration.
    pub fn search(
        mut self: Pin<&mut Self>,
        text: &QString,
        scope: &QString,
        protocol: &QString,
        tag: &QString,
        sort: &QString,
    ) -> QString {
        let scope = match scope.to_string().as_str() {
            "" | "all" => Scope::All,
            "favorites" => Scope::Favorites,
            "recent" => Scope::Recent,
            "ungrouped" => Scope::Ungrouped,
            "linked" => Scope::Linked,
            id => Scope::Group(id.to_owned()),
        };
        let tag = tag.to_string();
        let query = Query {
            text: text.to_string(),
            scope,
            protocol: Protocol::parse(&protocol.to_string()),
            tag: (!tag.is_empty()).then_some(tag),
            sort: Sort::parse(&sort.to_string()),
        };
        let library = self.file();
        let recent = self.recent.clone();
        let found = self
            .as_mut()
            .rust_mut()
            .searcher
            .search(&library.file, &recent, &query);
        let summaries = &self.summaries;
        let mut out = String::with_capacity(found.len() * 200 + 2);
        out.push('[');
        for (n, index) in found.iter().enumerate() {
            if let Some(summary) = summaries.get(*index) {
                if n > 0 {
                    out.push(',');
                }
                out.push_str(summary);
            }
        }
        out.push(']');
        QString::from(&out)
    }

    /// See the bridge declaration.
    pub fn host_json(&self, id: &QString) -> QString {
        let library = self.file();
        let Some(host) = library.file.host(&id.to_string()) else {
            return QString::default();
        };
        let mut value = serde_json::to_value(host).unwrap_or(Json::Null);
        if let Some(object) = value.as_object_mut() {
            object.insert("linked".to_owned(), Json::Bool(host.is_linked()));
            object.insert(
                "protocol".to_owned(),
                Json::String(host.protocol.as_str().to_owned()),
            );
            object.insert(
                "inherited".to_owned(),
                inherited_json(&library.file, host.group.as_deref(), host.protocol),
            );
        }
        QString::from(&value.to_string())
    }

    /// See the bridge declaration.
    pub fn inherited(&self, group: &QString, protocol: &QString) -> QString {
        let group = group.to_string();
        let protocol = Protocol::parse(&protocol.to_string()).unwrap_or_default();
        let library = self.file();
        QString::from(
            &inherited_json(
                &library.file,
                (!group.is_empty()).then_some(group.as_str()),
                protocol,
            )
            .to_string(),
        )
    }

    /// See the bridge declaration.
    pub fn find_host(&self, text: &QString) -> QString {
        self.file()
            .file
            .find_host(&text.to_string())
            .map(|host| QString::from(&host.id))
            .unwrap_or_default()
    }

    /// See the bridge declaration.
    pub fn group_json(&self, id: &QString) -> QString {
        let library = self.file();
        let Some(group) = library.file.group(&id.to_string()) else {
            return QString::default();
        };
        let mut value = serde_json::to_value(group).unwrap_or(Json::Null);
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "path".to_owned(),
                Json::String(library.file.group_path(&group.id)),
            );
            object.insert(
                "inherited".to_owned(),
                inherited_json(&library.file, group.parent.as_deref(), Protocol::Ssh),
            );
        }
        QString::from(&value.to_string())
    }

    /// See the bridge declaration.
    pub fn validate_host(&self, host: &QString) -> QString {
        let library = self.file();
        match host_from_json(&host.to_string()) {
            Ok(host) => problems_json(library.file.validate_host(&host)),
            Err(problems) => problems_json(problems),
        }
    }

    /// See the bridge declaration.
    pub fn validate_group(&self, group: &QString) -> QString {
        let library = self.file();
        match group_from_json(&group.to_string()) {
            Ok(group) => problems_json(library.file.validate_group(&group)),
            Err(problems) => problems_json(problems),
        }
    }

    /// See the bridge declaration.
    pub fn save_host(self: Pin<&mut Self>, host: &QString) -> QString {
        let Ok(mut host) = host_from_json(&host.to_string()) else {
            return QString::default();
        };
        let library = self.file();
        if !library.file.validate_host(&host).is_empty() {
            return QString::default();
        }
        if host.id.is_empty() {
            host.id = new_id();
        } else if host.is_linked() || library.file.host(&host.id).is_some_and(Host::is_linked) {
            return QString::default();
        }
        let id = host.id.clone();
        let saved = self.modify(move |file| {
            match file
                .hosts
                .iter_mut()
                .find(|existing| existing.id == host.id)
            {
                Some(existing) => {
                    // Keys this version doesn't know stay.
                    let extra = std::mem::take(&mut existing.extra);
                    *existing = host;
                    if existing.extra.is_empty() {
                        existing.extra = extra;
                    }
                }
                None => {
                    // New hosts go before the linked ones.
                    let at = file
                        .hosts
                        .iter()
                        .position(Host::is_linked)
                        .unwrap_or(file.hosts.len());
                    file.hosts.insert(at, host);
                }
            }
        });
        if saved.is_some() {
            QString::from(&id)
        } else {
            QString::default()
        }
    }

    /// See the bridge declaration.
    pub fn save_group(self: Pin<&mut Self>, group: &QString) -> QString {
        let Ok(mut group) = group_from_json(&group.to_string()) else {
            return QString::default();
        };
        let library = self.file();
        if !library.file.validate_group(&group).is_empty() {
            return QString::default();
        }
        if group.id.is_empty() {
            group.id = new_id();
        }
        let id = group.id.clone();
        let saved = self.modify(move |file| {
            match file
                .groups
                .iter_mut()
                .find(|existing| existing.id == group.id)
            {
                Some(existing) => {
                    let extra = std::mem::take(&mut existing.extra);
                    *existing = group;
                    if existing.extra.is_empty() {
                        existing.extra = extra;
                    }
                }
                None => file.groups.push(group),
            }
        });
        if saved.is_some() {
            QString::from(&id)
        } else {
            QString::default()
        }
    }

    /// See the bridge declaration.
    pub fn delete_hosts(mut self: Pin<&mut Self>, ids: &QString) -> i32 {
        let ids: HashSet<String> = ids_from(ids).into_iter().collect();
        let removed = self
            .as_mut()
            .modify(|file| {
                let before = file.hosts.len();
                file.hosts
                    .retain(|host| host.is_linked() || !ids.contains(&host.id));
                before - file.hosts.len()
            })
            .unwrap_or(0);
        if removed > 0 {
            let mut changed = false;
            for id in &ids {
                changed |= self.as_mut().rust_mut().recent.remove_host(id);
            }
            if changed {
                self.as_mut().save_recent();
                self.as_mut().refresh_recent();
            }
        }
        count(removed)
    }

    /// See the bridge declaration.
    pub fn delete_group(self: Pin<&mut Self>, id: &QString) -> bool {
        let id = id.to_string();
        self.modify(|file| {
            let Some(index) = file.groups.iter().position(|group| group.id == id) else {
                return false;
            };
            let parent = file.groups[index].parent.clone();
            file.groups.remove(index);
            for group in &mut file.groups {
                if group.parent.as_deref() == Some(id.as_str()) {
                    group.parent.clone_from(&parent);
                }
            }
            for host in &mut file.hosts {
                if host.group.as_deref() == Some(id.as_str()) {
                    host.group.clone_from(&parent);
                }
            }
            true
        })
        .unwrap_or(false)
    }

    /// See the bridge declaration.
    pub fn move_hosts(self: Pin<&mut Self>, ids: &QString, group: &QString) -> i32 {
        let ids: HashSet<String> = ids_from(ids).into_iter().collect();
        let group = group.to_string();
        let library = self.file();
        if !group.is_empty() && library.file.group(&group).is_none() {
            return 0;
        }
        let target = (!group.is_empty()).then_some(group);
        count(
            self.modify(|file| {
                let mut moved = 0;
                for host in &mut file.hosts {
                    if ids.contains(&host.id) && !host.is_linked() && host.group != target {
                        host.group.clone_from(&target);
                        moved += 1;
                    }
                }
                moved
            })
            .unwrap_or(0),
        )
    }

    /// See the bridge declaration.
    pub fn move_group(self: Pin<&mut Self>, id: &QString, parent: &QString) -> bool {
        let (id, parent) = (id.to_string(), parent.to_string());
        let library = self.file();
        let Some(mut group) = library.file.group(&id).cloned() else {
            return false;
        };
        group.parent = (!parent.is_empty()).then_some(parent);
        if !library.file.validate_group(&group).is_empty() {
            return false;
        }
        self.modify(|file| {
            if let Some(existing) = file
                .groups
                .iter_mut()
                .find(|existing| existing.id == group.id)
            {
                existing.parent = group.parent;
            }
        })
        .is_some()
    }

    /// See the bridge declaration.
    pub fn duplicate_host(self: Pin<&mut Self>, id: &QString) -> QString {
        let library = self.file();
        let Some(source) = library.file.host(&id.to_string()) else {
            return QString::default();
        };
        let mut copy = source.clone();
        copy.id = new_id();
        copy.favorite = false;
        let taken: HashSet<&str> = library
            .file
            .hosts
            .iter()
            .map(|host| host.name.as_str())
            .collect();
        let mut n = 2;
        let mut name = format!("{} ({n})", source.name);
        while taken.contains(name.as_str()) {
            n += 1;
            name = format!("{} ({n})", source.name);
        }
        copy.name = if source.is_linked() {
            source.name.clone()
        } else {
            name
        };
        let new = copy.id.clone();
        let after = source.id.clone();
        let saved = self.modify(move |file| {
            let at = file
                .hosts
                .iter()
                .position(|host| host.id == after && !host.is_linked())
                .map_or_else(
                    || {
                        file.hosts
                            .iter()
                            .position(Host::is_linked)
                            .unwrap_or(file.hosts.len())
                    },
                    |index| index + 1,
                );
            file.hosts.insert(at, copy);
        });
        if saved.is_some() {
            QString::from(&new)
        } else {
            QString::default()
        }
    }

    /// See the bridge declaration.
    pub fn set_favorite(self: Pin<&mut Self>, ids: &QString, favorite: bool) -> i32 {
        let ids: HashSet<String> = ids_from(ids).into_iter().collect();
        count(
            self.modify(|file| {
                let mut changed = 0;
                for host in &mut file.hosts {
                    if ids.contains(&host.id) && !host.is_linked() && host.favorite != favorite {
                        host.favorite = favorite;
                        changed += 1;
                    }
                }
                changed
            })
            .unwrap_or(0),
        )
    }

    fn ssh_args_of(&self, id: &str) -> Option<SshArgs> {
        let library = self.file();
        let host = library.file.host(id)?;
        if !matches!(host.protocol, Protocol::Ssh) {
            return None;
        }
        let args = library.file.ssh_args(host);
        args.check().ok().map(|()| args)
    }

    /// See the bridge declaration.
    pub fn ssh_command(&self, id: &QString) -> QString {
        self.ssh_args_of(&id.to_string())
            .map(|args| QString::from(&args.to_command_line()))
            .unwrap_or_default()
    }

    /// See the bridge declaration.
    pub fn connect_command(&self, id: &QString) -> QStringList {
        let mut list = QStringList::default();
        if let Some(args) = self.ssh_args_of(&id.to_string()) {
            list.append(QString::from("ssh"));
            for arg in args.to_args() {
                list.append(QString::from(&arg));
            }
        }
        list
    }

    /// See the bridge declaration.
    pub fn parse_target(&self, text: &QString) -> QString {
        let value = match target::parse(&text.to_string()) {
            Ok(target) => json!({
                "ok": true,
                "error": "",
                "protocol": target.protocol.as_str(),
                "user": target.user.clone().unwrap_or_default(),
                "host": target.host,
                "port": target.port.or(target.protocol.default_port()).map_or(Json::Null, Json::from),
                "jump": target.jump,
                "text": target.to_string(),
                "sprint": target.protocol.available_in().unwrap_or(0),
            }),
            Err(error) => json!({ "ok": false, "error": error.to_string() }),
        };
        QString::from(&value.to_string())
    }

    /// See the bridge declaration.
    pub fn target_command(&self, text: &QString) -> QStringList {
        let mut list = QStringList::default();
        let Ok(target) = target::parse(&text.to_string()) else {
            return list;
        };
        if target.protocol != Protocol::Ssh {
            return list;
        }
        let library = self.file();
        let mut args = target.ssh_args();
        // A jump that names a saved host goes to its address.
        args.jump = args
            .jump
            .iter()
            .map(|hop| library.file.jump_spec(hop))
            .collect();
        if args.check().is_err() {
            return list;
        }
        list.append(QString::from("ssh"));
        for arg in args.to_args() {
            list.append(QString::from(&arg));
        }
        list
    }

    /// See the bridge declaration.
    pub fn suggest(mut self: Pin<&mut Self>, text: &QString) -> QString {
        let typed = text.to_string();
        // `deploy@web-0:22` looks for hosts like `web-0`: the user and port aren't in a name.
        let wanted = target::parse(&typed).map_or_else(|_| typed.clone(), |target| target.host);
        let library = self.file();
        let recent = self.recent.clone();
        let found = self.as_mut().rust_mut().searcher.search(
            &library.file,
            &recent,
            &Query {
                text: wanted,
                sort: Sort::LastUsed,
                ..Query::default()
            },
        );
        let mut list: Vec<Json> = found
            .into_iter()
            .take(8)
            .filter_map(|index| library.file.hosts.get(index))
            .map(|host| {
                json!({
                    "kind": "host",
                    "id": host.id,
                    "name": host.name,
                    "target": target_text(&library.file, host),
                    "protocol": host.protocol.as_str(),
                })
            })
            .collect();
        let lower = typed.trim().to_lowercase();
        list.extend(
            recent
                .entries()
                .iter()
                .filter(|entry| entry.host.is_empty())
                .filter(|entry| lower.is_empty() || entry.target.to_lowercase().contains(&lower))
                .take(5)
                .map(|entry| json!({ "kind": "recent", "text": entry.target })),
        );
        QString::from(&Json::Array(list).to_string())
    }

    /// See the bridge declaration.
    pub fn record_host(mut self: Pin<&mut Self>, id: &QString) {
        let id = id.to_string();
        if self.file().file.host(&id).is_none() {
            return;
        }
        self.as_mut().rust_mut().recent.touch_host(&id, now());
        self.as_mut().save_recent();
        self.as_mut().refresh_recent();
        self.changed();
    }

    /// See the bridge declaration.
    pub fn record_target(mut self: Pin<&mut Self>, text: &QString) {
        let Ok(target) = target::parse(&text.to_string()) else {
            return;
        };
        self.as_mut()
            .rust_mut()
            .recent
            .touch_target(&target.to_string(), now());
        self.as_mut().save_recent();
        self.as_mut().refresh_recent();
        self.changed();
    }

    /// See the bridge declaration.
    pub fn default_ssh_config(&self) -> QString {
        QString::from(&self.home.join(".ssh").join("config").display().to_string())
    }

    fn config_path(&self, path: &QString) -> PathBuf {
        let path = path.to_string();
        if path.trim().is_empty() {
            self.home.join(".ssh").join("config")
        } else {
            opensesh_core::paths::expand_tilde(path.trim(), &self.home)
        }
    }

    /// `path` as written in `hosts.toml`: with `~` when it is in the home directory.
    fn source_text(&self, path: &Path) -> String {
        match path.strip_prefix(&self.home) {
            Ok(rest) if !self.home.as_os_str().is_empty() => {
                format!("~/{}", rest.to_string_lossy().replace('\\', "/"))
            }
            _ => path.display().to_string(),
        }
    }

    /// See the bridge declaration.
    pub fn preview_ssh_config(&self, path: &QString) -> QString {
        let path = self.config_path(path);
        // A small file the user asked to import.
        let config = ssh_config::load(&path, &self.home);
        let library = self.file();
        let source = self.source_text(&path);
        let linked = library
            .file
            .sources
            .iter()
            .any(|existing| existing.path == source);
        let preview = HostsFile {
            hosts: config.to_hosts(true, None),
            ..HostsFile::default()
        };
        let hosts: Vec<Json> = preview
            .hosts
            .iter()
            .map(|host| {
                json!({
                    "alias": host.name,
                    "target": target_text(&preview, host),
                    "jump": host.jump.clone().unwrap_or_default(),
                    "saved": library.file.hosts.iter().any(|saved| !saved.is_linked() && saved.name == host.name),
                })
            })
            .collect();
        let warnings: Vec<String> = config.warnings.iter().map(ToString::to_string).collect();
        QString::from(
            &json!({
                "path": path.display().to_string(),
                "source": source,
                "hosts": hosts,
                "warnings": warnings,
                "linked": linked,
            })
            .to_string(),
        )
    }

    /// See the bridge declaration.
    pub fn import_ssh_config(
        mut self: Pin<&mut Self>,
        path: &QString,
        mode: &QString,
        group_name: &QString,
    ) -> QString {
        let path = self.config_path(path);
        let source = self.source_text(&path);
        let mode = mode.to_string();
        if mode == "link" {
            let home = self.home.clone();
            let result = self.as_mut().modify(move |file| {
                if file.sources.iter().any(|existing| existing.path == source) {
                    return 0;
                }
                file.sources.push(Source {
                    kind: SOURCE_SSH_CONFIG.to_owned(),
                    path: source,
                    ..Source::default()
                });
                // The linked hosts are read again with the new source.
                file.hosts.retain(|host| !host.is_linked());
                let linked = ssh_config::load_sources(&file.sources, &home);
                let count = linked.hosts.len();
                file.hosts.extend(linked.to_hosts(true, None));
                count
            });
            let Some(added) = result else {
                return QString::default();
            };
            let files = ssh_config::load_sources(&self.file().file.sources, &self.home).files;
            self.as_mut().watch(files);
            return QString::from(
                &json!({ "added": added, "skipped": 0, "group": "" }).to_string(),
            );
        }
        // A copy, in a new group.
        let config = ssh_config::load(&path, &self.home);
        let name = group_name.to_string();
        let group = Group {
            id: new_id(),
            name: if name.trim().is_empty() {
                "~/.ssh/config".to_owned()
            } else {
                name.trim().to_owned()
            },
            ..Group::default()
        };
        let group_id = group.id.clone();
        let hosts = config.to_hosts(false, Some(&group_id));
        let result = self.modify(move |file| {
            let taken: HashSet<String> = file
                .hosts
                .iter()
                .filter(|host| !host.is_linked())
                .map(|host| host.name.clone())
                .collect();
            let (fresh, skipped): (Vec<Host>, Vec<Host>) = hosts
                .into_iter()
                .partition(|host| !taken.contains(&host.name));
            let added = fresh.len();
            if added > 0 {
                file.groups.push(group);
                let at = file
                    .hosts
                    .iter()
                    .position(Host::is_linked)
                    .unwrap_or(file.hosts.len());
                file.hosts.splice(at..at, fresh);
            }
            (added, skipped.len())
        });
        match result {
            Some((added, skipped)) => QString::from(
                &json!({
                    "added": added,
                    "skipped": skipped,
                    "group": if added > 0 { group_id } else { String::new() },
                })
                .to_string(),
            ),
            None => QString::default(),
        }
    }

    /// See the bridge declaration.
    pub fn unlink_source(mut self: Pin<&mut Self>, path: &QString) -> bool {
        let wanted = path.to_string();
        let home = self.home.clone();
        let removed = self
            .as_mut()
            .modify(move |file| {
                let before = file.sources.len();
                file.sources.retain(|source| source.path != wanted);
                if file.sources.len() == before {
                    return false;
                }
                file.hosts.retain(|host| !host.is_linked());
                file.hosts
                    .extend(ssh_config::load_sources(&file.sources, &home).to_hosts(true, None));
                true
            })
            .unwrap_or(false);
        if removed {
            let files = ssh_config::load_sources(&self.file().file.sources, &self.home).files;
            self.watch(files);
        }
        removed
    }

    /// See the bridge declaration.
    pub fn load_fixture(mut self: Pin<&mut Self>, count: i32) {
        if !is_test_run() {
            return;
        }
        let mut file = sample_hosts(usize::try_from(count).unwrap_or(0));
        // A few hosts of other kinds, for the editor and the screenshots.
        file.hosts.push(Host {
            id: "FIXTURE-RDP".to_owned(),
            name: "desktop-01".to_owned(),
            protocol: Protocol::Rdp,
            address: "10.9.0.10".to_owned(),
            user: Some("Administrator".to_owned()),
            tags: vec!["windows".to_owned()],
            icon: "os-windows".to_owned(),
            ..Host::default()
        });
        file.hosts.push(Host {
            id: "FIXTURE-SERIAL".to_owned(),
            name: "switch-console".to_owned(),
            protocol: Protocol::Serial,
            address: "/dev/ttyUSB0".to_owned(),
            ..Host::default()
        });
        {
            let mut state = self.as_mut().rust_mut();
            state.locked = false;
            state.file_problems.clear();
            state.linked_problems.clear();
            state.recent = RecentList::default();
            state.recent.touch_host("H00003", 1);
            state.recent.touch_host("H00011", 2);
        }
        self.publish(file);
    }
}

impl cxx_qt::Initialize for qobject::Hosts {
    fn initialize(mut self: Pin<&mut Self>) {
        let Some(services) = services::get() else {
            tracing::warn!("Hosts created before services; the list starts empty");
            self.publish(HostsFile::default());
            return;
        };
        let config = services.paths.config_dir().to_path_buf();
        let data = services.paths.data_dir().to_path_buf();
        let home = opensesh_core::paths::home_dir().unwrap_or_default();
        // Two small files read at startup, before the first frame.
        let disk = load_disk(&config, &home);
        let recent = RecentList::load(&data.join(RECENT_FILE));
        let files = disk.linked.files.clone();
        {
            let mut state = self.as_mut().rust_mut();
            state.file_path = QString::from(&config.join(HOSTS_FILE).display().to_string());
            state.config_dir = Some(config);
            state.data_dir = Some(data);
            state.home = home;
            state.recent = recent;
            state.locked = disk.locked;
            state.file_problems = disk.problems;
            state.linked_problems = disk
                .linked
                .warnings
                .iter()
                .map(ToString::to_string)
                .collect();
        }
        for problem in &self.file_problems {
            tracing::warn!("hosts.toml: {problem}");
        }
        self.as_mut().publish(disk.file);
        self.watch(files);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_editor_json_becomes_a_host() {
        let host = host_from_json(
            r#"{"id":"","name":" web ","address":" 10.0.0.1 ","user":"","port":2222,
               "tags":["a"," a ","b"],"jump":["bastion",""],"ssh":{"compression":true,"x11":""},
               "terminal":{"font_size":13}}"#,
        )
        .unwrap();
        assert_eq!(host.name, "web");
        assert_eq!(host.address, "10.0.0.1");
        assert_eq!(host.user, None);
        assert_eq!(host.port, Some(2222));
        assert_eq!(host.tags, vec!["a", "b"]);
        assert_eq!(host.jump, Some(vec!["bastion".to_owned()]));
        assert_eq!(host.ssh.compression, Some(true));
        assert_eq!(host.ssh.x11, None);
        assert!(host.terminal.contains_key("font_size"));
        assert_eq!(
            host_from_json(r#"{"name":"x","port":70000}"#).unwrap_err(),
            vec![("port", "invalid")]
        );
        assert_eq!(
            host_from_json(r#"{"name":"x","port":0}"#).unwrap_err(),
            vec![("port", "invalid")]
        );
        assert!(host_from_json("[").is_err());
        let group =
            group_from_json(r#"{"name":"G","defaults":{"user":"deploy","port":22,"jump":[]}}"#)
                .unwrap();
        assert_eq!(group.defaults.user.as_deref(), Some("deploy"));
    }

    #[test]
    fn summaries_show_the_inherited_target() {
        let (file, _) = HostsFile::from_toml_str(
            r#"
            [[group]]
            id = "G"
            name = "Prod"
            color = "teal"
            [group.defaults]
            user = "deploy"
            port = 2222
            [[host]]
            id = "H"
            name = "web"
            group = "G"
            address = "fe80::1"
            "#,
        )
        .unwrap();
        let value: Json = serde_json::from_str(&summary(&file, &file.hosts[0])).unwrap();
        assert_eq!(value["target"], "deploy@[fe80::1]:2222");
        assert_eq!(value["color"], "teal");
        assert_eq!(value["groupPath"], "Prod");
        let groups = groups_json(&file);
        assert_eq!(groups[0]["total"], 1);
        let inherited = inherited_json(&file, Some("G"), Protocol::Ssh);
        assert_eq!(inherited["user"]["origin"], "group");
        assert_eq!(inherited["user"]["groupName"], "Prod");
        assert_eq!(inherited["ssh.x11"]["origin"], "default");
    }
}
