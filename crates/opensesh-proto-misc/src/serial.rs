//! Serial ports as terminal backends (PLAN Sprint 12).
//!
//! - **Ports:** [`ports`] lists this computer's (with what they are: a USB adapter's maker and
//!   product), for the host editor.
//! - **Opening:** a port opens with its host's settings (speed, data bits, parity, stop bits,
//!   flow control) on a thread of its own (a driver may take a while), and reads and writes on
//!   two more: `serialport` is blocking, and none of it may run on the GUI thread.
//! - **Typing:** Enter sends CR, LF or CR LF ([`Newline`]), and with local echo the client shows
//!   what is typed (for devices that don't echo).
//! - **The hexadecimal view** ([`SerialControl::set_hex`]) shows received bytes as hex with
//!   their text beside, 16 to a line; it can be turned on and off while the session runs.
//! - **A port that goes away** (a USB adapter pulled out) ends the session with a reason;
//!   reconnecting opens it again.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use opensesh_core::hosts::{FlowControl, Newline, Parity};
use opensesh_ssh::log::SessionLog;
use opensesh_term::backend::{BackendError, BackendEvent, TermSize, TerminalBackend};

/// How long a read waits before it looks at the stop flag (and flushes a partial hex line).
const READ_TIMEOUT: Duration = Duration::from_millis(100);
/// A partial hex line gets its text column after this long without data.
const HEX_FLUSH: Duration = Duration::from_millis(250);
/// How long a break lasts.
const BREAK: Duration = Duration::from_millis(250);

/// A serial port of this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortInfo {
    /// What to open (`/dev/ttyUSB0`, `COM3`).
    pub name: String,
    /// What it is, when the system says (`FTDI FT232R USB UART`, `Bluetooth`).
    pub description: String,
}

/// The serial ports of this computer, by name (quick, but call it off the GUI thread: Windows
/// asks its device setup API).
#[must_use]
pub fn ports() -> Vec<PortInfo> {
    let mut list: Vec<PortInfo> = serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .map(|port| PortInfo {
            description: match port.port_type {
                serialport::SerialPortType::UsbPort(usb) => {
                    let named: Vec<String> = [usb.manufacturer, usb.product]
                        .into_iter()
                        .flatten()
                        .filter(|text| !text.trim().is_empty())
                        .collect();
                    if named.is_empty() {
                        format!("USB {:04x}:{:04x}", usb.vid, usb.pid)
                    } else {
                        named.join(" ")
                    }
                }
                serialport::SerialPortType::BluetoothPort => "Bluetooth".to_owned(),
                serialport::SerialPortType::PciPort | serialport::SerialPortType::Unknown => {
                    String::new()
                }
            },
            name: port.port_name,
        })
        .collect();
    list.sort_by(|a, b| natural(&a.name).cmp(&natural(&b.name)));
    list.dedup_by(|a, b| a.name == b.name);
    list
}

/// `COM10` after `COM9`: the name with its number apart.
fn natural(name: &str) -> (String, u64) {
    let digits = name.len() - name.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let (stem, number) = name.split_at(name.len() - digits);
    (stem.to_owned(), number.parse().unwrap_or(0))
}

/// How to open a port and type into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerialSpec {
    /// The device.
    pub device: String,
    /// Speed in bits per second.
    pub baud: u32,
    /// Data bits (5 to 8).
    pub data_bits: u8,
    /// Parity.
    pub parity: Parity,
    /// Stop bits (1 or 2).
    pub stop_bits: u8,
    /// Flow control.
    pub flow_control: FlowControl,
    /// What Enter sends.
    pub newline: Newline,
    /// Show what is typed.
    pub local_echo: bool,
    /// The session log: its file and whether raw.
    pub log: Option<(PathBuf, bool)>,
}

impl SerialSpec {
    /// A short description of the settings: `115200 8N1`.
    #[must_use]
    pub fn summary(&self) -> String {
        let parity = match self.parity {
            Parity::None => 'N',
            Parity::Even => 'E',
            Parity::Odd => 'O',
        };
        let flow = match self.flow_control {
            FlowControl::None => "",
            FlowControl::Software => ", XON/XOFF",
            FlowControl::Hardware => ", RTS/CTS",
        };
        format!(
            "{} {}{}{}{flow}",
            self.baud, self.data_bits, parity, self.stop_bits
        )
    }
}

/// A device the backend reads and writes: a serial port, or the test runs' loopback.
pub trait Device: Send {
    /// Reads what arrived; `Ok(0)` when nothing came within the read timeout.
    ///
    /// # Errors
    ///
    /// When the device is gone.
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize>;
    /// Writes all of `bytes`.
    ///
    /// # Errors
    ///
    /// When the device is gone.
    fn write_all(&mut self, bytes: &[u8]) -> std::io::Result<()>;
    /// A second handle on the same device (one thread reads, the other writes).
    ///
    /// # Errors
    ///
    /// When the device can't be shared.
    fn try_clone(&self) -> std::io::Result<Box<dyn Device>>;
    /// Holds the line in the break state for a moment (a router's "break to ROM monitor").
    ///
    /// # Errors
    ///
    /// When the device can't.
    fn send_break(&mut self, duration: Duration) -> std::io::Result<()>;
}

