//! `Transfers` QML singleton (Sprint 8): the transfer queue every window shares, the questions
//! about files in the way, and remote files being edited.
//!
//! - **Jobs** copy between two file panes (by `paneId`), or from this computer's files (dropped
//!   from the file manager) to a pane. `jobs` lists them as JSON; the queue's events arrive from
//!   the SSH runtime and are applied on the GUI thread.
//! - **Editing** downloads a file into a private folder, opens it (with the configured command,
//!   or the view opens it with the system's editor), and uploads it each time it is saved. Before
//!   uploading, the server's copy is checked: if someone changed it since, the user decides. A
//!   refused upload can be done again through `sudo tee` when the user asks and types the
//!   password.

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
        /// The transfer queue and remote edits.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, jobs, READ, NOTIFY = jobs_changed)]
        #[qproperty(i32, active, READ, NOTIFY = jobs_changed)]
        #[qproperty(f64, speed, READ, NOTIFY = jobs_changed)]
        #[qproperty(QString, question, READ, NOTIFY = question_changed)]
        #[qproperty(QString, edits, READ, NOTIFY = edits_changed)]
        type Transfers = super::TransfersRust;

        /// `jobs`, `active` or `speed` changed.
        #[qsignal]
        #[cxx_name = "jobsChanged"]
        fn jobs_changed(self: Pin<&mut Transfers>);

        /// `question` changed.
        #[qsignal]
        #[cxx_name = "questionChanged"]
        fn question_changed(self: Pin<&mut Transfers>);

        /// `edits` changed.
        #[qsignal]
        #[cxx_name = "editsChanged"]
        fn edits_changed(self: Pin<&mut Transfers>);

        /// A job finished: `state` is `done`, `failed` or `cancelled`.
        #[qsignal]
        fn finished(
            self: Pin<&mut Transfers>,
            id: i32,
            state: QString,
            label: QString,
            detail: QString,
        );

        /// Edit `id` is ready at `local`; `opened` when the configured command opened it (else the
        /// view opens it with the system's editor).
        #[qsignal]
        #[cxx_name = "editReady"]
        fn edit_ready(self: Pin<&mut Transfers>, id: i32, local: QString, opened: bool);

        /// Something about edit `id`: `saved`, `conflict`, `denied`, `failed` (with `detail`).
        #[qsignal]
        #[cxx_name = "editEvent"]
        fn edit_event(
            self: Pin<&mut Transfers>,
            id: i32,
            what: QString,
            name: QString,
            detail: QString,
        );

        /// Copies `paths` of pane `from_pane` into folder `destination` of pane `to_pane`; with
        /// `remove_sources`, a move. Returns the job id, 0 when a pane is gone.
        #[qinvokable]
        fn copy(
            self: Pin<&mut Transfers>,
            from_pane: i32,
            paths: &QStringList,
            to_pane: i32,
            destination: &QString,
            remove_sources: bool,
        ) -> i32;

        /// Copies files of this computer (dropped from the file manager) into folder
        /// `destination` of pane `to_pane`.
        #[qinvokable]
        #[cxx_name = "copyLocal"]
        fn copy_local(
            self: Pin<&mut Transfers>,
            paths: &QStringList,
            to_pane: i32,
            destination: &QString,
        ) -> i32;

        /// Copies `paths` of pane `from_pane` to folder `destination` of this computer.
        #[qinvokable]
        #[cxx_name = "copyToLocal"]
        fn copy_to_local(
            self: Pin<&mut Transfers>,
            from_pane: i32,
            paths: &QStringList,
            destination: &QString,
        ) -> i32;

        /// Pauses job `id`.
        #[qinvokable]
        fn pause(self: &Transfers, id: i32);

        /// Resumes job `id`.
        #[qinvokable]
        fn resume(self: &Transfers, id: i32);

        /// Cancels job `id`.
        #[qinvokable]
        fn cancel(self: &Transfers, id: i32);

        /// Runs a failed job again, on its panes' current connections.
        #[qinvokable]
        fn retry(self: &Transfers, id: i32);

        /// Forgets finished jobs.
        #[qinvokable]
        #[cxx_name = "clearFinished"]
        fn clear_finished(self: &Transfers);

        /// Answers the question of job `id`: `overwrite`, `newer`, `resume`, `skip`, `rename`
        /// or `cancel`; `for_all` for the job's next files.
        #[qinvokable]
        fn answer(self: Pin<&mut Transfers>, id: i32, choice: &QString, for_all: bool);

        /// Edits `path` of pane `pane`. Returns the edit id, 0 when the pane is gone.
        #[qinvokable]
        fn edit(self: Pin<&mut Transfers>, pane: i32, path: &QString) -> i32;

        /// After a `conflict`: `overwrite` uploads anyway, `discard` downloads the server's copy
        /// over the local one, anything else keeps waiting for the next save.
        #[qinvokable]
        #[cxx_name = "resolveEdit"]
        fn resolve_edit(self: Pin<&mut Transfers>, id: i32, choice: &QString);

        /// After `denied`: uploads through `sudo tee` with `password` (empty when sudo needs
        /// none). The password is used once and not kept.
        #[qinvokable]
        #[cxx_name = "saveWithSudo"]
        fn save_with_sudo(self: Pin<&mut Transfers>, id: i32, password: &QString);

        /// Stops editing `id` and deletes the local copy.
        #[qinvokable]
        #[cxx_name = "stopEdit"]
        fn stop_edit(self: Pin<&mut Transfers>, id: i32);
    }

    impl cxx_qt::Initialize for Transfers {}
    impl cxx_qt::Threading for Transfers {}
}

