//! `SftpBrowser` (Sprint 8): one file pane, this computer's files or a server's, as a list model
//! QML shows in a virtualized list (10,000 entries stay smooth).
//!
//! A remote pane opens its files on a terminal's connection (`terminalSession`, following its
//! `connectionSerial`: no second login) or on a connection of its own (`hostId` or `target`),
//! whose questions it shows like a terminal pane (`connection`, `prompt`, `answerPrompt`).
//! Everything touching files runs on the SSH runtime; results come back to the GUI thread and
//! operations report with `done(token, code, detail)`. The pane's file system is registered under
//! `paneId`, so `Transfers` can copy between any two panes.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!(<QtCore/QAbstractListModel>);
        /// The Qt base class.
        type QAbstractListModel;

        include!("cxx-qt-lib/qstring.h");
        /// Qt string type from cxx-qt-lib.
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qstringlist.h");
        /// Qt string list type from cxx-qt-lib.
        type QStringList = cxx_qt_lib::QStringList;

        include!("cxx-qt-lib/qmodelindex.h");
        /// Qt model index type from cxx-qt-lib.
        type QModelIndex = cxx_qt_lib::QModelIndex;

        include!("cxx-qt-lib/qvariant.h");
        /// Qt variant type from cxx-qt-lib.
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qhash.h");
        /// Role names.
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
    }

    extern "RustQt" {
        /// A file pane.
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(QString, mode, READ, WRITE, NOTIFY = source_changed)]
        #[qproperty(QString, host_id, cxx_name = "hostId", READ, WRITE, NOTIFY = source_changed)]
        #[qproperty(QString, target, READ, WRITE, NOTIFY = source_changed)]
        #[qproperty(i32, terminal_session, cxx_name = "terminalSession", READ, WRITE, NOTIFY = source_changed)]
        #[qproperty(i32, connection_serial, cxx_name = "connectionSerial", READ, WRITE = set_connection_serial, NOTIFY = source_changed)]
        #[qproperty(QString, start_path, cxx_name = "startPath", READ, WRITE, NOTIFY = source_changed)]
        #[qproperty(i32, pane_id, cxx_name = "paneId", READ, CONSTANT)]
        #[qproperty(QString, status, READ, NOTIFY = status_changed)]
        #[qproperty(QString, error, READ, NOTIFY = status_changed)]
        #[qproperty(QString, error_detail, cxx_name = "errorDetail", READ, NOTIFY = status_changed)]
        #[qproperty(bool, busy, READ, NOTIFY = status_changed)]
        #[qproperty(bool, remote, READ, NOTIFY = status_changed)]
        #[qproperty(QString, storage, READ, NOTIFY = status_changed)]
        #[qproperty(QString, connection, READ, NOTIFY = status_changed)]
        #[qproperty(QString, prompt, READ, NOTIFY = status_changed)]
        #[qproperty(QString, path, READ, NOTIFY = listing_changed)]
        #[qproperty(QString, crumbs, READ, NOTIFY = listing_changed)]
        #[qproperty(QString, home, READ, NOTIFY = listing_changed)]
        #[qproperty(QString, separator, READ, NOTIFY = listing_changed)]
        #[qproperty(i32, count, READ, NOTIFY = listing_changed)]
        #[qproperty(i32, hidden_count, cxx_name = "hiddenCount", READ, NOTIFY = listing_changed)]
        #[qproperty(bool, show_hidden, cxx_name = "showHidden", READ, WRITE = set_show_hidden, NOTIFY = listing_changed)]
        #[qproperty(QString, sort_key, cxx_name = "sortKey", READ, WRITE = set_sort_key, NOTIFY = listing_changed)]
        #[qproperty(bool, sort_ascending, cxx_name = "sortAscending", READ, WRITE = set_sort_ascending, NOTIFY = listing_changed)]
        #[qproperty(f64, space_free, cxx_name = "spaceFree", READ, NOTIFY = space_changed)]
        #[qproperty(f64, space_total, cxx_name = "spaceTotal", READ, NOTIFY = space_changed)]
        type SftpBrowser = super::SftpBrowserRust;

        /// `mode`, `hostId`, `target`, `terminalSession`, `connectionSerial` or `startPath`.
        #[qsignal]
        #[cxx_name = "sourceChanged"]
        fn source_changed(self: Pin<&mut SftpBrowser>);

        /// `status`, `error`, `busy`, `connection` or `prompt` changed.
        #[qsignal]
        #[cxx_name = "statusChanged"]
        fn status_changed(self: Pin<&mut SftpBrowser>);

        /// A new listing (or the same folder listed again).
        #[qsignal]
        #[cxx_name = "listingChanged"]
        fn listing_changed(self: Pin<&mut SftpBrowser>);

        /// `spaceFree` and `spaceTotal` changed.
        #[qsignal]
        #[cxx_name = "spaceChanged"]
        fn space_changed(self: Pin<&mut SftpBrowser>);

        /// Operation `token` ended: `code` empty on success, else an error code and its detail.
        #[qsignal]
        fn done(self: Pin<&mut SftpBrowser>, token: i32, code: QString, detail: QString);

        /// The preview of `token`: `kind` is `text`, `image` (content is a data URL), `binary`,
        /// `too-big` or an error code.
        #[qsignal]
        #[cxx_name = "previewReady"]
        fn preview_ready(self: Pin<&mut SftpBrowser>, token: i32, kind: QString, content: QString);

        fn set_connection_serial(self: Pin<&mut SftpBrowser>, serial: i32);
        fn set_show_hidden(self: Pin<&mut SftpBrowser>, show: bool);
        fn set_sort_key(self: Pin<&mut SftpBrowser>, key: QString);
        fn set_sort_ascending(self: Pin<&mut SftpBrowser>, ascending: bool);

        /// Opens the pane's files (connecting when it needs to), at `startPath` or home.
        #[qinvokable]
        fn start(self: Pin<&mut SftpBrowser>);

        /// Lists folder `path` (`~` is home).
        #[qinvokable]
        fn navigate(self: Pin<&mut SftpBrowser>, path: &QString);

        /// Lists the parent folder.
        #[qinvokable]
        fn up(self: Pin<&mut SftpBrowser>);

        /// Lists the current folder again.
        #[qinvokable]
        fn refresh(self: Pin<&mut SftpBrowser>);

        /// The full path of row `row` (empty when out of range).
        #[qinvokable]
        #[cxx_name = "pathAt"]
        fn path_at(self: &SftpBrowser, row: i32) -> QString;

        /// `name` inside the current folder.
        #[qinvokable]
        #[cxx_name = "childPath"]
        fn child_path(self: &SftpBrowser, name: &QString) -> QString;

        /// Row `row` as JSON: every field, with `path` and `permissionsText`.
        #[qinvokable]
        #[cxx_name = "entryJson"]
        fn entry_json(self: &SftpBrowser, row: i32) -> QString;

        /// The row of the entry called `name`, or -1.
        #[qinvokable]
        #[cxx_name = "rowOf"]
        fn row_of(self: &SftpBrowser, name: &QString) -> i32;

        /// The name of row `row` (empty when out of range).
        #[qinvokable]
        #[cxx_name = "nameAt"]
        fn name_at(self: &SftpBrowser, row: i32) -> QString;

        /// The names of rows `from` to `to`, both included, in either order.
        #[qinvokable]
        #[cxx_name = "namesBetween"]
        fn names_between(self: &SftpBrowser, from: i32, to: i32) -> QStringList;

        /// Every shown name.
        #[qinvokable]
        #[cxx_name = "allNames"]
        fn all_names(self: &SftpBrowser) -> QStringList;

        /// `names` as full paths in the current folder.
        #[qinvokable]
        #[cxx_name = "pathsOf"]
        fn paths_of(self: &SftpBrowser, names: &QStringList) -> QStringList;

        /// Moves `path` (of this pane's files) into `folder`, keeping its name.
        #[qinvokable]
        #[cxx_name = "moveInto"]
        fn move_into(self: Pin<&mut SftpBrowser>, path: &QString, folder: &QString) -> i32;

        /// Opens row `row`: a folder (or a link to one) is listed and `true` returned; a file
        /// returns `false` (the view decides what opening it means).
        #[qinvokable]
        #[cxx_name = "openRow"]
        fn open_row(self: Pin<&mut SftpBrowser>, row: i32) -> bool;

        /// A new folder `name` in the current one.
        #[qinvokable]
        fn mkdir(self: Pin<&mut SftpBrowser>, name: &QString) -> i32;

        /// A new empty file `name` in the current folder.
        #[qinvokable]
        #[cxx_name = "createFile"]
        fn create_file(self: Pin<&mut SftpBrowser>, name: &QString) -> i32;

        /// Renames row `row` to `name`.
        #[qinvokable]
        fn rename(self: Pin<&mut SftpBrowser>, row: i32, name: &QString) -> i32;

        /// Deletes these paths (folders with their contents).
        #[qinvokable]
        fn remove(self: Pin<&mut SftpBrowser>, paths: &QStringList) -> i32;

        /// Sets the permission bits of these paths.
        #[qinvokable]
        fn chmod(self: Pin<&mut SftpBrowser>, paths: &QStringList, mode: i32) -> i32;

        /// A symbolic link `name` in the current folder pointing to `target`.
        #[qinvokable]
        fn symlink(self: Pin<&mut SftpBrowser>, name: &QString, target: &QString) -> i32;

        /// A link to row `row` (an S3 object) that works without the keys for `seconds`; it is
        /// the `done` detail.
        #[qinvokable]
        #[cxx_name = "temporaryLink"]
        fn temporary_link(self: Pin<&mut SftpBrowser>, row: i32, seconds: i32) -> i32;

        /// Reads the start of row `row` for a quick look (`previewReady`).
        #[qinvokable]
        fn preview(self: Pin<&mut SftpBrowser>, row: i32) -> i32;

        /// Answers question `id` of the pane's own connection (see `TerminalItem.answerPrompt`).
        #[qinvokable]
        #[cxx_name = "answerPrompt"]
        fn answer_prompt(
            self: Pin<&mut SftpBrowser>,
            id: i32,
            action: &QString,
            secrets: &QStringList,
        ) -> bool;

        /// Connects again after the connection was lost.
        #[qinvokable]
        fn reconnect(self: Pin<&mut SftpBrowser>);

        /// The shell integration lines for `shell` (`bash` or `zsh`), to show and copy.
        #[qinvokable]
        #[cxx_name = "shellIntegration"]
        fn shell_integration(self: &SftpBrowser, shell: &QString) -> QString;

        /// Adds the shell integration to the rc file of `shell` on the server (only when the user
        /// asked for it); reports with `done`.
        #[qinvokable]
        #[cxx_name = "installShellIntegration"]
        fn install_shell_integration(self: Pin<&mut SftpBrowser>, shell: &QString) -> i32;
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &SftpBrowser, index: &QModelIndex, role: i32) -> QVariant;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &SftpBrowser) -> QHash_i32_QByteArray;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &SftpBrowser, parent: &QModelIndex) -> i32;

        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut SftpBrowser>);

        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut SftpBrowser>);
    }

    impl cxx_qt::Threading for SftpBrowser {}
}

