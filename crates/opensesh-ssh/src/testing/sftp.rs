//! An SFTP server over a folder, for [`Rules::sftp_root`](super::Rules::sftp_root): the tests
//! and the smoke test browse and transfer against it. The folder is the server's `/`, which is
//! also the user's home; paths can't leave it. Symbolic links follow OpenSSH's argument order.

use std::collections::HashMap;
use std::fs::{File as StdFile, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode, Version,
};
use russh_sftp::server::{Handler, StatusReply};

use crate::sftp::path::normalize_posix;

enum Open {
    File(StdFile),
    /// A listing, handed out once.
    Dir(Option<Vec<File>>),
}

/// The handler of one SFTP session.
pub(super) struct Server {
    root: PathBuf,
    handles: HashMap<String, Open>,
    next: u64,
}

impl Server {
    pub(super) fn new(root: PathBuf) -> Self {
        Self {
            root,
            handles: HashMap::new(),
            next: 1,
        }
    }

    /// The real path of a path of the session (`/` is the root folder).
    fn real(&self, path: &str) -> PathBuf {
        let absolute = if path.starts_with('/') {
            path.to_owned()
        } else {
            format!("/{path}")
        };
        let normal = normalize_posix(&absolute);
        self.root.join(normal.trim_start_matches('/'))
    }

    fn handle(&mut self, open: Open) -> String {
        let handle = self.next.to_string();
        self.next += 1;
        self.handles.insert(handle.clone(), open);
        handle
    }
}

fn ok(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Ok".to_owned(),
        language_tag: "en-US".to_owned(),
    }
}

fn reply(error: &std::io::Error) -> StatusReply {
    let code = match error.kind() {
        std::io::ErrorKind::NotFound => StatusCode::NoSuchFile,
        std::io::ErrorKind::PermissionDenied => StatusCode::PermissionDenied,
        _ => StatusCode::Failure,
    };
    StatusReply::new(code).with_message(error.to_string())
}

fn secs(time: std::io::Result<std::time::SystemTime>) -> Option<u32> {
    let time = time.ok()?;
    let secs = time.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    u32::try_from(secs).ok()
}

fn attrs(meta: &std::fs::Metadata) -> FileAttributes {
    let file_type = meta.file_type();
    let kind = if file_type.is_symlink() {
        0o120_000
    } else if file_type.is_dir() {
        0o040_000
    } else {
        0o100_000
    };
    #[cfg(unix)]
    let bits = {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o7777
    };
    #[cfg(not(unix))]
    let bits = {
        let base = if meta.permissions().readonly() {
            0o444
        } else {
            0o644
        };
        if file_type.is_dir() {
            base | 0o111
        } else {
            base
        }
    };
    FileAttributes {
        size: Some(meta.len()),
        uid: Some(1000),
        user: None,
        gid: Some(1000),
        group: None,
        permissions: Some(kind | bits),
        atime: secs(meta.accessed()),
        mtime: secs(meta.modified()),
    }
}

fn set(path: &Path, attrs: &FileAttributes) -> std::io::Result<()> {
    if let Some(mode) = attrs.permissions {
        #[cfg(unix)]
        let permissions = {
            use std::os::unix::fs::PermissionsExt;
            std::fs::Permissions::from_mode(mode & 0o7777)
        };
        #[cfg(not(unix))]
        let permissions = {
            let mut permissions = std::fs::metadata(path)?.permissions();
            permissions.set_readonly(mode & 0o200 == 0);
            permissions
        };
        std::fs::set_permissions(path, permissions)?;
    }
    if let Some(modified) = attrs.mtime
        && !path.is_dir()
    {
        let at =
            |secs: u32| std::time::UNIX_EPOCH + std::time::Duration::from_secs(u64::from(secs));
        let times = std::fs::FileTimes::new()
            .set_modified(at(modified))
            .set_accessed(at(attrs.atime.unwrap_or(modified)));
        OpenOptions::new()
            .write(true)
            .open(path)?
            .set_times(times)?;
    }
    Ok(())
}

impl Handler for Server {
    type Error = StatusReply;

    fn unimplemented(&self) -> Self::Error {
        StatusReply::new(StatusCode::OpUnsupported)
    }

    async fn init(
        &mut self,
        _version: u32,
        _extensions: HashMap<String, String>,
    ) -> Result<Version, Self::Error> {
        Ok(Version::new())
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        flags: OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        let path = self.real(&filename);
        let mut options = OpenOptions::new();
        options
            .read(flags.contains(OpenFlags::READ))
            .write(flags.contains(OpenFlags::WRITE) || flags.contains(OpenFlags::APPEND))
            .append(flags.contains(OpenFlags::APPEND))
            .truncate(flags.contains(OpenFlags::TRUNCATE));
        if flags.contains(OpenFlags::EXCLUDE) {
            options.create_new(true);
        } else if flags.contains(OpenFlags::CREATE) {
            options.create(true);
        }
        let file = options.open(&path).map_err(|error| reply(&error))?;
        Ok(Handle {
            id,
            handle: self.handle(Open::File(file)),
        })
    }

