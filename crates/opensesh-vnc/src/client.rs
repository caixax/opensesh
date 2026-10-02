//! A VNC connection: TCP, the version and security handshake ([`connect`]), then the session
//! ([`run`]): updates drawn into a [`Canvas`] and handed out as the rectangles that changed, and
//! input, the clipboard and resizing sent to the server.
//!
//! The session asks for 32-bit pixels and, in order, CopyRect, Tight, ZRLE, Hextile and Raw,
//! with the cursor, desktop size, extended desktop size and last rectangle pseudo-encodings, and
//! Tight's JPEG quality and compression levels. A read-only session never sends input or the
//! clipboard.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _, ReadHalf};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use crate::VncError;
use crate::canvas::{Area, Canvas};
use crate::decode::{self, Cursor, Decoders};
use crate::messages::{self, ServerInit, Version};
use crate::security::{self, Choice};
use crate::tls::Certificate;

/// The largest desktop accepted: 8192 pixels a side.
const MAX_SIDE: u16 = 8192;
/// The largest cursor accepted.
const MAX_CURSOR: u16 = 256;

/// A byte stream: TCP, or TLS over it.
pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

/// What to connect to, and how.
#[derive(Debug, Clone)]
pub struct Settings {
    /// Where to connect (a name or an address).
    pub address: String,
    /// The port.
    pub port: u16,
    /// The server's name for TLS (SNI).
    pub server_name: String,
    /// The user name, for VeNCrypt's user name and password authentication.
    pub user: String,
    /// Share the desktop with other clients (else the server may disconnect them).
    pub shared: bool,
    /// Send no input and no clipboard.
    pub read_only: bool,
    /// Tight's JPEG quality, 0 to 9 (none: lossless only).
    pub quality: Option<u8>,
    /// The zlib level asked for, 0 to 9.
    pub compression: u8,
    /// How long connecting may take.
    pub timeout: Duration,
}

/// A connection after the handshake.
pub struct Connected {
    /// The stream.
    pub stream: Box<dyn Stream>,
    /// The version both speak.
    pub version: Version,
    /// The server's init.
    pub init: ServerInit,
    /// The session runs over TLS (VeNCrypt).
    pub encrypted: bool,
}

impl std::fmt::Debug for Connected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connected")
            .field("version", &self.version)
            .field("init", &self.init)
            .finish_non_exhaustive()
    }
}

/// Reads the security result: 0 is success; otherwise the reason (3.8 only).
async fn security_result<S: AsyncRead + Unpin>(
    stream: &mut S,
    version: Version,
) -> Result<(), VncError> {
    if stream.read_u32().await? == 0 {
        return Ok(());
    }
    let reason = if version == Version::V3_8 {
        messages::read_string(stream).await.unwrap_or_default()
    } else {
        String::new()
    };
    Err(VncError::Auth(if reason.is_empty() {
        "the server refused the password".to_owned()
    } else {
        reason
    }))
}

/// Connects and does the handshake. `decide` gets the server's certificate when VeNCrypt brings
/// TLS, and says whether to go on: nothing secret is sent before it says yes.
///
/// # Errors
///
/// [`VncError::Auth`] when the server refused the password, [`VncError::Refused`] when the
/// server refused the connection or the certificate wasn't accepted, and the others.
pub async fn connect<F, Fut>(
    settings: &Settings,
    password: &str,
    decide: F,
) -> Result<Connected, VncError>
where
    F: FnOnce(Certificate) -> Fut,
    Fut: Future<Output = bool>,
{
    let target = (settings.address.as_str(), settings.port);
    let tcp = tokio::time::timeout(settings.timeout, TcpStream::connect(target))
        .await
        .map_err(|_| VncError::Timeout)??;
    let _ = tcp.set_nodelay(true);
    tokio::time::timeout(settings.timeout, handshake(tcp, settings, password, decide))
        .await
        .map_err(|_| VncError::Timeout)?
}