use core::pin::Pin;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};
use opensesh_core::watch::FileWatcher;
use opensesh_ssh::sftp::transfer::{Choice, Conflict, Event, JobId, Progress, Request, State};
use opensesh_ssh::sftp::{Fs, FsError, path};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value as Json, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::sftp as app;

/// A remote file being edited.
struct Edit {
    pane: i32,
    remote: String,
    local: PathBuf,
    name: String,
    /// The server's modification time and size when last downloaded or uploaded.
    known: (Option<i64>, u64),
    state: &'static str,
    detail: String,
    watcher: Option<FileWatcher>,
    /// An upload is running; a save meanwhile uploads again after it.
    uploading: bool,
    again: bool,
}

/// Rust state behind `Transfers`.
#[derive(Default)]
pub struct TransfersRust {
    jobs: QString,
    active: i32,
    speed: f64,
    question: QString,
    edits: QString,
    list: Vec<Progress>,
    /// The panes of each job, for `retry`.
    panes: HashMap<JobId, (i32, i32)>,
    conflicts: Vec<Conflict>,
    edit_list: HashMap<i32, Edit>,
    next_edit: i32,
}

impl Drop for TransfersRust {
    fn drop(&mut self) {
        for edit in self.edit_list.values() {
            if let Some(dir) = edit.local.parent() {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
    }
}

fn job_id(id: i32) -> JobId {
    u64::try_from(id).unwrap_or(0)
}

fn progress_json(progress: &Progress) -> Json {
    let (code, message) = match &progress.state {
        State::Failed { code, message } => (*code, message.as_str()),
        _ => ("", ""),
    };
    json!({
        "id": progress.id,
        "label": progress.label,
        "destination": progress.destination,
        "fromRemote": progress.from_remote,
        "toRemote": progress.to_remote,
        "state": progress.state.as_str(),
        "errorCode": code,
        "error": message,
        "filesTotal": progress.files_total,
        "filesDone": progress.files_done,
        "filesSkipped": progress.files_skipped,
        "bytesTotal": progress.bytes_total,
        "bytesDone": progress.bytes_done,
        "speed": progress.speed,
        "eta": progress.eta,
        "current": progress.current,
    })
}

fn conflict_json(conflict: &Conflict) -> Json {
    json!({
        "job": conflict.job,
        "path": conflict.path,
        "name": conflict.source.name,
        "sourceSize": conflict.source.size,
        "sourceModified": conflict.source.modified,
        "existingSize": conflict.existing.size,
        "existingModified": conflict.existing.modified,
        "resumable": conflict.resumable,
    })
}

fn spawn<T: Send + 'static>(
    thread: cxx_qt::CxxQtThread<qobject::Transfers>,
    work: impl std::future::Future<Output = T> + Send + 'static,
    then: impl FnOnce(Pin<&mut qobject::Transfers>, T) + Send + 'static,
) {
    let Some(runtime) = opensesh_ssh::runtime() else {
        return;
    };
    runtime.spawn(async move {
        let result = work.await;
        let _ = thread.queue(move |object| then(object, result));
    });
}

/// Downloads `remote` of `fs` into `local`; returns its time and size on the server.
async fn download(fs: Fs, remote: String, local: PathBuf) -> Result<(Option<i64>, u64), FsError> {
    let entry = fs.stat(&remote).await?;
    let mut reader = fs.open_read(&remote, 0).await?;
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| FsError::io(&remote, &error))?;
    let shown = local.display().to_string();
    tokio::fs::write(&local, &bytes)
        .await
        .map_err(|error| FsError::io(&shown, &error))?;
    Ok((entry.modified, entry.size))
}

