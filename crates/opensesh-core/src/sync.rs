//! Sync between computers without a cloud of our own (Sprint 16, ADR 0037): the settings folder
//! can live in a Git repository or a Syncthing folder, and what reaches it from elsewhere is
//! merged instead of overwritten.
//!
//! - **Where the settings are:** [`location`] reads the pointer file ([`LOCATION_FILE`]) in the
//!   default settings folder; [`set_location`] changes it (used at the next start).
//! - **Saving merges:** the settings files are read through [`Baselines`], which remembers what
//!   each file held when this instance read it. When a save finds the file changed on disk since
//!   then (another computer through Syncthing, a `git pull`, another instance),
//!   [`write_merging`] merges the three versions instead of overwriting: records (hosts, groups,
//!   snippets, tunnels...) by their `id`, other values key by key ([`merge3`]). A lock file
//!   ([`LOCK_FILE`]) keeps two instances on one computer from writing a folder at once.
//! - **Conflicts:** Syncthing's `*.sync-conflict-*` copies and files with Git's conflict markers
//!   are found by [`find_conflicts`]; [`differences`] lists what differs between the two sides
//!   by record, and [`resolve`] builds the result from the user's choices.
//!
//! Merging is for TOML files; anything else is written as given.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use toml::{Table, Value};

use crate::fsutil::{self, WriteOutcome};

/// The pointer to a settings folder elsewhere, in the default settings folder.
pub const LOCATION_FILE: &str = "location.toml";

/// The lock file of a settings folder (empty; locked while a file of the folder is written).
pub const LOCK_FILE: &str = ".opensesh.lock";

/// What a Git repository of settings should ignore (backups, temporary files, the lock).
pub const GIT_IGNORE: &str = "# OpenSesh: backups, temporary files and the write lock.\n*.bak.*\n.*.tmp-*\n.*.bak-*\n.opensesh.lock\n";

/// The settings folder the pointer in `default_dir` names, if any (`~` is not expanded: the
/// path is written absolute).
#[must_use]
pub fn location(default_dir: &Path) -> Option<PathBuf> {
    let text = fs::read_to_string(default_dir.join(LOCATION_FILE)).ok()?;
    let table: Table = text.parse().ok()?;
    let folder = table.get("folder")?.as_str()?.trim();
    (!folder.is_empty()).then(|| PathBuf::from(folder))
}

/// Points the settings at `folder`, or back at `default_dir` itself with `None`.
///
/// # Errors
///
/// When the pointer can't be written or removed.
pub fn set_location(default_dir: &Path, folder: Option<&Path>) -> io::Result<()> {
    let pointer = default_dir.join(LOCATION_FILE);
    match folder {
        None => match fs::remove_file(&pointer) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        },
        Some(folder) => {
            let mut table = Table::new();
            table.insert(
                "folder".to_owned(),
                Value::String(folder.display().to_string()),
            );
            let text = toml::to_string(&table).map_err(io::Error::other)?;
            let text =
                format!("# OpenSesh keeps its settings in this folder (Settings, Sync).\n{text}");
            fsutil::atomic_write(&pointer, text.as_bytes(), 0).map(|_| ())
        }
    }
}

/// What each settings file held when this instance last read or wrote it: the base of the
/// three-way merge.
#[derive(Debug, Default)]
pub struct Baselines {
    files: Mutex<HashMap<PathBuf, Vec<u8>>>,
}

static GLOBAL: LazyLock<Arc<Baselines>> = LazyLock::new(Arc::default);

impl Baselines {
    /// The running instance's baselines.
    #[must_use]
    pub fn global() -> Arc<Self> {
        Arc::clone(&GLOBAL)
    }

    /// Reads `path` and remembers what it held.
    ///
    /// # Errors
    ///
    /// As [`fs::read_to_string`].
    pub fn read_to_string(&self, path: &Path) -> io::Result<String> {
        let text = fs::read_to_string(path)?;
        self.remember(path, text.as_bytes().to_vec());
        Ok(text)
    }

    /// Remembers that `path` holds `bytes` as far as this instance knows.
    pub fn remember(&self, path: &Path, bytes: Vec<u8>) {
        self.files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(path.to_path_buf(), bytes);
    }