async fn handshake<F, Fut>(
    mut tcp: TcpStream,
    settings: &Settings,
    password: &str,
    decide: F,
) -> Result<Connected, VncError>
where
    F: FnOnce(Certificate) -> Fut,
    Fut: Future<Output = bool>,
{
    let mut line = [0_u8; 12];
    tcp.read_exact(&mut line).await?;
    let version = Version::from_server(&line)?;
    tcp.write_all(version.line()).await?;
    tcp.flush().await?;

    let choice = if version == Version::V3_3 {
        match tcp.read_u32().await? {
            0 => {
                let reason = messages::read_string(&mut tcp).await?;
                return Err(VncError::Refused(reason));
            }
            1 => Choice::None,
            2 => Choice::VncAuth,
            other => {
                return Err(VncError::Unsupported(format!(
                    "the server asks for security type {other}, which this client doesn't have"
                )));
            }
        }
    } else {
        let count = tcp.read_u8().await?;
        if count == 0 {
            let reason = messages::read_string(&mut tcp).await?;
            return Err(VncError::Refused(reason));
        }
        let mut offered = vec![0; usize::from(count)];
        tcp.read_exact(&mut offered).await?;
        let (kind, choice) = security::choose(&offered)?;
        tcp.write_all(&[kind]).await?;
        tcp.flush().await?;
        choice
    };

    let encrypted = choice == Choice::VeNCrypt;
    let mut stream: Box<dyn Stream> = match choice {
        Choice::None => {
            if version == Version::V3_8 {
                security_result(&mut tcp, version).await?;
            }
            Box::new(tcp)
        }
        Choice::VncAuth => {
            security::vnc_auth(&mut tcp, password).await?;
            security_result(&mut tcp, version).await?;
            Box::new(tcp)
        }
        Choice::VeNCrypt => {
            let subtype = security::vencrypt_negotiate(&mut tcp).await?;
            let (mut tls, der) = crate::tls::upgrade(tcp, &settings.server_name).await?;
            if !decide(Certificate::read(&der)).await {
                return Err(VncError::Refused(
                    "the server's certificate wasn't accepted".to_owned(),
                ));
            }
            match subtype {
                security::vencrypt::X509_PLAIN => {
                    security::plain(&mut tls, &settings.user, password).await?;
                }
                security::vencrypt::X509_VNC => security::vnc_auth(&mut tls, password).await?,
                _ => {}
            }
            security_result(&mut tls, version).await?;
            Box::new(tls)
        }
    };

    stream.write_all(&[u8::from(settings.shared)]).await?;
    stream.flush().await?;
    let init = messages::read_server_init(&mut stream).await?;
    if init.width > MAX_SIDE || init.height > MAX_SIDE {
        return Err(VncError::Protocol(format!(
            "a desktop of {}x{}",
            init.width, init.height
        )));
    }
    Ok(Connected {
        stream,
        version,
        init,
        encrypted,
    })
}

/// What the app sends to a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// A key, as an X keysym.
    Key {
        /// The keysym.
        keysym: u32,
        /// Down or up.
        down: bool,
    },
    /// The pointer: where it is and the buttons down (see [`messages::pointer_event`]).
    Pointer {
        /// The buttons down.
        buttons: u8,
        /// X.
        x: u16,
        /// Y.
        y: u16,
    },
    /// This computer's clipboard text.
    Clipboard(String),
    /// Ask the server for this desktop size (where it allows it).
    Resize {
        /// Width.
        width: u16,
        /// Height.
        height: u16,
    },
}

/// What a session tells the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
    /// The desktop's size, at the start and when it changes (black until drawn).
    Size {
        /// Width.
        width: u16,
        /// Height.
        height: u16,
    },
    /// A rectangle that changed, RGBA.
    Pixels {
        /// Where.
        area: Area,
        /// Its pixels.
        rgba: Vec<u8>,
    },
    /// The cursor's shape.
    Cursor(Cursor),
    /// The server's clipboard text.
    Clipboard(String),
}

/// What the reader learned that the writer needs.
#[derive(Debug, Default)]
struct Shared {
    /// The server answers SetDesktopSize (it sent an extended desktop size).
    resizable: AtomicBool,
    /// The first screen's id, for SetDesktopSize.
    screen: AtomicU32,
    /// Told when the server turned out to be resizable.
    ready: tokio::sync::Notify,
}

/// The encodings asked for, in order of preference.
fn encodings(settings: &Settings) -> Vec<i32> {
    let mut list = vec![
        decode::COPY_RECT,
        decode::TIGHT,
        decode::ZRLE,
        decode::HEXTILE,
        decode::RAW,
        decode::CURSOR,
        decode::DESKTOP_SIZE,
        decode::EXTENDED_DESKTOP_SIZE,
        decode::LAST_RECT,
        decode::COMPRESS_0 + i32::from(settings.compression.min(9)),
    ];
    if let Some(quality) = settings.quality {
        list.push(decode::QUALITY_0 + i32::from(quality.min(9)));
    }
    list
}

