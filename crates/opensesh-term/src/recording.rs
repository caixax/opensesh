//! Session recordings in asciicast v2 (PLAN Sprint 10): a JSON header line, then one JSON array
//! per event: `[seconds, "o", text]` for output and `[seconds, "r", "COLSxROWS"]` for a resize.
//! What was typed is never recorded (asciinema's `"i"` events), so a password typed at a prompt
//! that doesn't echo it stays out of the file.
//!
//! [`Recorder`] is a session [`Tap`]: it timestamps what the program prints and hands it to a
//! writer thread, which appends to the file (flushing when output pauses). [`read`] parses a
//! file for [`player`], a backend that prints a recording again at its pace or faster, with
//! pause, speed and restart; pauses longer than [`IDLE_LIMIT`] are shortened, as asciinema's
//! player does.

use std::fs::File;
use std::io::{BufWriter, Write as _};
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use serde_json::{Value, json};

use crate::backend::{BackendError, BackendEvent, TermSize, TerminalBackend};
use crate::session::Tap;

/// The longest pause a recording plays: longer ones are shortened to this.
pub const IDLE_LIMIT: f64 = 2.0;

/// Why a recording couldn't be read.
#[derive(Debug, thiserror::Error)]
pub enum CastError {
    /// The first line isn't an asciicast v2 header.
    #[error("not an asciicast v2 recording: {0}")]
    Header(String),
    /// A line isn't an event.
    #[error("line {line}: {message}")]
    Event {
        /// The line (from 1).
        line: usize,
        /// Why.
        message: String,
    },
}

/// A recording's header.
#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    /// Columns when it started.
    pub width: u16,
    /// Lines when it started.
    pub height: u16,
    /// When it started (seconds since 1970), if it says.
    pub timestamp: Option<i64>,
    /// Its title, if it has one.
    pub title: Option<String>,
}

/// One event of a recording.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The program printed this, at this many seconds.
    Output(f64, String),
    /// The terminal was resized to columns x lines.
    Resize(f64, u16, u16),
}

impl Event {
    /// When it happened, in seconds from the start.
    #[must_use]
    pub fn time(&self) -> f64 {
        match self {
            Self::Output(time, _) | Self::Resize(time, _, _) => *time,
        }
    }
}

/// A recording.
#[derive(Debug, Clone, PartialEq)]
pub struct Cast {
    /// Its header.
    pub header: Header,
    /// Its events, in time order.
    pub events: Vec<Event>,
}

impl Cast {
    /// How long it lasts, in seconds.
    #[must_use]
    pub fn duration(&self) -> f64 {
        self.events.last().map_or(0.0, Event::time)
    }
}

/// Reads a recording.
///
/// # Errors
///
/// [`CastError`] when it isn't asciicast v2 or a line isn't an event. Events of kinds other than
/// output and resize (input, markers) are skipped.
pub fn read(text: &str) -> Result<Cast, CastError> {
    let mut lines = text
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty());
    let (_, first) = lines
        .next()
        .ok_or_else(|| CastError::Header("the file is empty".to_owned()))?;
    let header: Value =
        serde_json::from_str(first).map_err(|error| CastError::Header(error.to_string()))?;
    if header["version"].as_i64() != Some(2) {
        return Err(CastError::Header("the version is not 2".to_owned()));
    }
    let size = |key: &str| {
        header[key]
            .as_u64()
            .and_then(|value| u16::try_from(value).ok())
            .filter(|value| *value > 0)
            .ok_or_else(|| CastError::Header(format!("no {key}")))
    };
    let header = Header {
        width: size("width")?,
        height: size("height")?,
        timestamp: header["timestamp"].as_i64(),
        title: header["title"].as_str().map(str::to_owned),
    };
    let mut events = Vec::new();
    for (index, line) in lines {
        let bad = |message: &str| CastError::Event {
            line: index + 1,
            message: message.to_owned(),
        };
        let value: Value = serde_json::from_str(line).map_err(|error| bad(&error.to_string()))?;
        let (Some(time), Some(code), Some(data)) =
            (value[0].as_f64(), value[1].as_str(), value[2].as_str())
        else {
            return Err(bad("expected [time, code, data]"));
        };
        if !time.is_finite() || time < 0.0 {
            return Err(bad("the time is not valid"));
        }
        match code {
            "o" => events.push(Event::Output(time, data.to_owned())),
            "r" => {
                let parsed = data.split_once('x').and_then(|(columns, lines)| {
                    Some((columns.parse().ok()?, lines.parse().ok()?))
                });
                match parsed {
                    Some((columns, lines)) => events.push(Event::Resize(time, columns, lines)),
                    None => return Err(bad("a resize is COLSxROWS")),
                }
            }
            _ => {}
        }
    }
    // Times never go back (asciinema writes them in order; a hand-edited file might not).
    let mut last = 0.0_f64;
    for event in &mut events {
        let time = match event {
            Event::Output(time, _) | Event::Resize(time, _, _) => time,
        };
        *time = time.max(last);
        last = *time;
    }
    Ok(Cast { header, events })
}