use core::pin::Pin;
use std::collections::VecDeque;
use std::future::Future;
use std::sync::Arc;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{
    QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QStringList, QVariant,
};
use opensesh_ssh::connect::Note;
use opensesh_ssh::prompt::{Answer, Asker, Request};
use opensesh_ssh::sftp::entry::{self, Entry, Kind, SortKey};
use opensesh_ssh::sftp::path::{self, Style};
use opensesh_ssh::sftp::{Fs, FsError};
use serde_json::json;
use tokio::io::AsyncReadExt;

use crate::sftp::{self as app, Source};

const ROLE_NAME: i32 = 257;
const ROLE_KIND: i32 = 258;
const ROLE_SIZE: i32 = 259;
const ROLE_MODIFIED: i32 = 260;
const ROLE_PERMISSIONS: i32 = 261;
const ROLE_MODE: i32 = 262;
const ROLE_OWNER: i32 = 263;
const ROLE_LINK_TARGET: i32 = 264;
const ROLE_HIDDEN: i32 = 265;
const ROLE_DIR_LIKE: i32 = 266;
const ROLE_TARGET_KIND: i32 = 267;

/// How much of a file the quick look reads.
const PREVIEW_TEXT: u64 = 512 * 1024;
const PREVIEW_IMAGE: u64 = 8 * 1024 * 1024;