    /// What `path` held when it was last read or written here.
    #[must_use]
    pub fn get(&self, path: &Path) -> Option<Vec<u8>> {
        self.files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(path)
            .cloned()
    }

    /// Forgets `path` (it was removed).
    pub fn forget(&self, path: &Path) {
        self.files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(path);
    }
}

/// Reads a settings file, remembering what it held for the merge of later saves.
///
/// # Errors
///
/// As [`fs::read_to_string`].
pub fn read_to_string(path: &Path) -> io::Result<String> {
    GLOBAL.read_to_string(path)
}

/// The folder's write lock, held until dropped.
struct FolderLock(#[allow(dead_code, reason = "held for its lock")] fs::File);

impl FolderLock {
    /// Locks the folder of `path`; `None` when the lock file can't be made (a read-only
    /// folder): the write goes on without it.
    fn acquire(path: &Path) -> Option<Self> {
        let dir = path.parent().filter(|dir| !dir.as_os_str().is_empty())?;
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join(LOCK_FILE))
            .ok()?;
        file.lock().ok()?;
        Some(Self(file))
    }
}

/// Writes `bytes` to `path` as [`fsutil::atomic_write`] does, but when the file changed on disk
/// since `baselines` last saw it, writes the three-way merge of what was read, `bytes` and what
/// is there now ([`WriteOutcome::Merged`]). Files this instance never read are written as given.
///
/// # Errors
///
/// As [`fsutil::atomic_write`], or when the current file can't be read.
pub fn write_merging(
    path: &Path,
    bytes: &[u8],
    backups: usize,
    baselines: &Baselines,
) -> io::Result<WriteOutcome> {
    let Some(base) = baselines.get(path) else {
        return fsutil::atomic_write(path, bytes, backups);
    };
    let _lock = FolderLock::acquire(path);
    let current = match fs::read(path) {
        Ok(current) => Some(current),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    let mut merged = false;
    let out: Cow<'_, [u8]> = match current {
        Some(current) if current != base && current != bytes && is_toml(path) => {
            let texts = (
                std::str::from_utf8(&base),
                std::str::from_utf8(bytes),
                std::str::from_utf8(&current),
            );
            match texts {
                (Ok(base), Ok(ours), Ok(theirs)) => match merge3(base, ours, theirs) {
                    Ok(result) => {
                        merged = true;
                        if !result.conflicts.is_empty() {
                            tracing::info!(
                                path = %path.display(),
                                conflicts = result.conflicts.len(),
                                "changed here and elsewhere; this instance's values kept"
                            );
                        }
                        Cow::Owned(result.text.into_bytes())
                    }
                    // What is there can't be read: this version wins, and the other stays in
                    // the backups.
                    Err(_) => Cow::Borrowed(bytes),
                },
                _ => Cow::Borrowed(bytes),
            }
        }
        _ => Cow::Borrowed(bytes),
    };
    let outcome = fsutil::atomic_write(path, &out, backups)?;
    // The base of what this instance holds: its own version, until it reads the merged file.
    baselines.remember(path, bytes.to_vec());
    Ok(if merged {
        WriteOutcome::Merged
    } else {
        outcome
    })
}

fn is_toml(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == "toml")
}

/// Something changed on both sides in different ways; this side's value was kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// The table or array it is in (`host`, `snippet`, `config`...).
    pub section: String,
    /// The record's id, or empty for a plain value.
    pub id: String,
    /// The record's name, when it has one.
    pub name: String,
}

/// A merge's result.
#[derive(Debug, Clone, PartialEq)]
pub struct Merged {
    /// The merged file.
    pub text: String,
    /// Where both sides changed the same thing.
    pub conflicts: Vec<Conflict>,
}

/// The three-way merge of TOML texts: `ours` and `theirs` both started from `base`.
///
/// # Errors
///
/// When one of them isn't TOML.
pub fn merge3(base: &str, ours: &str, theirs: &str) -> Result<Merged, String> {
    let parse = |text: &str| text.parse::<Table>().map_err(|error| error.to_string());
    let (base, ours, theirs) = (parse(base)?, parse(ours)?, parse(theirs)?);
    let mut conflicts = Vec::new();
    let table = merge_tables("", Some(&base), &ours, &theirs, &mut conflicts);
    let text = toml::to_string(&table).map_err(|error| error.to_string())?;
    Ok(Merged { text, conflicts })
}

