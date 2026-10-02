//! A small S3 server for tests and the app's smoke test (which must not reach the network). It
//! keeps buckets and objects in memory and speaks path-style requests over plain HTTP on
//! 127.0.0.1. It checks that each request is signed with [`ACCESS_KEY`] (in the `Authorization`
//! header or a presigned link's query), not the signature itself.
//!
//! What it answers: listing buckets and objects (`ListObjectsV2` with prefixes, the `/` delimiter
//! and pages), making and deleting buckets, reading objects (with ranges), writing them (in one
//! request or a multipart upload, copies included), deleting one or many. Nothing here runs
//! unless a test or the smoke test starts it.

use std::collections::{BTreeMap, HashMap};
use std::convert::Infallible;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use aws_sdk_s3::primitives::{DateTime, DateTimeFormat};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{HeaderMap, Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use secrecy::SecretString;
use tokio::net::TcpListener;

use crate::S3Spec;

/// The access key the server knows.
pub const ACCESS_KEY: &str = "OPENSESHTESTKEY";
/// Its secret key (not checked: the server doesn't verify signatures).
pub const SECRET_KEY: &str = "opensesh-test-secret";

/// What a test server starts with.
#[derive(Debug, Clone, Default)]
pub struct Rules {
    /// Buckets made at the start.
    pub buckets: Vec<String>,
    /// Entries per listing page (0: 1000, as S3).
    pub page_size: usize,
}

/// A running test server.
#[derive(Debug, Clone)]
pub struct TestServer {
    /// Its port on 127.0.0.1.
    pub port: u16,
    store: Arc<Mutex<Store>>,
}

impl TestServer {
    /// Its address.
    #[must_use]
    pub fn endpoint(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// A spec that reaches it with the right keys.
    #[must_use]
    pub fn spec(&self) -> S3Spec {
        S3Spec {
            endpoint: self.endpoint(),
            region: String::new(),
            path_style: true,
            access_key: ACCESS_KEY.to_owned(),
            secret_key: SecretString::from(SECRET_KEY.to_owned()),
        }
    }

    /// The bytes of object `key` in `bucket`.
    #[must_use]
    pub fn object(&self, bucket: &str, key: &str) -> Option<Vec<u8>> {
        lock(&self.store)
            .buckets
            .get(bucket)
            .and_then(|objects| objects.get(key))
            .map(|stored| stored.data.as_ref().clone())
    }

    /// How many multipart uploads are started and neither finished nor aborted.
    #[must_use]
    pub fn uploads_in_progress(&self) -> usize {
        lock(&self.store).uploads.len()
    }

    /// The keys of `bucket`, in order.
    #[must_use]
    pub fn keys(&self, bucket: &str) -> Vec<String> {
        lock(&self.store)
            .buckets
            .get(bucket)
            .map(|objects| objects.keys().cloned().collect())
            .unwrap_or_default()
    }
}

/// Starts a server on 127.0.0.1 (a free port) on the current tokio runtime.
///
/// # Errors
///
/// When the port can't be bound.
pub async fn serve(rules: Rules) -> std::io::Result<TestServer> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = listener.local_addr()?.port();
    let mut store = Store {
        page_size: if rules.page_size == 0 {
            1000
        } else {
            rules.page_size
        },
        ..Store::default()
    };
    for bucket in rules.buckets {
        store.buckets.insert(bucket, BTreeMap::new());
    }
    let store = Arc::new(Mutex::new(store));
    let shared = Arc::clone(&store);
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let store = Arc::clone(&shared);
            tokio::spawn(async move {
                let service = service_fn(move |request| {
                    let store = Arc::clone(&store);
                    async move { Ok::<_, Infallible>(handle(&store, request).await) }
                });
                let _ = http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
        }
    });
    Ok(TestServer { port, store })
}

#[derive(Debug)]
struct Stored {
    data: Arc<Vec<u8>>,
    modified: i64,
    tag: String,
}

#[derive(Debug)]
struct Pending {
    bucket: String,
    key: String,
    parts: BTreeMap<i32, (String, Vec<u8>)>,
}

#[derive(Debug, Default)]
struct Store {
    buckets: BTreeMap<String, BTreeMap<String, Stored>>,
    uploads: HashMap<String, Pending>,
    next: u64,
    page_size: usize,
}