enum Message {
    Header(TermSize),
    Output(f64, String),
    Resize(f64, TermSize),
}

/// Records a session into a file: add it to the session with `Session::add_tap`, remove it (or
/// end the session) to stop. The file is complete once the tap is dropped.
pub struct Recorder {
    sender: Sender<Message>,
    started: Instant,
    /// The start of a UTF-8 character split across output chunks.
    carry: Vec<u8>,
}

impl std::fmt::Debug for Recorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recorder").finish_non_exhaustive()
    }
}

/// Starts a recording into `path` (a new file, `0600` on Unix), titled `title`, of a terminal
/// of type `term`.
///
/// # Errors
///
/// When the file can't be created or the writer thread can't start.
pub fn recorder(path: &Path, title: &str, term: &str) -> std::io::Result<Recorder> {
    let file = create_private(path)?;
    let (sender, receiver) = crossbeam_channel::unbounded();
    let title = title.to_owned();
    let term = term.to_owned();
    std::thread::Builder::new()
        .name("opensesh-recording".to_owned())
        .spawn(move || write_cast(BufWriter::new(file), &receiver, &title, &term))?;
    Ok(Recorder {
        sender,
        started: Instant::now(),
        carry: Vec::new(),
    })
}

fn create_private(path: &Path) -> std::io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn write_cast(mut out: BufWriter<File>, receiver: &Receiver<Message>, title: &str, term: &str) {
    let mut dirty = false;
    loop {
        let message = match receiver.recv_timeout(Duration::from_millis(500)) {
            Ok(message) => message,
            Err(RecvTimeoutError::Timeout) => {
                if dirty && out.flush().is_ok() {
                    dirty = false;
                }
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => break,
        };
        let line = match message {
            Message::Header(size) => {
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |elapsed| elapsed.as_secs());
                json!({
                    "version": 2,
                    "width": size.columns,
                    "height": size.lines,
                    "timestamp": timestamp,
                    "title": title,
                    "env": { "TERM": term },
                })
            }
            Message::Output(time, text) => json!([round(time), "o", text]),
            Message::Resize(time, size) => {
                json!([round(time), "r", format!("{}x{}", size.columns, size.lines)])
            }
        };
        if writeln!(out, "{line}").is_err() {
            tracing::warn!("a recording could not be written; it stops here");
            return;
        }
        dirty = true;
    }
    if let Err(error) = out.flush() {
        tracing::warn!("the end of a recording could not be written: {error}");
    }
}

/// Seconds with microseconds, as asciinema writes them.
fn round(time: f64) -> f64 {
    (time * 1_000_000.0).round() / 1_000_000.0
}

/// How many bytes of `bytes` end on a whole UTF-8 character (up to 3 trailing bytes may start
/// one that isn't complete yet).
fn complete_utf8(bytes: &[u8]) -> usize {
    for back in 1..=bytes.len().min(3) {
        let byte = bytes[bytes.len() - back];
        if byte & 0b1100_0000 == 0b1000_0000 {
            continue;
        }
        let needed = match byte {
            0xC0..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF7 => 4,
            _ => 1,
        };
        return if needed > back {
            bytes.len() - back
        } else {
            bytes.len()
        };
    }
    bytes.len()
}

