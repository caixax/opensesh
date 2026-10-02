//! Sprint 13 spike: an RDP client on IronRDP (connector, session, our own TLS on rustls with
//! `ring`) against an in-process IronRDP server with NLA. It checks that:
//!
//! 1. the connection sequence completes with CredSSP (NTLM), the server's certificate captured
//!    by our verifier before any credential is sent;
//! 2. graphics arrive as rectangles into a `DecodedImage`, with the server's pixels;
//! 3. a key press and a mouse move reach the server as scancodes and positions;
//! 4. a wrong password is refused with a reason.
//!
//! Run: `cargo run --release` (prints what happened, exits non-zero on failure).

use std::net::SocketAddr;
use std::num::{NonZeroU16, NonZeroUsize};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail};
use ironrdp::connector::{self, ClientConnector, ConnectionResult, Credentials, DesktopSize};
use ironrdp::graphics::image_processing::PixelFormat;
use ironrdp::input::{Database, MousePosition, Operation, Scancode};
use ironrdp::pdu::gcc::KeyboardType;
use ironrdp::pdu::rdp::capability_sets::MajorPlatformType;
use ironrdp::pdu::rdp::client_info::{PerformanceFlags, TimezoneInfo};
use ironrdp::server::{
    BitmapUpdate, DisplayUpdate, KeyboardEvent, MouseEvent, RdpServer, RdpServerDisplay,
    RdpServerDisplayUpdates, RdpServerInputHandler,
};
use ironrdp::session::image::DecodedImage;
use ironrdp::session::{ActiveStage, ActiveStageBuilder, ActiveStageOutput};
use ironrdp_tokio::{FramedWrite as _, TokioFramed, split_tokio_framed};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::rustls;

const CERT: &str = include_str!("../cert.pem");
const KEY: &str = include_str!("../key.pem");
const USER: &str = "tester";
const PASSWORD: &str = "right password";
const WIDTH: u16 = 320;
const HEIGHT: u16 = 200;

// ---------------------------------------------------------------- the server

#[derive(Default)]
struct Seen {
    keys: Vec<(u8, bool, bool)>,
    moves: Vec<(u16, u16)>,
}

struct Input(Arc<Mutex<Seen>>);

impl RdpServerInputHandler for Input {
    fn keyboard(&mut self, event: KeyboardEvent) {
        let mut seen = self.0.lock().unwrap();
        match event {
            KeyboardEvent::Pressed { code, extended } => seen.keys.push((code, extended, true)),
            KeyboardEvent::Released { code, extended } => seen.keys.push((code, extended, false)),
            _ => {}
        }
    }

    fn mouse(&mut self, event: MouseEvent) {
        if let MouseEvent::Move { x, y } = event {
            self.0.lock().unwrap().moves.push((x, y));
        }
    }
}

struct Display;

struct Updates {
    sent: bool,
}

#[async_trait::async_trait]
impl RdpServerDisplayUpdates for Updates {
    async fn next_update(&mut self) -> anyhow::Result<Option<DisplayUpdate>> {
        if self.sent {
            // Nothing more: wait forever (cancellation safe).
            std::future::pending::<()>().await;
        }
        self.sent = true;
        // A 64x32 block of one colour (B, G, R, A in memory for Bgra32... see the format).
        let (w, h) = (64_u16, 32_u16);
        let mut data = Vec::with_capacity(usize::from(w) * usize::from(h) * 4);
        for _ in 0..usize::from(w) * usize::from(h) {
            data.extend_from_slice(&[0x30, 0x60, 0x90, 0xFF]);
        }
        Ok(Some(DisplayUpdate::Bitmap(BitmapUpdate {
            x: 10,
            y: 20,
            width: NonZeroU16::new(w).unwrap(),
            height: NonZeroU16::new(h).unwrap(),
            format: ironrdp::server::PixelFormat::BgrA32,
            data: data.into(),
            stride: NonZeroUsize::new(usize::from(w) * 4).unwrap(),
        })))
    }
}

