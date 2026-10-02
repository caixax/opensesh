//! One RDP session (ADR 0034): connecting (TCP, our TLS, the server's certificate decided by the
//! app, NLA), then the active loop: graphics as changed rectangles, the pointer, input, the text
//! clipboard and resizing. Everything the app must hear goes out as [`FromHelper`] messages;
//! everything it asks comes in as [`ToHelper`] ones.

use std::time::Duration;

use ironrdp::cliprdr::CliprdrClient;
use ironrdp::cliprdr::pdu::{ClipboardFormatId, FormatDataResponse};
use ironrdp::connector::{
    self, ClientConnector, ConnectionResult, ConnectorErrorKind, Credentials, DesktopSize,
};
use ironrdp::displaycontrol::client::DisplayControlClient;
use ironrdp::dvc::DrdynvcClient;
use ironrdp::graphics::image_processing::PixelFormat;
use ironrdp::input::{Database, MouseButton, MousePosition, Operation, Scancode, WheelRotations};
use ironrdp::pdu::gcc::KeyboardType;
use ironrdp::pdu::input::fast_path::FastPathInputEvent;
use ironrdp::pdu::rdp::capability_sets::{
    BitmapCodecs, CaptureFlags, Codec, CodecProperty, EntropyBits, MajorPlatformType,
    RemoteFxContainer, RfxCaps, RfxCapset, RfxClientCapsContainer, RfxICap, RfxICapFlags,
};
use ironrdp::pdu::rdp::client_info::{PerformanceFlags, TimezoneInfo};
use ironrdp::session::image::DecodedImage;
use ironrdp::session::{ActiveStage, ActiveStageBuilder, ActiveStageOutput};
use ironrdp_tokio::{FramedWrite as _, TokioFramed, split_tokio_framed};
use opensesh_rdp_protocol::frame::Rect;
use opensesh_rdp_protocol::keys::CTRL_ALT_DEL;
use opensesh_rdp_protocol::{
    Button, Connect, Control, Event, FromHelper, Pixels, PointerPicture, Status, ToHelper,
};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{Receiver, Sender, UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::clipboard::{self, ClipboardEvent};

/// The code of the `RemoteFX` codec in the bitmap codecs capability (MS-RDPRFX).
const REMOTEFX: u8 = 3;

/// How a connection ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ended {
    /// The app ended it (or went away).
    Shutdown,
    /// Not connected, or no longer: a code and why.
    Lost(&'static str, String),
    /// The password was refused.
    Refused(String),
    /// Connect again at a new size (the server has no display control).
    Resize(u16, u16),
}

/// What a session keeps between connections.
#[derive(Debug, Default)]
pub struct Memory {
    /// A certificate the app accepted for this session (its fingerprint).
    pub trusted: Option<String>,
    /// This computer's clipboard text, to offer the server.
    pub clipboard: Option<String>,
}

type Upgraded = TokioFramed<crate::tls::TlsStream<TcpStream>>;

/// NLA's network client: Kerberos (which would reach a KDC) isn't supported, NTLM needs none.
struct NoKerberos;

impl ironrdp_tokio::NetworkClient for NoKerberos {
    async fn send(
        &mut self,
        _request: &sspi::generator::NetworkRequest,
    ) -> connector::ConnectorResult<Vec<u8>> {
        Err(connector::general_err!("Kerberos isn't supported"))
    }
}

/// `DOMAIN\user` or `user@domain` as user and domain (a domain given apart wins).
fn split_user(user: &str, domain: Option<&str>) -> (String, Option<String>) {
    let domain = domain.map(str::trim).filter(|domain| !domain.is_empty());
    if let Some((parsed_domain, name)) = user.split_once('\\') {
        return (
            name.to_owned(),
            domain
                .map(str::to_owned)
                .or_else(|| Some(parsed_domain.to_owned())),
        );
    }
    if domain.is_none()
        && let Some((name, parsed_domain)) = user.rsplit_once('@')
    {
        return (name.to_owned(), Some(parsed_domain.to_owned()));
    }
    (user.to_owned(), domain.map(str::to_owned))
}

/// The bitmap codecs we decode: RemoteFX (and the plain bitmaps every server has).
fn codecs() -> BitmapCodecs {
    BitmapCodecs(vec![Codec {
        id: REMOTEFX,
        property: CodecProperty::RemoteFx(RemoteFxContainer::ClientContainer(
            RfxClientCapsContainer {
                capture_flags: CaptureFlags::empty(),
                caps_data: RfxCaps(RfxCapset(vec![RfxICap {
                    flags: RfxICapFlags::empty(),
                    entropy_bits: EntropyBits::Rlgr3,
                }])),
            },
        )),
    }])
}

/// What a connector error means, for people (without IronRDP's source locations).
fn describe(error: &connector::ConnectorError) -> (&'static str, String) {
    match error.kind() {
        ConnectorErrorKind::Credssp(_) | ConnectorErrorKind::AccessDenied => (
            "auth",
            "the user name or password was refused (or the account may not sign in remotely)"
                .to_owned(),
        ),
        ConnectorErrorKind::Negotiation(failure) => (
            "protocol",
            format!("the server refused the security protocol: {failure}"),
        ),
        ConnectorErrorKind::Reason(reason) => ("protocol", reason.clone()),
        other => {
            let mut text = other.to_string();
            let mut source = std::error::Error::source(error);
            while let Some(cause) = source {
                text = format!("{text}: {cause}");
                source = cause.source();
            }
            ("protocol", text)
        }
    }
}

fn label(settings: &Connect) -> String {
    if settings.port == 3389 {
        settings.server_name.clone()
    } else {
        format!("{}:{}", settings.server_name, settings.port)
    }
}

fn config(
    settings: &Connect,
    user: String,
    domain: Option<String>,
    password: &str,
) -> connector::Config {
    connector::Config {
        desktop_size: DesktopSize {
            width: settings.width,
            height: settings.height,
        },
        desktop_scale_factor: if settings.scale_factor > 100 {
            settings.scale_factor
        } else {
            0
        },
        // NLA first; plain TLS for servers without it (xrdp), whose login then uses the
        // credentials sent in the session's info (autologon).
        enable_tls: true,
        enable_credssp: true,
        credentials: Credentials::UsernamePassword {
            username: user,
            password: password.to_owned(),
        },
        domain,
        client_build: 0,
        client_name: settings.client_name.chars().take(15).collect(),
        keyboard_type: KeyboardType::IbmEnhanced,
        keyboard_subtype: 0,
        keyboard_functional_keys_count: 12,
        keyboard_layout: settings.keyboard_layout,
        ime_file_name: String::new(),
        bitmap: Some(connector::BitmapConfig {
            lossy_compression: true,
            color_depth: 32,
            codecs: codecs(),
        }),
        dig_product_id: String::new(),
        client_dir: String::new(),
        alternate_shell: String::new(),
        work_dir: String::new(),
        platform: MajorPlatformType::UNSPECIFIED,
        hardware_id: None,
        request_data: None,
        autologon: true,
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

/// The session's way out to the app.
#[derive(Debug, Clone)]
pub struct Out(pub Sender<FromHelper>);

impl Out {
    /// Sends a message; `false` when the app is gone.
    pub async fn send(&self, message: FromHelper) -> bool {
        self.0.send(message).await.is_ok()
    }

    async fn status(&self, status: Status) {
        self.send(FromHelper::Event(Event::Status(status))).await;
    }
}

/// Connects with `settings` and `password`, runs the session, and says how it ended. `inbox`
/// carries what the app asks; `memory` keeps the accepted certificate and the clipboard.
pub async fn connect_and_run(
    settings: &mut Connect,
    password: &str,
    inbox: &mut Receiver<ToHelper>,
    memory: &mut Memory,
    out: &Out,
) -> Ended {
    let (clipboard_events, clipboard_receiver) = unbounded_channel();
    let (result, framed) =
        match connect(settings, password, inbox, memory, out, clipboard_events).await {
            Ok(connected) => connected,
            Err(ended) => return ended,
        };
    active(
        settings,
        result,
        framed,
        clipboard_receiver,
        inbox,
        memory,
        out,
    )
    .await
}

/// Waits for the app's answer about the certificate, keeping what else it says.
async fn certificate_answer(
    inbox: &mut Receiver<ToHelper>,
    settings: &mut Connect,
    memory: &mut Memory,
) -> Result<bool, Ended> {
    loop {
        match inbox.recv().await {
            Some(ToHelper::Control(Control::Certificate { accept })) => return Ok(accept),
            Some(ToHelper::Control(Control::Resize { width, height })) => {
                settings.width = width;
                settings.height = height;
            }
            Some(ToHelper::Clipboard(text)) => memory.clipboard = Some(text),
            Some(ToHelper::Control(Control::Disconnect)) | None => return Err(Ended::Shutdown),
            Some(_) => {}
        }
    }
}

async fn connect(
    settings: &mut Connect,
    password: &str,
    inbox: &mut Receiver<ToHelper>,
    memory: &mut Memory,
    out: &Out,
    clipboard_events: UnboundedSender<ClipboardEvent>,
) -> Result<(ConnectionResult, Upgraded), Ended> {
    let timeout = Duration::from_secs(settings.timeout_secs.max(1));
    let late = || {
        Ended::Lost(
            "timeout",
            format!("the server didn't answer in {} s", timeout.as_secs()),
        )
    };
    out.status(Status::Connecting {
        label: label(settings),
    })
    .await;
    let address = (settings.address.as_str(), settings.port);
    let stream = match tokio::time::timeout(timeout, TcpStream::connect(address)).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(error)) => {
            return Err(Ended::Lost(
                "network",
                format!("could not connect to {}: {error}", label(settings)),
            ));
        }
        Err(_) => return Err(late()),
    };
    let _ = stream.set_nodelay(true);
    let local = stream
        .local_addr()
        .map_err(|error| Ended::Lost("network", error.to_string()))?;
    let (user, domain) = split_user(&settings.user, settings.domain.as_deref());
    let mut connector =
        ClientConnector::new(config(settings, user.clone(), domain, password), local)
            .with_static_channel(
                DrdynvcClient::new()
                    .with_dynamic_channel(DisplayControlClient::new(|_| Ok(Vec::new()))),
            );
    if settings.clipboard {
        connector.attach_static_channel(CliprdrClient::new(Box::new(clipboard::Backend::new(
            clipboard_events,
        ))));
    }
    let lost = |error: connector::ConnectorError| {
        let (code, reason) = describe(&error);
        if code == "auth" {
            Ended::Refused(reason)
        } else {
            Ended::Lost(code, reason)
        }
    };
    let mut framed = TokioFramed::new(stream);
    let should_upgrade = tokio::time::timeout(
        timeout,
        ironrdp_tokio::connect_begin(&mut framed, &mut connector),
    )
    .await
    .map_err(|_| late())?
    .map_err(lost)?;
    let (stream, leftover) = framed.into_inner();
    let (tls_stream, der) =
        tokio::time::timeout(timeout, crate::tls::upgrade(stream, &settings.server_name))
            .await
            .map_err(|_| late())?
            .map_err(|error| Ended::Lost("protocol", format!("TLS: {error}")))?;
    // The certificate is decided before any credential goes out.
    let certificate = crate::certificate::read(&der).map_err(|error| {
        Ended::Lost(
            "certificate",
            format!("the server's certificate can't be read: {error}"),
        )
    })?;
    if memory.trusted.as_deref() != Some(certificate.fingerprint.as_str()) {
        out.send(FromHelper::Event(Event::Certificate {
            fingerprint: certificate.fingerprint.clone(),
            subject: certificate.subject.clone(),
            key_type: certificate.key_type.clone(),
        }))
        .await;
        if !certificate_answer(inbox, settings, memory).await? {
            return Err(Ended::Lost(
                "certificate",
                "the server's certificate wasn't accepted".to_owned(),
            ));
        }
        memory.trusted = Some(certificate.fingerprint.clone());
    }
    let upgraded = ironrdp_tokio::mark_as_upgraded(should_upgrade, &mut connector);
    let mut framed = TokioFramed::new_with_leftover(tls_stream, leftover);
    out.status(Status::Authenticating {
        label: format!("{user}@{}", label(settings)),
    })
    .await;
    let result = tokio::time::timeout(
        timeout,
        ironrdp_tokio::connect_finalize(
            upgraded,
            connector,
            &mut framed,
            &mut NoKerberos,
            connector::ServerName::new(settings.server_name.clone()),
            certificate.public_key,
            None,
        ),
    )
    .await
    .map_err(|_| late())?
    .map_err(lost)?;
    Ok((result, framed))
}

/// The pixels of `rect` in `image` (RGBA), clipped to it.
fn pixels_of(image: &DecodedImage, rect: Rect) -> Option<Pixels> {
    let (width, height) = (usize::from(image.width()), usize::from(image.height()));
    let x = usize::from(rect.x).min(width);
    let y = usize::from(rect.y).min(height);
    let right = (usize::from(rect.x) + usize::from(rect.width)).min(width);
    let bottom = (usize::from(rect.y) + usize::from(rect.height)).min(height);
    if right <= x || bottom <= y {
        return None;
    }
    let data = image.data();
    let mut rgba = Vec::with_capacity((right - x) * (bottom - y) * 4);
    for row in y..bottom {
        rgba.extend_from_slice(data.get(row * width * 4 + x * 4..row * width * 4 + right * 4)?);
    }
    Some(Pixels {
        rect: Rect {
            x: u16::try_from(x).ok()?,
            y: u16::try_from(y).ok()?,
            width: u16::try_from(right - x).ok()?,
            height: u16::try_from(bottom - y).ok()?,
        },
        rgba,
    })
}

/// The whole desktop's size, then its pixels.
async fn show_all(image: &DecodedImage, out: &Out) -> bool {
    if !out
        .send(FromHelper::Event(Event::Size {
            width: image.width(),
            height: image.height(),
        }))
        .await
    {
        return false;
    }
    match pixels_of(
        image,
        Rect {
            x: 0,
            y: 0,
            width: image.width(),
            height: image.height(),
        },
    ) {
        Some(pixels) => out.send(FromHelper::Pixels(pixels)).await,
        None => true,
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "one loop: its branches read best together"
)]
async fn active(
    settings: &mut Connect,
    result: ConnectionResult,
    framed: Upgraded,
    mut clipboard_events: UnboundedReceiver<ClipboardEvent>,
    inbox: &mut Receiver<ToHelper>,
    memory: &mut Memory,
    out: &Out,
) -> Ended {
    let desktop = result.desktop_size;
    let activation = result.activation_factory;
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
    if !show_all(&image, out).await {
        return Ended::Shutdown;
    }
    out.status(Status::Connected {
        width: desktop.width,
        height: desktop.height,
    })
    .await;
    let (mut reader, mut writer) = split_tokio_framed(framed);
    let mut database = Database::new();
    loop {
        let outputs = tokio::select! {
            pdu = reader.read_pdu() => match pdu {
                Ok((action, payload)) => match stage.process(&mut image, action, &payload) {
                    Ok(outputs) => outputs,
                    Err(error) => return Ended::Lost("protocol", error.to_string()),
                },
                Err(error) => return Ended::Lost("lost", format!("the connection was lost: {error}")),
            },
            event = clipboard_events.recv() => {
                let Some(event) = event else { continue };
                match clipboard(&mut stage, event, memory.clipboard.as_deref(), out).await {
                    Ok(outputs) => outputs,
                    Err(error) => {
                        tracing::info!("clipboard: {error}");
                        Vec::new()
                    }
                }
            }
            message = inbox.recv() => {
                let Some(message) = message else {
                    return Ended::Shutdown;
                };
                match message {
                    ToHelper::Control(Control::Disconnect) => {
                        if let Ok(outputs) = stage.graceful_shutdown() {
                            for output in outputs {
                                if let ActiveStageOutput::ResponseFrame(frame) = output {
                                    let _ = writer.write_all(&frame).await;
                                }
                            }
                        }
                        return Ended::Shutdown;
                    }
                    ToHelper::Control(Control::Resize { width, height }) => {
                        let (width, height) = (u32::from(width.max(200)), u32::from(height.max(200)));
                        settings.width = u16::try_from(width).unwrap_or(u16::MAX);
                        settings.height = u16::try_from(height).unwrap_or(u16::MAX);
                        let scale = (settings.scale_factor > 100).then_some(settings.scale_factor);
                        match stage.encode_resize(width, height, scale, None) {
                            Some(Ok(frame)) => vec![ActiveStageOutput::ResponseFrame(frame)],
                            Some(Err(error)) => {
                                tracing::info!("resize: {error}");
                                Vec::new()
                            }
                            // No display control: connect again at that size.
                            None => {
                                if let Ok(outputs) = stage.graceful_shutdown() {
                                    for output in outputs {
                                        if let ActiveStageOutput::ResponseFrame(frame) = output {
                                            let _ = writer.write_all(&frame).await;
                                        }
                                    }
                                }
                                return Ended::Resize(settings.width, settings.height);
                            }
                        }
                    }
                    ToHelper::Clipboard(text) => {
                        memory.clipboard = Some(text);
                        match clipboard(&mut stage, ClipboardEvent::Offer, memory.clipboard.as_deref(), out).await {
                            Ok(outputs) => outputs,
                            Err(error) => {
                                tracing::info!("clipboard: {error}");
                                Vec::new()
                            }
                        }
                    }
                    ToHelper::Control(control) => {
                        let events = input_events(&mut database, control);
                        match stage.process_fastpath_input(&mut image, &events) {
                            Ok(outputs) => outputs,
                            Err(error) => return Ended::Lost("protocol", error.to_string()),
                        }
                    }
                    ToHelper::Password(_) => Vec::new(),
                }
            }
        };
        for output in outputs {
            match output {
                ActiveStageOutput::ResponseFrame(frame) => {
                    if let Err(error) = writer.write_all(&frame).await {
                        return Ended::Lost("lost", format!("the connection was lost: {error}"));
                    }
                }
                ActiveStageOutput::GraphicsUpdate(region) => {
                    let rect = Rect {
                        x: region.left,
                        y: region.top,
                        width: region.right.saturating_sub(region.left).saturating_add(1),
                        height: region.bottom.saturating_sub(region.top).saturating_add(1),
                    };
                    if let Some(pixels) = pixels_of(&image, rect)
                        && !out.send(FromHelper::Pixels(pixels)).await
                    {
                        return Ended::Shutdown;
                    }
                }
                ActiveStageOutput::PointerDefault => {
                    out.send(FromHelper::Event(Event::PointerDefault)).await;
                }
                ActiveStageOutput::PointerHidden => {
                    out.send(FromHelper::Event(Event::PointerHidden)).await;
                }
                ActiveStageOutput::PointerBitmap(pointer) => {
                    out.send(FromHelper::Pointer(PointerPicture {
                        width: pointer.width,
                        height: pointer.height,
                        hot_x: pointer.hotspot_x,
                        hot_y: pointer.hotspot_y,
                        rgba: pointer.bitmap_data.clone(),
                    }))
                    .await;
                }
                ActiveStageOutput::DeactivateAll => {
                    // The server changed the desktop (a resize): the activation sequence again,
                    // then a new image at the new size.
                    let mut sequence = activation.create();
                    let mut buf = ironrdp::core::WriteBuf::new();
                    loop {
                        let written = match ironrdp_tokio::single_sequence_step_read(
                            &mut reader,
                            &mut sequence,
                            &mut buf,
                        )
                        .await
                        {
                            Ok(written) => written,
                            Err(error) => return Ended::Lost("protocol", describe(&error).1),
                        };
                        if written.size().is_some()
                            && let Err(error) = writer.write_all(buf.filled()).await
                        {
                            return Ended::Lost(
                                "lost",
                                format!("the connection was lost: {error}"),
                            );
                        }
                        if let ironrdp::connector::connection_activation::ConnectionActivationState::Finalized {
                            desktop_size,
                            share_id,
                            enable_server_pointer,
                            pointer_software_rendering,
                        } = sequence.connection_activation_state()
                        {
                            image = DecodedImage::new(PixelFormat::RgbA32, desktop_size.width, desktop_size.height);
                            stage.set_fastpath_processor(
                                ironrdp::session::fast_path::ProcessorBuilder {
                                    io_channel_id: sequence.io_channel_id(),
                                    user_channel_id: sequence.user_channel_id(),
                                    share_id,
                                    enable_server_pointer,
                                    pointer_software_rendering,
                                    bulk_decompressor: None,
                                }
                                .build(),
                            );
                            stage.set_share_id(share_id);
                            stage.set_enable_server_pointer(enable_server_pointer);
                            if !show_all(&image, out).await {
                                return Ended::Shutdown;
                            }
                            out.status(Status::Connected {
                                width: desktop_size.width,
                                height: desktop_size.height,
                            })
                            .await;
                            break;
                        }
                    }
                }
                ActiveStageOutput::Terminate(reason) => {
                    return Ended::Lost("ended", reason.description());
                }
                _ => {}
            }
        }
    }
}

/// What the clipboard asks for, done: the frames to send.
async fn clipboard(
    stage: &mut ActiveStage,
    event: ClipboardEvent,
    local_text: Option<&str>,
    out: &Out,
) -> Result<Vec<ActiveStageOutput>, String> {
    if let ClipboardEvent::Received(text) = event {
        out.send(FromHelper::Clipboard(text)).await;
        return Ok(Vec::new());
    }
    let Some(cliprdr) = stage.get_svc_processor_mut::<CliprdrClient>() else {
        return Ok(Vec::new());
    };
    let messages = match event {
        // An empty list when there is nothing to offer: the channel only becomes ready on
        // both sides once the client sent one (MS-RDPECLIP 1.3.2.1).
        ClipboardEvent::Offer => {
            let formats = match local_text {
                Some(_) => vec![clipboard::text_format()],
                None => Vec::new(),
            };
            cliprdr
                .initiate_copy(&formats)
                .map_err(|error| error.to_string())?
        }
        ClipboardEvent::Fetch => cliprdr
            .initiate_paste(ClipboardFormatId::CF_UNICODETEXT)
            .map_err(|error| error.to_string())?,
        ClipboardEvent::Send => {
            let response = match local_text {
                Some(text) => FormatDataResponse::new_unicode_string(text),
                None => FormatDataResponse::new_error(),
            };
            cliprdr
                .submit_format_data(response)
                .map_err(|error| error.to_string())?
        }
        ClipboardEvent::Received(_) => return Ok(Vec::new()),
    };
    let frame = stage
        .process_svc_processor_messages(messages)
        .map_err(|error| error.to_string())?;
    Ok(vec![ActiveStageOutput::ResponseFrame(frame)])
}

/// The fast-path events of an input.
fn input_events(database: &mut Database, control: Control) -> Vec<FastPathInputEvent> {
    let events = match control {
        Control::Key {
            code,
            extended,
            pressed,
        } => {
            let scancode = Scancode::from_u8(extended, code);
            database.apply([if pressed {
                Operation::KeyPressed(scancode)
            } else {
                Operation::KeyReleased(scancode)
            }])
        }
        Control::Unicode { character, pressed } => database.apply([if pressed {
            Operation::UnicodeKeyPressed(character)
        } else {
            Operation::UnicodeKeyReleased(character)
        }]),
        Control::Move { x, y } => database.apply([Operation::MouseMove(MousePosition { x, y })]),
        Control::Button { button, pressed } => {
            let button = match button {
                Button::Left => MouseButton::Left,
                Button::Middle => MouseButton::Middle,
                Button::Right => MouseButton::Right,
                Button::Back => MouseButton::X1,
                Button::Forward => MouseButton::X2,
            };
            database.apply([if pressed {
                Operation::MouseButtonPressed(button)
            } else {
                Operation::MouseButtonReleased(button)
            }])
        }
        Control::Wheel {
            vertical,
            horizontal,
        } => {
            let mut operations = Vec::new();
            if vertical != 0 {
                operations.push(Operation::WheelRotations(WheelRotations {
                    is_vertical: true,
                    rotation_units: vertical,
                }));
            }
            if horizontal != 0 {
                operations.push(Operation::WheelRotations(WheelRotations {
                    is_vertical: false,
                    rotation_units: horizontal,
                }));
            }
            database.apply(operations)
        }
        Control::ReleaseAll => database.release_all(),
        Control::Locks { caps, num, scroll } => {
            return vec![ironrdp::input::synchronize_event(scroll, num, caps, false)];
        }
        Control::CtrlAltDel => {
            let scancode =
                |key: opensesh_rdp_protocol::keys::Key| Scancode::from_u8(key.extended, key.code);
            let presses = CTRL_ALT_DEL.map(|key| Operation::KeyPressed(scancode(key)));
            let releases = CTRL_ALT_DEL.map(|key| Operation::KeyReleased(scancode(key)));
            let mut events = database.apply(presses);
            events.extend(database.apply(releases.into_iter().rev()));
            events
        }
        Control::Connect(_)
        | Control::Certificate { .. }
        | Control::Resize { .. }
        | Control::Reconnect
        | Control::Disconnect => return Vec::new(),
    };
    events.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn users_and_domains() {
        assert_eq!(
            split_user("CORP\\ana", None),
            ("ana".into(), Some("CORP".into()))
        );
        assert_eq!(
            split_user("ana@corp.local", None),
            ("ana".into(), Some("corp.local".into()))
        );
        assert_eq!(
            split_user("ana", Some("CORP")),
            ("ana".into(), Some("CORP".into()))
        );
        assert_eq!(split_user("ana", Some(" ")), ("ana".into(), None));
        assert_eq!(
            split_user("OLD\\ana", Some("NEW")),
            ("ana".into(), Some("NEW".into()))
        );
    }

    #[test]
    fn ctrl_alt_del_presses_then_releases() {
        let mut database = Database::new();
        let events = input_events(&mut database, Control::CtrlAltDel);
        assert_eq!(events.len(), 6);
        assert!(!database.is_key_pressed(Scancode::from_u8(false, 0x1D)));
    }
}