/// Runs the session until the server ends it, the connection breaks, or `inputs` closes (the
/// app disconnected: `Ok`).
///
/// # Errors
///
/// Why the session ended.
pub async fn run(
    connected: Connected,
    settings: &Settings,
    mut inputs: mpsc::UnboundedReceiver<Input>,
    out: mpsc::Sender<Update>,
) -> Result<(), VncError> {
    let Connected { stream, init, .. } = connected;
    let (reader, mut writer) = tokio::io::split(stream);
    let (to_writer, mut writes) = mpsc::unbounded_channel::<Vec<u8>>();
    let shared = Arc::new(Shared::default());

    let _ = to_writer.send(messages::set_pixel_format());
    let _ = to_writer.send(messages::set_encodings(&encodings(settings)));
    let _ = to_writer.send(messages::update_request(false, init.width, init.height));
    if out
        .send(Update::Size {
            width: init.width,
            height: init.height,
        })
        .await
        .is_err()
    {
        return Ok(());
    }

    let reading = tokio::spawn(read_loop(
        reader,
        Canvas::new(init.width, init.height),
        out,
        to_writer.clone(),
        Arc::clone(&shared),
    ));
    let mut reading = std::pin::pin!(reading);
    // A size asked for before the server said whether it takes SetDesktopSize.
    let mut pending: Option<(u16, u16)> = None;
    loop {
        tokio::select! {
            () = shared.ready.notified(), if pending.is_some() => {
                if let Some((width, height)) = pending.take() {
                    let screen = shared.screen.load(Ordering::Relaxed);
                    writer.write_all(&messages::set_desktop_size(width, height, screen)).await?;
                    writer.flush().await?;
                }
            }
            ended = &mut reading => {
                return match ended {
                    Ok(result) => result,
                    Err(_) => Err(VncError::Protocol("the session's reader stopped".into())),
                };
            }
            write = writes.recv() => {
                let Some(message) = write else { continue };
                writer.write_all(&message).await?;
                writer.flush().await?;
            }
            input = inputs.recv() => {
                let Some(input) = input else {
                    return Ok(());
                };
                let message = match input {
                    _ if settings.read_only && !matches!(input, Input::Resize { .. }) => continue,
                    Input::Key { keysym, down } => messages::key_event(keysym, down),
                    Input::Pointer { buttons, x, y } => messages::pointer_event(buttons, x, y),
                    Input::Clipboard(text) => messages::client_cut_text(&text),
                    Input::Resize { width, height } => {
                        if !shared.resizable.load(Ordering::Relaxed) {
                            pending = Some((width, height));
                            continue;
                        }
                        let screen = shared.screen.load(Ordering::Relaxed);
                        messages::set_desktop_size(width, height, screen)
                    }
                };
                writer.write_all(&message).await?;
                writer.flush().await?;
            }
        }
    }
}