#[async_trait::async_trait]
impl RdpServerDisplay for Display {
    async fn size(&mut self) -> ironrdp::server::DesktopSize {
        ironrdp::server::DesktopSize {
            width: WIDTH,
            height: HEIGHT,
        }
    }

    async fn updates(&mut self) -> anyhow::Result<Box<dyn RdpServerDisplayUpdates>> {
        Ok(Box::new(Updates { sent: false }))
    }
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn public_key_of(der: &[u8]) -> anyhow::Result<Vec<u8>> {
    use x509_cert::der::Decode as _;
    let cert = x509_cert::Certificate::from_der(der)?;
    Ok(cert
        .tbs_certificate
        .subject_public_key_info
        .subject_public_key
        .as_bytes()
        .context("unaligned public key")?
        .to_owned())
}

async fn serve(seen: Arc<Mutex<Seen>>) -> anyhow::Result<SocketAddr> {
    use rustls::pki_types::pem::PemObject as _;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    let cert = CertificateDer::from_pem_slice(CERT.as_bytes())?;
    let key = PrivateKeyDer::from_pem_slice(KEY.as_bytes())?;
    let public_key = public_key_of(&cert)?;
    let config = rustls::ServerConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)?;
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    // The server isn't Send: built and run on a thread of its own, with a local task set.
    let listener = listener.into_std()?;
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let local = tokio::task::LocalSet::new();
        local.block_on(&runtime, async move {
            let mut server = RdpServer::builder()
                .with_addr(addr)
                .with_hybrid(acceptor, public_key)
                .with_input_handler(Input(seen))
                .with_display_handler(Display)
                .build();
            server.set_credentials(Some(ironrdp::server::Credentials {
                username: USER.to_owned(),
                password: PASSWORD.to_owned(),
                domain: None,
            }));
            let listener = TcpListener::from_std(listener).unwrap();
            while let Ok((stream, _)) = listener.accept().await {
                if let Err(error) = server.run_connection(stream).await {
                    println!("server: connection ended: {error:#}");
                }
            }
        });
    });
    Ok(addr)
}

// ---------------------------------------------------------------- the client

/// Accepts any certificate and keeps it, for the caller to check (as SSH host keys).
#[derive(Debug, Default)]
struct Capture(Mutex<Option<Vec<u8>>>);

impl rustls::client::danger::ServerCertVerifier for Capture {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        _: &[rustls::pki_types::CertificateDer<'_>],
        _: &rustls::pki_types::ServerName<'_>,
        _: &[u8],
        _: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        *self.0.lock().unwrap() = Some(end_entity.to_vec());
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &provider().signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &provider().signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        provider().signature_verification_algorithms.supported_schemes()
    }
}

/// TLS without key logging and without resumption (CredSSP doesn't allow it): the stream and the
/// server's certificate (DER).
async fn tls<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    host: &str,
) -> anyhow::Result<(tokio_rustls::client::TlsStream<S>, Vec<u8>)> {
    let capture = Arc::new(Capture::default());
    let mut config = rustls::ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(capture.clone())
        .with_no_client_auth();
    config.resumption = rustls::client::Resumption::disabled();
    let name = rustls::pki_types::ServerName::try_from(host.to_owned())?;
    let mut stream = tokio_rustls::TlsConnector::from(Arc::new(config))
        .connect(name, stream)
        .await?;
    stream.flush().await?;
    let cert = capture.0.lock().unwrap().take().context("no certificate")?;
    Ok((stream, cert))
}

struct NoKerberos;

impl ironrdp_tokio::NetworkClient for NoKerberos {
    async fn send(&mut self, _: &sspi::generator::NetworkRequest) -> connector::ConnectorResult<Vec<u8>> {
        Err(connector::general_err!("Kerberos isn't supported"))
    }
}