struct Port(Box<dyn serialport::SerialPort>);

fn io_error(error: serialport::Error) -> std::io::Error {
    std::io::Error::other(error.description)
}

impl Device for Port {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        match self.0.read(buffer) {
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => Ok(0),
            // A closed tty reads as end of file: the device went away.
            Ok(0) => Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof)),
            other => other,
        }
    }

    fn write_all(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        Write::write_all(&mut self.0, bytes)?;
        self.0.flush()
    }

    fn try_clone(&self) -> std::io::Result<Box<dyn Device>> {
        Ok(Box::new(Self(self.0.try_clone().map_err(io_error)?)))
    }

    fn send_break(&mut self, duration: Duration) -> std::io::Result<()> {
        self.0.set_break().map_err(io_error)?;
        std::thread::sleep(duration);
        self.0.clear_break().map_err(io_error)
    }
}

/// Opens `spec`'s device.
///
/// # Errors
///
/// Why it didn't open, for people (with the usual fix for a permission problem).
pub fn open(spec: &SerialSpec) -> Result<Box<dyn Device>, String> {
    let data_bits = match spec.data_bits {
        5 => serialport::DataBits::Five,
        6 => serialport::DataBits::Six,
        7 => serialport::DataBits::Seven,
        _ => serialport::DataBits::Eight,
    };
    let parity = match spec.parity {
        Parity::None => serialport::Parity::None,
        Parity::Even => serialport::Parity::Even,
        Parity::Odd => serialport::Parity::Odd,
    };
    let stop_bits = if spec.stop_bits == 2 {
        serialport::StopBits::Two
    } else {
        serialport::StopBits::One
    };
    let flow_control = match spec.flow_control {
        FlowControl::None => serialport::FlowControl::None,
        FlowControl::Software => serialport::FlowControl::Software,
        FlowControl::Hardware => serialport::FlowControl::Hardware,
    };
    serialport::new(spec.device.as_str(), spec.baud)
        .data_bits(data_bits)
        .parity(parity)
        .stop_bits(stop_bits)
        .flow_control(flow_control)
        .timeout(READ_TIMEOUT)
        .open()
        .map(|port| Box::new(Port(port)) as Box<dyn Device>)
        .map_err(|error| match error.kind {
            serialport::ErrorKind::Io(std::io::ErrorKind::PermissionDenied) if cfg!(unix) => format!(
                "no permission to open {}: add your user to the group that owns it (dialout or uucp), then log in again",
                spec.device
            ),
            serialport::ErrorKind::Io(std::io::ErrorKind::PermissionDenied)
            | serialport::ErrorKind::NoDevice => format!(
                "{} is in use by another program, or isn't there",
                spec.device
            ),
            _ => format!("could not open {}: {}", spec.device, error.description),
        })
}

/// The controls of a running serial session. Cheap to clone.
#[derive(Debug, Clone, Default)]
pub struct SerialControl {
    hex: Arc<AtomicBool>,
    breaks: Arc<AtomicBool>,
}

impl SerialControl {
    /// Shows received bytes in hexadecimal, or as text.
    pub fn set_hex(&self, on: bool) {
        self.hex.store(on, Ordering::Relaxed);
    }

    /// Whether received bytes show in hexadecimal.
    #[must_use]
    pub fn hex(&self) -> bool {
        self.hex.load(Ordering::Relaxed)
    }

    /// Sends a break (once the writer is free).
    pub fn send_break(&self) {
        self.breaks.store(true, Ordering::Relaxed);
    }
}

/// Received bytes as a hex dump: 16 to a line, a gap after 8, and their text (`.` for what
/// isn't printable) when a line is full or the line goes quiet.
#[derive(Debug, Default)]
pub struct HexView {
    line: Vec<u8>,
}

impl HexView {
    /// What to print for `bytes`.
    pub fn format(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for &byte in bytes {
            if self.line.len() == 8 {
                out.push(b' ');
            }
            out.extend(format!("{byte:02X} ").bytes());
            self.line.push(byte);
            if self.line.len() == 16 {
                out.extend(self.finish());
            }
        }
        out
    }