/// The key that identifies an array's records (`id`, else `path`), when every element of all
/// the given arrays is a table with it as a string.
fn record_key(arrays: &[&[Value]]) -> Option<&'static str> {
    if arrays.iter().all(|items| items.is_empty()) {
        return None;
    }
    ["id", "path"].into_iter().find(|key| {
        arrays.iter().all(|items| {
            items
                .iter()
                .all(|item| item.get(*key).and_then(Value::as_str).is_some())
        })
    })
}

fn record_id<'a>(item: &'a Value, key: &str) -> &'a str {
    item.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn record_name(item: &Value) -> String {
    item.get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn merge_tables(
    section: &str,
    base: Option<&Table>,
    ours: &Table,
    theirs: &Table,
    conflicts: &mut Vec<Conflict>,
) -> Table {
    let mut out = Table::new();
    for (key, value) in ours {
        let at = if section.is_empty() { key } else { section };
        let base_value = base.and_then(|base| base.get(key));
        if let Some(merged) = merge_value(at, base_value, Some(value), theirs.get(key), conflicts) {
            out.insert(key.clone(), merged);
        }
    }
    for (key, value) in theirs {
        if ours.contains_key(key) {
            continue;
        }
        let at = if section.is_empty() { key } else { section };
        let base_value = base.and_then(|base| base.get(key));
        if let Some(merged) = merge_value(at, base_value, None, Some(value), conflicts) {
            out.insert(key.clone(), merged);
        }
    }
    out
}

fn merge_value(
    section: &str,
    base: Option<&Value>,
    ours: Option<&Value>,
    theirs: Option<&Value>,
    conflicts: &mut Vec<Conflict>,
) -> Option<Value> {
    if ours == theirs || theirs == base {
        return ours.cloned();
    }
    if ours == base {
        return theirs.cloned();
    }
    match (ours, theirs) {
        (Some(Value::Table(ours)), Some(Value::Table(theirs))) => Some(Value::Table(merge_tables(
            section,
            base.and_then(Value::as_table),
            ours,
            theirs,
            conflicts,
        ))),
        (Some(Value::Array(ours)), Some(Value::Array(theirs))) => {
            let base_items = base
                .and_then(Value::as_array)
                .map_or(&[][..], Vec::as_slice);
            match record_key(&[base_items, ours, theirs]) {
                Some(key) => Some(Value::Array(merge_records(
                    section, key, base_items, ours, theirs, conflicts,
                ))),
                None => {
                    conflicts.push(plain_conflict(section));
                    Some(Value::Array(ours.clone()))
                }
            }
        }
        _ => {
            conflicts.push(plain_conflict(section));
            ours.cloned()
        }
    }
}

fn plain_conflict(section: &str) -> Conflict {
    Conflict {
        section: section.to_owned(),
        id: String::new(),
        name: String::new(),
    }
}

fn merge_records(
    section: &str,
    key: &str,
    base: &[Value],
    ours: &[Value],
    theirs: &[Value],
    conflicts: &mut Vec<Conflict>,
) -> Vec<Value> {
    let by_id = |items: &[Value]| -> HashMap<String, Value> {
        items
            .iter()
            .map(|item| (record_id(item, key).to_owned(), item.clone()))
            .collect()
    };
    let (base_ids, their_ids) = (by_id(base), by_id(theirs));
    let our_ids: HashSet<&str> = ours.iter().map(|item| record_id(item, key)).collect();
    let mut conflict = |item: &Value| {
        conflicts.push(Conflict {
            section: section.to_owned(),
            id: record_id(item, key).to_owned(),
            name: record_name(item),
        });
    };
    let mut out: Vec<Value> = Vec::new();
    for item in ours {
        let id = record_id(item, key);
        let base_item = base_ids.get(id);
        match (their_ids.get(id), base_item) {
            (Some(their_item), _) => {
                let mut nested = Vec::new();
                if let Some(merged) = merge_value(
                    section,
                    base_item,
                    Some(item),
                    Some(their_item),
                    &mut nested,
                ) {
                    if !nested.is_empty() {
                        conflict(item);
                    }
                    out.push(merged);
                }
            }
            // Added here.
            (None, None) => out.push(item.clone()),
            // Removed there, unchanged here.
            (None, Some(base_item)) if base_item == item => {}
            // Removed there, changed here: kept.
            (None, Some(_)) => {
                conflict(item);
                out.push(item.clone());
            }
        }
    }
    for (index, item) in theirs.iter().enumerate() {
        let id = record_id(item, key);
        if our_ids.contains(id) {
            continue;
        }
        match base_ids.get(id) {
            // Removed here, unchanged there.
            Some(base_item) if base_item == item => continue,
            // Removed here, changed there: kept.
            Some(_) => conflict(item),
            // Added there.
            None => {}
        }
        // After the record it follows there, else first.
        let at = theirs[..index]
            .iter()
            .rev()
            .find_map(|before| {
                let before = record_id(before, key);
                out.iter()
                    .position(|placed| record_id(placed, key) == before)
            })
            .map_or(0, |position| position + 1);
        out.insert(at, item.clone());
    }
    out
}

/// How a record differs between the two sides of a conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Only this side has it.
    OnlyHere,
    /// Only the other side has it.
    OnlyThere,
    /// Both have it, differently.
    Different,
}