fn config(password: &str) -> connector::Config {
    connector::Config {
        desktop_size: DesktopSize { width: WIDTH, height: HEIGHT },
        desktop_scale_factor: 0,
        enable_tls: false,
        enable_credssp: true,
        credentials: Credentials::UsernamePassword {
            username: USER.to_owned(),
            password: password.to_owned(),
        },
        domain: None,
        client_build: 0,
        client_name: "opensesh-spike".to_owned(),
        keyboard_type: KeyboardType::IbmEnhanced,
        keyboard_subtype: 0,
        keyboard_functional_keys_count: 12,
        keyboard_layout: 0,
        ime_file_name: String::new(),
        bitmap: Some(connector::BitmapConfig {
            lossy_compression: true,
            color_depth: 32,
            codecs: ironrdp::pdu::rdp::capability_sets::client_codecs_capabilities(&["qoi:off", "qoiz:off"])
                .map_err(|help| anyhow::anyhow!("{help}"))
                .unwrap(),
        }),
        dig_product_id: String::new(),
        client_dir: String::new(),
        alternate_shell: String::new(),
        work_dir: String::new(),
        platform: MajorPlatformType::UNSPECIFIED,
        hardware_id: None,
        request_data: None,
        autologon: false,
        enable_audio_playback: false,
        performance_flags: PerformanceFlags::default(),
        license_cache: None,
        timezone_info: TimezoneInfo::default(),
        compression_type: None,
        enable_server_pointer: true,
        pointer_software_rendering: false,
        multitransport_flags: None,
    }
}

type Upgraded = TokioFramed<tokio_rustls::client::TlsStream<TcpStream>>;