impl Recorder {
    fn elapsed(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }
}

impl Tap for Recorder {
    fn attached(&mut self, size: TermSize) {
        let _ = self.sender.send(Message::Header(size));
    }

    fn output(&mut self, bytes: &[u8]) {
        self.carry.extend_from_slice(bytes);
        let complete = complete_utf8(&self.carry);
        if complete == 0 {
            return;
        }
        let text = String::from_utf8_lossy(&self.carry[..complete]).into_owned();
        self.carry.drain(..complete);
        let _ = self.sender.send(Message::Output(self.elapsed(), text));
    }

    fn resize(&mut self, size: TermSize) {
        let _ = self.sender.send(Message::Resize(self.elapsed(), size));
    }
}

/// What the player is doing, shared with its controls.
#[derive(Debug)]
struct PlayState {
    playing: bool,
    speed: f64,
    /// Where it is, in seconds of the recording (pauses shortened).
    position: f64,
    /// How long it plays (known once the file is read).
    duration: f64,
    restart: bool,
    /// Where to jump to (back means from the start again, the screen cleared).
    seek: Option<f64>,
    stop: bool,
}

/// The controls of a [`player`]. Cheap to clone.
#[derive(Debug, Clone)]
pub struct PlayerControl {
    state: Arc<(Mutex<PlayState>, Condvar)>,
}

impl PlayerControl {
    fn change(&self, change: impl FnOnce(&mut PlayState)) {
        let (lock, wake) = &*self.state;
        change(&mut lock.lock().unwrap_or_else(PoisonError::into_inner));
        wake.notify_all();
    }