    /// Ends the current line (padded, so the text column lines up); nothing when it is empty.
    pub fn finish(&mut self) -> Vec<u8> {
        if self.line.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        let missing = 16 - self.line.len();
        out.extend(std::iter::repeat_n(
            b' ',
            missing * 3 + usize::from(self.line.len() <= 8),
        ));
        out.extend(b" \x1b[2m|");
        out.extend(self.line.iter().map(|&byte| {
            if (0x20..0x7f).contains(&byte) {
                byte
            } else {
                b'.'
            }
        }));
        out.extend(b"|\x1b[0m\r\n");
        self.line.clear();
        out
    }

    /// Whether a line is under way.
    #[must_use]
    pub fn pending(&self) -> bool {
        !self.line.is_empty()
    }
}

/// What typing `typed` sends: Enter (CR) as `newline`.
#[must_use]
pub fn encode(typed: &[u8], newline: Newline) -> Vec<u8> {
    let mut out = Vec::with_capacity(typed.len() + 1);
    for &byte in typed {
        match (byte, newline) {
            (b'\r', Newline::Cr) => out.push(b'\r'),
            (b'\r', Newline::Lf) => out.push(b'\n'),
            (b'\r', Newline::Crlf) => out.extend(b"\r\n"),
            _ => out.push(byte),
        }
    }
    out
}

enum Command {
    Input(Vec<u8>),
    Shutdown,
}

struct Handle {
    commands: Sender<Command>,
    stop: Arc<AtomicBool>,
}

impl TerminalBackend for Handle {
    fn write(&self, bytes: &[u8]) -> Result<(), BackendError> {
        self.commands
            .send(Command::Input(bytes.to_vec()))
            .map_err(|_| BackendError::Closed)
    }

    fn resize(&self, _size: TermSize) -> Result<(), BackendError> {
        Ok(())
    }

    fn shutdown(&self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.commands.send(Command::Shutdown);
    }
}

/// What [`start`] gives: the engine's handle and events, and the session's controls.
pub type Started = (
    Box<dyn TerminalBackend>,
    Receiver<BackendEvent>,
    SerialControl,
);

/// Starts a serial session on `spec`'s device, opened on a thread of its own.
///
/// # Errors
///
/// [`BackendError::Thread`] when a thread can't start.
pub fn start(spec: SerialSpec) -> Result<Started, BackendError> {
    let opener = spec.clone();
    start_with(spec, move || open(&opener))
}

/// [`start`] on the device `open` gives (the test runs' loopback).
///
/// # Errors
///
/// [`BackendError::Thread`] when a thread can't start.
pub fn start_with(
    spec: SerialSpec,
    open: impl FnOnce() -> Result<Box<dyn Device>, String> + Send + 'static,
) -> Result<Started, BackendError> {
    let (events, receiver) = crossbeam_channel::bounded(512);
    let (commands, command_receiver) = crossbeam_channel::unbounded();
    let stop = Arc::new(AtomicBool::new(false));
    let control = SerialControl::default();
    let shared = control.clone();
    let stopping = Arc::clone(&stop);
    std::thread::Builder::new()
        .name("opensesh-serial".to_owned())
        .spawn(move || run(&spec, open, &events, &command_receiver, &shared, &stopping))
        .map_err(BackendError::Thread)?;
    Ok((Box::new(Handle { commands, stop }), receiver, control))
}

fn note(events: &Sender<BackendEvent>, text: &str) {
    let _ = events.send(BackendEvent::Output(
        format!("\x1b[2m{text}\x1b[0m\r\n").into_bytes(),
    ));
}

