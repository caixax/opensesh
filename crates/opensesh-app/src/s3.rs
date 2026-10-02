//! S3 storage in the file views (Sprint 12, ADR 0033): a saved S3 host, or `s3://` quick-connect
//! text, becomes the file system of a pane.
//!
//! The keys: the access key is the host's user name or its identity's, and the secret key is the
//! identity's password, fetched from the keychain worker when the pane opens (a locked vault
//! stays locked: the pane offers to unlock). Without one, the pane asks for it, and it is used
//! for that pane only. A test run goes to its in-process S3 server with its keys, and asks
//! nothing.

use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};

use opensesh_core::hosts::{Host, HostsFile, Protocol, target};
use opensesh_s3::{S3, S3Error, S3Spec};
use opensesh_ssh::SshError;
use opensesh_ssh::prompt::{self, Answer, Asker, Prompt};
use opensesh_ssh::sftp::Fs;
use opensesh_ssh::sftp::s3::S3Fs;
use secrecy::SecretString;

use crate::bridge::app_info::is_test_run;
use crate::keychain::{self, Job};
use crate::sftp::OpenError;

/// The port of the smoke test's S3 server (0 until it starts).
static TEST_SERVER: AtomicU16 = AtomicU16::new(0);

/// Sends every S3 connection of a test run to its server on `port`.
pub fn set_test_server(port: u16) {
    TEST_SERVER.store(port, Ordering::Relaxed);
}

/// How many times a typed secret key may be wrong.
const ATTEMPTS: usize = 3;

/// The S3 host of saved host `id`, or of quick-connect `text`, and where its pane starts; `None`
/// when it isn't S3.
#[must_use]
pub fn host_of(id: Option<&str>, text: Option<&str>) -> Option<(Host, String)> {
    match (id, text) {
        (Some(id), _) => crate::hosts::current()
            .file
            .host(id)
            .filter(|host| host.protocol == Protocol::S3)
            .map(|host| (host.clone(), String::new())),
        (None, Some(text)) => target::parse(text)
            .ok()
            .filter(|target| target.protocol == Protocol::S3)
            .map(|target| (target.to_host(), target.path)),
        (None, None) => None,
    }
}

fn open_error(code: &'static str, detail: impl Into<String>) -> OpenError {
    OpenError {
        code,
        detail: detail.into(),
    }
}

/// The access key, and the secret key when the identity has one.
async fn keys(file: &HostsFile, host: &Host) -> Result<(String, Option<SecretString>), OpenError> {
    let resolved = file.resolve(host);
    let identity = resolved.identity().map(str::to_owned);
    let access = resolved
        .user()
        .map(str::to_owned)
        .or_else(|| identity.as_deref().and_then(keychain::identity_user))
        .filter(|user| !user.trim().is_empty())
        .ok_or_else(|| {
            open_error(
                "invalid",
                "no access key: give the host an identity whose user name is the access key",
            )
        })?;
    let Some(identity) = identity else {
        return Ok((access, None));
    };
    let (reply, answer) = tokio::sync::oneshot::channel();
    if !keychain::request(Job::ConnectionSecrets { identity, reply }) {
        return Ok((access, None));
    }
    match answer.await {
        Ok(Ok(secrets)) => Ok((access, secrets.password)),
        Ok(Err("locked")) => Err(open_error(
            SshError::SecretsLocked.code(),
            SshError::SecretsLocked.to_string(),
        )),
        // A missing identity leaves the question.
        Ok(Err(_)) | Err(_) => Ok((access, None)),
    }
}

/// Opens the buckets of `host`, starting at `start` (a test listing that also checks the keys).
///
/// # Errors
///
/// [`OpenError`]: the keys were refused, the server can't be reached, the vault is locked, or
/// the host can't be used.
pub async fn open(host: &Host, start: &str, asker: &Asker) -> Result<Fs, OpenError> {
    let library = crate::hosts::current();
    let (endpoint, access, mut secret) = if is_test_run() {
        let port = TEST_SERVER.load(Ordering::Relaxed);
        if port == 0 {
            return Err(open_error(
                "invalid",
                "a test run connects only to its own S3 server",
            ));
        }
        (
            format!("http://127.0.0.1:{port}"),
            opensesh_s3::testing::ACCESS_KEY.to_owned(),
            Some(SecretString::from(opensesh_s3::testing::SECRET_KEY)),
        )
    } else {
        let (access, secret) = keys(&library.file, host).await?;
        (host.address.trim().to_owned(), access, secret)
    };
    let stored = secret.is_some();
    let server = opensesh_s3::endpoint_url(&endpoint)
        .ok()
        .flatten()
        .unwrap_or_else(|| "AWS".to_owned());
    let start = if start.is_empty() { "/" } else { start };
    for attempt in 0..ATTEMPTS {
        let secret_key = match secret.take() {
            Some(secret) => secret,
            None => match prompt::ask(
                asker,
                Prompt::Password {
                    target: format!("{access}@{server}"),
                    retry: attempt > 0,
                },
            )
            .await
            {
                Answer::Secrets(mut secrets) if !secrets.is_empty() => secrets.swap_remove(0),
                _ => return Err(open_error("cancelled", "")),
            },
        };
        let spec = S3Spec {
            endpoint: endpoint.clone(),
            region: host.s3.region().to_owned(),
            path_style: host.s3.path_style(),
            access_key: access.clone(),
            secret_key,
        };
        let s3 = S3::new(&spec).map_err(|error| open_error("invalid", error.to_string()))?;
        let fs = S3Fs::new(s3);
        match check(&fs, start).await {
            Ok(()) => return Ok(Fs::S3(Arc::new(fs))),
            // A typed key may be retyped; a stored one is what it is.
            Err(S3Error::Denied { .. }) if !stored && attempt + 1 < ATTEMPTS => {}
            Err(S3Error::Denied { message, .. }) => return Err(open_error("auth", message)),
            Err(error @ S3Error::Unreachable(_)) => {
                return Err(open_error("network", error.to_string()));
            }
            Err(S3Error::Timeout) => {
                return Err(open_error("timeout", S3Error::Timeout.to_string()));
            }
            // Anything else (a bucket that isn't there): the pane says it when it lists.
            Err(_) => return Ok(Fs::S3(Arc::new(fs))),
        }
    }
    Err(open_error("auth", "the secret key was refused"))
}

/// Lists where the pane starts: the bucket's top for a path inside one (keys may be allowed one
/// bucket and not the list of buckets), else the buckets.
async fn check(fs: &S3Fs, start: &str) -> Result<(), S3Error> {
    let bucket = start
        .trim_start_matches('/')
        .split('/')
        .next()
        .unwrap_or_default();
    let client = fs.client();
    if bucket.is_empty() {
        client.buckets().await.map(drop)
    } else {
        client.list(bucket, "").await.map(drop)
    }
}