/// Why an upload didn't happen.
enum UploadError {
    /// The server's copy changed since (its time and size now).
    Changed,
    Fs(FsError),
}

/// Uploads `local` over `remote` unless the server's copy changed since `known` (with `force`, it
/// uploads anyway). Returns the new time and size.
async fn upload(
    fs: Fs,
    local: PathBuf,
    remote: String,
    known: (Option<i64>, u64),
    force: bool,
) -> Result<(Option<i64>, u64), UploadError> {
    if !force {
        let now = fs.stat(&remote).await.map_err(UploadError::Fs)?;
        if (now.modified, now.size) != known {
            return Err(UploadError::Changed);
        }
    }
    let shown = local.display().to_string();
    let bytes = tokio::fs::read(&local)
        .await
        .map_err(|error| UploadError::Fs(FsError::io(&shown, &error)))?;
    let mut writer = fs
        .open_write(&remote, None)
        .await
        .map_err(UploadError::Fs)?;
    writer
        .write_all(&bytes)
        .await
        .map_err(|error| UploadError::Fs(FsError::io(&remote, &error)))?;
    writer
        .shutdown()
        .await
        .map_err(|error| UploadError::Fs(FsError::io(&remote, &error)))?;
    let after = fs.stat(&remote).await.map_err(UploadError::Fs)?;
    Ok((after.modified, after.size))
}

impl qobject::Transfers {
    /// Applies an event of the queue.
    fn apply(mut self: Pin<&mut Self>, event: Event) {
        match event {
            Event::Progress(progress) => {
                let finished = progress.state.is_finished();
                let was_finished = self
                    .list
                    .iter()
                    .find(|job| job.id == progress.id)
                    .is_some_and(|job| job.state.is_finished());
                let announce = (finished && !was_finished).then(|| {
                    let detail = match &progress.state {
                        State::Failed { message, .. } => message.clone(),
                        _ => String::new(),
                    };
                    (
                        progress.id,
                        progress.state.as_str(),
                        progress.label.clone(),
                        detail,
                    )
                });
                {
                    let mut state = self.as_mut().rust_mut();
                    match state.list.iter_mut().find(|job| job.id == progress.id) {
                        Some(job) => *job = progress,
                        None => state.list.push(progress),
                    }
                }
                self.as_mut().publish_jobs();
                if let Some((id, state, label, detail)) = announce {
                    self.as_mut()
                        .rust_mut()
                        .conflicts
                        .retain(|conflict| conflict.job != id);
                    self.as_mut().publish_question();
                    self.as_mut().finished(
                        i32::try_from(id).unwrap_or(0),
                        QString::from(state),
                        QString::from(&label),
                        QString::from(&detail),
                    );
                }
            }
            Event::Conflict(conflict) => {
                self.as_mut().rust_mut().conflicts.push(conflict);
                self.publish_question();
            }
            Event::Removed(id) => {
                {
                    let mut state = self.as_mut().rust_mut();
                    state.list.retain(|job| job.id != id);
                    state.panes.remove(&id);
                }
                self.publish_jobs();
            }
        }
    }

