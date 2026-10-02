//! A small RDP server for OpenSesh's tests and smoke test (ADR 0034), on `ironrdp-server`. It is
//! never part of the app: it brings `aws-lc` (through `tokio-rustls`'s default features).
//!
//! It listens on 127.0.0.1 with NLA for [`USER`] and [`PASSWORD`] and a fixed test certificate,
//! shows a made-up desktop at the size the client asks for (again after a display control
//! resize), paints a square that changes colour with each key it gets, records the input, and
//! shares text both ways on the clipboard: what the client copies lands in [`Seen::clipboard`]
//! (and a second square, green, appears next to the first), and it offers [`SERVER_TEXT`]. Each
//! connection gets a desktop of its own, so several clients can be connected at once.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "a test server")]

use std::net::SocketAddr;
use std::num::{NonZeroU16, NonZeroUsize};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use ironrdp_server::{
    BitmapUpdate, CliprdrServerFactory, Credentials, DesktopSize, DisplayUpdate, KeyboardEvent,
    MouseEvent, PixelFormat, RdpServer, RdpServerDisplay, RdpServerDisplayUpdates,
    RdpServerInputHandler, ServerEvent, ServerEventSender,
};
use tokio::net::TcpListener;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

/// The user the server knows.
pub const USER: &str = "tester";
/// Its password.
pub const PASSWORD: &str = "right password";
/// What the server offers on its clipboard.
pub const SERVER_TEXT: &str = "Copied on the OpenSesh RDP test server";
/// The test certificate (PEM).
pub const CERTIFICATE: &str = include_str!("test-cert.pem");
const KEY: &str = include_str!("test-key.pem");

/// What the server got from the client.
#[derive(Debug, Default, Clone)]
pub struct Seen {
    /// Keys: (scancode, extended, pressed).
    pub keys: Vec<(u8, bool, bool)>,
    /// Characters sent as Unicode.
    pub unicode: Vec<u16>,
    /// Mouse positions.
    pub moves: Vec<(u16, u16)>,
    /// Left clicks (presses).
    pub clicks: usize,
    /// Wheel steps.
    pub wheel: Vec<i16>,
    /// The client's clipboard text.
    pub clipboard: Option<String>,
    /// Desktop sizes shown.
    pub sizes: Vec<(u16, u16)>,
}

/// A running test server.
#[derive(Debug, Clone)]
pub struct TestServer {
    /// Its port on 127.0.0.1.
    pub port: u16,
    seen: Arc<Mutex<Seen>>,
}