/// Rust state behind `SftpBrowser`.
pub struct SftpBrowserRust {
    mode: QString,
    host_id: QString,
    target: QString,
    terminal_session: i32,
    connection_serial: i32,
    start_path: QString,
    pane_id: i32,
    status: QString,
    error: QString,
    error_detail: QString,
    busy: bool,
    remote: bool,
    /// `local`, `sftp` or `s3`.
    storage: QString,
    connection: QString,
    prompt: QString,
    path: QString,
    crumbs: QString,
    home: QString,
    separator: QString,
    count: i32,
    hidden_count: i32,
    show_hidden: bool,
    sort_key: QString,
    sort_ascending: bool,
    /// Bytes free and in all on the server's file system of the folder; -1 when it doesn't say.
    space_free: f64,
    space_total: f64,
    fs: Option<Fs>,
    /// Everything in the folder; `rows` is what is shown.
    all: Vec<Entry>,
    rows: Vec<Entry>,
    started: bool,
    /// Grows with each listing asked for: a late answer to an older one is dropped.
    generation: u64,
    /// The folder of the listing in flight, if any: a refresh meanwhile lists it, not `path`.
    listing: Option<String>,
    /// Grows with each connection attempt, likewise.
    attempt: u64,
    next_token: i32,
    requests: VecDeque<Request>,
}

impl Default for SftpBrowserRust {
    fn default() -> Self {
        Self {
            mode: QString::from("local"),
            host_id: QString::default(),
            target: QString::default(),
            terminal_session: 0,
            connection_serial: 0,
            start_path: QString::default(),
            pane_id: app::new_pane_id(),
            status: QString::from("idle"),
            error: QString::default(),
            error_detail: QString::default(),
            busy: false,
            remote: false,
            storage: QString::from("local"),
            connection: QString::default(),
            prompt: QString::default(),
            path: QString::default(),
            crumbs: QString::from("[]"),
            home: QString::default(),
            separator: QString::from(std::path::MAIN_SEPARATOR_STR),
            count: 0,
            hidden_count: 0,
            show_hidden: app::settings().show_hidden,
            sort_key: QString::from("name"),
            sort_ascending: true,
            space_free: -1.0,
            space_total: -1.0,
            fs: None,
            all: Vec::new(),
            rows: Vec::new(),
            started: false,
            generation: 0,
            listing: None,
            attempt: 0,
            next_token: 0,
            requests: VecDeque::new(),
        }
    }
}

impl Drop for SftpBrowserRust {
    fn drop(&mut self) {
        app::unregister(self.pane_id);
        // Questions nobody will answer count as cancelled.
        for request in self.requests.drain(..) {
            request.answer(Answer::Cancel);
        }
    }
}

fn qstring(value: &str) -> QString {
    QString::from(value)
}

fn owner(entry: &Entry) -> String {
    match (entry.uid, entry.gid) {
        (Some(uid), Some(gid)) => format!("{uid}:{gid}"),
        (Some(uid), None) => uid.to_string(),
        _ => String::new(),
    }
}

