//! Saved hosts and groups (PLAN §4.3, Sprint 5).
//!
//! - This module: the model and `hosts.toml` (groups, hosts and linked sources such as
//!   `~/.ssh/config`), lenient loading with warnings, and inheritance: every host field a
//!   group can set comes from the host, else the nearest group that sets it, else the built-in
//!   default ([`HostsFile::resolve`]); each resolved value knows where it came from.
//! - [`search`]: fuzzy search, filters and sorting.
//! - [`target`]: the quick-connect parser and the OpenSSH command line.
//! - [`recent`]: recent connections.
//! - [`detected`]: the operating system the SSH client found on each host.
//!
//! Secrets are never stored here: a host or group names an identity of the keychain
//! (`keychain.toml`, Sprint 6), whose password and key live in the vault.

pub mod containers;
pub mod detected;
pub mod recent;
pub mod search;
pub mod target;

pub use containers::ContainerOptions;

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use toml::{Table, Value};

use crate::config::Warning;
use crate::terminal::settings::TerminalOverrides;

/// File name in the config directory.
pub const HOSTS_FILE: &str = "hosts.toml";

/// Current layout of `hosts.toml`.
pub const SCHEMA_VERSION: i64 = 1;

/// Longest id accepted (ULIDs have 26 characters; ids from linked sources are longer).
const MAX_ID_LEN: usize = 200;

const HEADER: &str = "# OpenSesh hosts and groups. Edit them in the Hosts view or here: changes apply live.\n\
                      # Secrets never go in this file.\n";

/// How a host is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    /// Secure shell (the default).
    #[default]
    Ssh,
    /// A file browser over SSH.
    Sftp,
    /// Telnet.
    Telnet,
    /// A serial port; the address is the device.
    Serial,
    /// Mosh.
    Mosh,
    /// Remote desktop.
    Rdp,
    /// VNC.
    Vnc,
    /// A local shell.
    Local,
    /// `docker exec` into a container; the address is the container.
    Docker,
    /// `kubectl exec` into a pod; the address is the pod.
    Kube,
    /// S3 storage (AWS or a compatible server); the address is the endpoint, empty for AWS.
    S3,
}

impl Protocol {
    /// Every protocol, in menu order.
    pub const ALL: [Self; 11] = [
        Self::Ssh,
        Self::Sftp,
        Self::Telnet,
        Self::Serial,
        Self::Mosh,
        Self::Rdp,
        Self::Vnc,
        Self::Local,
        Self::Docker,
        Self::Kube,
        Self::S3,
    ];

    /// The name in files and URLs.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ssh => "ssh",
            Self::Sftp => "sftp",
            Self::Telnet => "telnet",
            Self::Serial => "serial",
            Self::Mosh => "mosh",
            Self::Rdp => "rdp",
            Self::Vnc => "vnc",
            Self::Local => "local",
            Self::Docker => "docker",
            Self::Kube => "kube",
            Self::S3 => "s3",
        }
    }

    /// The protocol named `text` (any case).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|protocol| protocol.as_str().eq_ignore_ascii_case(text.trim()))
    }

    /// The usual port, for protocols that have one.
    #[must_use]
    pub const fn default_port(self) -> Option<u16> {
        match self {
            Self::Ssh | Self::Sftp | Self::Mosh => Some(22),
            Self::Telnet => Some(23),
            Self::Rdp => Some(3389),
            Self::Vnc => Some(5900),
            Self::Serial | Self::Local | Self::Docker | Self::Kube | Self::S3 => None,
        }
    }

    /// Whether the host needs an address (a device for serial, a container or pod name). An
    /// S3 host without one is on AWS.
    #[must_use]
    pub const fn needs_address(self) -> bool {
        !matches!(self, Self::Local | Self::S3)
    }

    /// Whether the address is a network host with a port.
    #[must_use]
    pub const fn is_network(self) -> bool {
        self.default_port().is_some()
    }

    /// The sprint that brings connecting with this protocol, `None` when it works today.
    #[must_use]
    pub const fn available_in(self) -> Option<u8> {
        match self {
            Self::Ssh | Self::Local => None,
            Self::Sftp => Some(8),
            Self::Telnet | Self::Serial | Self::Mosh | Self::Docker | Self::Kube | Self::S3 => None,
            Self::Rdp => None,
            Self::Vnc => None,
        }
    }
}

/// Which SSH implementation connects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SshBackend {
    /// The built-in client (Sprint 7).
    #[default]
    Internal,
    /// The system's OpenSSH `ssh`.
    Openssh,
}

impl SshBackend {
    /// The name in files.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Internal => "internal",
            Self::Openssh => "openssh",
        }
    }
}

/// X11 forwarding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum X11Forwarding {
    /// No forwarding.
    #[default]
    Off,
    /// Untrusted forwarding (`ssh -X`).
    Untrusted,
    /// Trusted forwarding (`ssh -Y`); the remote programs get full access to the display.
    Trusted,
}

/// Serial parity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Parity {
    /// None.
    #[default]
    None,
    /// Even.
    Even,
    /// Odd.
    Odd,
}

/// Serial flow control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FlowControl {
    /// None.
    #[default]
    None,
    /// XON/XOFF.
    Software,
    /// RTS/CTS.
    Hardware,
}

/// What Enter sends on a serial line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Newline {
    /// A carriage return (what most devices expect, and PuTTY's default).
    #[default]
    Cr,
    /// A line feed.
    Lf,
    /// Both.
    Crlf,
}

/// A session log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionLog {
    /// No log.
    #[default]
    Off,
    /// Clean text (escape sequences removed).
    Text,
    /// Everything the server printed, escape sequences included.
    Raw,
}

impl SessionLog {
    /// The name in files.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Text => "text",
            Self::Raw => "raw",
        }
    }
}

/// SSH options a host or a group sets (each one optional: unset means inherited).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshOptions {
    /// Which client connects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<SshBackend>,
    /// Forward the local SSH agent (off by default, PLAN §8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_forwarding: Option<bool>,
    /// X11 forwarding (off by default, PLAN §8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x11: Option<X11Forwarding>,
    /// Show the server's Wayland programs here through Waypipe (off by default; Sprint 15).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waypipe: Option<bool>,
    /// Keepalive interval in seconds; 0 turns it off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keepalive_secs: Option<u32>,
    /// Compression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compression: Option<bool>,
    /// Text typed into the shell once it is ready (the snippet library arrives in Sprint 10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub startup_snippet: Option<String>,
    /// Also offer old algorithms (SHA-1, CBC) for old equipment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_algorithms: Option<bool>,
    /// Authentication methods in order: `publickey`, `keyboard-interactive`, `password`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_order: Option<Vec<String>>,
    /// A proxy for the first hop: `socks5://[user@]host:port` or `http://[user@]host:port`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<String>,
    /// A command that carries the connection (`%h`, `%p`, `%r`), like OpenSSH's ProxyCommand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_command: Option<String>,
    /// Environment variables to send (the server's `AcceptEnv` decides).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<BTreeMap<String, String>>,
    /// Send `LANG` and `LC_*`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_locale: Option<bool>,
    /// A command to run instead of the login shell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Reconnect by itself after the connection is lost.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_reconnect: Option<bool>,
    /// Keep a log of the session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log: Option<SessionLog>,
    /// Ask the server which OS it runs, for the host's icon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detect_os: Option<bool>,
    /// Watch the server with the remote monitor (the status bar's readings).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitor: Option<bool>,
    /// The SSH agent to use instead of the usual one (like OpenSSH's `IdentityAgent`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_socket: Option<String>,
}

