//! S3 storage (PLAN Sprint 12, ADR 0033), for AWS and the servers that speak its API (MinIO,
//! RustFS, Ceph, Garage...), through the official AWS SDK. This crate never depends on Qt.
//!
//! - [`S3`]: a client for one endpoint and one pair of keys: buckets, folders (`ListObjectsV2`
//!   with the `/` delimiter), reading with ranges, writing ([`S3::writer`]: one `PutObject` for a
//!   small file, else a multipart upload in 32 MB parts), copying, deleting, new folders (an
//!   empty object ending in `/`) and temporary links (presigned URLs).
//! - [`testing`]: a small in-process S3 server for tests and the app's smoke test.
//!
//! The keys come from the caller (the app's vault): nothing is read from `~/.aws` or the
//! environment, and the secret key is never logged or put in an error. Checksums are only sent
//! where the API requires them, which the S3-compatible servers all accept.

pub mod testing;
mod writer;

use std::time::Duration;

use aws_sdk_s3::Client;
use aws_sdk_s3::config::{
    BehaviorVersion, Credentials, Region, RequestChecksumCalculation, ResponseChecksumValidation,
};
use aws_sdk_s3::error::{DisplayErrorContext, ProvideErrorMetadata, SdkError};
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{
    BucketLocationConstraint, CompletedMultipartUpload, CompletedPart, CreateBucketConfiguration,
    Delete, ObjectIdentifier,
};
use aws_smithy_http_client::tls::{self, rustls_provider::CryptoMode};
use secrecy::{ExposeSecret, SecretString};
use tokio::io::AsyncRead;

pub use writer::S3Writer;

/// The region when none is given.
pub const DEFAULT_REGION: &str = "us-east-1";

/// The size of an upload's parts: files up to this size go in one request.
pub const PART_SIZE: usize = 32 * 1024 * 1024;

/// The largest object `CopyObject` copies in one request; bigger ones are copied in parts.
const COPY_LIMIT: u64 = 5 * 1024 * 1024 * 1024;

/// The parts of a big copy.
const COPY_PART: u64 = 512 * 1024 * 1024;

/// The longest a temporary link may last (what SigV4 allows).
pub const MAX_LINK_LIFETIME: Duration = Duration::from_secs(7 * 24 * 3600);

/// Where to connect, and as whom.
#[derive(Clone)]
pub struct S3Spec {
    /// `https://host[:port]` or `http://host[:port]`; a bare host means HTTPS; empty is AWS (the
    /// region's endpoint).
    pub endpoint: String,
    /// The region (empty: [`DEFAULT_REGION`]).
    pub region: String,
    /// Buckets in the path (`host/bucket/key`) rather than in the host name
    /// (`bucket.host/key`): what most S3-compatible servers want.
    pub path_style: bool,
    /// The access key id.
    pub access_key: String,
    /// The secret key.
    pub secret_key: SecretString,
}

impl std::fmt::Debug for S3Spec {
    /// The secret key is left out.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Spec")
            .field("endpoint", &self.endpoint)
            .field("region", &self.region)
            .field("path_style", &self.path_style)
            .field("access_key", &self.access_key)
            .finish_non_exhaustive()
    }
}

/// Why an S3 operation failed. Messages name buckets and keys, never the secret key.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum S3Error {
    /// No such bucket or object.
    #[error("{what}: not found")]
    NotFound {
        /// The bucket or object.
        what: String,
    },
    /// The keys may not do this (or are wrong).
    #[error("{what}: access denied ({message})")]
    Denied {
        /// The bucket or object.
        what: String,
        /// What the server said.
        message: String,
    },
    /// The bucket exists already.
    #[error("{what}: already exists")]
    Exists {
        /// The bucket.
        what: String,
    },
    /// The server couldn't be reached.
    #[error("could not reach the server: {0}")]
    Unreachable(String),
    /// The server didn't answer in time.
    #[error("the server didn't answer in time")]
    Timeout,
    /// The server refused for another reason.
    #[error("{what}: {message}")]
    Failed {
        /// The bucket or object.
        what: String,
        /// The server's error code and message.
        message: String,
    },
    /// A setting or a name that can't be used.
    #[error("{0}")]
    Invalid(String),
    /// The upload was abandoned.
    #[error("cancelled")]
    Cancelled,
}