async fn connect(addr: SocketAddr, password: &str) -> anyhow::Result<(ConnectionResult, Upgraded, Vec<u8>)> {
    let stream = TcpStream::connect(addr).await?;
    let local = stream.local_addr()?;
    let mut framed = TokioFramed::new(stream);
    let mut connector = ClientConnector::new(config(password), local);
    let should_upgrade = ironrdp_tokio::connect_begin(&mut framed, &mut connector).await?;
    let (stream, leftover) = framed.into_inner();
    let (tls_stream, cert) = tls(stream, "127.0.0.1").await?;
    // Here the app checks `cert` (trust on first use) before any credential goes out.
    let upgraded = ironrdp_tokio::mark_as_upgraded(should_upgrade, &mut connector);
    let mut framed = TokioFramed::new_with_leftover(tls_stream, leftover);
    let result = ironrdp_tokio::connect_finalize(
        upgraded,
        connector,
        &mut framed,
        &mut NoKerberos,
        connector::ServerName::new("127.0.0.1"),
        public_key_of(&cert)?,
        None,
    )
    .await?;
    Ok((result, framed, cert))
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let started = Instant::now();
    let seen = Arc::new(Mutex::new(Seen::default()));
    let addr = serve(Arc::clone(&seen)).await?;
    println!("server on {addr}");

    // A wrong password first.
    match connect(addr, "wrong").await {
        Ok(_) => bail!("a wrong password was accepted"),
        Err(error) => println!("wrong password refused: {error:#}"),
    }

    let (result, framed, cert) = connect(addr, PASSWORD).await?;
    println!(
        "connected in {:?}: desktop {}x{}, certificate {} bytes",
        started.elapsed(),
        result.desktop_size.width,
        result.desktop_size.height,
        cert.len()
    );
    let desktop = result.desktop_size;
    let mut stage: ActiveStage = ActiveStageBuilder {
        static_channels: result.static_channels,
        user_channel_id: result.user_channel_id,
        io_channel_id: result.io_channel_id,
        message_channel_id: result.message_channel_id,
        share_id: result.share_id,
        compression_type: result.compression_type,
        enable_server_pointer: result.enable_server_pointer,
        pointer_software_rendering: result.pointer_software_rendering,
    }
    .build();
    let mut image = DecodedImage::new(PixelFormat::RgbA32, desktop.width, desktop.height);
    let (mut reader, mut writer) = split_tokio_framed(framed);

    // Graphics: wait for the block.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut regions = Vec::new();
    let painted = |image: &DecodedImage| {
        let offset = (22 * usize::from(image.width()) + 12) * 4;
        image.data()[offset + 3] != 0
    };
    while !painted(&image) {
        if Instant::now() > deadline {
            bail!("no graphics");
        }
        let (action, payload) =
            tokio::time::timeout(Duration::from_secs(3), reader.read_pdu()).await??;
        println!("pdu {action:?}, {} bytes", payload.len());
        for output in stage.process(&mut image, action, &payload)? {
            match output {
                ActiveStageOutput::ResponseFrame(frame) => {
                    println!("  response {} bytes", frame.len());
                    writer.write_all(&frame).await?;
                }
                ActiveStageOutput::GraphicsUpdate(region) => {
                    println!("  graphics {region:?}");
                    regions.push(region);
                }
                ActiveStageOutput::Terminate(reason) => bail!("terminated: {reason:?}"),
                other => println!("  {}", match other { ActiveStageOutput::PointerDefault => "pointer default", ActiveStageOutput::PointerHidden => "pointer hidden", ActiveStageOutput::PointerPosition { .. } => "pointer position", ActiveStageOutput::PointerBitmap(_) => "pointer bitmap", ActiveStageOutput::DeactivateAll => "deactivate all", _ => "other" }),
            }
        }
    }
    let pixel = |x: usize, y: usize| {
        let offset = (y * usize::from(image.width()) + x) * 4;
        image.data()[offset..offset + 4].to_vec()
    };
    println!(
        "graphics: {regions:?}; pixel (12, 22) = {:?} (RGBA), (0, 0) = {:?}",
        pixel(12, 22),
        pixel(0, 0)
    );
    // RemoteFX is lossy: close to the server's colour, not equal.
    let close = |a: u8, b: u8| a.abs_diff(b) <= 8;
    let got = pixel(12, 22);
    if !(close(got[0], 0x90) && close(got[1], 0x60) && close(got[2], 0x30)) {
        bail!("the pixel isn't the server's colour");
    }

    // Input: 'A' (scancode 0x1E) pressed and released, the Delete key (extended 0x53), a move.
    let mut database = Database::new();
    let events = database.apply([
        Operation::KeyPressed(Scancode::from_u8(false, 0x1E)),
        Operation::KeyReleased(Scancode::from_u8(false, 0x1E)),
        Operation::KeyPressed(Scancode::from_u8(true, 0x53)),
        Operation::KeyReleased(Scancode::from_u8(true, 0x53)),
        Operation::MouseMove(MousePosition { x: 100, y: 50 }),
    ]);
    for output in stage.process_fastpath_input(&mut image, &events)? {
        if let ActiveStageOutput::ResponseFrame(frame) = output {
            writer.write_all(&frame).await?;
        }
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        {
            let seen = seen.lock().unwrap();
            if seen.keys.len() >= 4 && !seen.moves.is_empty() {
                println!("server saw keys {:?} and moves {:?}", seen.keys, seen.moves);
                break;
            }
        }
        if Instant::now() > deadline {
            bail!("the input didn't arrive: {:?} {:?}", seen.lock().unwrap().keys, seen.lock().unwrap().moves);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // A resize request (display control, if the server offers it).
    match stage.encode_resize(640, 400, None, None) {
        Some(frame) => {
            writer.write_all(&frame?).await?;
            println!("resize sent through display control");
        }
        None => println!("the server has no display control: a resize means reconnecting"),
    }

    for output in stage.graceful_shutdown()? {
        if let ActiveStageOutput::ResponseFrame(frame) = output {
            writer.write_all(&frame).await?;
        }
    }
    println!("all good in {:?}", started.elapsed());
    Ok(())
}