impl SshOptions {
    /// Whether nothing is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// SFTP options.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SftpOptions {
    /// Follow the terminal's working directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_cwd: Option<bool>,
    /// Folder to start in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_dir: Option<String>,
}

impl SftpOptions {
    /// Whether nothing is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Serial line settings (a serial host's address is its device).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SerialOptions {
    /// Speed in bits per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baud: Option<u32>,
    /// Data bits (5 to 8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_bits: Option<u8>,
    /// Parity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parity: Option<Parity>,
    /// Stop bits (1 or 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_bits: Option<u8>,
    /// Flow control.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow_control: Option<FlowControl>,
    /// What Enter sends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub newline: Option<Newline>,
    /// Show what is typed (for devices that don't echo).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_echo: Option<bool>,
}

impl SerialOptions {
    /// The speed when none is set.
    pub const DEFAULT_BAUD: u32 = 115_200;

    /// Whether nothing is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// How a remote desktop fits its pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scaling {
    /// The desktop follows the pane's size.
    #[default]
    Dynamic,
    /// A fixed size, scaled to fit the pane.
    Fit,
    /// A fixed size, one desktop pixel per screen pixel.
    Actual,
}

impl Scaling {
    /// The name in files and the UI.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dynamic => "dynamic",
            Self::Fit => "fit",
            Self::Actual => "actual",
        }
    }
}

/// Remote desktop settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RdpOptions {
    /// The Windows domain (else from the user name, `DOMAIN\user` or `user@domain`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// How the desktop fits the pane (unset: it follows the pane's size).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scaling: Option<Scaling>,
    /// The desktop's size as `WIDTHxHEIGHT`, for the fixed scalings (unset: the pane's size).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    /// Share the text clipboard (unset: on).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clipboard: Option<bool>,
}

impl RdpOptions {
    /// Whether nothing is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The fixed size, when one is set and valid (200 to 8192 pixels a side).
    #[must_use]
    pub fn size(&self) -> Option<(u16, u16)> {
        parse_resolution(self.resolution.as_deref()?)
    }
}

/// How much a VNC desktop's pictures are compressed (Tight's JPEG).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VncQuality {
    /// No JPEG: every pixel exact.
    Lossless,
    /// JPEG at quality 2: the least bandwidth.
    Low,
    /// JPEG at quality 5.
    Medium,
    /// JPEG at quality 8.
    High,
}

impl VncQuality {
    /// Tight's JPEG quality level (none: lossless).
    #[must_use]
    pub const fn jpeg_level(self) -> Option<u8> {
        match self {
            Self::Lossless => None,
            Self::Low => Some(2),
            Self::Medium => Some(5),
            Self::High => Some(8),
        }
    }
}

/// VNC settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VncOptions {
    /// How the desktop fits the pane (unset: scaled to fit; following the pane needs a server
    /// that resizes, as TigerVNC does).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scaling: Option<Scaling>,
    /// Only watch: no keys, pointer or clipboard go to the server (unset: off).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
    /// The pictures' compression (unset: high).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<VncQuality>,
    /// Share the text clipboard (unset: on).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clipboard: Option<bool>,
    /// Let other viewers stay connected (unset: on).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared: Option<bool>,
}

impl VncOptions {
    /// Whether nothing is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// `WIDTHxHEIGHT` (200 to 8192 each).
#[must_use]
pub fn parse_resolution(text: &str) -> Option<(u16, u16)> {
    let (width, height) = text.trim().split_once(['x', 'X', '×'])?;
    let side = |text: &str| {
        text.trim()
            .parse::<u16>()
            .ok()
            .filter(|side| (200..=8192).contains(side))
    };
    Some((side(width)?, side(height)?))
}

/// S3 settings (an S3 host's address is its endpoint; its identity holds the access key as the
/// user name and the secret key as the password).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct S3Options {
    /// The region (unset: [`S3Options::DEFAULT_REGION`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// Buckets in the path rather than in the host name (unset: on, what most S3-compatible
    /// servers want).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_style: Option<bool>,
}

impl S3Options {
    /// The region when none is set.
    pub const DEFAULT_REGION: &str = "us-east-1";

    /// Whether nothing is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The region.
    #[must_use]
    pub fn region(&self) -> &str {
        self.region
            .as_deref()
            .map(str::trim)
            .filter(|region| !region.is_empty())
            .unwrap_or(Self::DEFAULT_REGION)
    }

    /// Path-style addressing.
    #[must_use]
    pub fn path_style(&self) -> bool {
        self.path_style.unwrap_or(true)
    }
}

/// Whether `text` can be an S3 endpoint: `host[:port]`, with `http://` or `https://` in front or
/// not, and no path.
#[must_use]
pub fn is_endpoint(text: &str) -> bool {
    let text = text.trim();
    let rest = match text.split_once("://") {
        Some((scheme, rest)) => {
            if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
                return false;
            }
            rest
        }
        None => text,
    };
    let rest = rest.trim_end_matches('/');
    !rest.is_empty()
        && !rest.contains('/')
        && target::parse_endpoint(rest).is_ok_and(|(user, _, _)| user.is_none())
}

/// Whether `region` can be a region name (letters, digits and hyphens).
#[must_use]
pub fn is_region(region: &str) -> bool {
    !region.is_empty()
        && region
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// What a group gives its hosts and subgroups.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HostDefaults {
    /// User name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Port.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Jump hosts, first hop first: saved host ids or names, or `user@host:port`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jump: Option<Vec<String>>,
    /// Keychain identity id (user name, password and/or key).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    /// Private key file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_file: Option<String>,
    /// Terminal profile id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// SSH options.
    #[serde(default, skip_serializing_if = "SshOptions::is_empty")]
    pub ssh: SshOptions,
    /// SFTP options.
    #[serde(default, skip_serializing_if = "SftpOptions::is_empty")]
    pub sftp: SftpOptions,
    /// Terminal options over the profile (the `[terminal]` keys of a profile).
    #[serde(default, skip_serializing_if = "Table::is_empty")]
    pub terminal: Table,
}

impl HostDefaults {
    /// Whether nothing is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A group of hosts, possibly inside another group.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Group {
    /// Stable id (a ULID).
    #[serde(default)]
    pub id: String,
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// The group it is in, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// A tab color name (`Theme.tabColorNames`) or `#RRGGBB`; empty for none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub color: String,
    /// Markdown notes.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// What its hosts inherit.
    #[serde(default, skip_serializing_if = "HostDefaults::is_empty")]
    pub defaults: HostDefaults,
    /// Keys this version doesn't know, written back unchanged.
    #[serde(flatten)]
    pub extra: Table,
}

fn auto_icon() -> String {
    "auto".to_owned()
}