    fn read<T>(&self, read: impl FnOnce(&PlayState) -> T) -> T {
        read(&self.state.0.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Plays (from the start again once it reached the end).
    pub fn play(&self) {
        self.change(|state| {
            if state.duration > 0.0 && state.position >= state.duration {
                state.restart = true;
            }
            state.playing = true;
        });
    }

    /// Pauses.
    pub fn pause(&self) {
        self.change(|state| state.playing = false);
    }

    /// Plays this many times faster (0.25 to 16).
    pub fn set_speed(&self, speed: f64) {
        self.change(|state| state.speed = speed.clamp(0.25, 16.0));
    }

    /// Clears the terminal and plays from the start.
    pub fn restart(&self) {
        self.change(|state| {
            state.restart = true;
            state.playing = true;
        });
    }

    /// Jumps to `seconds` (clamped to the recording), playing or paused as it was: everything
    /// before it shows at once.
    pub fn seek(&self, seconds: f64) {
        if seconds.is_finite() {
            self.change(|state| state.seek = Some(seconds.max(0.0)));
        }
    }

    /// Whether it plays now.
    #[must_use]
    pub fn playing(&self) -> bool {
        self.read(|state| state.playing)
    }

    /// Where it is, in seconds (pauses longer than [`IDLE_LIMIT`] shortened).
    #[must_use]
    pub fn position(&self) -> f64 {
        self.read(|state| state.position)
    }

    /// How long it plays, in seconds (pauses shortened; 0 until the file is read).
    #[must_use]
    pub fn duration(&self) -> f64 {
        self.read(|state| state.duration)
    }

    /// The speed.
    #[must_use]
    pub fn speed(&self) -> f64 {
        self.read(|state| state.speed)
    }
}

/// The recording with pauses longer than [`IDLE_LIMIT`] shortened: its output events and when
/// each plays.
fn timeline(cast: &Cast) -> Vec<(f64, String)> {
    let mut out = Vec::new();
    let (mut previous, mut played) = (0.0_f64, 0.0_f64);
    for event in &cast.events {
        played += (event.time() - previous).min(IDLE_LIMIT);
        previous = event.time();
        if let Event::Output(_, text) = event {
            out.push((played, text.clone()));
        }
    }
    out
}

fn new_control() -> PlayerControl {
    PlayerControl {
        state: Arc::new((
            Mutex::new(PlayState {
                playing: false,
                speed: 1.0,
                position: 0.0,
                duration: 0.0,
                restart: false,
                seek: None,
                stop: false,
            }),
            Condvar::new(),
        )),
    }
}

/// Starts the player thread with what `load` gives (read there, off the caller's thread).
fn start_player(
    load: impl FnOnce() -> Result<Cast, String> + Send + 'static,
) -> (
    Box<dyn TerminalBackend>,
    Receiver<BackendEvent>,
    PlayerControl,
) {
    let (sender, receiver) = crossbeam_channel::unbounded();
    let control = new_control();
    let shared = control.clone();
    let spawned = std::thread::Builder::new()
        .name("opensesh-player".to_owned())
        .spawn(move || match load() {
            Ok(cast) => {
                let events = timeline(&cast);
                let duration = events.last().map_or(0.0, |(time, _)| *time);
                shared.change(|state| state.duration = duration);
                play(&events, &sender, &shared);
            }
            Err(message) => {
                let _ = sender.send(BackendEvent::Error(message));
            }
        });
    if let Err(error) = spawned {
        tracing::warn!("the player could not start: {error}");
    }
    (
        Box::new(Player {
            control: control.clone(),
        }),
        receiver,
        control,
    )
}

/// A backend that prints `cast` again at its pace (paused at first); input is ignored. The
/// terminal keeps its own size: a recording made at another size may wrap differently.
#[must_use]
pub fn player(
    cast: &Cast,
) -> (
    Box<dyn TerminalBackend>,
    Receiver<BackendEvent>,
    PlayerControl,
) {
    let cast = cast.clone();
    start_player(move || Ok(cast))
}

/// [`player`] for the recording in `path`, read on the player's thread; a file that can't be
/// read or isn't a recording shows why in the terminal.
#[must_use]
pub fn player_file(
    path: std::path::PathBuf,
) -> (
    Box<dyn TerminalBackend>,
    Receiver<BackendEvent>,
    PlayerControl,
) {
    start_player(move || {
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        read(&text).map_err(|error| format!("{}: {error}", path.display()))
    })
}

fn play(events: &[(f64, String)], sender: &Sender<BackendEvent>, control: &PlayerControl) {
    let (lock, wake) = &*control.state;
    let mut next = 0;
    let mut state = lock.lock().unwrap_or_else(PoisonError::into_inner);
    let mut last = Instant::now();
    loop {
        if state.stop {
            return;
        }
        let restart = std::mem::take(&mut state.restart);
        let target = if restart {
            state.seek = None;
            Some(0.0)
        } else {
            state.seek.take()
        };
        let now = Instant::now();
        let mut catch_up = false;
        if let Some(target) = target {
            let target = target.min(state.duration);
            if restart || target < state.position {
                next = 0;
                // A full reset (RIS) clears the screen and the modes the recording set.
                if sender
                    .send(BackendEvent::Output(b"\x1bc".to_vec()))
                    .is_err()
                {
                    return;
                }
            }
            state.position = target;
            catch_up = true;
        } else if state.playing {
            state.position += now.duration_since(last).as_secs_f64() * state.speed;
        }
        last = now;
        // Everything due goes out at once (while paused, only what a jump went past; not even
        // what is due at 0).
        while let Some((time, text)) = events.get(next) {
            if !(state.playing || catch_up) || *time > state.position {
                break;
            }
            if sender
                .send(BackendEvent::Output(text.clone().into_bytes()))
                .is_err()
            {
                return;
            }
            next += 1;
        }
        let wait = match events.get(next) {
            None => {
                // The end: it stops there.
                state.playing = false;
                state.position = state.duration;
                None
            }
            Some((time, _)) if state.playing => Some(Duration::from_secs_f64(
                ((time - state.position) / state.speed).clamp(0.001, 0.05),
            )),
            Some(_) => None,
        };
        state = match wait {
            Some(wait) => {
                wake.wait_timeout(state, wait)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0
            }
            None => wake.wait(state).unwrap_or_else(PoisonError::into_inner),
        };
    }
}

struct Player {
    control: PlayerControl,
}

impl TerminalBackend for Player {
    fn write(&self, _bytes: &[u8]) -> Result<(), BackendError> {
        Ok(())
    }

    fn resize(&self, _size: TermSize) -> Result<(), BackendError> {
        Ok(())
    }

    fn shutdown(&self) {
        self.control.change(|state| state.stop = true);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests")]

    use super::*;

    #[test]
    fn utf8_boundaries() {
        assert_eq!(complete_utf8(b"abc"), 3);
        assert_eq!(complete_utf8("é".as_bytes()), 2);
        assert_eq!(complete_utf8(&"é".as_bytes()[..1]), 0);
        assert_eq!(complete_utf8(&"a€".as_bytes()[..3]), 1);
        assert_eq!(complete_utf8(&"𝄞".as_bytes()[..3]), 0);
        assert_eq!(complete_utf8(b""), 0);
    }

    #[test]
    fn a_recording_written_then_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.cast");
        let mut recorder = recorder(&path, "web-01", "xterm-256color").unwrap();
        recorder.attached(TermSize::new(100, 30));
        recorder.output(b"hello \xc3");
        recorder.output(b"\xa9t\xc3\xa9\r\n");
        recorder.input(b"secret password\r");
        recorder.resize(TermSize::new(120, 40));
        recorder.output(b"\x1b[1mbold\x1b[0m");
        assert!(
            create_private(&path).is_err(),
            "an existing recording is never overwritten"
        );
        drop(recorder);
        // The writer thread finishes the file.
        let deadline = Instant::now() + Duration::from_secs(5);
        let cast = loop {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            if text.lines().count() == 5 {
                break read(&text).unwrap();
            }
            assert!(Instant::now() < deadline, "{text}");
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(
            (
                cast.header.width,
                cast.header.height,
                cast.header.title.as_deref()
            ),
            (100, 30, Some("web-01"))
        );
        assert!(cast.header.timestamp.is_some());
        let texts: Vec<&Event> = cast.events.iter().collect();
        assert!(matches!(texts[0], Event::Output(_, text) if text == "hello "));
        assert!(matches!(texts[1], Event::Output(_, text) if text == "été\r\n"));
        assert!(matches!(texts[2], Event::Resize(_, 120, 40)));
        assert!(matches!(texts[3], Event::Output(_, text) if text == "\u{1b}[1mbold\u{1b}[0m"));
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("secret"), "typed input is never recorded");
    }

    #[test]
    fn reading_asciinema_files() {
        let text = "{\"version\": 2, \"width\": 80, \"height\": 24, \"timestamp\": 1504467315, \"env\": {\"SHELL\": \"/bin/zsh\"}}\n\
                    [0.248848, \"o\", \"\\u001b[1;31mHello \\u001b[32mWorld!\\u001b[0m\\n\"]\n\
                    [1.001376, \"o\", \"That was ok\\rThis is better.\"]\n\
                    [1.5, \"i\", \"typed\"]\n\
                    [2.0, \"m\", \"marker\"]\n\
                    [2.143733, \"o\", \" \"]\n\
                    [6.541828, \"o\", \"Bye!\"]\n";
        let cast = read(text).unwrap();
        assert_eq!(cast.events.len(), 4);
        assert!((cast.duration() - 6.541_828).abs() < 1e-9);
        // The 4.4 s pause plays as 2 s.
        let times: Vec<f64> = timeline(&cast).iter().map(|(time, _)| *time).collect();
        assert!(
            (times[3] - (2.143_733 + IDLE_LIMIT)).abs() < 1e-6,
            "{times:?}"
        );

        for (bad, why) in [
            ("", "empty"),
            ("{\"version\": 1, \"width\": 80, \"height\": 24}", "version"),
            ("{\"version\": 2, \"height\": 24}", "width"),
            (
                "{\"version\": 2, \"width\": 80, \"height\": 24}\n[\"x\", \"o\", \"a\"]",
                "[time",
            ),
            (
                "{\"version\": 2, \"width\": 80, \"height\": 24}\n[1, \"r\", \"80-24\"]",
                "COLSxROWS",
            ),
            (
                "{\"version\": 2, \"width\": 80, \"height\": 24}\nnot json",
                "line 2",
            ),
        ] {
            let error = read(bad).unwrap_err().to_string();
            assert!(error.contains(why), "{bad:?}: {error}");
        }
    }

    #[test]
    fn the_player_plays_pauses_and_restarts() {
        let cast = read(
            "{\"version\": 2, \"width\": 80, \"height\": 24}\n\
             [0.0, \"o\", \"one \"]\n\
             [0.2, \"o\", \"two \"]\n\
             [0.4, \"o\", \"three\"]\n",
        )
        .unwrap();
        let (backend, events, control) = player(&cast);
        assert!(!control.playing());
        assert!(
            events.recv_timeout(Duration::from_millis(200)).is_err(),
            "paused at first"
        );
        control.set_speed(4.0);
        control.play();
        let mut printed = String::new();
        while let Ok(BackendEvent::Output(bytes)) = events.recv_timeout(Duration::from_secs(2)) {
            printed.push_str(&String::from_utf8(bytes).unwrap());
            if printed.ends_with("three") {
                break;
            }
        }
        assert_eq!(printed, "one two three");
        // It stops at the end; play starts again with a reset.
        let deadline = Instant::now() + Duration::from_secs(2);
        while control.playing() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        control.play();
        let first = events.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(first, BackendEvent::Output(b"\x1bc".to_vec()));
        control.pause();
        backend.shutdown();
        assert!(backend.write(b"ignored").is_ok());
    }

    fn printed_until(events: &Receiver<BackendEvent>, end: &str) -> String {
        let mut printed = String::new();
        while let Ok(BackendEvent::Output(bytes)) = events.recv_timeout(Duration::from_secs(2)) {
            printed.push_str(&String::from_utf8(bytes).unwrap());
            if printed.ends_with(end) {
                break;
            }
        }
        printed
    }

    #[test]
    fn a_jump_shows_everything_before_it_and_back_starts_over() {
        let cast = read(
            "{\"version\": 2, \"width\": 80, \"height\": 24}\n\
             [0.0, \"o\", \"one \"]\n\
             [1.0, \"o\", \"two \"]\n\
             [2.0, \"o\", \"three\"]\n",
        )
        .unwrap();
        let (_backend, events, control) = player(&cast);
        let deadline = Instant::now() + Duration::from_secs(2);
        while control.duration() == 0.0 {
            assert!(Instant::now() < deadline, "the duration is known once read");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!((control.duration() - 2.0).abs() < 1e-9);
        // Forward while paused: what it went past, at once, and it stays paused.
        control.seek(1.5);
        assert_eq!(printed_until(&events, "two "), "one two ");
        assert!(!control.playing());
        assert!((control.position() - 1.5).abs() < 1e-9);
        // Back: a reset, then everything up to there again.
        control.seek(0.5);
        assert_eq!(printed_until(&events, "one "), "\x1bcone ");
        // Past the end: all of it.
        control.seek(60.0);
        assert_eq!(printed_until(&events, "three"), "two three");
        assert!((control.position() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn a_file_that_is_not_a_recording_says_why() {
        let folder = std::env::temp_dir().join(format!("opensesh-cast-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("broken.cast");
        std::fs::write(&path, "not a recording").unwrap();
        let (_backend, events, _control) = player_file(path.clone());
        match events.recv_timeout(Duration::from_secs(2)).unwrap() {
            BackendEvent::Error(message) => assert!(message.contains("broken.cast"), "{message}"),
            other => panic!("expected an error, got {other:?}"),
        }
        let (_backend, events, _control) = player_file(folder.join("missing.cast"));
        assert!(matches!(
            events.recv_timeout(Duration::from_secs(2)).unwrap(),
            BackendEvent::Error(_)
        ));
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
