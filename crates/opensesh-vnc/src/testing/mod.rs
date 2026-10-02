//! A small RFB server for OpenSesh's tests, smoke test and screenshots (ADR 0035): never the
//! network, only 127.0.0.1.
//!
//! It speaks RFB 3.3, 3.7 or 3.8 with the security types of its [`Rules`] (none, VNC
//! authentication for [`PASSWORD`], VeNCrypt's X509 subtypes with a fixed test certificate and
//! [`USER`]), and shows a made-up desktop: a gradient, a "window", a square that changes colour
//! with each key pressed and, once the client sent clipboard text, a second square, green. Its
//! updates go in Tight, ZRLE, Hextile and Raw in turn (whichever the client asked for), with the
//! cursor and the extended desktop size, which follows the client's SetDesktopSize. It records
//! what the client sent ([`Seen`]) and can copy text to the client's clipboard
//! ([`TestServer::copy`]). Each connection gets a desktop of its own.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use flate2::{Compress, Compression, FlushCompress};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_rustls::rustls;

use crate::decode;
use crate::messages::Version;
use crate::security::{self, vencrypt};

/// The password the server takes.
pub const PASSWORD: &str = "right password";
/// The user name VeNCrypt's Plain takes.
pub const USER: &str = "tester";
/// The desktop's name.
pub const NAME: &str = "OpenSesh VNC test server";
/// What [`TestServer::copy`] sends when nothing else is given (the smoke test).
pub const SERVER_TEXT: &str = "Copied on the OpenSesh VNC test server";
/// The test certificate (ECDSA P-256, `CN=opensesh-vnc-test-server`, never used elsewhere).
pub const CERTIFICATE: &str = include_str!("test-cert.pem");
const KEY: &str = include_str!("test-key.pem");

/// The key square's colours, one more key pressed each.
pub const KEY_COLOURS: [[u8; 3]; 4] = [
    [0x5E, 0x81, 0xAC],
    [0xA3, 0xBE, 0x8C],
    [0xEB, 0xCB, 0x8B],
    [0xBF, 0x61, 0x6A],
];
/// The clipboard square's colour.
pub const CLIPBOARD_COLOUR: [u8; 3] = [0xA3, 0xBE, 0x8C];

/// The security the server offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offer {
    /// None.
    None,
    /// VNC authentication.
    VncAuth,
    /// VeNCrypt with these subtypes (X509 ones; others are listed but refused).
    VeNCrypt(&'static [u32]),
}

/// How the server behaves.
#[derive(Debug, Clone)]
pub struct Rules {
    /// The version it announces.
    pub version: Version,
    /// The security types it offers, in order.
    pub offers: Vec<Offer>,
    /// The desktop's size before the client asks for another.
    pub size: (u16, u16),
    /// Text copied to each client's clipboard once it is connected (the smoke test's).
    pub greeting: Option<&'static str>,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            version: Version::V3_8,
            offers: vec![Offer::VncAuth],
            size: (1024, 768),
            greeting: None,
        }
    }
}

/// What the clients sent.
#[derive(Debug, Clone, Default)]
pub struct Seen {
    /// Keys: keysym and down.
    pub keys: Vec<(u32, bool)>,
    /// Pointer events: buttons, x, y.
    pub pointer: Vec<(u8, u16, u16)>,
    /// The last clipboard text.
    pub clipboard: Option<String>,
    /// The encodings the last client asked for.
    pub encodings: Vec<i32>,
    /// The sizes the clients asked for.
    pub sizes: Vec<(u16, u16)>,
    /// The shared flags of the clients' inits.
    pub shared: Vec<bool>,
    /// The encodings the server sent pixels in.
    pub sent: Vec<i32>,
    /// User names that logged in (VeNCrypt's Plain).
    pub users: Vec<String>,
}