fn is_auto(icon: &String) -> bool {
    icon.is_empty() || icon == "auto"
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// A saved host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Host {
    /// Stable id (a ULID; `ssh_config:<alias>` for linked `~/.ssh/config` entries).
    #[serde(default)]
    pub id: String,
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// Its group, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// How it is reached.
    #[serde(default)]
    pub protocol: Protocol,
    /// Host name or IP address (a device for serial, a container or pod for docker and kube).
    #[serde(default)]
    pub address: String,
    /// Port; unset means the group's, else the protocol's usual one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// User name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Keychain identity id; an empty one means none, even if the group has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    /// Private key file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_file: Option<String>,
    /// Jump hosts, first hop first; an empty list means none, even if the group has some.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jump: Option<Vec<String>>,
    /// Terminal profile id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Tags.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Listed under Favorites.
    #[serde(default, skip_serializing_if = "is_false")]
    pub favorite: bool,
    /// A tab color name or `#RRGGBB`; empty for the group's.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub color: String,
    /// `auto` (by operating system) or an icon name (`os-debian`, `server`, ...).
    #[serde(default = "auto_icon", skip_serializing_if = "is_auto")]
    pub icon: String,
    /// Markdown notes.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// SSH options.
    #[serde(default, skip_serializing_if = "SshOptions::is_empty")]
    pub ssh: SshOptions,
    /// SFTP options.
    #[serde(default, skip_serializing_if = "SftpOptions::is_empty")]
    pub sftp: SftpOptions,
    /// Serial line settings.
    #[serde(default, skip_serializing_if = "SerialOptions::is_empty")]
    pub serial: SerialOptions,
    /// How a container or pod is entered (docker and kube hosts).
    #[serde(default, skip_serializing_if = "ContainerOptions::is_empty")]
    pub container: ContainerOptions,
    /// S3 settings.
    #[serde(default, skip_serializing_if = "S3Options::is_empty")]
    pub s3: S3Options,
    /// Remote desktop settings.
    #[serde(default, skip_serializing_if = "RdpOptions::is_empty")]
    pub rdp: RdpOptions,
    /// VNC settings.
    #[serde(default, skip_serializing_if = "VncOptions::is_empty")]
    pub vnc: VncOptions,
    /// Terminal options over the profile.
    #[serde(default, skip_serializing_if = "Table::is_empty")]
    pub terminal: Table,
    /// Keys this version doesn't know, written back unchanged.
    #[serde(flatten)]
    pub extra: Table,
}

impl Default for Host {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            group: None,
            protocol: Protocol::Ssh,
            address: String::new(),
            port: None,
            user: None,
            identity: None,
            identity_file: None,
            jump: None,
            profile: None,
            tags: Vec::new(),
            favorite: false,
            color: String::new(),
            icon: auto_icon(),
            notes: String::new(),
            ssh: SshOptions::default(),
            sftp: SftpOptions::default(),
            serial: SerialOptions::default(),
            container: ContainerOptions::default(),
            s3: S3Options::default(),
            rdp: RdpOptions::default(),
            vnc: VncOptions::default(),
            terminal: Table::new(),
            extra: Table::new(),
        }
    }
}

impl Host {
    /// Whether the host comes from a linked source and can't be edited.
    #[must_use]
    pub fn is_linked(&self) -> bool {
        self.id.starts_with(LINKED_PREFIX)
    }
}

/// Ids of hosts read from a linked `~/.ssh/config` start with this.
pub const LINKED_PREFIX: &str = "ssh_config:";

/// Kind of a linked source.
pub const SOURCE_SSH_CONFIG: &str = "ssh_config";

/// A file whose hosts are shown read-only and followed when it changes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Source {
    /// `ssh_config`.
    pub kind: String,
    /// The file (`~` allowed).
    pub path: String,
    /// Keys this version doesn't know, written back unchanged.
    #[serde(flatten)]
    pub extra: Table,
}

/// Why `hosts.toml` can't be read.
#[derive(Debug, thiserror::Error)]
pub enum HostsError {
    /// The file exists but can't be read.
    #[error("could not read {path}")]
    Read {
        /// The file.
        path: String,
        /// Why.
        #[source]
        source: std::io::Error,
    },
    /// Not valid TOML.
    #[error("{path} is not valid TOML: {message}")]
    Syntax {
        /// The file.
        path: String,
        /// The parser's message.
        message: String,
    },
}

/// The contents of `hosts.toml`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HostsFile {
    /// Groups, in file order.
    pub groups: Vec<Group>,
    /// Hosts, in file order.
    pub hosts: Vec<Host>,
    /// Linked sources.
    pub sources: Vec<Source>,
    /// Written by a newer OpenSesh: shown, never saved over.
    pub read_only: bool,
    /// Top-level keys this version doesn't know.
    pub extra: Table,
    /// The app's defaults (Settings > SSH, `[ssh]` of `config.toml`) as a host table: what a
    /// key resolves to when no group sets it, before the built-in value. Never saved here.
    pub base: Table,
}

/// Where a resolved value comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// The built-in default (or the protocol's).
    Default,
    /// A group's defaults (its id).
    Group(String),
    /// The host itself.
    Host,
}

/// Keys a group can give its hosts, as dotted paths in a host or `[group.defaults]` table.
pub const INHERITED_KEYS: &[&str] = &[
    "user",
    "port",
    "jump",
    "identity",
    "identity_file",
    "profile",
    "ssh.backend",
    "ssh.agent_forwarding",
    "ssh.x11",
    "ssh.waypipe",
    "ssh.keepalive_secs",
    "ssh.compression",
    "ssh.startup_snippet",
    "ssh.legacy_algorithms",
    "ssh.auth_order",
    "ssh.proxy",
    "ssh.proxy_command",
    "ssh.env",
    "ssh.send_locale",
    "ssh.command",
    "ssh.auto_reconnect",
    "ssh.log",
    "ssh.detect_os",
    "ssh.monitor",
    "ssh.agent_socket",
    "sftp.follow_cwd",
    "sftp.start_dir",
];

/// Authentication methods in their default order.
pub const DEFAULT_AUTH_ORDER: [&str; 3] = ["publickey", "keyboard-interactive", "password"];

/// The built-in value of an inherited key (`None` for the protocol's port, no user, no key).
fn builtin(key: &str, protocol: Protocol) -> Option<Value> {
    match key {
        "port" => protocol
            .default_port()
            .map(|port| Value::Integer(i64::from(port))),
        "jump" => Some(Value::Array(Vec::new())),
        "ssh.backend" => Some(Value::String("internal".to_owned())),
        "ssh.agent_forwarding"
        | "ssh.waypipe"
        | "ssh.compression"
        | "ssh.legacy_algorithms"
        | "ssh.auto_reconnect" => Some(Value::Boolean(false)),
        "ssh.send_locale" | "ssh.detect_os" | "ssh.monitor" => Some(Value::Boolean(true)),
        "ssh.x11" => Some(Value::String("off".to_owned())),
        "ssh.keepalive_secs" => Some(Value::Integer(30)),
        "ssh.startup_snippet"
        | "ssh.proxy"
        | "ssh.proxy_command"
        | "ssh.command"
        | "ssh.agent_socket" => Some(Value::String(String::new())),
        "ssh.auth_order" => Some(Value::Array(
            DEFAULT_AUTH_ORDER
                .iter()
                .map(|method| Value::String((*method).to_owned()))
                .collect(),
        )),
        "ssh.env" => Some(Value::Table(Table::new())),
        "ssh.log" => Some(Value::String("off".to_owned())),
        "sftp.follow_cwd" => Some(Value::Boolean(true)),
        "sftp.start_dir" => Some(Value::String("~".to_owned())),
        _ => None,
    }
}

