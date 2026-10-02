//! S3 storage behind the file views' operations (PLAN Sprint 12, ADR 0033). `/` lists the
//! buckets, and `/bucket/a/b` is the object or folder `a/b` of `bucket`. Folders are key prefixes:
//! an empty object ending in `/` keeps an empty one, and making a folder makes that object.
//!
//! S3 has no permissions, settable times, links, or writing at an offset: those operations are
//! [`FsError::Unsupported`], and an upload that stops starts over. Renaming is a copy and a
//! delete (of every object of a folder).

use std::time::Duration;

use opensesh_s3::{PART_SIZE, S3, S3Error};

use super::FsError;
use super::entry::{Entry, Kind};
use super::fs::{Reader, Writer};
use super::path::normalize_posix;

/// An S3 endpoint's buckets as files.
#[derive(Debug, Clone)]
pub struct S3Fs {
    s3: S3,
    part_size: usize,
}

/// Where a path points.
enum Place {
    /// `/`: the buckets.
    Root,
    /// A bucket.
    Bucket(String),
    /// An object or a folder: the bucket and the key (without a trailing `/`).
    Key(String, String),
}

impl S3Fs {
    /// The buckets of `s3`, with uploads in [`PART_SIZE`] parts.
    #[must_use]
    pub fn new(s3: S3) -> Self {
        Self::with_part_size(s3, PART_SIZE)
    }

    /// The same with parts of `part_size` bytes (tests use small ones).
    #[must_use]
    pub fn with_part_size(s3: S3, part_size: usize) -> Self {
        Self { s3, part_size }
    }

    /// The client.
    #[must_use]
    pub fn client(&self) -> &S3 {
        &self.s3
    }

    /// Where listings start.
    #[must_use]
    pub fn home(&self) -> String {
        "/".to_owned()
    }

    fn place(path: &str) -> Place {
        let path = normalize_posix(
            if path.starts_with('/') {
                path.to_owned()
            } else {
                format!("/{path}")
            }
            .as_str(),
        );
        let trimmed = path.trim_start_matches('/');
        match trimmed.split_once('/') {
            _ if trimmed.is_empty() => Place::Root,
            None => Place::Bucket(trimmed.to_owned()),
            Some((bucket, key)) => Place::Key(bucket.to_owned(), key.to_owned()),
        }
    }

    fn name_of(path: &str) -> String {
        normalize_posix(path)
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_owned()
    }

    fn dir(name: String, modified: Option<i64>) -> Entry {
        Entry {
            name,
            kind: Kind::Dir,
            size: 0,
            modified,
            mode: 0o755,
            uid: None,
            gid: None,
            link_target: None,
            target_kind: None,
        }
    }

    fn file(name: String, size: u64, modified: Option<i64>) -> Entry {
        Entry {
            name,
            kind: Kind::File,
            size,
            modified,
            mode: 0o644,
            uid: None,
            gid: None,
            link_target: None,
            target_kind: None,
        }
    }

    /// The folders and objects in `dir` (the buckets at `/`).
    ///
    /// # Errors
    ///
    /// [`FsError`] about `dir`.
    pub async fn list(&self, dir: &str) -> Result<Vec<Entry>, FsError> {
        let fail = |error| fs_error(dir, error);
        let (bucket, prefix) = match Self::place(dir) {
            Place::Root => {
                let buckets = self.s3.buckets().await.map_err(fail)?;
                return Ok(buckets
                    .into_iter()
                    .map(|bucket| Self::dir(bucket.name, bucket.created))
                    .collect());
            }
            Place::Bucket(bucket) => (bucket, String::new()),
            Place::Key(bucket, key) => (bucket, format!("{key}/")),
        };
        let listing = self.s3.list(&bucket, &prefix).await.map_err(fail)?;
        let mut entries: Vec<Entry> = listing
            .folders
            .iter()
            .filter_map(|folder| {
                let name = folder.get(prefix.len()..)?.trim_end_matches('/');
                (!name.is_empty()).then(|| Self::dir(name.to_owned(), None))
            })
            .collect();
        entries.extend(listing.objects.into_iter().filter_map(|object| {
            // The folder's own marker isn't listed in it.
            let name = object.key.get(prefix.len()..)?;
            (!name.is_empty() && !name.contains('/'))
                .then(|| Self::file(name.to_owned(), object.size, object.modified))
        }));
        Ok(entries)
    }