impl Store {
    fn tag(&mut self) -> String {
        self.next += 1;
        format!("\"{:032x}\"", self.next)
    }
}

fn lock(store: &Mutex<Store>) -> MutexGuard<'_, Store> {
    store.lock().unwrap_or_else(PoisonError::into_inner)
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| i64::try_from(elapsed.as_secs()).unwrap_or(0))
}

fn date(secs: i64, format: DateTimeFormat) -> String {
    DateTime::from_secs(secs).fmt(format).unwrap_or_default()
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%'
            && let Some(hex) = text.get(index + 1..index + 3)
            && let Ok(value) = u8::from_str_radix(hex, 16)
        {
            out.push(value);
            index += 3;
            continue;
        }
        out.push(if byte == b'+' { b' ' } else { byte });
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The texts of every `<tag>...</tag>` in `xml`.
fn elements(xml: &str, tag: &str) -> Vec<String> {
    let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
    let mut found = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        let after = &rest[start + open.len()..];
        let Some(end) = after.find(&close) else {
            break;
        };
        found.push(unescape(&after[..end]));
        rest = &after[end + close.len()..];
    }
    found
}

type Reply = Response<Full<Bytes>>;

fn reply(status: StatusCode, headers: &[(&str, String)], body: impl Into<Bytes>) -> Reply {
    let mut response = Response::new(Full::new(body.into()));
    *response.status_mut() = status;
    for (name, value) in headers {
        if let (Ok(name), Ok(value)) = (
            hyper::header::HeaderName::from_bytes(name.as_bytes()),
            hyper::header::HeaderValue::from_str(value),
        ) {
            response.headers_mut().insert(name, value);
        }
    }
    response
}

fn xml(status: StatusCode, body: &str) -> Reply {
    reply(
        status,
        &[("content-type", "application/xml".to_owned())],
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{body}"),
    )
}

fn error(status: StatusCode, code: &str, message: &str) -> Reply {
    xml(
        status,
        &format!(
            "<Error><Code>{code}</Code><Message>{}</Message><RequestId>1</RequestId></Error>",
            escape(message)
        ),
    )
}

fn no_bucket() -> Reply {
    error(
        StatusCode::NOT_FOUND,
        "NoSuchBucket",
        "The specified bucket does not exist.",
    )
}

fn no_key() -> Reply {
    error(
        StatusCode::NOT_FOUND,
        "NoSuchKey",
        "The specified key does not exist.",
    )
}

fn authorized(headers: &HeaderMap, query: &HashMap<String, String>) -> bool {
    let credential = format!("{ACCESS_KEY}/");
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains(&format!("Credential={credential}")))
        || query
            .get("X-Amz-Credential")
            .is_some_and(|value| value.starts_with(&credential))
}

async fn handle(store: &Mutex<Store>, request: Request<Incoming>) -> Reply {
    let method = request.method().clone();
    let headers = request.headers().clone();
    let path = request.uri().path().to_owned();
    let query: HashMap<String, String> = request
        .uri()
        .query()
        .unwrap_or_default()
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            (decode(name), decode(value))
        })
        .collect();
    if !authorized(&headers, &query) {
        return error(
            StatusCode::FORBIDDEN,
            "InvalidAccessKeyId",
            "The access key you provided does not exist in our records.",
        );
    }
    let body = match request.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => Bytes::new(),
    };
    let trimmed = path.trim_start_matches('/');
    let (bucket, key) = match trimmed.split_once('/') {
        Some((bucket, key)) => (decode(bucket), decode(key)),
        None => (decode(trimmed), String::new()),
    };
    let mut store = lock(store);
    if bucket.is_empty() {
        return if method == Method::GET {
            list_buckets(&store)
        } else {
            error(StatusCode::METHOD_NOT_ALLOWED, "MethodNotAllowed", "no")
        };
    }
    if key.is_empty() {
        return bucket_request(&mut store, &method, &bucket, &query, &body);
    }
    if !store.buckets.contains_key(&bucket) {
        return no_bucket();
    }
    object_request(&mut store, &method, &headers, &bucket, &key, &query, body)
}