    fn publish_jobs(mut self: Pin<&mut Self>) {
        let list: Vec<Json> = self.list.iter().map(progress_json).collect();
        let active = self
            .list
            .iter()
            .filter(|job| !job.state.is_finished())
            .count();
        let speed: f64 = self
            .list
            .iter()
            .filter(|job| job.state == State::Running)
            .map(|job| job.speed)
            .sum();
        {
            let mut state = self.as_mut().rust_mut();
            state.jobs = QString::from(&Json::Array(list).to_string());
            state.active = i32::try_from(active).unwrap_or(i32::MAX);
            state.speed = speed;
        }
        self.jobs_changed();
    }

    fn publish_question(mut self: Pin<&mut Self>) {
        let text = self
            .conflicts
            .first()
            .map(|conflict| conflict_json(conflict).to_string())
            .unwrap_or_default();
        if self.question.to_string() != text {
            self.as_mut().rust_mut().question = QString::from(&text);
            self.question_changed();
        }
    }

    fn add(mut self: Pin<&mut Self>, request: Request, panes: (i32, i32)) -> i32 {
        let Some(queue) = app::queue() else {
            return 0;
        };
        let id = queue.add(request);
        self.as_mut().rust_mut().panes.insert(id, panes);
        i32::try_from(id).unwrap_or(0)
    }

    /// See the bridge declaration.
    pub fn copy(
        self: Pin<&mut Self>,
        from_pane: i32,
        paths: &QStringList,
        to_pane: i32,
        destination: &QString,
        remove_sources: bool,
    ) -> i32 {
        let (Some(from), Some(to)) = (app::pane(from_pane), app::pane(to_pane)) else {
            return 0;
        };
        let request = Request {
            from,
            sources: paths.iter().map(ToString::to_string).collect(),
            to,
            destination: destination.to_string(),
            options: app::options(),
            remove_sources,
        };
        self.add(request, (from_pane, to_pane))
    }

    /// See the bridge declaration.
    pub fn copy_local(
        self: Pin<&mut Self>,
        paths: &QStringList,
        to_pane: i32,
        destination: &QString,
    ) -> i32 {
        let Some(to) = app::pane(to_pane) else {
            return 0;
        };
        let request = Request {
            from: Fs::local(),
            sources: paths.iter().map(ToString::to_string).collect(),
            to,
            destination: destination.to_string(),
            options: app::options(),
            remove_sources: false,
        };
        self.add(request, (0, to_pane))
    }

    /// See the bridge declaration.
    pub fn copy_to_local(
        self: Pin<&mut Self>,
        from_pane: i32,
        paths: &QStringList,
        destination: &QString,
    ) -> i32 {
        let Some(from) = app::pane(from_pane) else {
            return 0;
        };
        let request = Request {
            from,
            sources: paths.iter().map(ToString::to_string).collect(),
            to: Fs::local(),
            destination: destination.to_string(),
            options: app::options(),
            remove_sources: false,
        };
        self.add(request, (from_pane, 0))
    }

    /// See the bridge declaration.
    pub fn pause(&self, id: i32) {
        if let Some(queue) = app::queue() {
            queue.pause(job_id(id));
        }
    }

    /// See the bridge declaration.
    pub fn resume(&self, id: i32) {
        if let Some(queue) = app::queue() {
            queue.resume(job_id(id));
        }
    }