impl TestServer {
    /// What it got so far.
    #[must_use]
    pub fn seen(&self) -> Seen {
        lock(&self.seen).clone()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The desktop's pixels: a background, a "window" with a title bar, the key square (its colour
/// changes with each key) and, once the client copied text, the clipboard square. BGRA,
/// `width * 4` a row.
fn paint(width: u16, height: u16, keys: usize, clipboard: bool) -> Vec<u8> {
    let (w, h) = (usize::from(width), usize::from(height));
    let mut pixels = vec![0_u8; w * h * 4];
    let square = [
        [0x5E, 0x81, 0xAC],
        [0xA3, 0xBE, 0x8C],
        [0xEB, 0xCB, 0x8B],
        [0xBF, 0x61, 0x6A],
    ][keys % 4];
    for y in 0..h {
        for x in 0..w {
            // A vertical gradient behind everything.
            let shade = u8::try_from(40 + (y * 40) / h.max(1)).unwrap_or(80);
            let mut rgb = [shade / 2, shade, shade + 20];
            let (left, top, right, bottom) = (w / 8, h / 6, w * 7 / 8, h * 5 / 6);
            if (left..right).contains(&x) && (top..bottom).contains(&y) {
                rgb = if y < top + 28 {
                    [0x3B, 0x42, 0x52]
                } else {
                    [0xEC, 0xEF, 0xF4]
                };
            }
            if (24..72).contains(&x) && (24..72).contains(&y) {
                rgb = square;
            }
            if clipboard && (80..128).contains(&x) && (24..72).contains(&y) {
                rgb = [0xA3, 0xBE, 0x8C];
            }
            let at = (y * w + x) * 4;
            pixels[at..at + 4].copy_from_slice(&[rgb[2], rgb[1], rgb[0], 0xFF]);
        }
    }
    pixels
}

fn full(width: u16, height: u16, keys: usize, clipboard: bool) -> DisplayUpdate {
    DisplayUpdate::Bitmap(BitmapUpdate {
        x: 0,
        y: 0,
        width: NonZeroU16::new(width).unwrap(),
        height: NonZeroU16::new(height).unwrap(),
        format: PixelFormat::BgrA32,
        data: paint(width, height, keys, clipboard).into(),
        stride: NonZeroUsize::new(usize::from(width) * 4).unwrap(),
    })
}

/// The display's state, shared with the input handler (keys repaint the square).
#[derive(Default)]
struct Shared {
    size: (u16, u16),
    keys: usize,
    /// The client copied text.
    clipboard: bool,
    updates: Option<UnboundedSender<DisplayUpdate>>,
    seen: Arc<Mutex<Seen>>,
}

impl Shared {
    fn repaint(&self) {
        if let Some(updates) = &self.updates {
            let (width, height) = self.size;
            let _ = updates.send(full(width, height, self.keys, self.clipboard));
        }
    }
}

struct Display(Arc<Mutex<Shared>>);

struct Updates(UnboundedReceiver<DisplayUpdate>);

#[async_trait::async_trait]
impl RdpServerDisplayUpdates for Updates {
    async fn next_update(&mut self) -> anyhow::Result<Option<DisplayUpdate>> {
        // Cancellation safe: a received update is returned at once.
        Ok(self.0.recv().await)
    }
}

#[async_trait::async_trait]
impl RdpServerDisplay for Display {
    async fn size(&mut self) -> DesktopSize {
        let (width, height) = lock(&self.0).size;
        DesktopSize { width, height }
    }

    async fn request_initial_size(&mut self, client_size: DesktopSize) -> DesktopSize {
        let mut shared = lock(&self.0);
        shared.size = (client_size.width.max(64), client_size.height.max(64));
        let (width, height) = shared.size;
        DesktopSize { width, height }
    }

    async fn updates(&mut self) -> anyhow::Result<Box<dyn RdpServerDisplayUpdates>> {
        let (sender, receiver) = unbounded_channel();
        let mut shared = lock(&self.0);
        shared.updates = Some(sender);
        let size = shared.size;
        lock(&shared.seen).sizes.push(size);
        shared.repaint();
        Ok(Box::new(Updates(receiver)))
    }

    fn request_layout(&mut self, layout: ironrdp_displaycontrol::pdu::DisplayControlMonitorLayout) {
        let Some(monitor) = layout.monitors().first() else {
            return;
        };
        let (width, height) = monitor.dimensions();
        let mut shared = lock(&self.0);
        shared.size = (
            u16::try_from(width).unwrap_or(1024),
            u16::try_from(height).unwrap_or(768),
        );
        let (width, height) = shared.size;
        lock(&shared.seen).sizes.push((width, height));
        if let Some(updates) = &shared.updates {
            let _ = updates.send(DisplayUpdate::Resize(DesktopSize { width, height }));
        }
        shared.repaint();
    }
}

struct Input(Arc<Mutex<Shared>>);

impl RdpServerInputHandler for Input {
    fn keyboard(&mut self, event: KeyboardEvent) {
        let mut shared = lock(&self.0);
        let seen = Arc::clone(&shared.seen);
        match event {
            KeyboardEvent::Pressed { code, extended } => {
                lock(&seen).keys.push((code, extended, true));
                shared.keys += 1;
                shared.repaint();
            }
            KeyboardEvent::Released { code, extended } => {
                lock(&seen).keys.push((code, extended, false))
            }
            KeyboardEvent::UnicodePressed(unit) => lock(&seen).unicode.push(unit),
            _ => {}
        }
    }

    fn mouse(&mut self, event: MouseEvent) {
        let shared = lock(&self.0);
        let mut seen = lock(&shared.seen);
        match event {
            MouseEvent::Move { x, y } => seen.moves.push((x, y)),
            MouseEvent::LeftPressed => seen.clicks += 1,
            MouseEvent::VerticalScroll { value } => seen.wheel.push(value),
            _ => {}
        }
    }
}

/// The server's clipboard: it takes the client's text and offers [`SERVER_TEXT`].
struct Clipboard {
    events: Arc<Mutex<Option<UnboundedSender<ServerEvent>>>>,
    shared: Arc<Mutex<Shared>>,
}

impl std::fmt::Debug for Clipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Clipboard").finish_non_exhaustive()
    }
}