/// A running server.
#[derive(Debug)]
pub struct TestServer {
    /// Its port on 127.0.0.1.
    pub port: u16,
    seen: Arc<Mutex<Seen>>,
    copies: broadcast::Sender<String>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl TestServer {
    /// What the clients sent so far.
    #[must_use]
    pub fn seen(&self) -> Seen {
        lock(&self.seen).clone()
    }

    /// Copies `text` to every connected client's clipboard.
    pub fn copy(&self, text: &str) {
        let _ = self.copies.send(text.to_owned());
    }
}

/// Starts a server on 127.0.0.1 (a free port), on the current tokio runtime.
///
/// # Errors
///
/// When the port can't be bound.
pub async fn serve(rules: Rules) -> std::io::Result<TestServer> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let seen = Arc::new(Mutex::new(Seen::default()));
    let (copies, _) = broadcast::channel(16);
    let server = TestServer {
        port,
        seen: Arc::clone(&seen),
        copies: copies.clone(),
    };
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let rules = rules.clone();
            let seen = Arc::clone(&seen);
            let copies = copies.subscribe();
            tokio::spawn(async move {
                if let Err(error) = connection(stream, &rules, &seen, copies).await {
                    tracing::debug!("VNC test server: a connection ended: {error}");
                }
            });
        }
    });
    Ok(server)
}

fn acceptor() -> std::io::Result<tokio_rustls::TlsAcceptor> {
    use rustls::pki_types::pem::PemObject as _;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    let cert =
        CertificateDer::from_pem_slice(CERTIFICATE.as_bytes()).map_err(std::io::Error::other)?;
    let key = PrivateKeyDer::from_pem_slice(KEY.as_bytes()).map_err(std::io::Error::other)?;
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(std::io::Error::other)?
    .with_no_client_auth()
    .with_single_cert(vec![cert], key)
    .map_err(std::io::Error::other)?;
    Ok(tokio_rustls::TlsAcceptor::from(Arc::new(config)))
}

trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}

const CHALLENGE: [u8; 16] = *b"OpenSesh VNC 16b";

async fn result<S: AsyncWrite + Unpin>(
    stream: &mut S,
    ok: bool,
    version: Version,
) -> std::io::Result<()> {
    stream.write_all(&u32::from(!ok).to_be_bytes()).await?;
    if !ok && version == Version::V3_8 {
        let reason = b"wrong password";
        stream
            .write_all(&u32::try_from(reason.len()).unwrap_or(0).to_be_bytes())
            .await?;
        stream.write_all(reason).await?;
    }
    stream.flush().await
}

async fn vnc_auth<S: AsyncRead + AsyncWrite + Unpin>(stream: &mut S) -> std::io::Result<bool> {
    stream.write_all(&CHALLENGE).await?;
    stream.flush().await?;
    let mut response = [0; 16];
    stream.read_exact(&mut response).await?;
    Ok(response == security::vnc_response(PASSWORD, &CHALLENGE))
}