    /// See the bridge declaration.
    pub fn cancel(&self, id: i32) {
        if let Some(queue) = app::queue() {
            queue.cancel(job_id(id));
        }
    }

    /// See the bridge declaration.
    pub fn retry(&self, id: i32) {
        let id = job_id(id);
        let (from, to) = self.panes.get(&id).copied().unwrap_or((0, 0));
        let fresh = |pane: i32| if pane > 0 { app::pane(pane) } else { None };
        if let Some(queue) = app::queue() {
            queue.retry(id, fresh(from), fresh(to));
        }
    }

    /// See the bridge declaration.
    pub fn clear_finished(&self) {
        if let Some(queue) = app::queue() {
            queue.clear_finished();
        }
    }

    /// See the bridge declaration.
    pub fn answer(mut self: Pin<&mut Self>, id: i32, choice: &QString, for_all: bool) {
        let job = job_id(id);
        if let Some(queue) = app::queue() {
            queue.answer(job, Choice::parse(&choice.to_string()), for_all);
        }
        // The first question of this job is answered; the next comes as its own event.
        let position = self
            .conflicts
            .iter()
            .position(|conflict| conflict.job == job);
        if let Some(position) = position {
            self.as_mut().rust_mut().conflicts.remove(position);
        }
        self.publish_question();
    }

    fn publish_edits(mut self: Pin<&mut Self>) {
        let mut list: Vec<(&i32, &Edit)> = self.edit_list.iter().collect();
        list.sort_by_key(|(id, _)| **id);
        let value: Vec<Json> = list
            .into_iter()
            .map(|(id, edit)| {
                json!({
                    "id": id,
                    "name": edit.name,
                    "remote": edit.remote,
                    "local": edit.local.display().to_string(),
                    "state": edit.state,
                    "detail": edit.detail,
                })
            })
            .collect();
        self.as_mut().rust_mut().edits = QString::from(&Json::Array(value).to_string());
        self.edits_changed();
    }

    fn set_edit(mut self: Pin<&mut Self>, id: i32, state: &'static str, detail: &str) {
        let name = {
            let mut guard = self.as_mut().rust_mut();
            let Some(edit) = guard.edit_list.get_mut(&id) else {
                return;
            };
            edit.state = state;
            edit.detail = detail.to_owned();
            edit.name.clone()
        };
        self.as_mut().publish_edits();
        if matches!(state, "saved" | "conflict" | "denied" | "failed") {
            self.edit_event(
                id,
                QString::from(state),
                QString::from(&name),
                QString::from(detail),
            );
        }
    }

    /// See the bridge declaration.
    pub fn edit(mut self: Pin<&mut Self>, pane: i32, remote: &QString) -> i32 {
        let Some(fs) = app::pane(pane) else {
            return 0;
        };
        let remote = remote.to_string();
        let id = {
            let mut state = self.as_mut().rust_mut();
            state.next_edit += 1;
            state.next_edit
        };
        let name = path::file_name(fs.style(), &remote);
        let dir = match app::edit_dir(u64::try_from(id).unwrap_or(0) + edit_serial_base()) {
            Ok(dir) => dir,
            Err(error) => {
                tracing::warn!("no folder for editing: {error}");
                return 0;
            }
        };
        let local = dir.join(&name);
        self.as_mut().rust_mut().edit_list.insert(
            id,
            Edit {
                pane,
                remote: remote.clone(),
                local: local.clone(),
                name,
                known: (None, 0),
                state: "downloading",
                detail: String::new(),
                watcher: None,
                uploading: false,
                again: false,
            },
        );
        self.as_mut().publish_edits();
        let thread = self.qt_thread();
        spawn(
            thread,
            download(fs, remote, local.clone()),
            move |mut object, result| match result {
                Ok(known) => {
                    if let Some(edit) = object.as_mut().rust_mut().edit_list.get_mut(&id) {
                        edit.known = known;
                    }
                    object.as_mut().watch(id, &local);
                    object.as_mut().set_edit(id, "watching", "");
                    let opened = open_with_command(&local);
                    object.edit_ready(id, QString::from(&local.display().to_string()), opened);
                }
                Err(error) => object.set_edit(id, "failed", &error.to_string()),
            },
        );
        id
    }