impl Clipboard {
    fn send(&self, message: ironrdp_cliprdr::backend::ClipboardMessage) {
        if let Some(events) = lock(&self.events).as_ref() {
            let _ = events.send(ServerEvent::Clipboard(message));
        }
    }
}

impl ironrdp_core::AsAny for Clipboard {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl ironrdp_cliprdr::backend::CliprdrBackend for Clipboard {
    fn temporary_directory(&self) -> &str {
        ""
    }

    fn client_capabilities(&self) -> ironrdp_cliprdr::pdu::ClipboardGeneralCapabilityFlags {
        ironrdp_cliprdr::pdu::ClipboardGeneralCapabilityFlags::empty()
    }

    fn on_ready(&mut self) {
        self.on_request_format_list();
    }

    fn on_request_format_list(&mut self) {
        use ironrdp_cliprdr::backend::ClipboardMessage;
        use ironrdp_cliprdr::pdu::{ClipboardFormat, ClipboardFormatId};
        self.send(ClipboardMessage::SendInitiateCopy(vec![
            ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT),
        ]));
    }

    fn on_process_negotiated_capabilities(
        &mut self,
        _capabilities: ironrdp_cliprdr::pdu::ClipboardGeneralCapabilityFlags,
    ) {
    }

    fn on_remote_copy(&mut self, available_formats: &[ironrdp_cliprdr::pdu::ClipboardFormat]) {
        use ironrdp_cliprdr::backend::ClipboardMessage;
        use ironrdp_cliprdr::pdu::ClipboardFormatId;
        if available_formats
            .iter()
            .any(|format| format.id == ClipboardFormatId::CF_UNICODETEXT)
        {
            self.send(ClipboardMessage::SendInitiatePaste(
                ClipboardFormatId::CF_UNICODETEXT,
            ));
        }
    }

    fn on_format_data_request(&mut self, _request: ironrdp_cliprdr::pdu::FormatDataRequest) {
        use ironrdp_cliprdr::backend::ClipboardMessage;
        self.send(ClipboardMessage::SendFormatData(
            ironrdp_cliprdr::pdu::FormatDataResponse::new_unicode_string(SERVER_TEXT),
        ));
    }

    fn on_format_data_response(&mut self, response: ironrdp_cliprdr::pdu::FormatDataResponse<'_>) {
        if let Ok(text) = response.to_unicode_string() {
            let mut shared = lock(&self.shared);
            lock(&shared.seen).clipboard = Some(text);
            shared.clipboard = true;
            shared.repaint();
        }
    }

    fn on_file_contents_request(&mut self, _request: ironrdp_cliprdr::pdu::FileContentsRequest) {}

    fn on_file_contents_response(
        &mut self,
        _response: ironrdp_cliprdr::pdu::FileContentsResponse<'_>,
    ) {
    }

    fn on_lock(&mut self, _data_id: ironrdp_cliprdr::pdu::LockDataId) {}

    fn on_unlock(&mut self, _data_id: ironrdp_cliprdr::pdu::LockDataId) {}
}

