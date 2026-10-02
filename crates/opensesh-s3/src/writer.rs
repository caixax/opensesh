//! [`S3Writer`]: an object upload as an `AsyncWrite`. The bytes go through a pipe to a task
//! that gathers them into parts: a file no bigger than a part goes in one `PutObject`, a bigger
//! one in a multipart upload, a part at a time. Shutting the writer down finishes the upload and
//! waits for it; dropping it without shutting it down abandons the upload.

use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, ready};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, DuplexStream};
use tokio::sync::oneshot;

use crate::{S3, S3Error};

/// The pipe between the writer and its upload task.
const PIPE: usize = 1024 * 1024;

/// An upload in progress (see [`S3::writer`]).
pub struct S3Writer {
    pipe: Option<DuplexStream>,
    /// Set when the writer is shut down: the end of the bytes is the end of the file.
    finishing: Arc<AtomicBool>,
    /// How the upload ended.
    done: Option<oneshot::Receiver<Result<(), S3Error>>>,
}

impl std::fmt::Debug for S3Writer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Writer").finish_non_exhaustive()
    }
}

impl S3Writer {
    pub(crate) fn start(s3: S3, bucket: String, key: String, part_size: usize) -> Self {
        let (pipe, reader) = tokio::io::duplex(PIPE);
        let finishing = Arc::new(AtomicBool::new(false));
        let (report, done) = oneshot::channel();
        let upload = Upload {
            s3,
            bucket,
            key,
            part_size: part_size.max(1),
            finishing: Arc::clone(&finishing),
        };
        tokio::spawn(async move {
            let _ = report.send(upload.run(reader).await);
        });
        Self {
            pipe: Some(pipe),
            finishing,
            done: Some(done),
        }
    }

    /// Why the upload stopped, when it already did; otherwise `error` as it is.
    fn upload_error(&mut self, cx: &mut Context<'_>, error: io::Error) -> io::Error {
        if let Some(done) = self.done.as_mut()
            && let Poll::Ready(Ok(Err(reason))) = Pin::new(done).poll(cx)
        {
            self.done = None;
            return io::Error::other(reason.to_string());
        }
        error
    }
}

impl AsyncWrite for S3Writer {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let Some(pipe) = self.pipe.as_mut() else {
            return Poll::Ready(Err(io::Error::from(io::ErrorKind::BrokenPipe)));
        };
        match ready!(Pin::new(pipe).poll_write(cx, buf)) {
            Ok(written) => Poll::Ready(Ok(written)),
            Err(error) => Poll::Ready(Err(self.upload_error(cx, error))),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.pipe.as_mut() {
            Some(pipe) => Pin::new(pipe).poll_flush(cx),
            None => Poll::Ready(Ok(())),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.finishing.store(true, Ordering::Release);
        if let Some(pipe) = self.pipe.as_mut() {
            ready!(Pin::new(pipe).poll_shutdown(cx))?;
            self.pipe = None;
        }
        let Some(done) = self.done.as_mut() else {
            return Poll::Ready(Ok(()));
        };
        let result = ready!(Pin::new(done).poll(cx));
        self.done = None;
        Poll::Ready(match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(io::Error::other(error.to_string())),
            Err(_) => Err(io::Error::other("the upload stopped")),
        })
    }
}

/// The upload task.
struct Upload {
    s3: S3,
    bucket: String,
    key: String,
    part_size: usize,
    finishing: Arc<AtomicBool>,
}

impl Upload {
    async fn run(self, mut reader: DuplexStream) -> Result<(), S3Error> {
        let mut upload_id: Option<String> = None;
        let result = self.parts(&mut reader, &mut upload_id).await;
        if result.is_err()
            && let Some(id) = &upload_id
        {
            // Nothing half-written stays on the server.
            if let Err(error) = self.s3.abort_multipart(&self.bucket, &self.key, id).await {
                tracing::info!("an abandoned upload wasn't aborted: {error}");
            }
        }
        result
    }

    async fn parts(
        &self,
        reader: &mut DuplexStream,
        upload_id: &mut Option<String>,
    ) -> Result<(), S3Error> {
        let mut parts: Vec<(i32, String)> = Vec::new();
        loop {
            let (buffer, end) = fill(reader, self.part_size).await;
            if end {
                if !self.finishing.load(Ordering::Acquire) {
                    // The writer was dropped, not shut down: abandon.
                    return Err(S3Error::Cancelled);
                }
                return match upload_id {
                    None => self.s3.put(&self.bucket, &self.key, buffer).await,
                    Some(id) => {
                        if !buffer.is_empty() {
                            let number = next_number(&parts)?;
                            let tag = self
                                .s3
                                .upload_part(&self.bucket, &self.key, id, number, buffer)
                                .await?;
                            parts.push((number, tag));
                        }
                        self.s3
                            .complete_multipart(&self.bucket, &self.key, id, parts)
                            .await
                    }
                };
            }
            // A full part, and more may come.
            let id = match upload_id {
                Some(id) => id.clone(),
                None => {
                    let id = self.s3.create_multipart(&self.bucket, &self.key).await?;
                    *upload_id = Some(id.clone());
                    id
                }
            };
            let number = next_number(&parts)?;
            let tag = self
                .s3
                .upload_part(&self.bucket, &self.key, &id, number, buffer)
                .await?;
            parts.push((number, tag));
        }
    }
}

fn next_number(parts: &[(i32, String)]) -> Result<i32, S3Error> {
    i32::try_from(parts.len() + 1)
        .ok()
        .filter(|number| *number <= 10_000)
        .ok_or_else(|| S3Error::Invalid("the file has more than 10,000 parts".to_owned()))
}

/// Reads up to `size` bytes: them, and whether the stream ended. A read error is an end too:
/// the writer is gone.
async fn fill(reader: &mut (impl AsyncRead + Unpin), size: usize) -> (Vec<u8>, bool) {
    let mut buffer = vec![0; size];
    let mut filled = 0;
    while filled < size {
        match reader.read(&mut buffer[filled..]).await {
            Ok(0) | Err(_) => {
                buffer.truncate(filled);
                return (buffer, true);
            }
            Ok(read) => filled += read,
        }
    }
    (buffer, false)
}