fn list_buckets(store: &Store) -> Reply {
    let created = date(now(), DateTimeFormat::DateTime);
    let buckets: String = store
        .buckets
        .keys()
        .map(|name| {
            format!(
                "<Bucket><Name>{}</Name><CreationDate>{created}</CreationDate></Bucket>",
                escape(name)
            )
        })
        .collect();
    xml(
        StatusCode::OK,
        &format!(
            "<ListAllMyBucketsResult><Owner><ID>tester</ID></Owner><Buckets>{buckets}</Buckets></ListAllMyBucketsResult>"
        ),
    )
}

fn bucket_request(
    store: &mut Store,
    method: &Method,
    bucket: &str,
    query: &HashMap<String, String>,
    body: &Bytes,
) -> Reply {
    let exists = store.buckets.contains_key(bucket);
    match *method {
        Method::PUT if exists => error(
            StatusCode::CONFLICT,
            "BucketAlreadyOwnedByYou",
            "Your previous request to create the named bucket succeeded.",
        ),
        Method::PUT => {
            store.buckets.insert(bucket.to_owned(), BTreeMap::new());
            reply(StatusCode::OK, &[("location", format!("/{bucket}"))], "")
        }
        _ if !exists => no_bucket(),
        Method::HEAD => reply(StatusCode::OK, &[], ""),
        Method::DELETE => {
            if store
                .buckets
                .get(bucket)
                .is_some_and(|objects| !objects.is_empty())
            {
                error(
                    StatusCode::CONFLICT,
                    "BucketNotEmpty",
                    "The bucket you tried to delete is not empty.",
                )
            } else {
                store.buckets.remove(bucket);
                reply(StatusCode::NO_CONTENT, &[], "")
            }
        }
        Method::POST if query.contains_key("delete") => {
            let text = String::from_utf8_lossy(body);
            if let Some(objects) = store.buckets.get_mut(bucket) {
                for key in elements(&text, "Key") {
                    objects.remove(&key);
                }
            }
            xml(StatusCode::OK, "<DeleteResult></DeleteResult>")
        }
        Method::GET => list_objects(store, bucket, query),
        _ => error(StatusCode::METHOD_NOT_ALLOWED, "MethodNotAllowed", "no"),
    }
}

fn list_objects(store: &Store, bucket: &str, query: &HashMap<String, String>) -> Reply {
    let Some(objects) = store.buckets.get(bucket) else {
        return no_bucket();
    };
    let prefix = query.get("prefix").cloned().unwrap_or_default();
    let delimiter = query.get("delimiter").cloned().unwrap_or_default();
    let max = query
        .get("max-keys")
        .and_then(|max| max.parse::<usize>().ok())
        .unwrap_or(1000)
        .min(store.page_size)
        .max(1);
    // The token is the last entry of the previous page (a key, or a folder ending in the
    // delimiter whose keys are all skipped).
    let after = query.get("continuation-token").cloned().unwrap_or_default();
    let mut contents = String::new();
    let mut folders: Vec<String> = Vec::new();
    let mut count = 0;
    let mut last = String::new();
    let mut truncated = false;
    for (key, stored) in objects.range(prefix.clone()..) {
        if !key.starts_with(&prefix) {
            break;
        }
        if !after.is_empty()
            && (key.as_str() <= after.as_str()
                || (!delimiter.is_empty()
                    && after.ends_with(&delimiter)
                    && key.starts_with(&after)))
        {
            continue;
        }
        let rest = &key[prefix.len()..];
        let folder = (!delimiter.is_empty())
            .then(|| rest.find(&delimiter))
            .flatten()
            .map(|at| format!("{prefix}{}", &rest[..at + delimiter.len()]));
        if let Some(folder) = &folder
            && folders.last() == Some(folder)
        {
            continue;
        }
        if count == max {
            truncated = true;
            break;
        }
        count += 1;
        match folder {
            Some(folder) => {
                last.clone_from(&folder);
                folders.push(folder);
            }
            None => {
                last.clone_from(key);
                contents.push_str(&format!(
                    "<Contents><Key>{}</Key><LastModified>{}</LastModified><ETag>{}</ETag><Size>{}</Size><StorageClass>STANDARD</StorageClass></Contents>",
                    escape(key),
                    date(stored.modified, DateTimeFormat::DateTime),
                    escape(&stored.tag),
                    stored.data.len()
                ));
            }
        }
    }
    let prefixes: String = folders
        .iter()
        .map(|folder| {
            format!(
                "<CommonPrefixes><Prefix>{}</Prefix></CommonPrefixes>",
                escape(folder)
            )
        })
        .collect();
    let next = if truncated {
        format!(
            "<NextContinuationToken>{}</NextContinuationToken>",
            escape(&last)
        )
    } else {
        String::new()
    };
    xml(
        StatusCode::OK,
        &format!(
            "<ListBucketResult><Name>{}</Name><Prefix>{}</Prefix><Delimiter>{}</Delimiter><MaxKeys>{max}</MaxKeys><KeyCount>{count}</KeyCount><IsTruncated>{truncated}</IsTruncated>{contents}{prefixes}{next}</ListBucketResult>",
            escape(bucket),
            escape(&prefix),
            escape(&delimiter)
        ),
    )
}