impl S3Error {
    /// Whether trying again later can work.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Unreachable(_) | Self::Timeout)
    }
}

/// An SDK error about `what`.
fn sdk_error<E>(what: &str, error: &SdkError<E>) -> S3Error
where
    E: ProvideErrorMetadata + std::error::Error + Send + Sync + 'static,
{
    let what = what.to_owned();
    match error {
        SdkError::TimeoutError(_) => S3Error::Timeout,
        SdkError::DispatchFailure(failure) if failure.is_timeout() => S3Error::Timeout,
        SdkError::DispatchFailure(_) => {
            S3Error::Unreachable(DisplayErrorContext(error).to_string())
        }
        SdkError::ServiceError(service) => {
            let status = service.raw().status().as_u16();
            let code = service.err().code().unwrap_or_default();
            let message = service
                .err()
                .message()
                .map_or_else(|| format!("HTTP {status}"), str::to_owned);
            match (status, code) {
                (404, _) | (_, "NoSuchKey" | "NoSuchBucket" | "NotFound") => {
                    S3Error::NotFound { what }
                }
                (401 | 403, _)
                | (_, "AccessDenied" | "InvalidAccessKeyId" | "SignatureDoesNotMatch") => {
                    S3Error::Denied { what, message }
                }
                (_, "BucketAlreadyExists" | "BucketAlreadyOwnedByYou") => S3Error::Exists { what },
                _ => S3Error::Failed {
                    what,
                    message: if code.is_empty() {
                        message
                    } else {
                        format!("{code}: {message}")
                    },
                },
            }
        }
        _ => S3Error::Failed {
            what,
            message: DisplayErrorContext(error).to_string(),
        },
    }
}

/// `text` as an endpoint URL (`None` for AWS's own).
///
/// # Errors
///
/// [`S3Error::Invalid`] when it isn't an `http` or `https` URL with a host.
pub fn endpoint_url(text: &str) -> Result<Option<String>, S3Error> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let (scheme, rest) = match text.split_once("://") {
        Some((scheme, rest)) => (scheme.to_ascii_lowercase(), rest),
        None => ("https".to_owned(), text),
    };
    if scheme != "http" && scheme != "https" {
        return Err(S3Error::Invalid(format!(
            "{text}: use an http:// or https:// address"
        )));
    }
    let rest = rest.trim_end_matches('/');
    let host = rest.split('/').next().unwrap_or_default();
    if host.is_empty() || host.starts_with('-') || host.chars().any(char::is_whitespace) {
        return Err(S3Error::Invalid(format!("{text}: no server address")));
    }
    if rest.len() > host.len() {
        return Err(S3Error::Invalid(format!(
            "{text}: the address is the server only (buckets are chosen in the view)"
        )));
    }
    Ok(Some(format!("{scheme}://{host}")))
}

/// Whether `name` can be a bucket name (3 to 63 lowercase letters, digits, dots and hyphens,
/// starting and ending with a letter or digit).
#[must_use]
pub fn is_bucket_name(name: &str) -> bool {
    let edge = |c: Option<char>| c.is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    (3..=63).contains(&name.len())
        && edge(name.chars().next())
        && edge(name.chars().last())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
}

/// `key` for a URL path or `x-amz-copy-source`: percent-encoded except unreserved characters
/// and `/`.
#[must_use]
pub fn encode_key(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    for byte in key.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The content type a browser wants for `key` (temporary links open in one); `None` leaves the
/// server's default.
#[must_use]
pub fn content_type(key: &str) -> Option<&'static str> {
    let extension = key.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match extension.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "txt" | "log" | "md" | "csv" => "text/plain; charset=utf-8",
        "json" => "application/json",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "zip" => "application/zip",
        _ => return None,
    })
}

/// A bucket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bucket {
    /// Its name.
    pub name: String,
    /// When it was made, in seconds since the Unix epoch.
    pub created: Option<i64>,
}

/// An object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object {
    /// Its key.
    pub key: String,
    /// Size in bytes.
    pub size: u64,
    /// Last modified, in seconds since the Unix epoch.
    pub modified: Option<i64>,
}

/// One level of a bucket.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing {
    /// The folders (common prefixes, each ending in `/`).
    pub folders: Vec<String>,
    /// The objects.
    pub objects: Vec<Object>,
}