/// One difference between the two sides of a conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difference {
    /// The array or top-level key (`host`, `group`, `snippet`, `schema_version`...).
    pub section: String,
    /// The record's id; empty for a plain value.
    pub id: String,
    /// The record's name, when it has one.
    pub name: String,
    /// How it differs.
    pub change: Change,
}

/// Which side a difference takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// This computer's version.
    Here,
    /// The other version.
    There,
}

/// What differs between two versions of a file, by record (top level only).
#[must_use]
pub fn differences(here: &Table, there: &Table) -> Vec<Difference> {
    let mut out = Vec::new();
    let mut keys: Vec<&String> = here.keys().collect();
    keys.extend(there.keys().filter(|key| !here.contains_key(*key)));
    for key in keys {
        let (ours, theirs) = (here.get(key), there.get(key));
        if ours == theirs {
            continue;
        }
        let arrays = (
            ours.and_then(Value::as_array),
            theirs.and_then(Value::as_array),
        );
        let record = match arrays {
            (Some(ours), Some(theirs)) => record_key(&[ours, theirs]),
            (Some(items), None) | (None, Some(items)) => record_key(&[items]),
            (None, None) => None,
        };
        let Some(record) = record else {
            out.push(Difference {
                section: key.clone(),
                id: String::new(),
                name: String::new(),
                change: match (ours, theirs) {
                    (Some(_), None) => Change::OnlyHere,
                    (None, Some(_)) => Change::OnlyThere,
                    _ => Change::Different,
                },
            });
            continue;
        };
        let empty = Vec::new();
        let ours = arrays.0.unwrap_or(&empty);
        let theirs = arrays.1.unwrap_or(&empty);
        for item in ours {
            let id = record_id(item, record);
            let other = theirs.iter().find(|other| record_id(other, record) == id);
            let change = match other {
                None => Change::OnlyHere,
                Some(other) if other == item => continue,
                Some(_) => Change::Different,
            };
            out.push(Difference {
                section: key.clone(),
                id: id.to_owned(),
                name: record_name(item),
                change,
            });
        }
        for item in theirs {
            let id = record_id(item, record);
            if !ours.iter().any(|ours| record_id(ours, record) == id) {
                out.push(Difference {
                    section: key.clone(),
                    id: id.to_owned(),
                    name: record_name(item),
                    change: Change::OnlyThere,
                });
            }
        }
    }
    out
}

/// The side a difference takes when the user doesn't choose: records on one side only are kept
/// (nothing is lost), and for the others this computer's version.
#[must_use]
pub fn default_side(change: Change) -> Side {
    match change {
        Change::OnlyThere => Side::There,
        Change::OnlyHere | Change::Different => Side::Here,
    }
}