    fn watch(mut self: Pin<&mut Self>, id: i32, local: &std::path::Path) {
        let thread = self.qt_thread();
        let watcher = FileWatcher::spawn(local, Duration::from_millis(400), move || {
            let _ = thread.queue(move |object| object.saved(id));
        });
        match watcher {
            Ok(watcher) => {
                if let Some(edit) = self.as_mut().rust_mut().edit_list.get_mut(&id) {
                    edit.watcher = Some(watcher);
                }
            }
            Err(error) => tracing::warn!("edited file not watched: {error}"),
        }
    }

    /// The local copy of edit `id` changed: upload it.
    fn saved(self: Pin<&mut Self>, id: i32) {
        self.upload_edit(id, false);
    }

    fn upload_edit(mut self: Pin<&mut Self>, id: i32, force: bool) {
        let Some((pane, remote, local, known)) = self.edit_list.get(&id).map(|edit| {
            (
                edit.pane,
                edit.remote.clone(),
                edit.local.clone(),
                edit.known,
            )
        }) else {
            return;
        };
        {
            let mut state = self.as_mut().rust_mut();
            let Some(edit) = state.edit_list.get_mut(&id) else {
                return;
            };
            if edit.uploading {
                edit.again = true;
                return;
            }
            // A file that is gone locally (the editor saved elsewhere) is not uploaded.
            if !local.is_file() {
                return;
            }
            edit.uploading = true;
        }
        let Some(fs) = app::pane(pane) else {
            if let Some(edit) = self.as_mut().rust_mut().edit_list.get_mut(&id) {
                edit.uploading = false;
            }
            self.set_edit(id, "failed", "disconnected");
            return;
        };
        self.as_mut().set_edit(id, "uploading", "");
        let thread = self.qt_thread();
        spawn(
            thread,
            upload(fs, local, remote, known, force),
            move |mut object, result| {
                let again = {
                    let mut state = object.as_mut().rust_mut();
                    let Some(edit) = state.edit_list.get_mut(&id) else {
                        return;
                    };
                    edit.uploading = false;
                    if let Ok(known) = &result {
                        edit.known = *known;
                    }
                    std::mem::take(&mut edit.again)
                };
                match result {
                    Ok(_) => object.as_mut().set_edit(id, "saved", ""),
                    Err(UploadError::Changed) => object.as_mut().set_edit(id, "conflict", ""),
                    Err(UploadError::Fs(FsError::PermissionDenied { .. })) => {
                        object.as_mut().set_edit(id, "denied", "");
                    }
                    Err(UploadError::Fs(error)) => {
                        object.as_mut().set_edit(id, "failed", &error.to_string())
                    }
                }
                if again {
                    object.upload_edit(id, false);
                }
            },
        );
    }

    /// See the bridge declaration.
    pub fn resolve_edit(mut self: Pin<&mut Self>, id: i32, choice: &QString) {
        match choice.to_string().as_str() {
            "overwrite" => self.upload_edit(id, true),
            "discard" => {
                let Some((pane, remote, local)) = self
                    .edit_list
                    .get(&id)
                    .map(|edit| (edit.pane, edit.remote.clone(), edit.local.clone()))
                else {
                    return;
                };
                let Some(fs) = app::pane(pane) else {
                    return;
                };
                // The watcher sees the download as a save; the times then match and nothing
                // is uploaded... except the file itself: stop watching while it comes.
                if let Some(edit) = self.as_mut().rust_mut().edit_list.get_mut(&id) {
                    edit.watcher = None;
                }
                let thread = self.qt_thread();
                spawn(
                    thread,
                    download(fs, remote, local.clone()),
                    move |mut object, result| match result {
                        Ok(known) => {
                            if let Some(edit) = object.as_mut().rust_mut().edit_list.get_mut(&id) {
                                edit.known = known;
                            }
                            object.as_mut().watch(id, &local);
                            object.set_edit(id, "watching", "");
                        }
                        Err(error) => object.set_edit(id, "failed", &error.to_string()),
                    },
                );
            }
            _ => self.set_edit(id, "watching", ""),
        }
    }