    async fn close(&mut self, id: u32, handle: String) -> Result<Status, Self::Error> {
        self.handles.remove(&handle);
        Ok(ok(id))
    }

    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, Self::Error> {
        let Some(Open::File(file)) = self.handles.get_mut(&handle) else {
            return Err(StatusReply::new(StatusCode::Failure));
        };
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| reply(&error))?;
        let mut data = vec![0; usize::try_from(len).unwrap_or(0).min(256 * 1024)];
        let read = file.read(&mut data).map_err(|error| reply(&error))?;
        if read == 0 {
            return Err(StatusReply::new(StatusCode::Eof));
        }
        data.truncate(read);
        Ok(Data { id, data })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, Self::Error> {
        let Some(Open::File(file)) = self.handles.get_mut(&handle) else {
            return Err(StatusReply::new(StatusCode::Failure));
        };
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| reply(&error))?;
        file.write_all(&data).map_err(|error| reply(&error))?;
        Ok(ok(id))
    }

    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let meta = std::fs::symlink_metadata(self.real(&path)).map_err(|error| reply(&error))?;
        Ok(Attrs {
            id,
            attrs: attrs(&meta),
        })
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let meta = std::fs::metadata(self.real(&path)).map_err(|error| reply(&error))?;
        Ok(Attrs {
            id,
            attrs: attrs(&meta),
        })
    }

    async fn fstat(&mut self, id: u32, handle: String) -> Result<Attrs, Self::Error> {
        let Some(Open::File(file)) = self.handles.get(&handle) else {
            return Err(StatusReply::new(StatusCode::Failure));
        };
        let meta = file.metadata().map_err(|error| reply(&error))?;
        Ok(Attrs {
            id,
            attrs: attrs(&meta),
        })
    }

    async fn setstat(
        &mut self,
        id: u32,
        path: String,
        attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        set(&self.real(&path), &attrs).map_err(|error| reply(&error))?;
        Ok(ok(id))
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        let mut files = Vec::new();
        for item in std::fs::read_dir(self.real(&path))
            .map_err(|error| reply(&error))?
            .flatten()
        {
            if let Ok(meta) = std::fs::symlink_metadata(item.path()) {
                files.push(File::new(
                    item.file_name().to_string_lossy().into_owned(),
                    attrs(&meta),
                ));
            }
        }
        Ok(Handle {
            id,
            handle: self.handle(Open::Dir(Some(files))),
        })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
        let Some(Open::Dir(files)) = self.handles.get_mut(&handle) else {
            return Err(StatusReply::new(StatusCode::Failure));
        };
        match files.take() {
            Some(files) if !files.is_empty() => Ok(Name { id, files }),
            _ => Err(StatusReply::new(StatusCode::Eof)),
        }
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
        std::fs::remove_file(self.real(&filename)).map_err(|error| reply(&error))?;
        Ok(ok(id))
    }

    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        std::fs::create_dir(self.real(&path)).map_err(|error| reply(&error))?;
        Ok(ok(id))
    }

    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
        std::fs::remove_dir(self.real(&path)).map_err(|error| reply(&error))?;
        Ok(ok(id))
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        let absolute = if path.starts_with('/') {
            path
        } else {
            format!("/{path}")
        };
        Ok(Name {
            id,
            files: vec![File::dummy(normalize_posix(&absolute))],
        })
    }

    async fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> Result<Status, Self::Error> {
        let to = self.real(&newpath);
        if to.exists() {
            return Err(StatusReply::new(StatusCode::Failure).with_message("the target exists"));
        }
        std::fs::rename(self.real(&oldpath), to).map_err(|error| reply(&error))?;
        Ok(ok(id))
    }

    async fn readlink(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        let target = std::fs::read_link(self.real(&path)).map_err(|error| reply(&error))?;
        Ok(Name {
            id,
            files: vec![File::dummy(target.display().to_string().replace('\\', "/"))],
        })
    }

    // OpenSSH's order: the first path is the target, the second the new link.
    async fn symlink(
        &mut self,
        id: u32,
        linkpath: String,
        targetpath: String,
    ) -> Result<Status, Self::Error> {
        let (target, link) = (linkpath, self.real(&targetpath));
        #[cfg(unix)]
        let result = std::os::unix::fs::symlink(&target, &link);
        #[cfg(windows)]
        let result = std::os::windows::fs::symlink_file(&target, &link);
        result.map_err(|error| reply(&error))?;
        Ok(ok(id))
    }
}