fn entry_value(entry: &Entry, full: &str) -> serde_json::Value {
    json!({
        "name": entry.name,
        "path": full,
        "kind": entry.kind.as_str(),
        "size": entry.size,
        "modified": entry.modified,
        "mode": entry.mode,
        "permissionsText": entry::permissions_text(entry.kind, entry.mode),
        "owner": owner(entry),
        "uid": entry.uid,
        "gid": entry.gid,
        "linkTarget": entry.link_target,
        "targetKind": entry.target_kind.map(Kind::as_str),
        "dirLike": entry.is_dir_like(),
        "hidden": entry.is_hidden(),
    })
}

/// The quick look of a file's first bytes.
fn preview_of(name: &str, bytes: &[u8], truncated: bool) -> (&'static str, String) {
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    let image = match extension.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "bmp" => Some("image/bmp"),
        "webp" => Some("image/webp"),
        "svg" => Some("image/svg+xml"),
        "ico" => Some("image/x-icon"),
        _ => None,
    };
    if let Some(mime) = image {
        if truncated {
            return ("too-big", String::new());
        }
        use base64ct::Encoding;
        return (
            "image",
            format!(
                "data:{mime};base64,{}",
                base64ct::Base64::encode_string(bytes)
            ),
        );
    }
    let head = bytes.get(..bytes.len().min(8192)).unwrap_or_default();
    if head.contains(&0) {
        return ("binary", String::new());
    }
    let mut text = String::from_utf8_lossy(bytes).into_owned();
    if truncated {
        text.push_str("\n…");
    }
    ("text", text)
}

impl qobject::SftpBrowser {
    fn token(mut self: Pin<&mut Self>) -> i32 {
        let mut state = self.as_mut().rust_mut();
        state.next_token = state.next_token.wrapping_add(1).max(1);
        state.next_token
    }