struct ClipboardFactory {
    events: Arc<Mutex<Option<UnboundedSender<ServerEvent>>>>,
    shared: Arc<Mutex<Shared>>,
}

impl ironrdp_cliprdr::backend::CliprdrBackendFactory for ClipboardFactory {
    fn build_cliprdr_backend(&self) -> Box<dyn ironrdp_cliprdr::backend::CliprdrBackend> {
        Box::new(Clipboard {
            events: Arc::clone(&self.events),
            shared: Arc::clone(&self.shared),
        })
    }
}

impl ServerEventSender for ClipboardFactory {
    fn set_sender(&mut self, sender: UnboundedSender<ServerEvent>) {
        *lock(&self.events) = Some(sender);
    }
}

impl CliprdrServerFactory for ClipboardFactory {}

/// The test certificate's DER.
#[must_use]
pub fn certificate_der() -> Vec<u8> {
    use tokio_rustls::rustls::pki_types::CertificateDer;
    use tokio_rustls::rustls::pki_types::pem::PemObject as _;
    CertificateDer::from_pem_slice(CERTIFICATE.as_bytes())
        .unwrap()
        .to_vec()
}

fn public_key(der: &[u8]) -> Vec<u8> {
    use x509_cert::der::Decode as _;
    let cert = x509_cert::Certificate::from_der(der).unwrap();
    cert.tbs_certificate
        .subject_public_key_info
        .subject_public_key
        .as_bytes()
        .unwrap()
        .to_owned()
}

/// Starts a server on 127.0.0.1 (a free port), on a thread of its own: `ironrdp-server`'s
/// futures aren't `Send`.
///
/// # Errors
///
/// When the port can't be bound.
pub fn serve() -> std::io::Result<TestServer> {
    use tokio_rustls::rustls;
    use tokio_rustls::rustls::pki_types::pem::PemObject as _;
    use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};

    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let address: SocketAddr = listener.local_addr()?;
    let seen = Arc::new(Mutex::new(Seen::default()));
    let server_seen = Arc::clone(&seen);
    std::thread::Builder::new()
        .name("opensesh-rdp-test-server".to_owned())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let local = tokio::task::LocalSet::new();
            local.block_on(&runtime, async move {
                let cert = CertificateDer::from_pem_slice(CERTIFICATE.as_bytes()).unwrap();
                let key = PrivateKeyDer::from_pem_slice(KEY.as_bytes()).unwrap();
                let key_bytes = public_key(&cert);
                let config = rustls::ServerConfig::builder_with_provider(Arc::new(
                    rustls::crypto::ring::default_provider(),
                ))
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_no_client_auth()
                .with_single_cert(vec![cert], key)
                .unwrap();
                let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
                let listener = TcpListener::from_std(listener).unwrap();
                // A server for each connection, so several clients can be connected at once.
                while let Ok((stream, _)) = listener.accept().await {
                    let shared = Arc::new(Mutex::new(Shared {
                        size: (1024, 768),
                        seen: Arc::clone(&server_seen),
                        ..Shared::default()
                    }));
                    let mut server = RdpServer::builder()
                        .with_addr(address)
                        .with_hybrid(acceptor.clone(), key_bytes.clone())
                        .with_input_handler(Input(Arc::clone(&shared)))
                        .with_display_handler(Display(Arc::clone(&shared)))
                        .with_cliprdr_factory(Some(Box::new(ClipboardFactory {
                            events: Arc::new(Mutex::new(None)),
                            shared: Arc::clone(&shared),
                        })))
                        .with_honor_client_desktop_size(true)
                        .build();
                    server.set_credentials(Some(Credentials {
                        username: USER.to_owned(),
                        password: PASSWORD.to_owned(),
                        domain: None,
                    }));
                    tokio::task::spawn_local(async move {
                        if let Err(error) = server.run_connection(stream).await {
                            tracing::debug!("RDP test server: a connection ended: {error:#}");
                        }
                    });
                }
            });
        })?;
    Ok(TestServer {
        port: address.port(),
        seen,
    })
}