/// The handshake, then the session.
async fn connection(
    mut tcp: TcpStream,
    rules: &Rules,
    seen: &Arc<Mutex<Seen>>,
    copies: broadcast::Receiver<String>,
) -> std::io::Result<()> {
    let version = rules.version;
    tcp.write_all(version.line()).await?;
    let mut line = [0; 12];
    tcp.read_exact(&mut line).await?;
    let mut stream: Box<dyn Io> = if version == Version::V3_3 {
        // The server picks: the first of its offers (none or VNC authentication).
        match rules.offers.first() {
            Some(Offer::VncAuth) => {
                tcp.write_all(&2_u32.to_be_bytes()).await?;
                let ok = vnc_auth(&mut tcp).await?;
                result(&mut tcp, ok, version).await?;
                if !ok {
                    return Ok(());
                }
            }
            _ => tcp.write_all(&1_u32.to_be_bytes()).await?,
        }
        Box::new(tcp)
    } else {
        let types: Vec<u8> = rules
            .offers
            .iter()
            .map(|offer| match offer {
                Offer::None => security::NONE,
                Offer::VncAuth => security::VNC_AUTH,
                Offer::VeNCrypt(_) => security::VENCRYPT,
            })
            .collect();
        tcp.write_all(&[u8::try_from(types.len()).unwrap_or(0)])
            .await?;
        tcp.write_all(&types).await?;
        tcp.flush().await?;
        let chosen = tcp.read_u8().await?;
        match chosen {
            security::NONE => {
                if version == Version::V3_8 {
                    result(&mut tcp, true, version).await?;
                }
                Box::new(tcp)
            }
            security::VNC_AUTH => {
                let ok = vnc_auth(&mut tcp).await?;
                result(&mut tcp, ok, version).await?;
                if !ok {
                    return Ok(());
                }
                Box::new(tcp)
            }
            security::VENCRYPT => {
                let subtypes = rules
                    .offers
                    .iter()
                    .find_map(|offer| match offer {
                        Offer::VeNCrypt(subtypes) => Some(*subtypes),
                        _ => None,
                    })
                    .unwrap_or(&[]);
                tcp.write_all(&[0, 2]).await?;
                let mut client = [0; 2];
                tcp.read_exact(&mut client).await?;
                tcp.write_all(&[0, u8::try_from(subtypes.len()).unwrap_or(0)])
                    .await?;
                for subtype in subtypes {
                    tcp.write_all(&subtype.to_be_bytes()).await?;
                }
                tcp.flush().await?;
                let subtype = tcp.read_u32().await?;
                let x509 = [
                    vencrypt::X509_NONE,
                    vencrypt::X509_VNC,
                    vencrypt::X509_PLAIN,
                ];
                if !x509.contains(&subtype) || !subtypes.contains(&subtype) {
                    tcp.write_all(&[0]).await?;
                    return Ok(());
                }
                tcp.write_all(&[1]).await?;
                tcp.flush().await?;
                let mut tls = acceptor()?.accept(tcp).await?;
                let ok = match subtype {
                    vencrypt::X509_VNC => vnc_auth(&mut tls).await?,
                    vencrypt::X509_PLAIN => {
                        let user_len = usize::try_from(tls.read_u32().await?).unwrap_or(0);
                        let password_len = usize::try_from(tls.read_u32().await?).unwrap_or(0);
                        let mut user = vec![0; user_len.min(1024)];
                        tls.read_exact(&mut user).await?;
                        let mut password = vec![0; password_len.min(1024)];
                        tls.read_exact(&mut password).await?;
                        let user = String::from_utf8_lossy(&user).into_owned();
                        let ok = user == USER && password == PASSWORD.as_bytes();
                        if ok {
                            lock(seen).users.push(user);
                        }
                        ok
                    }
                    _ => true,
                };
                result(&mut tls, ok, version).await?;
                if !ok {
                    return Ok(());
                }
                Box::new(tls)
            }
            _ => return Ok(()),
        }
    };
    let shared = stream.read_u8().await? != 0;
    lock(seen).shared.push(shared);
    let (width, height) = rules.size;
    stream.write_all(&width.to_be_bytes()).await?;
    stream.write_all(&height.to_be_bytes()).await?;
    // Its own pixel format (the client replaces it): 32 bits, red in the low byte.
    stream
        .write_all(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 0, 8, 16, 0, 0, 0])
        .await?;
    stream
        .write_all(&u32::try_from(NAME.len()).unwrap_or(0).to_be_bytes())
        .await?;
    stream.write_all(NAME.as_bytes()).await?;
    if let Some(text) = rules.greeting {
        stream.write_all(&cut_text(text)).await?;
    }
    stream.flush().await?;
    Session::new(width, height).run(stream, seen, copies).await
}

/// One client's desktop.
struct Session {
    width: u16,
    height: u16,
    keys: usize,
    clipboard: bool,
    encodings: Vec<i32>,
    /// The next encoding to send pixels in (they take turns).
    turn: usize,
    zrle: Compress,
    tight: Compress,
    /// What changed since the last update; a full update is owed.
    dirty: Option<(u16, u16, u16, u16)>,
    full: bool,
    /// The extended desktop size owed (reason, status).
    size_reply: Option<(u16, u16)>,
    cursor_sent: bool,
    /// An update request waiting for something to change.
    waiting: bool,
}