fn lookup<'a>(table: &'a Table, path: &str) -> Option<&'a Value> {
    let mut parts = path.split('.');
    let first = parts.next()?;
    let mut value = table.get(first)?;
    for part in parts {
        value = value.as_table()?.get(part)?;
    }
    Some(value)
}

fn to_table<T: Serialize>(value: &T) -> Table {
    Value::try_from(value)
        .ok()
        .and_then(|value| match value {
            Value::Table(table) => Some(table),
            _ => None,
        })
        .unwrap_or_default()
}

/// A resolved field: its value (none when nothing sets it) and where it comes from.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    /// The value, as it would be written in the file.
    pub value: Option<Value>,
    /// Where it comes from.
    pub origin: Origin,
}

/// A host with every inherited field resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedHost {
    /// The host as saved.
    pub host: Host,
    /// Each of [`INHERITED_KEYS`].
    pub fields: BTreeMap<&'static str, Resolved>,
}

impl ResolvedHost {
    fn value(&self, key: &str) -> Option<&Value> {
        self.fields.get(key).and_then(|field| field.value.as_ref())
    }

    /// A string field, `None` when unset or empty.
    #[must_use]
    pub fn string(&self, key: &str) -> Option<&str> {
        self.value(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
    }

    /// A boolean field.
    #[must_use]
    pub fn flag(&self, key: &str) -> bool {
        self.value(key).and_then(Value::as_bool).unwrap_or(false)
    }

    /// A list of strings, trimmed, without empty entries.
    #[must_use]
    pub fn list(&self, key: &str) -> Vec<String> {
        self.value(key)
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|item| item.trim().to_owned())
                    .filter(|item| !item.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A table of strings (environment variables), sorted by name.
    #[must_use]
    pub fn string_map(&self, key: &str) -> BTreeMap<String, String> {
        self.value(key)
            .and_then(Value::as_table)
            .map(|table| {
                table
                    .iter()
                    .filter_map(|(name, value)| {
                        value.as_str().map(|value| (name.clone(), value.to_owned()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The session log setting.
    #[must_use]
    pub fn session_log(&self) -> SessionLog {
        match self.string("ssh.log") {
            Some("text") => SessionLog::Text,
            Some("raw") => SessionLog::Raw,
            _ => SessionLog::Off,
        }
    }

    /// The user name.
    #[must_use]
    pub fn user(&self) -> Option<&str> {
        self.string("user")
    }

    /// The keychain identity id.
    #[must_use]
    pub fn identity(&self) -> Option<&str> {
        self.string("identity")
    }

    /// The port.
    #[must_use]
    pub fn port(&self) -> Option<u16> {
        self.value("port")
            .and_then(Value::as_integer)
            .and_then(|port| u16::try_from(port).ok())
            .filter(|port| *port > 0)
    }

    /// The jump host references.
    #[must_use]
    pub fn jump(&self) -> Vec<String> {
        self.value("jump")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(Value::as_str)
                    .map(|item| item.trim().to_owned())
                    .filter(|item| !item.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The keepalive interval in seconds (0: off).
    #[must_use]
    pub fn keepalive_secs(&self) -> u32 {
        self.value("ssh.keepalive_secs")
            .and_then(Value::as_integer)
            .and_then(|secs| u32::try_from(secs).ok())
            .unwrap_or(0)
    }

    /// X11 forwarding.
    #[must_use]
    pub fn x11(&self) -> X11Forwarding {
        match self.string("ssh.x11") {
            Some("untrusted") => X11Forwarding::Untrusted,
            Some("trusted") => X11Forwarding::Trusted,
            _ => X11Forwarding::Off,
        }
    }
}

/// The profile and terminal overrides of one level of a host's chain (a group, then the host).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TerminalLevel {
    /// Profile id, if the level picks one.
    pub profile: Option<String>,
    /// The level's terminal options.
    pub overrides: TerminalOverrides,
}

fn warning(key: impl Into<String>, message: impl Into<String>) -> Warning {
    Warning {
        key: key.into(),
        message: message.into(),
    }
}

/// A new id for a host or a group.
#[must_use]
pub fn new_id() -> String {
    ulid::Ulid::generate().to_string()
}

fn valid_host_id(id: &str) -> bool {
    !id.trim().is_empty() && id.len() <= MAX_ID_LEN && !id.chars().any(char::is_control)
}

/// Tags trimmed, without empty or repeated ones (compared ignoring case).
#[must_use]
pub fn clean_tags(tags: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    tags.iter()
        .map(|tag| tag.trim().to_owned())
        .filter(|tag| !tag.is_empty() && seen.insert(tag.to_lowercase()))
        .collect()
}

impl HostsFile {
    /// Parses the file's text. Entries that can't be used are skipped with a warning; broken
    /// references (an unknown group, a group cycle) are cut with a warning.
    ///
    /// # Errors
    ///
    /// The TOML parser's message when the text isn't TOML.
    pub fn from_toml_str(text: &str) -> Result<(Self, Vec<Warning>), String> {
        let mut root: Table = text
            .parse()
            .map_err(|error: toml::de::Error| error.to_string())?;
        let mut warnings = Vec::new();
        let version = match root.remove("schema_version") {
            None => SCHEMA_VERSION,
            Some(Value::Integer(version)) => version,
            Some(other) => {
                warnings.push(warning(
                    "schema_version",
                    format!("expected an integer, found {}", other.type_str()),
                ));
                SCHEMA_VERSION
            }
        };
        let read_only = version > SCHEMA_VERSION;
        if read_only {
            warnings.push(warning(
                "schema_version",
                format!(
                    "version {version} is newer than this OpenSesh supports ({SCHEMA_VERSION}); \
                     hosts are read-only until you upgrade"
                ),
            ));
        }
        let groups: Vec<Group> = entries(&mut root, "group", &mut warnings);
        let hosts: Vec<Host> = entries(&mut root, "host", &mut warnings);
        let sources: Vec<Source> = entries(&mut root, "source", &mut warnings);
        let mut file = Self {
            groups,
            hosts,
            sources,
            read_only,
            extra: root,
            base: Table::new(),
        };
        file.check(&mut warnings);
        Ok((file, warnings))
    }

    /// Fixes what can be fixed and reports it: ids (missing, repeated), names, references to
    /// unknown groups, group cycles, ports, tags and terminal options.
    fn check(&mut self, warnings: &mut Vec<Warning>) {
        let mut ids = HashSet::new();
        let mut groups = Vec::with_capacity(self.groups.len());
        for (index, mut group) in std::mem::take(&mut self.groups).into_iter().enumerate() {
            let label = format!("group[{index}]");
            if !valid_host_id(&group.id) {
                group.id = new_id();
                warnings.push(warning(
                    &label,
                    "no valid id; a new one is saved with the next change",
                ));
            }
            if !ids.insert(group.id.clone()) {
                warnings.push(warning(
                    &label,
                    format!("id {} is used twice; skipped", group.id),
                ));
                continue;
            }
            group.name = group.name.trim().to_owned();
            if group.name.is_empty() {
                group.name = "Group".to_owned();
            }
            check_defaults(&group.defaults, &format!("{label}.defaults"), warnings);
            groups.push(group);
        }
        let group_ids: HashSet<String> = groups.iter().map(|group| group.id.clone()).collect();
        for group in &mut groups {
            if let Some(parent) = &group.parent
                && (!group_ids.contains(parent) || *parent == group.id)
            {
                warnings.push(warning(
                    format!("group {}", group.name),
                    format!("unknown parent group {parent}; moved to the top"),
                ));
                group.parent = None;
            }
        }
        // Cut cycles: walk each group's parents; a group met twice closes a loop.
        for index in 0..groups.len() {
            let mut seen = HashSet::new();
            let mut current = Some(groups[index].id.clone());
            while let Some(id) = current {
                if !seen.insert(id.clone()) {
                    if let Some(group) = groups.iter_mut().find(|group| group.id == id) {
                        warnings.push(warning(
                            format!("group {}", group.name),
                            "its parents form a loop; moved to the top",
                        ));
                        group.parent = None;
                    }
                    break;
                }
                current = groups
                    .iter()
                    .find(|group| group.id == id)
                    .and_then(|group| group.parent.clone());
            }
        }
        self.groups = groups;

        let mut hosts = Vec::with_capacity(self.hosts.len());
        for (index, mut host) in std::mem::take(&mut self.hosts).into_iter().enumerate() {
            let label = if host.name.trim().is_empty() {
                format!("host[{index}]")
            } else {
                format!("host {}", host.name.trim())
            };
            if !valid_host_id(&host.id) || host.is_linked() {
                host.id = new_id();
                warnings.push(warning(
                    &label,
                    "no valid id; a new one is saved with the next change",
                ));
            }
            if !ids.insert(host.id.clone()) {
                warnings.push(warning(
                    &label,
                    format!("id {} is used twice; skipped", host.id),
                ));
                continue;
            }
            host.address = host.address.trim().to_owned();
            host.name = host.name.trim().to_owned();
            if host.name.is_empty() {
                host.name = if host.address.is_empty() {
                    "Host".to_owned()
                } else {
                    host.address.clone()
                };
            }
            if let Some(group) = &host.group
                && !group_ids.contains(group)
            {
                warnings.push(warning(
                    &label,
                    format!("unknown group {group}; moved to the top"),
                ));
                host.group = None;
            }
            if host.port == Some(0) {
                warnings.push(warning(
                    format!("{label}.port"),
                    "0 is not a port; inherited instead",
                ));
                host.port = None;
            }
            host.tags = clean_tags(&host.tags);
            let (_, found, _) =
                TerminalOverrides::from_table(&host.terminal, &format!("{label}.terminal"));
            warnings.extend(found);
            hosts.push(host);
        }
        self.hosts = hosts;
    }

    /// The file's text (header comment, then the version, sources, groups and hosts).
    ///
    /// # Errors
    ///
    /// Only if a value can't be written as TOML (not expected).
    pub fn to_toml_string(&self) -> Result<String, String> {
        let mut root = self.extra.clone();
        root.insert("schema_version".to_owned(), Value::Integer(SCHEMA_VERSION));
        let array = |items: Vec<Table>| Value::Array(items.into_iter().map(Value::Table).collect());
        if !self.sources.is_empty() {
            root.insert(
                "source".to_owned(),
                array(self.sources.iter().map(to_table).collect()),
            );
        }
        if !self.groups.is_empty() {
            root.insert(
                "group".to_owned(),
                array(self.groups.iter().map(to_table).collect()),
            );
        }
        let hosts: Vec<Table> = self
            .hosts
            .iter()
            .filter(|host| !host.is_linked())
            .map(to_table)
            .collect();
        if !hosts.is_empty() {
            root.insert("host".to_owned(), array(hosts));
        }
        let body = toml::to_string(&root).map_err(|error| error.to_string())?;
        Ok(format!("{HEADER}\n{body}"))
    }

    /// Reads `path`; a missing file is an empty list.
    ///
    /// # Errors
    ///
    /// [`HostsError`] when the file can't be read or isn't TOML.
    pub fn load(path: &Path) -> Result<(Self, Vec<Warning>), HostsError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Self::default(), Vec::new()));
            }
            Err(source) => {
                return Err(HostsError::Read {
                    path: path.display().to_string(),
                    source,
                });
            }
        };
        Self::from_toml_str(&text).map_err(|message| HostsError::Syntax {
            path: path.display().to_string(),
            message,
        })
    }

    /// The group with `id`.
    #[must_use]
    pub fn group(&self, id: &str) -> Option<&Group> {
        self.groups.iter().find(|group| group.id == id)
    }

    /// The host with `id`.
    #[must_use]
    pub fn host(&self, id: &str) -> Option<&Host> {
        self.hosts.iter().find(|host| host.id == id)
    }

    /// The host with `id`, else the first one named `name` (ignoring case).
    #[must_use]
    pub fn find_host(&self, id_or_name: &str) -> Option<&Host> {
        let wanted = id_or_name.trim();
        self.host(wanted).or_else(|| {
            self.hosts
                .iter()
                .find(|host| host.name.eq_ignore_ascii_case(wanted))
        })
    }

    /// The groups from `group` up to the top, the nearest first. Stops at a loop.
    #[must_use]
    pub fn group_chain(&self, group: Option<&str>) -> Vec<&Group> {
        let mut chain: Vec<&Group> = Vec::new();
        let mut current = group.and_then(|id| self.group(id));
        while let Some(group) = current {
            if chain.iter().any(|seen| seen.id == group.id) {
                break;
            }
            chain.push(group);
            current = group.parent.as_deref().and_then(|id| self.group(id));
        }
        chain
    }

    /// "Parent / Child" names of a group.
    #[must_use]
    pub fn group_path(&self, id: &str) -> String {
        let mut names: Vec<&str> = self
            .group_chain(Some(id))
            .iter()
            .map(|group| group.name.as_str())
            .collect();
        names.reverse();
        names.join(" / ")
    }

    /// `id` and every group inside it, at any depth.
    #[must_use]
    pub fn subtree(&self, id: &str) -> HashSet<String> {
        let mut found: HashSet<String> = HashSet::new();
        found.insert(id.to_owned());
        loop {
            let before = found.len();
            for group in &self.groups {
                if group
                    .parent
                    .as_ref()
                    .is_some_and(|parent| found.contains(parent))
                {
                    found.insert(group.id.clone());
                }
            }
            if found.len() == before {
                return found;
            }
        }
    }

    /// What a host of `protocol` in `group` gets for each inherited key when it sets nothing:
    /// the nearest group that sets it, else the app's default ([`HostsFile::base`]), else the
    /// built-in one.
    #[must_use]
    pub fn inherited(
        &self,
        group: Option<&str>,
        protocol: Protocol,
    ) -> BTreeMap<&'static str, Resolved> {
        let chain: Vec<Table> = self
            .group_chain(group)
            .into_iter()
            .map(|group| to_table(&group.defaults))
            .collect();
        let ids: Vec<String> = self
            .group_chain(group)
            .into_iter()
            .map(|group| group.id.clone())
            .collect();
        INHERITED_KEYS
            .iter()
            .map(|key| {
                let from_group = chain
                    .iter()
                    .zip(&ids)
                    .find_map(|(table, id)| lookup(table, key).map(|value| (value.clone(), id)));
                let resolved = match from_group {
                    Some((value, id)) => Resolved {
                        value: Some(value),
                        origin: Origin::Group(id.clone()),
                    },
                    None => Resolved {
                        value: lookup(&self.base, key)
                            .cloned()
                            .or_else(|| builtin(key, protocol)),
                        origin: Origin::Default,
                    },
                };
                (*key, resolved)
            })
            .collect()
    }

    /// Every inherited field of `host`: its own value, else the inherited one.
    #[must_use]
    pub fn resolve(&self, host: &Host) -> ResolvedHost {
        let own = to_table(host);
        let mut fields = self.inherited(host.group.as_deref(), host.protocol);
        for (key, field) in &mut fields {
            if let Some(value) = lookup(&own, key) {
                *field = Resolved {
                    value: Some(value.clone()),
                    origin: Origin::Host,
                };
            }
        }
        ResolvedHost {
            host: host.clone(),
            fields,
        }
    }

    /// The terminal levels of `host`: each group from the outermost, then the host (the
    /// profile chain of ADR 0016: global, group, host, tab).
    #[must_use]
    pub fn terminal_levels(&self, host: &Host) -> Vec<TerminalLevel> {
        let level = |profile: &Option<String>, table: &Table| TerminalLevel {
            profile: profile.clone().filter(|id| !id.trim().is_empty()),
            overrides: TerminalOverrides::from_table(table, "terminal").0,
        };
        let mut levels: Vec<TerminalLevel> = self
            .group_chain(host.group.as_deref())
            .into_iter()
            .rev()
            .map(|group| level(&group.defaults.profile, &group.defaults.terminal))
            .collect();
        levels.push(level(&host.profile, &host.terminal));
        levels
    }

    /// A jump reference as `[user@]host[:port]`: a saved host (by id or name) becomes its
    /// resolved address, anything else is used as written.
    #[must_use]
    pub fn jump_spec(&self, reference: &str) -> String {
        match self.find_host(reference) {
            Some(host) if !host.address.is_empty() => {
                let resolved = self.resolve(host);
                let mut spec = String::new();
                if let Some(user) = resolved.user() {
                    spec.push_str(user);
                    spec.push('@');
                }
                spec.push_str(&target::bracket_ipv6(&host.address));
                if let Some(port) = resolved
                    .port()
                    .filter(|port| Some(*port) != host.protocol.default_port())
                {
                    spec.push(':');
                    spec.push_str(&port.to_string());
                }
                spec
            }
            _ => reference.trim().to_owned(),
        }
    }

    /// What is wrong with `host` before it is saved, as (field, code) pairs; the codes are
    /// `required`, `invalid` and `unknown` (the UI words them).
    #[must_use]
    pub fn validate_host(&self, host: &Host) -> Vec<(&'static str, &'static str)> {
        let mut problems = Vec::new();
        if host.name.trim().is_empty() {
            problems.push(("name", "required"));
        }
        let address = host.address.trim();
        if host.protocol.needs_address() && address.is_empty() {
            problems.push(("address", "required"));
        } else if !address.is_empty() {
            let valid = match host.protocol {
                Protocol::Serial => {
                    !address.starts_with('-') && !address.chars().any(char::is_control)
                }
                Protocol::Local => true,
                Protocol::Docker | Protocol::Kube => {
                    !address.starts_with('-')
                        && !address.chars().any(|c| c.is_whitespace() || c.is_control())
                }
                Protocol::S3 => is_endpoint(address),
                _ => target::check_name(address).is_ok() && !address.contains(['[', ']']),
            };
            if !valid {
                problems.push(("address", "invalid"));
            }
        }
        if host
            .user
            .as_deref()
            .map(str::trim)
            .is_some_and(|user| !user.is_empty() && target::check_user(user).is_err())
        {
            problems.push(("user", "invalid"));
        }
        if host.port == Some(0) {
            problems.push(("port", "invalid"));
        }
        if host
            .identity_file
            .as_deref()
            .is_some_and(|file| file.trim_start().starts_with('-'))
        {
            problems.push(("identity_file", "invalid"));
        }
        if host.jump.as_ref().is_some_and(|jump| {
            jump.iter().any(|hop| {
                self.find_host(hop).is_none() && target::parse_endpoint(hop.trim()).is_err()
            })
        }) {
            problems.push(("jump", "invalid"));
        }
        if host
            .group
            .as_ref()
            .is_some_and(|group| self.group(group).is_none())
        {
            problems.push(("group", "unknown"));
        }
        check_ssh(&host.ssh, &mut problems);
        if matches!(host.protocol, Protocol::Docker | Protocol::Kube) {
            containers::check(&host.container, &mut problems);
        }
        if host
            .rdp
            .resolution
            .as_deref()
            .map(str::trim)
            .is_some_and(|text| !text.is_empty() && parse_resolution(text).is_none())
        {
            problems.push(("rdp.resolution", "invalid"));
        }
        if host.rdp.domain.as_deref().is_some_and(|domain| {
            domain.trim().starts_with('-') || domain.chars().any(char::is_control)
        }) {
            problems.push(("rdp.domain", "invalid"));
        }
        if host
            .s3
            .region
            .as_deref()
            .map(str::trim)
            .is_some_and(|region| !region.is_empty() && !is_region(region))
        {
            problems.push(("s3.region", "invalid"));
        }
        problems
    }

    /// What is wrong with `group` before it is saved (see [`HostsFile::validate_host`]).
    #[must_use]
    pub fn validate_group(&self, group: &Group) -> Vec<(&'static str, &'static str)> {
        let mut problems = Vec::new();
        if group.name.trim().is_empty() {
            problems.push(("name", "required"));
        }
        if let Some(parent) = &group.parent {
            let inside_itself = !group.id.is_empty() && self.subtree(&group.id).contains(parent);
            if self.group(parent).is_none() || inside_itself {
                problems.push(("parent", "invalid"));
            }
        }
        let defaults = &group.defaults;
        if defaults
            .user
            .as_deref()
            .map(str::trim)
            .is_some_and(|user| !user.is_empty() && target::check_user(user).is_err())
        {
            problems.push(("user", "invalid"));
        }
        if defaults.port == Some(0) {
            problems.push(("port", "invalid"));
        }
        if defaults.jump.as_ref().is_some_and(|jump| {
            jump.iter().any(|hop| {
                self.find_host(hop).is_none() && target::parse_endpoint(hop.trim()).is_err()
            })
        }) {
            problems.push(("jump", "invalid"));
        }
        check_ssh(&defaults.ssh, &mut problems);
        problems
    }

    /// Hosts per group id (direct members only); `None` counts hosts at the top.
    #[must_use]
    pub fn counts(&self) -> BTreeMap<Option<String>, usize> {
        let mut counts = BTreeMap::new();
        for host in &self.hosts {
            *counts.entry(host.group.clone()).or_insert(0) += 1;
        }
        counts
    }
}

/// Deserializes each table of the array `key`, skipping (with a warning) what doesn't fit.
fn entries<T: for<'de> Deserialize<'de>>(
    root: &mut Table,
    key: &str,
    warnings: &mut Vec<Warning>,
) -> Vec<T> {
    let Some(value) = root.remove(key) else {
        return Vec::new();
    };
    let Value::Array(items) = value else {
        warnings.push(warning(key, format!("expected [[{key}]] tables; skipped")));
        return Vec::new();
    };
    items
        .into_iter()
        .enumerate()
        .filter_map(|(index, item)| match item.try_into::<T>() {
            Ok(entry) => Some(entry),
            Err(error) => {
                let message = error.to_string();
                let first = message.lines().next().unwrap_or_default().to_owned();
                warnings.push(warning(
                    format!("{key}[{index}]"),
                    format!("skipped: {first}"),
                ));
                None
            }
        })
        .collect()
}

/// Field problems of SSH options (`ssh.proxy`, `ssh.auth_order`, `ssh.env`).
fn check_ssh(ssh: &SshOptions, problems: &mut Vec<(&'static str, &'static str)>) {
    if ssh
        .proxy
        .as_deref()
        .map(str::trim)
        .is_some_and(|proxy| !proxy.is_empty() && target::parse_proxy(proxy).is_err())
    {
        problems.push(("ssh.proxy", "invalid"));
    }
    if ssh.auth_order.as_ref().is_some_and(|order| {
        order.is_empty()
            || order
                .iter()
                .any(|method| !DEFAULT_AUTH_ORDER.contains(&method.trim()))
    }) {
        problems.push(("ssh.auth_order", "invalid"));
    }
    if ssh.env.as_ref().is_some_and(|env| {
        env.keys().any(|name| {
            name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
    }) {
        problems.push(("ssh.env", "invalid"));
    }
}

fn check_defaults(defaults: &HostDefaults, label: &str, warnings: &mut Vec<Warning>) {
    if defaults.port == Some(0) {
        warnings.push(warning(format!("{label}.port"), "0 is not a port; ignored"));
    }
    let (_, found, _) =
        TerminalOverrides::from_table(&defaults.terminal, &format!("{label}.terminal"));
    warnings.extend(found);
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"
        schema_version = 1

        [[group]]
        id = "01J9ZG0000PROD"
        name = "Production"
        color = "#E6B450"
        [group.defaults]
        user = "deploy"
        jump = ["01J9ZH00BASTION"]
        profile = "prod"
        [group.defaults.ssh]
        keepalive_secs = 60

        [[group]]
        id = "WEB"
        name = "Web"
        parent = "01J9ZG0000PROD"
        [group.defaults]
        port = 2222

        [[host]]
        id = "01J9ZH00BASTION"
        name = "bastion"
        address = "bastion.example.com"
        user = "jump"
        port = 2200

        [[host]]
        id = "01J9ZK0000WEB01"
        name = "web-01"
        group = "WEB"
        protocol = "ssh"
        address = "10.0.1.21"
        tags = ["web", "nginx", "Web", " "]
        icon = "auto"
        notes = "Nginx + app."
        future_key = "kept"
        [host.ssh]
        compression = true
        [host.terminal]
        font_size = 13
    "##;

    #[test]
    fn a_hosts_file_round_trips() {
        let (file, warnings) = HostsFile::from_toml_str(SAMPLE).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(file.groups.len(), 2);
        assert_eq!(file.hosts.len(), 2);
        let web = file.host("01J9ZK0000WEB01").unwrap();
        assert_eq!(web.tags, vec!["web", "nginx"]);
        assert_eq!(
            web.extra.get("future_key").and_then(Value::as_str),
            Some("kept")
        );
        let text = file.to_toml_string().unwrap();
        assert!(text.starts_with("# OpenSesh hosts"));
        let (back, warnings) = HostsFile::from_toml_str(&text).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(back, file);
    }

    #[test]
    fn identities_are_inherited_and_can_be_turned_off() {
        let text = r#"
            [[group]]
            id = "G"
            name = "prod"
            [group.defaults]
            identity = "01J9ZM0000000000000000IDDE"

            [[host]]
            id = "A"
            name = "a"
            address = "a"
            group = "G"

            [[host]]
            id = "B"
            name = "b"
            address = "b"
            group = "G"
            identity = ""
        "#;
        let (file, warnings) = HostsFile::from_toml_str(text).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let a = file.resolve(file.host("A").unwrap());
        assert_eq!(a.identity(), Some("01J9ZM0000000000000000IDDE"));
        assert_eq!(a.fields["identity"].origin, Origin::Group("G".into()));
        let b = file.resolve(file.host("B").unwrap());
        assert_eq!(b.identity(), None);
        assert_eq!(b.fields["identity"].origin, Origin::Host);
    }

    #[test]
    fn hosts_inherit_from_the_nearest_group() {
        let (file, _) = HostsFile::from_toml_str(SAMPLE).unwrap();
        let web = file.host("01J9ZK0000WEB01").unwrap();
        let resolved = file.resolve(web);
        assert_eq!(resolved.user(), Some("deploy"));
        assert_eq!(
            resolved.fields["user"].origin,
            Origin::Group("01J9ZG0000PROD".into())
        );
        assert_eq!(resolved.port(), Some(2222));
        assert_eq!(resolved.fields["port"].origin, Origin::Group("WEB".into()));
        assert_eq!(resolved.keepalive_secs(), 60);
        assert!(resolved.flag("ssh.compression"));
        assert_eq!(resolved.fields["ssh.compression"].origin, Origin::Host);
        assert_eq!(resolved.fields["ssh.x11"].origin, Origin::Default);
        assert_eq!(resolved.jump(), vec!["01J9ZH00BASTION"]);
        assert_eq!(
            file.jump_spec("01J9ZH00BASTION"),
            "jump@bastion.example.com:2200"
        );
        assert_eq!(file.jump_spec("bastion"), "jump@bastion.example.com:2200");
        assert_eq!(file.jump_spec("me@other:22"), "me@other:22");
        assert_eq!(file.group_path("WEB"), "Production / Web");
        let levels = file.terminal_levels(web);
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0].profile.as_deref(), Some("prod"));
        assert_eq!(levels[2].overrides.font_size, Some(13.0));

        // The bastion sets its own user and port; nothing comes from a group.
        let bastion = file.resolve(file.host("01J9ZH00BASTION").unwrap());
        assert_eq!(bastion.port(), Some(2200));
        assert_eq!(bastion.fields["ssh.keepalive_secs"].origin, Origin::Default);
        // An empty jump list overrides the group's.
        let mut direct = web.clone();
        direct.jump = Some(Vec::new());
        assert!(file.resolve(&direct).jump().is_empty());
    }

    #[test]
    fn broken_entries_are_fixed_or_skipped_with_warnings() {
        let text = r#"
            [[group]]
            id = "A"
            name = "A"
            parent = "B"
            [[group]]
            id = "B"
            name = "B"
            parent = "A"
            [[group]]
            id = "A"
            name = "Again"
            [[host]]
            name = "no id"
            address = "h1"
            group = "missing"
            port = 0
            [[host]]
            id = "X"
            protocol = "carrier-pigeon"
            [[host]]
            id = "Y"
            address = "y"
            [host.terminal]
            font_size = "big"
        "#;
        let (file, warnings) = HostsFile::from_toml_str(text).unwrap();
        assert_eq!(file.groups.len(), 2);
        assert!(
            file.groups.iter().any(|group| group.parent.is_none()),
            "the loop is cut"
        );
        assert_eq!(file.hosts.len(), 2, "the unknown protocol is skipped");
        let first = &file.hosts[0];
        assert_eq!(first.id.len(), 26, "a ULID was given");
        assert_eq!(first.group, None);
        assert_eq!(first.port, None);
        assert_eq!(file.hosts[1].name, "y");
        let text: Vec<String> = warnings.iter().map(ToString::to_string).collect();
        assert!(text.iter().any(|w| w.contains("used twice")), "{text:?}");
        assert!(text.iter().any(|w| w.contains("loop")), "{text:?}");
        assert!(text.iter().any(|w| w.contains("unknown group")), "{text:?}");
        assert!(text.iter().any(|w| w.contains("skipped")), "{text:?}");
        assert!(text.iter().any(|w| w.contains("font_size")), "{text:?}");
        assert!(HostsFile::from_toml_str("[[host").is_err());
    }

    #[test]
    fn a_newer_file_is_read_only_and_linked_hosts_are_not_saved() {
        let (file, warnings) = HostsFile::from_toml_str("schema_version = 9").unwrap();
        assert!(file.read_only);
        assert_eq!(warnings.len(), 1);
        let mut file = HostsFile::default();
        file.hosts.push(Host {
            id: format!("{LINKED_PREFIX}web"),
            name: "web".into(),
            address: "web".into(),
            ..Host::default()
        });
        let text = file.to_toml_string().unwrap();
        assert!(!text.contains("[[host]]"), "{text}");
        assert!(file.hosts[0].is_linked());
    }

    #[test]
    fn hosts_and_groups_are_checked_before_saving() {
        let (file, _) = HostsFile::from_toml_str(SAMPLE).unwrap();
        let good = file.host("01J9ZK0000WEB01").unwrap().clone();
        assert!(file.validate_host(&good).is_empty());
        let bad = Host {
            name: " ".into(),
            address: "-oProxyCommand=x".into(),
            user: Some("a b".into()),
            port: Some(0),
            identity_file: Some("-F".into()),
            jump: Some(vec!["bastion".into(), "bad host".into()]),
            group: Some("nope".into()),
            ..Host::default()
        };
        let fields: Vec<&str> = file
            .validate_host(&bad)
            .iter()
            .map(|(field, _)| *field)
            .collect();
        assert_eq!(
            fields,
            vec![
                "name",
                "address",
                "user",
                "port",
                "identity_file",
                "jump",
                "group"
            ]
        );
        let local = Host {
            name: "shell".into(),
            protocol: Protocol::Local,
            ..Host::default()
        };
        assert!(file.validate_host(&local).is_empty());
        let serial = Host {
            name: "console".into(),
            protocol: Protocol::Serial,
            address: "/dev/ttyUSB0".into(),
            ..Host::default()
        };
        assert!(file.validate_host(&serial).is_empty());
        // Remote desktops: a size of 200 to 8192 pixels a side, a domain that isn't an option.
        assert_eq!(parse_resolution("1920x1080"), Some((1920, 1080)));
        assert_eq!(parse_resolution(" 1280 X 800 "), Some((1280, 800)));
        assert_eq!(parse_resolution("100x800"), None);
        assert_eq!(parse_resolution("1920"), None);
        let mut desktop = Host {
            name: "win11".into(),
            protocol: Protocol::Rdp,
            address: "win11.lan".into(),
            rdp: RdpOptions {
                domain: Some("CORP".into()),
                scaling: Some(Scaling::Fit),
                resolution: Some("1920x1080".into()),
                clipboard: Some(false),
            },
            ..Host::default()
        };
        assert!(file.validate_host(&desktop).is_empty());
        assert_eq!(desktop.rdp.size(), Some((1920, 1080)));
        let text = toml::to_string(&desktop).unwrap();
        assert!(text.contains("scaling = \"fit\""), "{text}");
        let back: Host = toml::from_str(&text).unwrap();
        assert_eq!(back.rdp, desktop.rdp);
        desktop.rdp.resolution = Some("huge".into());
        desktop.rdp.domain = Some("-x".into());
        let fields: Vec<&str> = file
            .validate_host(&desktop)
            .iter()
            .map(|(field, _)| *field)
            .collect();
        assert_eq!(fields, vec!["rdp.resolution", "rdp.domain"]);
        // A group can't move into itself or its own subgroup.
        let mut prod = file.group("01J9ZG0000PROD").unwrap().clone();
        prod.parent = Some("WEB".into());
        assert_eq!(file.validate_group(&prod), vec![("parent", "invalid")]);
    }

    #[test]
    fn app_defaults_sit_between_groups_and_built_in_values() {
        let mut file = HostsFile::default();
        let mut group = Group {
            id: "G".into(),
            name: "g".into(),
            ..Group::default()
        };
        group.defaults.ssh.keepalive_secs = Some(10);
        file.groups.push(group);
        file.hosts.push(Host {
            id: "H".into(),
            name: "h".into(),
            address: "h".into(),
            group: Some("G".into()),
            ..Host::default()
        });
        let config = crate::config::SshSettings {
            keepalive_secs: 99,
            detect_os: false,
            monitor: false,
            ..crate::config::SshSettings::default()
        };
        file.base = config.host_defaults();
        let resolved = file.resolve(&file.hosts[0]);
        assert_eq!(resolved.keepalive_secs(), 10);
        assert!(!resolved.flag("ssh.detect_os"));
        assert!(!resolved.flag("ssh.monitor"));
        assert_eq!(resolved.fields["ssh.detect_os"].origin, Origin::Default);
        // A host turns the monitor back on for itself.
        file.hosts[0].ssh.monitor = Some(true);
        assert!(file.resolve(&file.hosts[0]).flag("ssh.monitor"));
        file.hosts[0].ssh.monitor = None;
        // The app's defaults are never written to hosts.toml.
        assert!(!file.to_toml_string().unwrap().contains("detect_os"));
    }

    #[test]
    fn subtrees_and_counts() {
        let (file, _) = HostsFile::from_toml_str(SAMPLE).unwrap();
        let tree = file.subtree("01J9ZG0000PROD");
        assert!(tree.contains("WEB") && tree.len() == 2);
        let counts = file.counts();
        assert_eq!(counts.get(&Some("WEB".to_owned())), Some(&1));
        assert_eq!(counts.get(&None), Some(&1));
        assert_eq!(Protocol::parse("RDP"), Some(Protocol::Rdp));
        assert_eq!(Protocol::Rdp.default_port(), Some(3389));
        assert!(!Protocol::Local.needs_address());
        assert_eq!(new_id().len(), 26);
    }
}