/// The two versions joined: each difference takes the side in `choices` (by section and id),
/// else [`default_side`]; everything else is the same on both sides.
#[must_use]
pub fn resolve(here: &Table, there: &Table, choices: &HashMap<(String, String), Side>) -> Table {
    let differences = differences(here, there);
    let side = |difference: &Difference| {
        choices
            .get(&(difference.section.clone(), difference.id.clone()))
            .copied()
            .unwrap_or_else(|| default_side(difference.change))
    };
    let mut out = here.clone();
    for difference in &differences {
        let wanted = side(difference);
        if difference.id.is_empty() {
            if wanted == Side::There {
                match there.get(&difference.section) {
                    Some(value) => {
                        out.insert(difference.section.clone(), value.clone());
                    }
                    None => {
                        out.remove(&difference.section);
                    }
                }
            }
            continue;
        }
        let section = difference.section.clone();
        let key = ["id", "path"]
            .into_iter()
            .find(|key| {
                let has = |table: &Table| {
                    table
                        .get(&section)
                        .and_then(Value::as_array)
                        .is_some_and(|items| {
                            items
                                .iter()
                                .any(|item| record_id(item, key) == difference.id)
                        })
                };
                has(here) || has(there)
            })
            .unwrap_or("id");
        let their_item = there
            .get(&section)
            .and_then(Value::as_array)
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| record_id(item, key) == difference.id)
            })
            .cloned();
        let array = out
            .entry(section.clone())
            .or_insert_with(|| Value::Array(Vec::new()));
        let Value::Array(items) = array else {
            continue;
        };
        let position = items
            .iter()
            .position(|item| record_id(item, key) == difference.id);
        match (wanted, position, their_item) {
            (Side::There, Some(at), Some(item)) => items[at] = item,
            (Side::There, Some(at), None) => {
                items.remove(at);
            }
            (Side::There, None, Some(item)) => items.push(item),
            _ => {}
        }
    }
    out
}

/// Where a conflict comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictSource {
    /// A Syncthing copy of the other version, next to the file.
    Syncthing(PathBuf),
    /// Git's conflict markers in the file.
    Git,
}

/// A settings file in conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictFile {
    /// The file.
    pub path: PathBuf,
    /// Where the other version is.
    pub source: ConflictSource,
}

/// The two (or three) versions of a conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sides {
    /// This computer's.
    pub here: String,
    /// The other one.
    pub there: String,
    /// What both started from, when Git kept it (`diff3`).
    pub base: Option<String>,
}

/// The folders of a settings folder that hold settings files.
pub const FOLDERS: [&str; 3] = ["profiles", "themes", "workspaces"];

/// The original name of a Syncthing conflict copy (`a.sync-conflict-<date>-<time>-<device>.toml`
/// is a copy of `a.toml`).
#[must_use]
pub fn syncthing_original(name: &str) -> Option<String> {
    let (stem, rest) = name.split_once(".sync-conflict-")?;
    Some(match rest.split_once('.') {
        Some((_, extension)) => format!("{stem}.{extension}"),
        None => stem.to_owned(),
    })
}

/// The settings files of `dir` (and its profile, theme and workspace folders) in conflict.
#[must_use]
pub fn find_conflicts(dir: &Path) -> Vec<ConflictFile> {
    let mut out = Vec::new();
    let folders = std::iter::once(dir.to_path_buf()).chain(FOLDERS.iter().map(|f| dir.join(f)));
    for folder in folders {
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter(|entry| entry.path().is_file())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect();
        names.sort();
        for name in names {
            let path = folder.join(&name);
            if let Some(original) = syncthing_original(&name) {
                out.push(ConflictFile {
                    path: folder.join(original),
                    source: ConflictSource::Syncthing(path),
                });
            } else if name.ends_with(".toml")
                && fs::read_to_string(&path).is_ok_and(|text| has_git_markers(&text))
            {
                out.push(ConflictFile {
                    path,
                    source: ConflictSource::Git,
                });
            }
        }
    }
    out
}

fn has_git_markers(text: &str) -> bool {
    let mut opened = false;
    for line in text.lines() {
        if line.starts_with("<<<<<<<") {
            opened = true;
        } else if opened && line.starts_with(">>>>>>>") {
            return true;
        }
    }
    false
}