/// `bytes=a-b` or `bytes=a-` of a `len`-byte object: the range, `None` when unsatisfiable.
fn range(header: &str, len: usize) -> Option<(usize, usize)> {
    let spec = header.strip_prefix("bytes=")?;
    let (start, end) = spec.split_once('-')?;
    let start: usize = start.parse().ok()?;
    let end: usize = if end.is_empty() {
        len.checked_sub(1)?
    } else {
        end.parse::<usize>().ok()?.min(len.checked_sub(1)?)
    };
    (start <= end).then_some((start, end))
}

/// `bucket/key` of an `x-amz-copy-source` header.
fn copy_source(store: &Store, headers: &HeaderMap) -> Option<Arc<Vec<u8>>> {
    let source = decode(headers.get("x-amz-copy-source")?.to_str().ok()?);
    let source = source.trim_start_matches('/');
    let (bucket, key) = source.split_once('/')?;
    store
        .buckets
        .get(bucket)?
        .get(key)
        .map(|stored| Arc::clone(&stored.data))
}

fn object_request(
    store: &mut Store,
    method: &Method,
    headers: &HeaderMap,
    bucket: &str,
    key: &str,
    query: &HashMap<String, String>,
    body: Bytes,
) -> Reply {
    let upload = query.get("uploadId").cloned();
    match (method, upload) {
        (&Method::PUT, Some(id)) => {
            let Some(number) = query.get("partNumber").and_then(|n| n.parse::<i32>().ok()) else {
                return error(StatusCode::BAD_REQUEST, "InvalidArgument", "no part number");
            };
            let copied = headers.contains_key("x-amz-copy-source");
            let data = if copied {
                let Some(source) = copy_source(store, headers) else {
                    return no_key();
                };
                let wanted = headers
                    .get("x-amz-copy-source-range")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| range(value, source.len()));
                match wanted {
                    Some((start, end)) => source.get(start..=end).unwrap_or_default().to_vec(),
                    None => source.as_ref().clone(),
                }
            } else {
                body.to_vec()
            };
            let tag = store.tag();
            let Some(pending) = store.uploads.get_mut(&id) else {
                return error(StatusCode::NOT_FOUND, "NoSuchUpload", "no such upload");
            };
            pending.parts.insert(number, (tag.clone(), data));
            if copied {
                xml(
                    StatusCode::OK,
                    &format!(
                        "<CopyPartResult><ETag>{}</ETag><LastModified>{}</LastModified></CopyPartResult>",
                        escape(&tag),
                        date(now(), DateTimeFormat::DateTime)
                    ),
                )
            } else {
                reply(StatusCode::OK, &[("etag", tag)], "")
            }
        }
        (&Method::POST, None) if query.contains_key("uploads") => {
            store.next += 1;
            let id = format!("upload-{}", store.next);
            store.uploads.insert(
                id.clone(),
                Pending {
                    bucket: bucket.to_owned(),
                    key: key.to_owned(),
                    parts: BTreeMap::new(),
                },
            );
            xml(
                StatusCode::OK,
                &format!(
                    "<InitiateMultipartUploadResult><Bucket>{}</Bucket><Key>{}</Key><UploadId>{id}</UploadId></InitiateMultipartUploadResult>",
                    escape(bucket),
                    escape(key)
                ),
            )
        }
        (&Method::POST, Some(id)) => {
            let Some(pending) = store.uploads.remove(&id) else {
                return error(StatusCode::NOT_FOUND, "NoSuchUpload", "no such upload");
            };
            let text = String::from_utf8_lossy(&body);
            let numbers = elements(&text, "PartNumber");
            let tags = elements(&text, "ETag");
            let mut data = Vec::new();
            for (number, tag) in numbers.iter().zip(&tags) {
                let part = number
                    .parse::<i32>()
                    .ok()
                    .and_then(|number| pending.parts.get(&number));
                match part {
                    Some((stored_tag, bytes)) if stored_tag == tag => data.extend_from_slice(bytes),
                    _ => {
                        return error(
                            StatusCode::BAD_REQUEST,
                            "InvalidPart",
                            "One or more of the specified parts could not be found.",
                        );
                    }
                }
            }
            let tag = store.tag();
            if let Some(objects) = store.buckets.get_mut(&pending.bucket) {
                objects.insert(
                    pending.key.clone(),
                    Stored {
                        data: Arc::new(data),
                        modified: now(),
                        tag: tag.clone(),
                    },
                );
            }
            xml(
                StatusCode::OK,
                &format!(
                    "<CompleteMultipartUploadResult><Bucket>{}</Bucket><Key>{}</Key><ETag>{}</ETag></CompleteMultipartUploadResult>",
                    escape(&pending.bucket),
                    escape(&pending.key),
                    escape(&tag)
                ),
            )
        }
        (&Method::DELETE, Some(id)) => {
            store.uploads.remove(&id);
            reply(StatusCode::NO_CONTENT, &[], "")
        }
        (&Method::PUT, None) => {
            let copied = headers.contains_key("x-amz-copy-source");
            let data = if copied {
                match copy_source(store, headers) {
                    Some(source) => source,
                    None => return no_key(),
                }
            } else {
                Arc::new(body.to_vec())
            };
            let tag = store.tag();
            let modified = now();
            if let Some(objects) = store.buckets.get_mut(bucket) {
                objects.insert(
                    key.to_owned(),
                    Stored {
                        data,
                        modified,
                        tag: tag.clone(),
                    },
                );
            }
            if copied {
                xml(
                    StatusCode::OK,
                    &format!(
                        "<CopyObjectResult><ETag>{}</ETag><LastModified>{}</LastModified></CopyObjectResult>",
                        escape(&tag),
                        date(modified, DateTimeFormat::DateTime)
                    ),
                )
            } else {
                reply(StatusCode::OK, &[("etag", tag)], "")
            }
        }
        (&Method::GET | &Method::HEAD, None) => {
            let Some(stored) = store
                .buckets
                .get(bucket)
                .and_then(|objects| objects.get(key))
            else {
                return if *method == Method::HEAD {
                    reply(StatusCode::NOT_FOUND, &[], "")
                } else {
                    no_key()
                };
            };
            let len = stored.data.len();
            let mut headers_out = vec![
                ("etag", stored.tag.clone()),
                (
                    "last-modified",
                    date(stored.modified, DateTimeFormat::HttpDate),
                ),
                ("content-type", "application/octet-stream".to_owned()),
            ];
            let wanted = headers.get("range").and_then(|value| value.to_str().ok());
            let (status, start, end) = match wanted {
                Some(text) => match range(text, len) {
                    Some((start, end)) => {
                        headers_out.push(("content-range", format!("bytes {start}-{end}/{len}")));
                        (StatusCode::PARTIAL_CONTENT, start, end + 1)
                    }
                    None => {
                        return error(
                            StatusCode::RANGE_NOT_SATISFIABLE,
                            "InvalidRange",
                            "The requested range is not satisfiable",
                        );
                    }
                },
                None => (StatusCode::OK, 0, len),
            };
            if *method == Method::HEAD {
                headers_out.push(("content-length", len.to_string()));
                return reply(status, &headers_out, "");
            }
            let bytes = stored.data.get(start..end).unwrap_or_default().to_vec();
            reply(status, &headers_out, bytes)
        }
        (&Method::DELETE, None) => {
            if let Some(objects) = store.buckets.get_mut(bucket) {
                objects.remove(key);
            }
            reply(StatusCode::NO_CONTENT, &[], "")
        }
        _ => error(StatusCode::METHOD_NOT_ALLOWED, "MethodNotAllowed", "no"),
    }
}