/// A client for one endpoint and one pair of keys. Cheap to clone.
#[derive(Debug, Clone)]
pub struct S3 {
    client: Client,
    region: String,
}

impl S3 {
    /// A client for `spec`. Nothing is sent until an operation runs.
    ///
    /// # Errors
    ///
    /// [`S3Error::Invalid`] for a bad endpoint or missing keys.
    pub fn new(spec: &S3Spec) -> Result<Self, S3Error> {
        let endpoint = endpoint_url(&spec.endpoint)?;
        if spec.access_key.trim().is_empty() {
            return Err(S3Error::Invalid("no access key".to_owned()));
        }
        let region = if spec.region.trim().is_empty() {
            DEFAULT_REGION.to_owned()
        } else {
            spec.region.trim().to_owned()
        };
        let credentials = Credentials::new(
            spec.access_key.trim(),
            spec.secret_key.expose_secret(),
            None,
            None,
            "opensesh",
        );
        let http = aws_smithy_http_client::Builder::new()
            .tls_provider(tls::Provider::Rustls(CryptoMode::Ring))
            .build_https();
        let mut config = aws_sdk_s3::config::Builder::new()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new(region.clone()))
            .credentials_provider(credentials)
            .http_client(http)
            .force_path_style(spec.path_style)
            .request_checksum_calculation(RequestChecksumCalculation::WhenRequired)
            .response_checksum_validation(ResponseChecksumValidation::WhenRequired)
            .timeout_config(
                aws_sdk_s3::config::timeout::TimeoutConfig::builder()
                    .connect_timeout(Duration::from_secs(15))
                    .read_timeout(Duration::from_secs(120))
                    .build(),
            );
        if let Some(endpoint) = endpoint {
            config = config.endpoint_url(endpoint);
        }
        Ok(Self {
            client: Client::from_conf(config.build()),
            region,
        })
    }

    /// The buckets the keys can see.
    ///
    /// # Errors
    ///
    /// [`S3Error`] from the server.
    pub async fn buckets(&self) -> Result<Vec<Bucket>, S3Error> {
        let mut buckets = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let page = self
                .client
                .list_buckets()
                .set_continuation_token(token.take())
                .send()
                .await
                .map_err(|error| sdk_error("the buckets", &error))?;
            buckets.extend(page.buckets().iter().filter_map(|bucket| {
                Some(Bucket {
                    name: bucket.name()?.to_owned(),
                    created: bucket.creation_date().map(|date| date.secs()),
                })
            }));
            match page.continuation_token() {
                Some(next) if !next.is_empty() => token = Some(next.to_owned()),
                _ => break,
            }
        }
        Ok(buckets)
    }

    /// The folders and objects right under `prefix` (empty, or ending in `/`) in `bucket`.
    ///
    /// # Errors
    ///
    /// [`S3Error`] from the server.
    pub async fn list(&self, bucket: &str, prefix: &str) -> Result<Listing, S3Error> {
        let mut listing = Listing::default();
        self.each_page(bucket, prefix, Some("/"), |page| {
            listing.folders.extend(
                page.common_prefixes()
                    .iter()
                    .filter_map(|folder| folder.prefix().map(str::to_owned)),
            );
            listing.objects.extend(objects(page));
        })
        .await?;
        Ok(listing)
    }

    /// Every object under `prefix`, at any depth.
    ///
    /// # Errors
    ///
    /// [`S3Error`] from the server.
    pub async fn list_all(&self, bucket: &str, prefix: &str) -> Result<Vec<Object>, S3Error> {
        let mut all = Vec::new();
        self.each_page(bucket, prefix, None, |page| all.extend(objects(page)))
            .await?;
        Ok(all)
    }

    /// Whether anything is under `prefix`.
    ///
    /// # Errors
    ///
    /// [`S3Error`] from the server.
    pub async fn has_prefix(&self, bucket: &str, prefix: &str) -> Result<bool, S3Error> {
        let page = self
            .client
            .list_objects_v2()
            .bucket(bucket)
            .prefix(prefix)
            .max_keys(1)
            .send()
            .await
            .map_err(|error| sdk_error(&format!("{bucket}/{prefix}"), &error))?;
        Ok(!page.contents().is_empty())
    }

    async fn each_page(
        &self,
        bucket: &str,
        prefix: &str,
        delimiter: Option<&str>,
        mut each: impl FnMut(&aws_sdk_s3::operation::list_objects_v2::ListObjectsV2Output),
    ) -> Result<(), S3Error> {
        let mut token: Option<String> = None;
        loop {
            let page = self
                .client
                .list_objects_v2()
                .bucket(bucket)
                .prefix(prefix)
                .set_delimiter(delimiter.map(str::to_owned))
                .set_continuation_token(token.take())
                .send()
                .await
                .map_err(|error| sdk_error(&format!("{bucket}/{prefix}"), &error))?;
            each(&page);
            match (page.is_truncated(), page.next_continuation_token()) {
                (Some(true), Some(next)) => token = Some(next.to_owned()),
                _ => return Ok(()),
            }
        }
    }

    /// Whether `bucket` exists (and the keys may see it).
    ///
    /// # Errors
    ///
    /// [`S3Error`] other than "not found".
    pub async fn bucket_exists(&self, bucket: &str) -> Result<bool, S3Error> {
        match self.client.head_bucket().bucket(bucket).send().await {
            Ok(_) => Ok(true),
            Err(error) => match sdk_error(bucket, &error) {
                S3Error::NotFound { .. } => Ok(false),
                other => Err(other),
            },
        }
    }

    /// Object `key`, or `None` when there is none.
    ///
    /// # Errors
    ///
    /// [`S3Error`] other than "not found".
    pub async fn head(&self, bucket: &str, key: &str) -> Result<Option<Object>, S3Error> {
        match self
            .client
            .head_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
        {
            Ok(head) => Ok(Some(Object {
                key: key.to_owned(),
                size: head
                    .content_length()
                    .and_then(|size| u64::try_from(size).ok())
                    .unwrap_or(0),
                modified: head.last_modified().map(|date| date.secs()),
            })),
            Err(error) => match sdk_error(&format!("{bucket}/{key}"), &error) {
                S3Error::NotFound { .. } => Ok(None),
                other => Err(other),
            },
        }
    }

    /// Object `key` from byte `offset` on.
    ///
    /// # Errors
    ///
    /// [`S3Error`] from the server.
    pub async fn read(
        &self,
        bucket: &str,
        key: &str,
        offset: u64,
    ) -> Result<impl AsyncRead + Send + 'static, S3Error> {
        let output = self
            .client
            .get_object()
            .bucket(bucket)
            .key(key)
            .set_range((offset > 0).then(|| format!("bytes={offset}-")))
            .send()
            .await
            .map_err(|error| sdk_error(&format!("{bucket}/{key}"), &error))?;
        Ok(output.body.into_async_read())
    }

    /// Writes `body` as object `key` in one request.
    ///
    /// # Errors
    ///
    /// [`S3Error`] from the server.
    pub async fn put(&self, bucket: &str, key: &str, body: Vec<u8>) -> Result<(), S3Error> {
        self.client
            .put_object()
            .bucket(bucket)
            .key(key)
            .set_content_type(content_type(key).map(str::to_owned))
            .body(ByteStream::from(body))
            .send()
            .await
            .map_err(|error| sdk_error(&format!("{bucket}/{key}"), &error))?;
        Ok(())
    }

    /// A writer that uploads object `key`: one request for up to `part_size` bytes, else a
    /// multipart upload in parts of `part_size` ([`PART_SIZE`] unless a test says otherwise).
    /// Shutting it down finishes the upload and reports how it went; dropping it before
    /// abandons it (nothing is left on the server).
    #[must_use]
    pub fn writer(&self, bucket: &str, key: &str, part_size: usize) -> S3Writer {
        S3Writer::start(self.clone(), bucket.to_owned(), key.to_owned(), part_size)
    }

    /// Starts a multipart upload of `key`: its id.
    ///
    /// # Errors
    ///
    /// [`S3Error`] from the server.
    pub async fn create_multipart(&self, bucket: &str, key: &str) -> Result<String, S3Error> {
        let what = format!("{bucket}/{key}");
        let output = self
            .client
            .create_multipart_upload()
            .bucket(bucket)
            .key(key)
            .set_content_type(content_type(key).map(str::to_owned))
            .send()
            .await
            .map_err(|error| sdk_error(&what, &error))?;
        output
            .upload_id()
            .map(str::to_owned)
            .ok_or(S3Error::Failed {
                what,
                message: "no upload id".to_owned(),
            })
    }

    /// Uploads part `number` (from 1): its ETag.
    ///
    /// # Errors
    ///
    /// [`S3Error`] from the server.
    pub async fn upload_part(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
        number: i32,
        body: Vec<u8>,
    ) -> Result<String, S3Error> {
        let what = format!("{bucket}/{key}");
        let output = self
            .client
            .upload_part()
            .bucket(bucket)
            .key(key)
            .upload_id(upload_id)
            .part_number(number)
            .body(ByteStream::from(body))
            .send()
            .await
            .map_err(|error| sdk_error(&what, &error))?;
        output.e_tag().map(str::to_owned).ok_or(S3Error::Failed {
            what,
            message: format!("no ETag for part {number}"),
        })
    }

    /// Finishes a multipart upload with its `(number, ETag)` parts.
    ///
    /// # Errors
    ///
    /// [`S3Error`] from the server.
    pub async fn complete_multipart(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
        parts: Vec<(i32, String)>,
    ) -> Result<(), S3Error> {
        let parts = parts
            .into_iter()
            .map(|(number, tag)| {
                CompletedPart::builder()
                    .part_number(number)
                    .e_tag(tag)
                    .build()
            })
            .collect();
        self.client
            .complete_multipart_upload()
            .bucket(bucket)
            .key(key)
            .upload_id(upload_id)
            .multipart_upload(
                CompletedMultipartUpload::builder()
                    .set_parts(Some(parts))
                    .build(),
            )
            .send()
            .await
            .map_err(|error| sdk_error(&format!("{bucket}/{key}"), &error))?;
        Ok(())
    }

    /// Abandons a multipart upload (its parts are deleted).
    ///
    /// # Errors
    ///
    /// [`S3Error`] from the server.
    pub async fn abort_multipart(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
    ) -> Result<(), S3Error> {
        self.client
            .abort_multipart_upload()
            .bucket(bucket)
            .key(key)
            .upload_id(upload_id)
            .send()
            .await
            .map_err(|error| sdk_error(&format!("{bucket}/{key}"), &error))?;
        Ok(())
    }

    /// Copies object `from` of `bucket` (of `size` bytes) to `to` in `to_bucket`, inside the
    /// server; above 5 GiB in parts.
    ///
    /// # Errors
    ///
    /// [`S3Error`] from the server.
    pub async fn copy(
        &self,
        bucket: &str,
        from: &str,
        to_bucket: &str,
        to: &str,
        size: u64,
    ) -> Result<(), S3Error> {
        let source = format!("{bucket}/{}", encode_key(from));
        let what = format!("{to_bucket}/{to}");
        if size <= COPY_LIMIT {
            self.client
                .copy_object()
                .bucket(to_bucket)
                .key(to)
                .copy_source(source)
                .send()
                .await
                .map_err(|error| sdk_error(&what, &error))?;
            return Ok(());
        }
        let upload_id = self.create_multipart(to_bucket, to).await?;
        let copied = async {
            let mut parts = Vec::new();
            let mut start = 0;
            let mut number = 1;
            while start < size {
                let end = (start + COPY_PART).min(size) - 1;
                let output = self
                    .client
                    .upload_part_copy()
                    .bucket(to_bucket)
                    .key(to)
                    .upload_id(&upload_id)
                    .part_number(number)
                    .copy_source(&source)
                    .copy_source_range(format!("bytes={start}-{end}"))
                    .send()
                    .await
                    .map_err(|error| sdk_error(&what, &error))?;
                let tag = output
                    .copy_part_result()
                    .and_then(|result| result.e_tag())
                    .map(str::to_owned)
                    .ok_or(S3Error::Failed {
                        what: what.clone(),
                        message: format!("no ETag for part {number}"),
                    })?;
                parts.push((number, tag));
                start = end + 1;
                number += 1;
            }
            self.complete_multipart(to_bucket, to, &upload_id, parts)
                .await
        };
        let result = copied.await;
        if result.is_err() {
            let _ = self.abort_multipart(to_bucket, to, &upload_id).await;
        }
        result
    }

    /// Deletes object `key`.
    ///
    /// # Errors
    ///
    /// [`S3Error`] from the server.
    pub async fn delete(&self, bucket: &str, key: &str) -> Result<(), S3Error> {
        self.client
            .delete_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|error| sdk_error(&format!("{bucket}/{key}"), &error))?;
        Ok(())
    }

    /// Deletes `keys`, a thousand to a request.
    ///
    /// # Errors
    ///
    /// The first [`S3Error`]: of a request, or of a key the server didn't delete.
    pub async fn delete_many(&self, bucket: &str, keys: &[String]) -> Result<(), S3Error> {
        for chunk in keys.chunks(1000) {
            let objects = chunk
                .iter()
                .map(|key| {
                    ObjectIdentifier::builder()
                        .key(key)
                        .build()
                        .map_err(|error| S3Error::Invalid(error.to_string()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let delete = Delete::builder()
                .set_objects(Some(objects))
                .quiet(true)
                .build()
                .map_err(|error| S3Error::Invalid(error.to_string()))?;
            let output = self
                .client
                .delete_objects()
                .bucket(bucket)
                .delete(delete)
                .send()
                .await
                .map_err(|error| sdk_error(bucket, &error))?;
            if let Some(failed) = output.errors().first() {
                return Err(S3Error::Failed {
                    what: format!("{bucket}/{}", failed.key().unwrap_or_default()),
                    message: failed
                        .message()
                        .or(failed.code())
                        .unwrap_or("not deleted")
                        .to_owned(),
                });
            }
        }
        Ok(())
    }

    /// Makes bucket `name` (in the client's region).
    ///
    /// # Errors
    ///
    /// [`S3Error::Invalid`] for a name S3 doesn't allow, else [`S3Error`] from the server.
    pub async fn create_bucket(&self, name: &str) -> Result<(), S3Error> {
        if !is_bucket_name(name) {
            return Err(S3Error::Invalid(format!(
                "{name}: a bucket name is 3 to 63 lowercase letters, digits, dots and hyphens"
            )));
        }
        // us-east-1 is the one region that takes no location.
        let configuration = (self.region != DEFAULT_REGION).then(|| {
            CreateBucketConfiguration::builder()
                .location_constraint(BucketLocationConstraint::from(self.region.as_str()))
                .build()
        });
        self.client
            .create_bucket()
            .bucket(name)
            .set_create_bucket_configuration(configuration)
            .send()
            .await
            .map_err(|error| sdk_error(name, &error))?;
        Ok(())
    }

    /// Deletes bucket `name` (it must be empty).
    ///
    /// # Errors
    ///
    /// [`S3Error`] from the server.
    pub async fn delete_bucket(&self, name: &str) -> Result<(), S3Error> {
        self.client
            .delete_bucket()
            .bucket(name)
            .send()
            .await
            .map_err(|error| sdk_error(name, &error))?;
        Ok(())
    }

    /// A link that downloads object `key` without the keys, for `lifetime` (at most
    /// [`MAX_LINK_LIFETIME`]). It is signed here: nothing is sent.
    ///
    /// # Errors
    ///
    /// [`S3Error::Invalid`] for a lifetime of zero or over seven days.
    pub async fn presign_get(
        &self,
        bucket: &str,
        key: &str,
        lifetime: Duration,
    ) -> Result<String, S3Error> {
        if lifetime.is_zero() || lifetime > MAX_LINK_LIFETIME {
            return Err(S3Error::Invalid(
                "a temporary link lasts from a second to seven days".to_owned(),
            ));
        }
        let config = PresigningConfig::expires_in(lifetime)
            .map_err(|error| S3Error::Invalid(error.to_string()))?;
        let request = self
            .client
            .get_object()
            .bucket(bucket)
            .key(key)
            .presigned(config)
            .await
            .map_err(|error| sdk_error(&format!("{bucket}/{key}"), &error))?;
        Ok(request.uri().to_owned())
    }
}

fn objects(
    page: &aws_sdk_s3::operation::list_objects_v2::ListObjectsV2Output,
) -> impl Iterator<Item = Object> + '_ {
    page.contents().iter().filter_map(|object| {
        Some(Object {
            key: object.key()?.to_owned(),
            size: object
                .size()
                .and_then(|size| u64::try_from(size).ok())
                .unwrap_or(0),
            modified: object.last_modified().map(|date| date.secs()),
        })
    })
}

#[cfg(test)]
mod tests;