/// The sides of a text with Git's conflict markers (`<<<<<<<`, `|||||||` with `diff3`,
/// `=======`, `>>>>>>>`); `None` without markers.
#[must_use]
pub fn split_git_conflict(text: &str) -> Option<Sides> {
    #[derive(PartialEq)]
    enum Part {
        All,
        Here,
        Base,
        There,
    }
    let mut part = Part::All;
    let (mut here, mut there, mut base) = (String::new(), String::new(), String::new());
    let mut found = false;
    let mut has_base = false;
    for line in text.split_inclusive('\n') {
        let marker = |prefix: &str| line.starts_with(prefix);
        match part {
            Part::All if marker("<<<<<<<") => {
                part = Part::Here;
                found = true;
                continue;
            }
            Part::Here if marker("|||||||") => {
                part = Part::Base;
                has_base = true;
                continue;
            }
            Part::Here | Part::Base if marker("=======") => {
                part = Part::There;
                continue;
            }
            Part::There if marker(">>>>>>>") => {
                part = Part::All;
                continue;
            }
            _ => {}
        }
        match part {
            Part::All => {
                here.push_str(line);
                there.push_str(line);
                base.push_str(line);
            }
            Part::Here => here.push_str(line),
            Part::Base => base.push_str(line),
            Part::There => there.push_str(line),
        }
    }
    found.then(|| Sides {
        here,
        there,
        base: has_base.then_some(base),
    })
}

/// The versions of a conflict.
///
/// # Errors
///
/// When a file can't be read, or a Git conflict has no markers any more.
pub fn sides(conflict: &ConflictFile) -> io::Result<Sides> {
    match &conflict.source {
        ConflictSource::Syncthing(copy) => Ok(Sides {
            here: fs::read_to_string(&conflict.path).unwrap_or_default(),
            there: fs::read_to_string(copy)?,
            base: None,
        }),
        ConflictSource::Git => {
            let text = fs::read_to_string(&conflict.path)?;
            split_git_conflict(&text)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no conflict markers"))
        }
    }
}