    /// See the bridge declaration.
    pub fn save_with_sudo(mut self: Pin<&mut Self>, id: i32, password: &QString) {
        let Some((pane, remote, local)) = self
            .edit_list
            .get(&id)
            .map(|edit| (edit.pane, edit.remote.clone(), edit.local.clone()))
        else {
            return;
        };
        let Some(remote_fs) = app::pane(pane).and_then(|fs| fs.remote().cloned()) else {
            self.set_edit(id, "failed", "not a server");
            return;
        };
        let password = SecretString::from(password.to_string());
        self.as_mut().set_edit(id, "uploading", "");
        let thread = self.qt_thread();
        let work = async move {
            let shown = local.display().to_string();
            let content = tokio::fs::read(&local)
                .await
                .map_err(|error| FsError::io(&shown, &error).to_string())?;
            // Whether sudo asks for a password: with none needed, a password line would become
            // part of the file.
            let check = remote_fs
                .run("sudo -n true", &[])
                .await
                .map_err(|error| error.to_string())?;
            let mut input = zeroize::Zeroizing::new(Vec::with_capacity(content.len() + 64));
            if check.status != Some(0) {
                input.extend_from_slice(password.expose_secret().as_bytes());
                input.push(b'\n');
            }
            input.extend_from_slice(&content);
            let command = format!(
                "sudo -S -p '' tee -- {} > /dev/null",
                path::shell_quote(&remote)
            );
            let output = remote_fs
                .run(&command, &input)
                .await
                .map_err(|error| error.to_string())?;
            match output.status {
                Some(0) => {
                    let entry = remote_fs
                        .stat(&remote)
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok((entry.modified, entry.size))
                }
                _ => Err(String::from_utf8_lossy(&output.stderr).trim().to_owned()),
            }
        };
        spawn(thread, work, move |mut object, result| match result {
            Ok(known) => {
                if let Some(edit) = object.as_mut().rust_mut().edit_list.get_mut(&id) {
                    edit.known = known;
                }
                object.set_edit(id, "saved", "");
            }
            Err(detail) => object.set_edit(id, "failed", &detail),
        });
    }

    /// See the bridge declaration.
    pub fn stop_edit(mut self: Pin<&mut Self>, id: i32) {
        if let Some(edit) = self.as_mut().rust_mut().edit_list.remove(&id)
            && let Some(dir) = edit.local.parent()
        {
            let _ = std::fs::remove_dir_all(dir);
        }
        self.publish_edits();
    }
}

/// Edit folders of this run don't meet those left by an earlier one.
fn edit_serial_base() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
        .saturating_mul(1000)
}

/// Opens `file` with the configured editor command; `false` when there is none (or it failed).
fn open_with_command(file: &std::path::Path) -> bool {
    let command = app::settings().editor_command;
    let words = app::editor_command(&command, &file.display().to_string());
    let Some((program, args)) = words.split_first() else {
        return false;
    };
    match std::process::Command::new(program).args(args).spawn() {
        Ok(_) => true,
        Err(error) => {
            tracing::warn!("the editor command failed: {error}");
            false
        }
    }
}

impl cxx_qt::Initialize for qobject::Transfers {
    fn initialize(mut self: Pin<&mut Self>) {
        {
            let mut state = self.as_mut().rust_mut();
            state.jobs = QString::from("[]");
            state.edits = QString::from("[]");
        }
        let thread = self.qt_thread();
        app::set_sink(Box::new(move |event| {
            let _ = thread.queue(move |object| object.apply(event));
        }));
    }
}