    /// What `path` is.
    ///
    /// # Errors
    ///
    /// [`FsError::NotFound`] when it is neither an object nor a folder.
    pub async fn stat(&self, path: &str) -> Result<Entry, FsError> {
        let fail = |error| fs_error(path, error);
        match Self::place(path) {
            Place::Root => Ok(Self::dir("/".to_owned(), None)),
            Place::Bucket(bucket) => {
                if self.s3.bucket_exists(&bucket).await.map_err(fail)? {
                    Ok(Self::dir(bucket, None))
                } else {
                    Err(FsError::NotFound {
                        path: path.to_owned(),
                    })
                }
            }
            Place::Key(bucket, key) => {
                if let Some(object) = self.s3.head(&bucket, &key).await.map_err(fail)? {
                    return Ok(Self::file(
                        Self::name_of(path),
                        object.size,
                        object.modified,
                    ));
                }
                if self
                    .s3
                    .has_prefix(&bucket, &format!("{key}/"))
                    .await
                    .map_err(fail)?
                {
                    return Ok(Self::dir(Self::name_of(path), None));
                }
                Err(FsError::NotFound {
                    path: path.to_owned(),
                })
            }
        }
    }

    /// A new bucket (at `/`) or folder.
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path` (a bucket name S3 doesn't allow, one that exists).
    pub async fn mkdir(&self, path: &str) -> Result<(), FsError> {
        let fail = |error| fs_error(path, error);
        match Self::place(path) {
            Place::Root => Err(FsError::Exists {
                path: path.to_owned(),
            }),
            Place::Bucket(bucket) => self.s3.create_bucket(&bucket).await.map_err(fail),
            Place::Key(bucket, key) => self
                .s3
                .put(&bucket, &format!("{key}/"), Vec::new())
                .await
                .map_err(fail),
        }
    }

    /// A new empty object.
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn create_file(&self, path: &str) -> Result<(), FsError> {
        let (bucket, key) = Self::key_of(path)?;
        self.s3
            .put(&bucket, &key, Vec::new())
            .await
            .map_err(|error| fs_error(path, error))
    }

    fn key_of(path: &str) -> Result<(String, String), FsError> {
        match Self::place(path) {
            Place::Key(bucket, key) => Ok((bucket, key)),
            Place::Root | Place::Bucket(_) => Err(FsError::Failed {
                path: path.to_owned(),
                message: "files go in a bucket".to_owned(),
            }),
        }
    }

    /// Copies object `from` (of `size` bytes) to `to`, inside the server.
    ///
    /// # Errors
    ///
    /// [`FsError`] about the paths.
    pub async fn copy_file(&self, from: &str, to: &str, size: u64) -> Result<(), FsError> {
        let (from_bucket, from_key) = Self::key_of(from)?;
        let (to_bucket, to_key) = Self::key_of(to)?;
        self.s3
            .copy(&from_bucket, &from_key, &to_bucket, &to_key, size)
            .await
            .map_err(|error| fs_error(to, error))
    }

    /// Renames an object or a folder (a copy, then a delete). Buckets can't be renamed.
    ///
    /// # Errors
    ///
    /// [`FsError`] about the paths; [`FsError::Exists`] when `to` is taken.
    pub async fn rename(&self, from: &str, to: &str) -> Result<(), FsError> {
        let (Place::Key(from_bucket, from_key), Place::Key(to_bucket, to_key)) =
            (Self::place(from), Self::place(to))
        else {
            return Err(FsError::Unsupported {
                what: "renaming buckets".to_owned(),
            });
        };
        if self.exists(&to_bucket, &to_key, to).await? {
            return Err(FsError::Exists {
                path: to.to_owned(),
            });
        }
        let fail = |error| fs_error(from, error);
        if let Some(object) = self.s3.head(&from_bucket, &from_key).await.map_err(fail)? {
            self.s3
                .copy(&from_bucket, &from_key, &to_bucket, &to_key, object.size)
                .await
                .map_err(|error| fs_error(to, error))?;
            return self.s3.delete(&from_bucket, &from_key).await.map_err(fail);
        }
        let prefix = format!("{from_key}/");
        let objects = self
            .s3
            .list_all(&from_bucket, &prefix)
            .await
            .map_err(fail)?;
        if objects.is_empty() {
            return Err(FsError::NotFound {
                path: from.to_owned(),
            });
        }
        for object in &objects {
            let rest = object.key.get(prefix.len()..).unwrap_or_default();
            self.s3
                .copy(
                    &from_bucket,
                    &object.key,
                    &to_bucket,
                    &format!("{to_key}/{rest}"),
                    object.size,
                )
                .await
                .map_err(|error| fs_error(to, error))?;
        }
        let keys: Vec<String> = objects.into_iter().map(|object| object.key).collect();
        self.s3.delete_many(&from_bucket, &keys).await.map_err(fail)
    }

    async fn exists(&self, bucket: &str, key: &str, path: &str) -> Result<bool, FsError> {
        let fail = |error| fs_error(path, error);
        Ok(self.s3.head(bucket, key).await.map_err(fail)?.is_some()
            || self
                .s3
                .has_prefix(bucket, &format!("{key}/"))
                .await
                .map_err(fail)?)
    }

    /// Deletes an object, a folder (with its objects when `recursive`) or a bucket (emptied
    /// first when `recursive`).
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`; a folder or bucket that isn't empty without `recursive`.
    pub async fn remove(&self, path: &str, recursive: bool) -> Result<(), FsError> {
        let fail = |error| fs_error(path, error);
        let (bucket, prefix) = match Self::place(path) {
            Place::Root => {
                return Err(FsError::PermissionDenied {
                    path: path.to_owned(),
                });
            }
            Place::Bucket(bucket) => {
                if recursive {
                    let keys: Vec<String> = self
                        .s3
                        .list_all(&bucket, "")
                        .await
                        .map_err(fail)?
                        .into_iter()
                        .map(|object| object.key)
                        .collect();
                    self.s3.delete_many(&bucket, &keys).await.map_err(fail)?;
                }
                return self.s3.delete_bucket(&bucket).await.map_err(fail);
            }
            Place::Key(bucket, key) => {
                if self.s3.head(&bucket, &key).await.map_err(fail)?.is_some() {
                    return self.s3.delete(&bucket, &key).await.map_err(fail);
                }
                (bucket, format!("{key}/"))
            }
        };
        let keys: Vec<String> = self
            .s3
            .list_all(&bucket, &prefix)
            .await
            .map_err(fail)?
            .into_iter()
            .map(|object| object.key)
            .collect();
        if keys.is_empty() {
            return Err(FsError::NotFound {
                path: path.to_owned(),
            });
        }
        if !recursive && keys.iter().any(|key| *key != prefix) {
            return Err(FsError::Failed {
                path: path.to_owned(),
                message: "the folder isn't empty".to_owned(),
            });
        }
        self.s3.delete_many(&bucket, &keys).await.map_err(fail)
    }