/// Writes the resolved `text` over the file and removes Syncthing's copy.
///
/// # Errors
///
/// When the file can't be written (the copy is left then).
pub fn finish(conflict: &ConflictFile, text: &str) -> io::Result<()> {
    fsutil::atomic_write(&conflict.path, text.as_bytes(), fsutil::DEFAULT_BACKUPS)?;
    if let ConflictSource::Syncthing(copy) = &conflict.source {
        match fs::remove_file(copy) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(text: &str) -> Table {
        text.parse().unwrap()
    }

    const BASE: &str = r#"
schema_version = 1

[[host]]
id = "A"
name = "alpha"
address = "a.lan"

[[host]]
id = "B"
name = "beta"
address = "b.lan"

[[host]]
id = "C"
name = "gamma"
address = "c.lan"
"#;

    #[test]
    fn records_merge_by_id() {
        // Here: alpha's port changed, gamma removed, delta added.
        let ours = r#"
schema_version = 1

[[host]]
id = "A"
name = "alpha"
address = "a.lan"
port = 2222

[[host]]
id = "B"
name = "beta"
address = "b.lan"

[[host]]
id = "D"
name = "delta"
address = "d.lan"
"#;
        // There: alpha renamed, beta's address changed, epsilon added after beta.
        let theirs = r#"
schema_version = 1

[[host]]
id = "A"
name = "alpha one"
address = "a.lan"

[[host]]
id = "B"
name = "beta"
address = "b2.lan"

[[host]]
id = "E"
name = "epsilon"
address = "e.lan"

[[host]]
id = "C"
name = "gamma"
address = "c.lan"
"#;
        let merged = merge3(BASE, ours, theirs).unwrap();
        assert_eq!(merged.conflicts, []);
        let result = table(&merged.text);
        let hosts = result["host"].as_array().unwrap();
        let summary: Vec<(String, String, String, Option<i64>)> = hosts
            .iter()
            .map(|host| {
                (
                    host["id"].as_str().unwrap().to_owned(),
                    host["name"].as_str().unwrap().to_owned(),
                    host["address"].as_str().unwrap().to_owned(),
                    host.get("port").and_then(Value::as_integer),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("A".into(), "alpha one".into(), "a.lan".into(), Some(2222)),
                ("B".into(), "beta".into(), "b2.lan".into(), None),
                ("E".into(), "epsilon".into(), "e.lan".into(), None),
                ("D".into(), "delta".into(), "d.lan".into(), None),
            ]
        );
    }

    #[test]
    fn the_same_field_changed_on_both_sides_keeps_ours() {
        let ours = BASE.replace("b.lan", "here.lan");
        let theirs = BASE
            .replace("b.lan", "there.lan")
            .replace("c.lan", "c2.lan");
        let merged = merge3(BASE, &ours, &theirs).unwrap();
        assert_eq!(
            merged.conflicts,
            [Conflict {
                section: "host".to_owned(),
                id: "B".to_owned(),
                name: "beta".to_owned()
            }]
        );
        assert!(merged.text.contains("here.lan"));
        assert!(merged.text.contains("c2.lan"));
        // Removed there, changed here: kept, as a conflict.
        let ours = BASE.replace("c.lan", "c3.lan");
        let theirs = BASE.replace(
            "[[host]]\nid = \"C\"\nname = \"gamma\"\naddress = \"c.lan\"\n",
            "",
        );
        let merged = merge3(BASE, &ours, &theirs).unwrap();
        assert!(merged.text.contains("c3.lan"));
        assert_eq!(merged.conflicts.len(), 1);
        // Plain values.
        let merged = merge3("a = 1\nb = 1\n", "a = 2\nb = 1\n", "a = 1\nb = 3\n").unwrap();
        assert_eq!(table(&merged.text), table("a = 2\nb = 3\n"));
        let merged = merge3("a = 1\n", "a = 2\n", "a = 3\n").unwrap();
        assert_eq!(table(&merged.text), table("a = 2\n"));
        assert_eq!(merged.conflicts, [plain_conflict("a")]);
        assert!(merge3("a = 1", "not toml =", "a = 2").is_err());
    }

    #[test]
    fn saving_merges_what_changed_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hosts.toml");
        fs::write(&path, BASE).unwrap();
        let (one, two) = (Baselines::default(), Baselines::default());
        let read_one = one.read_to_string(&path).unwrap();
        let read_two = two.read_to_string(&path).unwrap();
        // Each instance changes something else; the second save merges.
        let first = read_one.replace("a.lan", "a1.lan");
        assert_eq!(
            write_merging(&path, first.as_bytes(), 0, &one).unwrap(),
            WriteOutcome::Written
        );
        let second = read_two.replace("c.lan", "c2.lan");
        assert_eq!(
            write_merging(&path, second.as_bytes(), 0, &two).unwrap(),
            WriteOutcome::Merged
        );
        let on_disk = fs::read_to_string(&path).unwrap();
        assert!(
            on_disk.contains("a1.lan") && on_disk.contains("c2.lan"),
            "{on_disk}"
        );
        // The second instance's next change, before it reads the merged file, still keeps the
        // first's.
        let third = second.replace("b.lan", "b3.lan");
        assert_eq!(
            write_merging(&path, third.as_bytes(), 0, &two).unwrap(),
            WriteOutcome::Merged
        );
        let on_disk = fs::read_to_string(&path).unwrap();
        for wanted in ["a1.lan", "b3.lan", "c2.lan"] {
            assert!(on_disk.contains(wanted), "{wanted} in {on_disk}");
        }
        // A file never read here is written as given.
        let other = dir.path().join("state.toml");
        fs::write(&other, "x = 1\n").unwrap();
        write_merging(&other, b"y = 2\n", 0, &one).unwrap();
        assert_eq!(fs::read_to_string(&other).unwrap(), "y = 2\n");
    }

    #[test]
    fn the_location_pointer() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(location(dir.path()), None);
        let folder = dir.path().join("Sync").join("opensesh");
        set_location(dir.path(), Some(&folder)).unwrap();
        assert_eq!(location(dir.path()), Some(folder));
        set_location(dir.path(), None).unwrap();
        assert_eq!(location(dir.path()), None);
        set_location(dir.path(), None).unwrap();
    }

    #[test]
    fn conflicts_are_found_and_split() {
        assert_eq!(
            syncthing_original("hosts.sync-conflict-20261002-101010-ABCDEFG.toml").as_deref(),
            Some("hosts.toml")
        );
        assert_eq!(
            syncthing_original("known_hosts.sync-conflict-20261002-101010-ABCDEFG").as_deref(),
            Some("known_hosts")
        );
        assert_eq!(syncthing_original("hosts.toml"), None);

        let text = "a = 1\n<<<<<<< HEAD\nb = 2\n||||||| base\nb = 1\n=======\nb = 3\n>>>>>>> origin/main\nc = 4\n";
        let split = split_git_conflict(text).unwrap();
        assert_eq!(split.here, "a = 1\nb = 2\nc = 4\n");
        assert_eq!(split.there, "a = 1\nb = 3\nc = 4\n");
        assert_eq!(split.base.as_deref(), Some("a = 1\nb = 1\nc = 4\n"));
        assert_eq!(split_git_conflict("a = 1\n"), None);

        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("hosts.toml"), BASE).unwrap();
        fs::write(
            dir.path()
                .join("hosts.sync-conflict-20261002-101010-ABCDEFG.toml"),
            BASE,
        )
        .unwrap();
        fs::create_dir(dir.path().join("profiles")).unwrap();
        fs::write(dir.path().join("profiles").join("dark.toml"), text).unwrap();
        fs::write(dir.path().join("snippets.toml"), "a = 1\n").unwrap();
        let found = find_conflicts(dir.path());
        assert_eq!(
            found,
            [
                ConflictFile {
                    path: dir.path().join("hosts.toml"),
                    source: ConflictSource::Syncthing(
                        dir.path()
                            .join("hosts.sync-conflict-20261002-101010-ABCDEFG.toml")
                    ),
                },
                ConflictFile {
                    path: dir.path().join("profiles").join("dark.toml"),
                    source: ConflictSource::Git,
                },
            ]
        );
        assert_eq!(sides(&found[1]).unwrap().there, "a = 1\nb = 3\nc = 4\n");
        finish(&found[0], "schema_version = 1\n").unwrap();
        assert!(find_conflicts(dir.path()).len() == 1);
    }

    #[test]
    fn differences_and_choices() {
        let here = table(
            &BASE
                .replace("b.lan", "here.lan")
                .replace("schema_version = 1", "schema_version = 1\ntheme = \"dark\""),
        );
        let there = table(&format!(
            "{}\n[[host]]\nid = \"E\"\nname = \"epsilon\"\naddress = \"e.lan\"\n",
            BASE.replace(
                "[[host]]\nid = \"A\"\nname = \"alpha\"\naddress = \"a.lan\"\n",
                ""
            )
        ));
        let found = differences(&here, &there);
        let summary: Vec<(&str, &str, Change)> = found
            .iter()
            .map(|d| (d.section.as_str(), d.id.as_str(), d.change))
            .collect();
        assert_eq!(
            summary,
            // Sections in the table's (sorted) order.
            [
                ("host", "A", Change::OnlyHere),
                ("host", "B", Change::Different),
                ("host", "E", Change::OnlyThere),
                ("theme", "", Change::OnlyHere),
            ]
        );
        // By default nothing is lost and this side wins the differences.
        let joined = resolve(&here, &there, &HashMap::new());
        let ids = |table: &Table| -> Vec<String> {
            table["host"]
                .as_array()
                .unwrap()
                .iter()
                .map(|h| h["id"].as_str().unwrap().to_owned())
                .collect()
        };
        assert_eq!(ids(&joined), ["A", "B", "C", "E"]);
        assert_eq!(joined["host"][1]["address"].as_str(), Some("here.lan"));
        assert_eq!(joined["theme"].as_str(), Some("dark"));
        // Choices: take theirs for B and A (A is gone there), and drop their E and our theme.
        let choices = HashMap::from([
            (("host".to_owned(), "A".to_owned()), Side::There),
            (("host".to_owned(), "B".to_owned()), Side::There),
            (("host".to_owned(), "E".to_owned()), Side::Here),
            (("theme".to_owned(), String::new()), Side::There),
        ]);
        let joined = resolve(&here, &there, &choices);
        assert_eq!(ids(&joined), ["B", "C"]);
        assert_eq!(joined["host"][0]["address"].as_str(), Some("b.lan"));
        assert!(joined.get("theme").is_none());
    }
}