impl Session {
    fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            keys: 0,
            clipboard: false,
            encodings: Vec::new(),
            turn: 0,
            zrle: Compress::new(Compression::default(), true),
            tight: Compress::new(Compression::default(), true),
            dirty: None,
            full: true,
            size_reply: Some((0, 0)),
            cursor_sent: false,
            waiting: false,
        }
    }

    /// The desktop's colour at a pixel.
    fn colour(&self, x: u16, y: u16) -> [u8; 3] {
        let (w, h) = (u32::from(self.width), u32::from(self.height));
        let (x32, y32) = (u32::from(x), u32::from(y));
        if (24..72).contains(&x) && (24..72).contains(&y) {
            return KEY_COLOURS[self.keys % KEY_COLOURS.len()];
        }
        if self.clipboard && (80..128).contains(&x) && (24..72).contains(&y) {
            return CLIPBOARD_COLOUR;
        }
        let (left, top, right, bottom) = (w / 8, h / 6, w * 7 / 8, h * 5 / 6);
        if (left..right).contains(&x32) && (top..bottom).contains(&y32) {
            return if y32 < top + 28 {
                [0x3B, 0x42, 0x52]
            } else {
                [0xEC, 0xEF, 0xF4]
            };
        }
        let shade = u8::try_from(40 + (y32 * 40) / h.max(1)).unwrap_or(80);
        [shade / 2, shade, shade + 20]
    }

    fn mark(&mut self, x: u16, y: u16, width: u16, height: u16) {
        self.dirty = Some(match self.dirty {
            None => (x, y, width, height),
            Some((dx, dy, dw, dh)) => {
                let left = dx.min(x);
                let top = dy.min(y);
                let right = (dx + dw).max(x + width);
                let bottom = (dy + dh).max(y + height);
                (left, top, right - left, bottom - top)
            }
        });
    }

    async fn run(
        mut self,
        stream: Box<dyn Io>,
        seen: &Arc<Mutex<Seen>>,
        mut copies: broadcast::Receiver<String>,
    ) -> std::io::Result<()> {
        let (mut reader, mut writer) = tokio::io::split(stream);
        loop {
            tokio::select! {
                copied = copies.recv() => {
                    if let Ok(text) = copied {
                        writer.write_all(&cut_text(&text)).await?;
                        writer.flush().await?;
                    }
                }
                kind = reader.read_u8() => {
                    self.message(kind?, &mut reader, seen).await?;
                }
            }
            if self.waiting && (self.full || self.dirty.is_some() || self.size_reply.is_some()) {
                self.waiting = false;
                let update = self.update(seen);
                writer.write_all(&update).await?;
                writer.flush().await?;
            }
        }
    }

    async fn message<R: AsyncRead + Unpin>(
        &mut self,
        kind: u8,
        reader: &mut R,
        seen: &Arc<Mutex<Seen>>,
    ) -> std::io::Result<()> {
        match kind {
            // SetPixelFormat: taken to be the client's (checked by the client's tests).
            0 => {
                let mut format = [0; 19];
                reader.read_exact(&mut format).await?;
            }
            2 => {
                let mut pad = [0; 1];
                reader.read_exact(&mut pad).await?;
                let count = reader.read_u16().await?;
                self.encodings.clear();
                for _ in 0..count {
                    self.encodings.push(reader.read_i32().await?);
                }
                lock(seen).encodings.clone_from(&self.encodings);
            }
            3 => {
                let incremental = reader.read_u8().await? != 0;
                let mut area = [0; 8];
                reader.read_exact(&mut area).await?;
                if !incremental {
                    self.full = true;
                }
                self.waiting = true;
            }
            4 => {
                let down = reader.read_u8().await? != 0;
                let mut pad = [0; 2];
                reader.read_exact(&mut pad).await?;
                let keysym = reader.read_u32().await?;
                lock(seen).keys.push((keysym, down));
                if down {
                    self.keys += 1;
                    self.mark(24, 24, 48, 48);
                }
            }
            5 => {
                let buttons = reader.read_u8().await?;
                let x = reader.read_u16().await?;
                let y = reader.read_u16().await?;
                lock(seen).pointer.push((buttons, x, y));
            }
            6 => {
                let mut pad = [0; 3];
                reader.read_exact(&mut pad).await?;
                let length = usize::try_from(reader.read_u32().await?).unwrap_or(0);
                let mut text = vec![0; length.min(1 << 20)];
                reader.read_exact(&mut text).await?;
                lock(seen).clipboard = Some(crate::messages::from_latin1(&text));
                self.clipboard = true;
                self.mark(80, 24, 48, 48);
            }
            251 => {
                let mut pad = [0; 1];
                reader.read_exact(&mut pad).await?;
                let width = reader.read_u16().await?;
                let height = reader.read_u16().await?;
                let screens = reader.read_u8().await?;
                reader.read_exact(&mut pad).await?;
                let mut layout = vec![0; usize::from(screens) * 16];
                reader.read_exact(&mut layout).await?;
                lock(seen).sizes.push((width, height));
                if (200..=8192).contains(&width) && (200..=8192).contains(&height) {
                    self.width = width;
                    self.height = height;
                    self.size_reply = Some((1, 0));
                    self.full = true;
                } else {
                    self.size_reply = Some((1, 3));
                }
            }
            other => {
                return Err(std::io::Error::other(format!(
                    "unknown client message {other}"
                )));
            }
        }
        Ok(())
    }

    /// A FramebufferUpdate with what is owed.
    fn update(&mut self, seen: &Arc<Mutex<Seen>>) -> Vec<u8> {
        let mut rects = Vec::new();
        let mut count = 0_u16;
        if let Some((reason, status)) = self.size_reply.take()
            && self.encodings.contains(&decode::EXTENDED_DESKTOP_SIZE)
        {
            rect_header(
                &mut rects,
                reason,
                status,
                self.width,
                self.height,
                decode::EXTENDED_DESKTOP_SIZE,
            );
            rects.extend_from_slice(&[1, 0, 0, 0]);
            rects.extend_from_slice(&1_u32.to_be_bytes());
            rects.extend_from_slice(&[0, 0, 0, 0]);
            rects.extend_from_slice(&self.width.to_be_bytes());
            rects.extend_from_slice(&self.height.to_be_bytes());
            rects.extend_from_slice(&0_u32.to_be_bytes());
            count += 1;
        }
        if !self.cursor_sent && self.encodings.contains(&decode::CURSOR) {
            self.cursor_sent = true;
            // An 8x8 arrow-ish triangle, its hot spot at the top left.
            rect_header(&mut rects, 0, 0, 8, 8, decode::CURSOR);
            for _ in 0..64 {
                rects.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0]);
            }
            for y in 0..8_u8 {
                rects.push(0xFF_u8 << (7 - y));
            }
            count += 1;
        }
        let area = if self.full {
            self.full = false;
            self.dirty = None;
            Some((0, 0, self.width, self.height))
        } else {
            self.dirty.take()
        };
        if let Some((x, y, width, height)) = area {
            let x = x.min(self.width);
            let y = y.min(self.height);
            let width = width.min(self.width - x);
            let height = height.min(self.height - y);
            let offered = [decode::TIGHT, decode::ZRLE, decode::HEXTILE, decode::RAW];
            let usable: Vec<i32> = offered
                .into_iter()
                .filter(|encoding| self.encodings.contains(encoding) || *encoding == decode::RAW)
                .collect();
            let encoding = usable[self.turn % usable.len()];
            self.turn += 1;
            lock(seen).sent.push(encoding);
            // Tight rectangles are at most 2048 pixels wide.
            let mut left = x;
            while left < x + width {
                let part = (x + width - left).min(2048);
                rect_header(&mut rects, left, y, part, height, encoding);
                self.encode(&mut rects, encoding, (left, y, part, height));
                count += 1;
                left += part;
            }
        }
        let mut message = vec![0, 0];
        message.extend_from_slice(&count.to_be_bytes());
        message.extend(rects);
        message
    }

    fn encode(
        &mut self,
        out: &mut Vec<u8>,
        encoding: i32,
        (x, y, width, height): (u16, u16, u16, u16),
    ) {
        match encoding {
            decode::TIGHT => {
                let first = self.colour(x, y);
                let uniform = (y..y + height)
                    .all(|row| (x..x + width).all(|col| self.colour(col, row) == first));
                if uniform {
                    out.push(0x80);
                    out.extend_from_slice(&first);
                    return;
                }
                let mut data = Vec::with_capacity(usize::from(width) * usize::from(height) * 3);
                for row in y..y + height {
                    for col in x..x + width {
                        data.extend_from_slice(&self.colour(col, row));
                    }
                }
                out.push(0x00);
                if data.len() < 12 {
                    out.extend(data);
                } else {
                    let compressed = deflate(&mut self.tight, &data);
                    compact(out, compressed.len());
                    out.extend(compressed);
                }
            }
            decode::ZRLE => {
                let mut tiles = Vec::new();
                for top in (y..y + height).step_by(64) {
                    for left in (x..x + width).step_by(64) {
                        let w = (x + width - left).min(64);
                        let h = (y + height - top).min(64);
                        let first = self.colour(left, top);
                        let uniform = (top..top + h)
                            .all(|row| (left..left + w).all(|col| self.colour(col, row) == first));
                        if uniform {
                            tiles.push(1);
                            tiles.extend_from_slice(&first);
                        } else {
                            tiles.push(0);
                            for row in top..top + h {
                                for col in left..left + w {
                                    tiles.extend_from_slice(&self.colour(col, row));
                                }
                            }
                        }
                    }
                }
                let compressed = deflate(&mut self.zrle, &tiles);
                out.extend_from_slice(&u32::try_from(compressed.len()).unwrap_or(0).to_be_bytes());
                out.extend(compressed);
            }
            decode::HEXTILE => {
                for top in (y..y + height).step_by(16) {
                    for left in (x..x + width).step_by(16) {
                        let w = (x + width - left).min(16);
                        let h = (y + height - top).min(16);
                        let first = self.colour(left, top);
                        let uniform = (top..top + h)
                            .all(|row| (left..left + w).all(|col| self.colour(col, row) == first));
                        if uniform {
                            out.push(2);
                            out.extend_from_slice(&[first[0], first[1], first[2], 0]);
                        } else {
                            out.push(1);
                            for row in top..top + h {
                                for col in left..left + w {
                                    let c = self.colour(col, row);
                                    out.extend_from_slice(&[c[0], c[1], c[2], 0]);
                                }
                            }
                        }
                    }
                }
            }
            _ => {
                for row in y..y + height {
                    for col in x..x + width {
                        let c = self.colour(col, row);
                        out.extend_from_slice(&[c[0], c[1], c[2], 0]);
                    }
                }
            }
        }
    }
}