    /// Object `path` from byte `offset` on.
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path`.
    pub async fn open_read(&self, path: &str, offset: u64) -> Result<Reader, FsError> {
        let (bucket, key) = Self::key_of(path)?;
        let reader = self
            .s3
            .read(&bucket, &key, offset)
            .await
            .map_err(|error| fs_error(path, error))?;
        Ok(Box::new(Box::pin(reader)))
    }

    /// An upload of object `path`; it is written when the writer is shut down. S3 can't go on
    /// from an offset.
    ///
    /// # Errors
    ///
    /// [`FsError::Unsupported`] for an offset; [`FsError::Failed`] outside a bucket.
    pub async fn open_write(&self, path: &str, offset: Option<u64>) -> Result<Writer, FsError> {
        if offset.is_some_and(|offset| offset > 0) {
            return Err(FsError::Unsupported {
                what: "resuming an upload into S3".to_owned(),
            });
        }
        let (bucket, key) = Self::key_of(path)?;
        Ok(Box::new(self.s3.writer(&bucket, &key, self.part_size)))
    }

    /// A link that downloads object `path` without the keys, for `lifetime`.
    ///
    /// # Errors
    ///
    /// [`FsError`] about `path` (a lifetime over seven days, a folder).
    pub async fn temporary_link(&self, path: &str, lifetime: Duration) -> Result<String, FsError> {
        let (bucket, key) = Self::key_of(path)?;
        self.s3
            .presign_get(&bucket, &key, lifetime)
            .await
            .map_err(|error| fs_error(path, error))
    }
}

/// An S3 error about `path`.
fn fs_error(path: &str, error: S3Error) -> FsError {
    let path = path.to_owned();
    match error {
        S3Error::NotFound { .. } => FsError::NotFound { path },
        S3Error::Denied { .. } => FsError::PermissionDenied { path },
        S3Error::Exists { .. } => FsError::Exists { path },
        S3Error::Unreachable(_) => FsError::Disconnected,
        S3Error::Timeout => FsError::Timeout,
        S3Error::Cancelled => FsError::Cancelled,
        S3Error::Failed { message, .. } | S3Error::Invalid(message) => {
            FsError::Failed { path, message }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places() {
        assert!(matches!(S3Fs::place("/"), Place::Root));
        assert!(matches!(S3Fs::place(""), Place::Root));
        assert!(matches!(S3Fs::place("/b/"), Place::Bucket(b) if b == "b"));
        assert!(matches!(S3Fs::place("/b/x/../y//z"), Place::Key(b, k) if b == "b" && k == "y/z"));
        assert_eq!(S3Fs::name_of("/b/a/c.txt"), "c.txt");
    }
}