    /// Runs `work` on the SSH runtime, then `then` with its result on the GUI thread.
    fn spawn<T: Send + 'static>(
        self: Pin<&mut Self>,
        work: impl Future<Output = T> + Send + 'static,
        then: impl FnOnce(Pin<&mut Self>, T) + Send + 'static,
    ) {
        let thread = self.qt_thread();
        let Some(runtime) = opensesh_ssh::runtime() else {
            return;
        };
        runtime.spawn(async move {
            let result = work.await;
            // The pane may be gone by now; then the result is dropped.
            let _ = thread.queue(move |object| then(object, result));
        });
    }

    fn set_status(mut self: Pin<&mut Self>, status: &str, error: &str, detail: &str) {
        {
            let mut state = self.as_mut().rust_mut();
            state.status = qstring(status);
            state.error = qstring(error);
            state.error_detail = qstring(detail);
        }
        self.status_changed();
    }

    fn set_busy(mut self: Pin<&mut Self>, busy: bool) {
        if self.busy != busy {
            self.as_mut().rust_mut().busy = busy;
            self.status_changed();
        }
    }

    fn style(&self) -> Style {
        self.fs.as_ref().map_or(Style::Local, Fs::style)
    }

    /// See the bridge declaration.
    pub fn start(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().started = true;
        self.open();
    }

    /// See the bridge declaration.
    pub fn reconnect(self: Pin<&mut Self>) {
        self.open();
    }

    /// Opens the file system of `mode` and lists the start folder.
    fn open(mut self: Pin<&mut Self>) {
        let pane = self.pane_id;
        app::unregister(pane);
        {
            let mut state = self.as_mut().rust_mut();
            state.attempt += 1;
            state.fs = None;
            for request in state.requests.drain(..) {
                request.answer(Answer::Cancel);
            }
            state.prompt = QString::default();
            state.connection = QString::default();
        }
        let start = self.start_path.to_string();
        if self.mode.to_string() != "remote" {
            let fs = Fs::local();
            app::register(pane, fs.clone());
            {
                let mut state = self.as_mut().rust_mut();
                state.home = qstring(&fs.home());
                state.remote = false;
                state.storage = qstring("local");
                state.separator = qstring(std::path::MAIN_SEPARATOR_STR);
                state.fs = Some(fs);
            }
            self.as_mut().set_status("ready", "", "");
            let path = if start.is_empty() {
                self.home.to_string()
            } else {
                start
            };
            self.list(path);
            return;
        }
        let source = if self.terminal_session > 0 {
            if self.connection_serial <= 0 {
                self.as_mut().set_status("disconnected", "no-session", "");
                return;
            }
            Source::Terminal(self.terminal_session)
        } else if !self.host_id.is_empty() {
            Source::Host(self.host_id.to_string())
        } else if !self.target.is_empty() {
            Source::Target(self.target.to_string())
        } else {
            self.as_mut().set_status("idle", "", "");
            return;
        };
        {
            let mut state = self.as_mut().rust_mut();
            state.remote = true;
            state.separator = qstring("/");
        }
        self.as_mut().set_status("connecting", "", "");
        let attempt = self.attempt;
        let thread = self.qt_thread();
        let asker: Asker = {
            let thread = thread.clone();
            Arc::new(move |request: Request| {
                let _ = thread.queue(move |object| object.push_request(request));
            })
        };
        let notes: opensesh_ssh::connect::Notes = Arc::new(move |note: Note| {
            let value = match note {
                Note::Connecting {
                    index,
                    count,
                    label,
                } => {
                    json!({ "state": "connecting", "index": index, "count": count, "label": label })
                }
                Note::Authenticating { label } => {
                    json!({ "state": "authenticating", "label": label })
                }
                Note::Banner(_) | Note::Warning(_) => return,
            };
            let text = value.to_string();
            let _ = thread.queue(move |mut object| {
                object.as_mut().rust_mut().connection = QString::from(&text);
                object.status_changed();
            });
        });
        self.spawn(
            async move { app::open_files(source, &asker, &notes).await },
            move |mut object, result| {
                if object.attempt != attempt {
                    return;
                }
                match result {
                    Ok((fs, from_source)) => {
                        app::register(object.pane_id, fs.clone());
                        // A source's own place (an s3:// path) unless the pane names one.
                        let start = if start.is_empty() { from_source } else { start };
                        {
                            let mut state = object.as_mut().rust_mut();
                            state.storage = qstring(if fs.s3().is_some() { "s3" } else { "sftp" });
                            state.home = qstring(&fs.home());
                            state.fs = Some(fs);
                            state.connection = QString::default();
                        }
                        object.as_mut().set_status("ready", "", "");
                        let path = if start.is_empty() {
                            "~".to_owned()
                        } else {
                            start
                        };
                        object.list(path);
                    }
                    Err(error) => {
                        object.as_mut().rust_mut().connection = QString::default();
                        object
                            .as_mut()
                            .set_status("error", error.code, &error.detail);
                    }
                }
            },
        );
    }

    fn push_request(mut self: Pin<&mut Self>, request: Request) {
        self.as_mut().rust_mut().requests.push_back(request);
        self.publish_prompt();
    }

    fn publish_prompt(mut self: Pin<&mut Self>) {
        let prompt = self
            .requests
            .front()
            .map(|request| {
                crate::bridge::terminal_view::ssh_prompt_json(request.id, &request.prompt)
            })
            .unwrap_or_default();
        self.as_mut().rust_mut().prompt = QString::from(&prompt);
        self.status_changed();
    }

    /// See the bridge declaration.
    pub fn answer_prompt(
        mut self: Pin<&mut Self>,
        id: i32,
        action: &QString,
        secrets: &QStringList,
    ) -> bool {
        let id = u64::try_from(id).unwrap_or(0);
        let position = self.requests.iter().position(|request| request.id == id);
        let Some(request) =
            position.and_then(|position| self.as_mut().rust_mut().requests.remove(position))
        else {
            return false;
        };
        let answer = match action.to_string().as_str() {
            "trust-once" => Answer::TrustOnce,
            "trust-save" => Answer::TrustAndRemember,
            "submit" => Answer::Secrets(
                secrets
                    .iter()
                    .map(|secret| secrecy::SecretString::from(secret.to_string()))
                    .collect(),
            ),
            _ => Answer::Cancel,
        };
        request.answer(answer);
        self.publish_prompt();
        true
    }

    /// See the bridge declaration.
    pub fn set_connection_serial(mut self: Pin<&mut Self>, serial: i32) {
        if self.connection_serial == serial {
            return;
        }
        self.as_mut().rust_mut().connection_serial = serial;
        self.as_mut().source_changed();
        // The terminal connected again (or dropped): follow it.
        if self.started && self.terminal_session > 0 {
            if serial > 0 {
                self.open();
            } else {
                app::unregister(self.pane_id);
                self.as_mut().rust_mut().fs = None;
                self.as_mut().set_status("disconnected", "no-session", "");
            }
        }
    }

    /// Lists `path` and shows it.
    fn list(mut self: Pin<&mut Self>, path: String) {
        let Some(fs) = self.fs.clone() else {
            return;
        };
        let generation = {
            let mut state = self.as_mut().rust_mut();
            state.generation += 1;
            state.listing = Some(path.clone());
            state.generation
        };
        self.as_mut().set_busy(true);
        self.spawn(
            async move {
                let canonical = fs.canonicalize(&path).await?;
                let entries = fs.list(&canonical).await?;
                Ok::<_, FsError>((canonical, entries))
            },
            move |mut object, result| {
                if object.generation != generation {
                    return;
                }
                object.as_mut().rust_mut().listing = None;
                object.as_mut().set_busy(false);
                match result {
                    Ok((path, entries)) => {
                        object.as_mut().rust_mut().all = entries;
                        object.as_mut().rust_mut().path = QString::from(&path);
                        object.as_mut().apply_view();
                        // A listing that works clears an earlier error.
                        if object.status.to_string() != "ready" || !object.error.is_empty() {
                            object.as_mut().set_status("ready", "", "");
                        }
                        object.as_mut().query_space(path);
                    }
                    Err(error) => {
                        let lost = error.is_transient();
                        object.as_mut().set_status(
                            if lost { "disconnected" } else { "ready" },
                            error.code(),
                            &error.to_string(),
                        );
                    }
                }
            },
        );
    }

    /// Asks the server how much space the folder's file system has (`statvfs@openssh.com`).
    fn query_space(self: Pin<&mut Self>, path: String) {
        let Some(remote) = self.fs.as_ref().and_then(Fs::remote).cloned() else {
            return;
        };
        let generation = self.generation;
        self.spawn(
            async move { remote.space(&path).await },
            move |mut object, result| {
                if object.generation != generation {
                    return;
                }
                #[allow(clippy::cast_precision_loss, reason = "sizes to show")]
                let (free, total) = match result {
                    Ok(Some(space)) => (space.available as f64, space.total as f64),
                    _ => (-1.0, -1.0),
                };
                {
                    let mut state = object.as_mut().rust_mut();
                    state.space_free = free;
                    state.space_total = total;
                }
                object.space_changed();
            },
        );
    }

    /// Filters and sorts `all` into the model.
    fn apply_view(mut self: Pin<&mut Self>) {
        let show_hidden = self.show_hidden;
        let key = SortKey::parse(&self.sort_key.to_string());
        let ascending = self.sort_ascending;
        let mut rows: Vec<Entry> = self
            .all
            .iter()
            .filter(|entry| show_hidden || !entry.is_hidden())
            .cloned()
            .collect();
        entry::sort(&mut rows, key, ascending);
        let hidden = self.all.iter().filter(|entry| entry.is_hidden()).count();
        let crumbs: Vec<serde_json::Value> = path::crumbs(self.style(), &self.path.to_string())
            .into_iter()
            .map(|(label, path)| json!({ "label": label, "path": path }))
            .collect();
        // SAFETY: the model is reset as a whole, between begin and end.
        unsafe {
            self.as_mut().begin_reset_model();
        }
        {
            let mut state = self.as_mut().rust_mut();
            state.count = i32::try_from(rows.len()).unwrap_or(i32::MAX);
            state.hidden_count = i32::try_from(hidden).unwrap_or(i32::MAX);
            state.rows = rows;
            state.crumbs = QString::from(&serde_json::Value::Array(crumbs).to_string());
        }
        // SAFETY: see above.
        unsafe {
            self.as_mut().end_reset_model();
        }
        self.listing_changed();
    }

    /// See the bridge declaration.
    pub fn set_show_hidden(mut self: Pin<&mut Self>, show: bool) {
        if self.show_hidden != show {
            self.as_mut().rust_mut().show_hidden = show;
            self.apply_view();
        }
    }

    /// See the bridge declaration.
    pub fn set_sort_key(mut self: Pin<&mut Self>, key: QString) {
        if self.sort_key != key {
            self.as_mut().rust_mut().sort_key = key;
            self.apply_view();
        }
    }

    /// See the bridge declaration.
    pub fn set_sort_ascending(mut self: Pin<&mut Self>, ascending: bool) {
        if self.sort_ascending != ascending {
            self.as_mut().rust_mut().sort_ascending = ascending;
            self.apply_view();
        }
    }

    /// See the bridge declaration.
    pub fn navigate(self: Pin<&mut Self>, path: &QString) {
        self.list(path.to_string());
    }

    /// See the bridge declaration.
    pub fn up(self: Pin<&mut Self>) {
        let style = self.style();
        if let Some(parent) = path::parent(style, &self.path.to_string()) {
            self.list(parent);
        }
    }

    /// See the bridge declaration.
    pub fn refresh(self: Pin<&mut Self>) {
        let path = self
            .listing
            .clone()
            .unwrap_or_else(|| self.path.to_string());
        let path = if path.is_empty() && self.remote {
            "~".to_owned()
        } else {
            path
        };
        if self.fs.is_some() {
            self.list(path);
        }
    }

    fn row(&self, row: i32) -> Option<&Entry> {
        usize::try_from(row).ok().and_then(|row| self.rows.get(row))
    }

    fn full(&self, name: &str) -> String {
        path::join(self.style(), &self.path.to_string(), name)
    }

    /// See the bridge declaration.
    pub fn path_at(&self, row: i32) -> QString {
        self.row(row)
            .map(|entry| QString::from(&self.full(&entry.name)))
            .unwrap_or_default()
    }

    /// See the bridge declaration.
    pub fn child_path(&self, name: &QString) -> QString {
        QString::from(&self.full(&name.to_string()))
    }

    /// See the bridge declaration.
    pub fn entry_json(&self, row: i32) -> QString {
        self.row(row)
            .map(|entry| QString::from(&entry_value(entry, &self.full(&entry.name)).to_string()))
            .unwrap_or_default()
    }

    /// See the bridge declaration.
    pub fn row_of(&self, name: &QString) -> i32 {
        let name = name.to_string();
        self.rows
            .iter()
            .position(|entry| entry.name == name)
            .and_then(|row| i32::try_from(row).ok())
            .unwrap_or(-1)
    }

    /// See the bridge declaration.
    pub fn name_at(&self, row: i32) -> QString {
        self.row(row)
            .map(|entry| QString::from(&entry.name))
            .unwrap_or_default()
    }

    /// See the bridge declaration.
    pub fn names_between(&self, from: i32, to: i32) -> QStringList {
        let (low, high) = if from <= to { (from, to) } else { (to, from) };
        let low = usize::try_from(low.max(0)).unwrap_or(0);
        let high = usize::try_from(high.max(0)).unwrap_or(0);
        self.rows
            .iter()
            .skip(low)
            .take(high.saturating_sub(low) + 1)
            .map(|entry| QString::from(&entry.name))
            .collect()
    }

    /// See the bridge declaration.
    pub fn all_names(&self) -> QStringList {
        self.rows
            .iter()
            .map(|entry| QString::from(&entry.name))
            .collect()
    }

    /// See the bridge declaration.
    pub fn paths_of(&self, names: &QStringList) -> QStringList {
        names
            .iter()
            .map(|name| QString::from(&self.full(&name.to_string())))
            .collect()
    }

    /// See the bridge declaration.
    pub fn move_into(self: Pin<&mut Self>, from: &QString, folder: &QString) -> i32 {
        let style = self.style();
        let from = from.to_string();
        let to = path::join(style, &folder.to_string(), &path::file_name(style, &from));
        self.operation(move |fs| async move { fs.rename(&from, &to).await })
    }

    /// See the bridge declaration.
    pub fn open_row(self: Pin<&mut Self>, row: i32) -> bool {
        let Some(entry) = self.row(row) else {
            return false;
        };
        if !entry.is_dir_like() {
            return false;
        }
        let full = self.full(&entry.name);
        self.list(full);
        true
    }

    /// Runs a file operation; reports with `done` and lists the folder again after it.
    fn operation<F>(mut self: Pin<&mut Self>, work: impl FnOnce(Fs) -> F + Send + 'static) -> i32
    where
        F: Future<Output = Result<(), FsError>> + Send + 'static,
    {
        let token = self.as_mut().token();
        let Some(fs) = self.fs.clone() else {
            self.done(token, qstring("disconnected"), QString::default());
            return token;
        };
        self.as_mut().set_busy(true);
        self.spawn(work(fs), move |mut object, result| {
            object.as_mut().set_busy(false);
            match result {
                Ok(()) => object
                    .as_mut()
                    .done(token, QString::default(), QString::default()),
                Err(error) => {
                    object.as_mut().done(
                        token,
                        qstring(error.code()),
                        QString::from(&error.to_string()),
                    );
                    if error.is_transient() {
                        object.as_mut().set_status(
                            "disconnected",
                            error.code(),
                            &error.to_string(),
                        );
                        return;
                    }
                }
            }
            object.refresh();
        });
        token
    }

    /// See the bridge declaration.
    pub fn temporary_link(mut self: Pin<&mut Self>, row: i32, seconds: i32) -> i32 {
        let token = self.as_mut().token();
        let path = self.path_at(row).to_string();
        let Some(s3) = self.fs.as_ref().and_then(Fs::s3).cloned() else {
            self.done(token, qstring("unsupported"), QString::default());
            return token;
        };
        let lifetime = std::time::Duration::from_secs(u64::try_from(seconds).unwrap_or(0));
        self.spawn(
            async move { s3.temporary_link(&path, lifetime).await },
            move |mut object, result| match result {
                Ok(link) => object
                    .as_mut()
                    .done(token, QString::default(), QString::from(&link)),
                Err(error) => object.as_mut().done(
                    token,
                    qstring(error.code()),
                    QString::from(&error.to_string()),
                ),
            },
        );
        token
    }

    /// See the bridge declaration.
    pub fn mkdir(self: Pin<&mut Self>, name: &QString) -> i32 {
        let path = self.full(&name.to_string());
        self.operation(move |fs| async move { fs.mkdir(&path).await })
    }

    /// See the bridge declaration.
    pub fn create_file(self: Pin<&mut Self>, name: &QString) -> i32 {
        let path = self.full(&name.to_string());
        self.operation(move |fs| async move { fs.create_file(&path).await })
    }

    /// See the bridge declaration.
    pub fn rename(self: Pin<&mut Self>, row: i32, name: &QString) -> i32 {
        let from = self.path_at(row).to_string();
        let to = self.full(&name.to_string());
        self.operation(move |fs| async move {
            if from.is_empty() {
                return Err(FsError::NotFound { path: to });
            }
            fs.rename(&from, &to).await
        })
    }

    /// See the bridge declaration.
    pub fn remove(self: Pin<&mut Self>, paths: &QStringList) -> i32 {
        let paths: Vec<String> = paths.iter().map(ToString::to_string).collect();
        self.operation(move |fs| async move {
            for path in paths {
                fs.remove(&path, true).await?;
            }
            Ok(())
        })
    }

    /// See the bridge declaration.
    pub fn chmod(self: Pin<&mut Self>, paths: &QStringList, mode: i32) -> i32 {
        let paths: Vec<String> = paths.iter().map(ToString::to_string).collect();
        let mode = u32::try_from(mode).unwrap_or(0o644) & 0o7777;
        self.operation(move |fs| async move {
            for path in paths {
                fs.chmod(&path, mode).await?;
            }
            Ok(())
        })
    }

    /// See the bridge declaration.
    pub fn symlink(self: Pin<&mut Self>, name: &QString, target: &QString) -> i32 {
        let link = self.full(&name.to_string());
        let target = target.to_string();
        self.operation(move |fs| async move { fs.symlink(&link, &target).await })
    }

    /// See the bridge declaration.
    pub fn preview(mut self: Pin<&mut Self>, row: i32) -> i32 {
        let token = self.as_mut().token();
        let (Some(fs), Some(entry)) = (self.fs.clone(), self.row(row).cloned()) else {
            self.preview_ready(token, qstring("not-found"), QString::default());
            return token;
        };
        let full = self.full(&entry.name);
        self.spawn(
            async move {
                let image = matches!(
                    entry
                        .name
                        .rsplit_once('.')
                        .map(|(_, ext)| ext.to_ascii_lowercase())
                        .as_deref(),
                    Some("png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" | "ico")
                );
                let limit = if image { PREVIEW_IMAGE } else { PREVIEW_TEXT };
                let reader = fs.open_read(&full, 0).await?;
                let mut bytes = Vec::new();
                reader
                    .take(limit + 1)
                    .read_to_end(&mut bytes)
                    .await
                    .map_err(|error| FsError::io(&full, &error))?;
                let truncated = bytes.len() as u64 > limit;
                bytes.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
                Ok::<_, FsError>(preview_of(&entry.name, &bytes, truncated))
            },
            move |mut object, result| match result {
                Ok((kind, content)) => {
                    object
                        .as_mut()
                        .preview_ready(token, qstring(kind), QString::from(&content))
                }
                Err(error) => object.as_mut().preview_ready(
                    token,
                    qstring(error.code()),
                    QString::from(&error.to_string()),
                ),
            },
        );
        token
    }

    /// See the bridge declaration.
    pub fn shell_integration(&self, shell: &QString) -> QString {
        QString::from(app::shell_integration(&shell.to_string()))
    }

    /// See the bridge declaration.
    pub fn install_shell_integration(mut self: Pin<&mut Self>, shell: &QString) -> i32 {
        let token = self.as_mut().token();
        let Some(remote) = self.fs.as_ref().and_then(Fs::remote).cloned() else {
            self.done(token, qstring("unsupported"), QString::default());
            return token;
        };
        let command = app::install_integration_command(&shell.to_string());
        self.spawn(
            async move { remote.run(&command, &[]).await },
            move |mut object, result| match result {
                Ok(output) if output.status == Some(0) => {
                    object
                        .as_mut()
                        .done(token, QString::default(), QString::default());
                }
                Ok(output) => object.as_mut().done(
                    token,
                    qstring("failed"),
                    QString::from(String::from_utf8_lossy(&output.stderr).trim()),
                ),
                Err(error) => object.as_mut().done(
                    token,
                    qstring("disconnected"),
                    QString::from(&error.to_string()),
                ),
            },
        );
        token
    }

    /// See the bridge declaration.
    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(entry) = self.row(index.row()) else {
            return QVariant::default();
        };
        match role {
            ROLE_NAME => QVariant::from(&QString::from(&entry.name)),
            ROLE_KIND => QVariant::from(&qstring(entry.kind.as_str())),
            ROLE_SIZE => QVariant::from(&i64::try_from(entry.size).unwrap_or(i64::MAX)),
            ROLE_MODIFIED => QVariant::from(&entry.modified.unwrap_or(-1)),
            ROLE_PERMISSIONS => QVariant::from(&QString::from(&entry::permissions_text(
                entry.kind, entry.mode,
            ))),
            ROLE_MODE => QVariant::from(&i32::try_from(entry.mode).unwrap_or(0)),
            ROLE_OWNER => QVariant::from(&QString::from(&owner(entry))),
            ROLE_LINK_TARGET => QVariant::from(&QString::from(
                entry.link_target.as_deref().unwrap_or_default(),
            )),
            ROLE_HIDDEN => QVariant::from(&entry.is_hidden()),
            ROLE_DIR_LIKE => QVariant::from(&entry.is_dir_like()),
            ROLE_TARGET_KIND => {
                QVariant::from(&qstring(entry.target_kind.map_or("", Kind::as_str)))
            }
            _ => QVariant::default(),
        }
    }

    /// See the bridge declaration.
    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        for (role, name) in [
            (ROLE_NAME, "name"),
            (ROLE_KIND, "kind"),
            (ROLE_SIZE, "size"),
            (ROLE_MODIFIED, "modified"),
            (ROLE_PERMISSIONS, "permissions"),
            (ROLE_MODE, "mode"),
            (ROLE_OWNER, "owner"),
            (ROLE_LINK_TARGET, "linkTarget"),
            (ROLE_HIDDEN, "hidden"),
            (ROLE_DIR_LIKE, "dirLike"),
            (ROLE_TARGET_KIND, "targetKind"),
        ] {
            roles.insert(role, QByteArray::from(name));
        }
        roles
    }

    /// See the bridge declaration.
    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        i32::try_from(self.rows.len()).unwrap_or(i32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews() {
        assert_eq!(
            preview_of("a.txt", b"hello", false),
            ("text", "hello".to_owned())
        );
        assert_eq!(preview_of("a.bin", b"he\0llo", false).0, "binary");
        let (kind, url) = preview_of("x.PNG", &[1, 2, 3], false);
        assert_eq!(
            (kind, url.as_str()),
            ("image", "data:image/png;base64,AQID")
        );
        assert_eq!(preview_of("big.jpg", &[1], true).0, "too-big");
        assert!(preview_of("log.txt", b"abc", true).1.ends_with('…'));
    }
}