/// ServerCutText with `text` as Latin-1.
fn cut_text(text: &str) -> Vec<u8> {
    let bytes = crate::messages::to_latin1(text);
    let mut message = vec![3, 0, 0, 0];
    message.extend_from_slice(&u32::try_from(bytes.len()).unwrap_or(0).to_be_bytes());
    message.extend_from_slice(&bytes);
    message
}

fn rect_header(out: &mut Vec<u8>, x: u16, y: u16, width: u16, height: u16, encoding: i32) {
    out.extend_from_slice(&x.to_be_bytes());
    out.extend_from_slice(&y.to_be_bytes());
    out.extend_from_slice(&width.to_be_bytes());
    out.extend_from_slice(&height.to_be_bytes());
    out.extend_from_slice(&encoding.to_be_bytes());
}

fn compact(out: &mut Vec<u8>, length: usize) {
    let mut length = length;
    for _ in 0..2 {
        let low = u8::try_from(length & 0x7F).unwrap_or(0);
        length >>= 7;
        if length == 0 {
            out.push(low);
            return;
        }
        out.push(low | 0x80);
    }
    out.push(u8::try_from(length & 0xFF).unwrap_or(0));
}

fn deflate(stream: &mut Compress, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / 2 + 1024);
    let mut consumed = 0;
    loop {
        if out.len() == out.capacity() {
            out.reserve(out.capacity().max(1024));
        }
        let before = stream.total_in();
        let _ = stream.compress_vec(&data[consumed..], &mut out, FlushCompress::Sync);
        consumed += usize::try_from(stream.total_in() - before).unwrap_or(0);
        if consumed >= data.len() && out.len() < out.capacity() {
            return out;
        }
    }
}