fn run(
    spec: &SerialSpec,
    open: impl FnOnce() -> Result<Box<dyn Device>, String>,
    events: &Sender<BackendEvent>,
    commands: &Receiver<Command>,
    control: &SerialControl,
    stop: &Arc<AtomicBool>,
) {
    note(
        events,
        &format!("Opening {} ({})...", spec.device, spec.summary()),
    );
    let mut device = match open() {
        Ok(device) => device,
        Err(reason) => {
            let _ = events.send(BackendEvent::Error(reason));
            let _ = events.send(BackendEvent::Exited(None));
            return;
        }
    };
    let writer = match device.try_clone() {
        Ok(writer) => writer,
        Err(error) => {
            let _ = events.send(BackendEvent::Error(format!(
                "could not use {}: {error}",
                spec.device
            )));
            let _ = events.send(BackendEvent::Exited(None));
            return;
        }
    };
    note(events, &format!("Connected to {}.", spec.device));
    let log = spec.log.as_ref().and_then(|(path, raw)| {
        SessionLog::open(path, *raw)
            .map_err(
                |error| tracing::warn!(path = %path.display(), "session log not written: {error}"),
            )
            .ok()
    });
    // The writer: typed text, breaks.
    let writing = {
        let events = events.clone();
        let commands = commands.clone();
        let control = control.clone();
        let stop = Arc::clone(stop);
        let spec = spec.clone();
        std::thread::Builder::new()
            .name("opensesh-serial-write".to_owned())
            .spawn(move || write_loop(&spec, writer, &events, &commands, &control, &stop))
    };
    if let Err(error) = writing {
        let _ = events.send(BackendEvent::Error(format!(
            "could not start a thread: {error}"
        )));
        let _ = events.send(BackendEvent::Exited(None));
        return;
    }
    // The reader.
    let mut hex = HexView::default();
    let mut showing_hex = false;
    let mut last_data = Instant::now();
    let mut buffer = vec![0_u8; 8192];
    while !stop.load(Ordering::Relaxed) {
        let read = device.read(&mut buffer);
        // After the read: bytes that arrive once the view changed show in the new one.
        let want_hex = control.hex();
        if want_hex != showing_hex {
            showing_hex = want_hex;
            let mut switch = hex.finish();
            switch.extend(if want_hex {
                "\r\n\x1b[2m(hexadecimal view)\x1b[0m\r\n".as_bytes()
            } else {
                "\r\n\x1b[2m(text view)\x1b[0m\r\n".as_bytes()
            });
            let _ = events.send(BackendEvent::Output(switch));
        }
        match read {
            Ok(0) => {
                if hex.pending() && last_data.elapsed() >= HEX_FLUSH {
                    let _ = events.send(BackendEvent::Output(hex.finish()));
                }
            }
            Ok(count) => {
                last_data = Instant::now();
                let bytes = &buffer[..count];
                if let Some(log) = &log {
                    log.write(bytes);
                }
                let shown = if showing_hex {
                    hex.format(bytes)
                } else {
                    bytes.to_vec()
                };
                if events.send(BackendEvent::Output(shown)).is_err() {
                    break;
                }
            }
            Err(error) => {
                if !stop.load(Ordering::Relaxed) {
                    let _ = events.send(BackendEvent::Error(format!(
                        "{} went away: {error}",
                        spec.device
                    )));
                    let _ = events.send(BackendEvent::Exited(None));
                }
                break;
            }
        }
    }
    stop.store(true, Ordering::Relaxed);
}

fn write_loop(
    spec: &SerialSpec,
    mut device: Box<dyn Device>,
    events: &Sender<BackendEvent>,
    commands: &Receiver<Command>,
    control: &SerialControl,
    stop: &Arc<AtomicBool>,
) {
    while !stop.load(Ordering::Relaxed) {
        if control.breaks.swap(false, Ordering::Relaxed) {
            if let Err(error) = device.send_break(BREAK) {
                note(events, &format!("The break wasn't sent: {error}"));
            } else {
                note(events, "Break sent.");
            }
        }
        match commands.recv_timeout(READ_TIMEOUT) {
            Ok(Command::Input(bytes)) => {
                if spec.local_echo {
                    let _ = events.send(BackendEvent::Output(crate::telnet::local_echo(&bytes)));
                }
                if device.write_all(&encode(&bytes, spec.newline)).is_err() {
                    // The reader says why.
                    return;
                }
            }
            Ok(Command::Shutdown) | Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                stop.store(true, Ordering::Relaxed);
                return;
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
        }
    }
}

/// A device for tests and the app's test runs (never a real port): a loopback plug that says
/// hello and sends back what is written.
pub mod testing {
    use std::collections::VecDeque;
    use std::sync::{Arc, Condvar, Mutex, PoisonError};
    use std::time::Duration;

    use super::{Device, READ_TIMEOUT};

    /// What the loopback sends first.
    pub const HELLO: &[u8] = b"OpenSesh serial test device\r\n";

    type Shared = Arc<(Mutex<VecDeque<u8>>, Condvar)>;

    struct Loopback(Shared);

    /// A new loopback device.
    #[must_use]
    pub fn loopback() -> Box<dyn Device> {
        let shared: Shared = Arc::default();
        shared
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(HELLO);
        Box::new(Loopback(shared))
    }

    impl Device for Loopback {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let (lock, ready) = &*self.0;
            let queue = lock.lock().unwrap_or_else(PoisonError::into_inner);
            let (mut queue, _) = ready
                .wait_timeout_while(queue, READ_TIMEOUT, |queue| queue.is_empty())
                .unwrap_or_else(PoisonError::into_inner);
            let count = queue.len().min(buffer.len());
            for (slot, byte) in buffer.iter_mut().zip(queue.drain(..count)) {
                *slot = byte;
            }
            Ok(count)
        }

        fn write_all(&mut self, bytes: &[u8]) -> std::io::Result<()> {
            let (lock, ready) = &*self.0;
            lock.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend(bytes);
            ready.notify_all();
            Ok(())
        }

        fn try_clone(&self) -> std::io::Result<Box<dyn Device>> {
            Ok(Box::new(Self(Arc::clone(&self.0))))
        }

        fn send_break(&mut self, _duration: Duration) -> std::io::Result<()> {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