async fn read_loop(
    mut reader: ReadHalf<Box<dyn Stream>>,
    mut canvas: Canvas,
    out: mpsc::Sender<Update>,
    writer: mpsc::UnboundedSender<Vec<u8>>,
    shared: Arc<Shared>,
) -> Result<(), VncError> {
    let mut decoders = Decoders::default();
    loop {
        match reader.read_u8().await? {
            0 => {
                let mut pad = [0; 1];
                reader.read_exact(&mut pad).await?;
                let count = reader.read_u16().await?;
                let mut full = false;
                for _ in 0..count {
                    let area = Area {
                        x: reader.read_u16().await?,
                        y: reader.read_u16().await?,
                        width: reader.read_u16().await?,
                        height: reader.read_u16().await?,
                    };
                    let encoding = reader.read_i32().await?;
                    match encoding {
                        decode::RAW
                        | decode::COPY_RECT
                        | decode::HEXTILE
                        | decode::ZRLE
                        | decode::TIGHT => {
                            canvas.check(area)?;
                            match encoding {
                                decode::RAW => {
                                    Decoders::raw(&mut reader, &mut canvas, area).await?
                                }
                                decode::COPY_RECT => {
                                    Decoders::copy_rect(&mut reader, &mut canvas, area).await?;
                                }
                                decode::HEXTILE => {
                                    Decoders::hextile(&mut reader, &mut canvas, area).await?;
                                }
                                decode::ZRLE => {
                                    decoders.zrle(&mut reader, &mut canvas, area).await?
                                }
                                _ => decoders.tight(&mut reader, &mut canvas, area).await?,
                            }
                            if area.width > 0 && area.height > 0 {
                                let rgba = canvas.read(area);
                                if out.send(Update::Pixels { area, rgba }).await.is_err() {
                                    return Ok(());
                                }
                            }
                        }
                        decode::CURSOR => {
                            if area.width > MAX_CURSOR || area.height > MAX_CURSOR {
                                return Err(VncError::Protocol("a cursor too big".into()));
                            }
                            let cursor = Decoders::cursor(&mut reader, area).await?;
                            if area.width > 0 && area.height > 0 {
                                let _ = out.send(Update::Cursor(cursor)).await;
                            }
                        }
                        decode::DESKTOP_SIZE => {
                            resize(&mut canvas, &out, area.width, area.height).await?;
                            full = true;
                        }
                        decode::EXTENDED_DESKTOP_SIZE => {
                            let screens = reader.read_u8().await?;
                            let mut pad = [0; 3];
                            reader.read_exact(&mut pad).await?;
                            for index in 0..screens {
                                let id = reader.read_u32().await?;
                                let mut rest = [0; 12];
                                reader.read_exact(&mut rest).await?;
                                if index == 0 {
                                    shared.screen.store(id, Ordering::Relaxed);
                                }
                            }
                            if !shared.resizable.swap(true, Ordering::Relaxed) {
                                shared.ready.notify_one();
                            }
                            // `x` is why (1: this client asked), `y` the result (0: done).
                            if area.y == 0
                                && (area.width, area.height) != (canvas.width(), canvas.height())
                            {
                                resize(&mut canvas, &out, area.width, area.height).await?;
                                full = true;
                            } else if area.x == 1 && area.y != 0 {
                                tracing::info!("vnc: the server refused the new size ({})", area.y);
                            }
                        }
                        decode::LAST_RECT => break,
                        other => {
                            return Err(VncError::Protocol(format!(
                                "a rectangle in encoding {other}, which wasn't asked for"
                            )));
                        }
                    }
                }
                let _ = writer.send(messages::update_request(
                    !full,
                    canvas.width(),
                    canvas.height(),
                ));
            }
            // SetColourMapEntries: not used with true colour.
            1 => {
                let mut header = [0; 5];
                reader.read_exact(&mut header).await?;
                let count = usize::from(u16::from_be_bytes([header[3], header[4]]));
                let mut colours = vec![0; count * 6];
                reader.read_exact(&mut colours).await?;
            }
            // Bell.
            2 => {}
            // ServerCutText.
            3 => {
                let mut pad = [0; 3];
                reader.read_exact(&mut pad).await?;
                let length = reader.read_i32().await?;
                // A negative length is the extended clipboard, which wasn't asked for: skipped.
                let size = usize::try_from(length.unsigned_abs()).unwrap_or(usize::MAX);
                let mut text = vec![0; size.min(messages::MAX_TEXT)];
                reader.read_exact(&mut text).await?;
                let mut rest = size - text.len();
                let mut sink = [0_u8; 4096];
                while rest > 0 {
                    let chunk = rest.min(sink.len());
                    reader.read_exact(&mut sink[..chunk]).await?;
                    rest -= chunk;
                }
                if length >= 0 {
                    let _ = out
                        .send(Update::Clipboard(messages::from_latin1(&text)))
                        .await;
                }
            }
            // EndOfContinuousUpdates: never asked for.
            150 => {}
            // ServerFence: its flags and payload.
            248 => {
                let mut header = [0; 7];
                reader.read_exact(&mut header).await?;
                let mut payload = vec![0; usize::from(reader.read_u8().await?)];
                reader.read_exact(&mut payload).await?;
            }
            other => {
                return Err(VncError::Protocol(format!("an unknown message ({other})")));
            }
        }
    }
}

async fn resize(
    canvas: &mut Canvas,
    out: &mpsc::Sender<Update>,
    width: u16,
    height: u16,
) -> Result<(), VncError> {
    if width > MAX_SIDE || height > MAX_SIDE {
        return Err(VncError::Protocol(format!("a desktop of {width}x{height}")));
    }
    canvas.resize(width, height);
    let _ = out.send(Update::Size { width, height }).await;
    Ok(())
}
