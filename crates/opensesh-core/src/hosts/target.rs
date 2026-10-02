//! Quick connect (PLAN Sprint 5): what the user types, parsed into a connection target, and the
//! OpenSSH command line of a target or a saved host (the `openssh` backend, used for every SSH
//! host until the built-in client arrives in Sprint 7).
//!
//! Accepted forms: `host`, `user@host`, `host:port`, `user@[::1]:2222`, a bare IPv6 address,
//! `ssh://`, `sftp://`, `telnet://`, `mosh://`, `rdp://` and `vnc://` URLs, the options `-J
//! jump[,jump]`, `-p port` and `-l user` (a leading `ssh`, `mosh` or `telnet` is allowed, so
//! a pasted command works), `serial:///dev/ttyUSB0?baud=115200` or `serial://COM3`, the
//! containers: `docker://[user@]name`, `podman://[user@]name` and
//! `kube://[namespace/]pod[?container=name&context=name]`, and S3 storage:
//! `s3://[access_key@]host[:port][/bucket/folder][?region=name&path_style=false]` (`s3+http://`
//! for a server without TLS).
//!
//! Hosts and users that start with `-` are refused: `ssh` would read them as options.

use std::fmt;

use super::containers::{self, ContainerOptions, Engine};
use super::{
    FlowControl, Host, HostsFile, Parity, Protocol, S3Options, SerialOptions, X11Forwarding,
};

/// A place to connect to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Target {
    /// How.
    pub protocol: Protocol,
    /// User name.
    pub user: Option<String>,
    /// Host name or address; the device for serial.
    pub host: String,
    /// Port, when given.
    pub port: Option<u16>,
    /// Jump hosts, first hop first, as `[user@]host[:port]`.
    pub jump: Vec<String>,
    /// Serial line settings.
    pub serial: SerialOptions,
    /// How a container or pod is entered.
    pub container: ContainerOptions,
    /// S3 settings (the host is then the endpoint URL).
    pub s3: S3Options,
    /// Where to start (S3: `/bucket/folder`); empty for the default.
    pub path: String,
}

impl Target {
    /// The target as an unsaved host (no group: the app's defaults apply).
    #[must_use]
    pub fn to_host(&self) -> Host {
        Host {
            name: self.host.clone(),
            protocol: self.protocol,
            address: self.host.clone(),
            port: self.port,
            user: self.user.clone(),
            jump: (!self.jump.is_empty()).then(|| self.jump.clone()),
            serial: self.serial.clone(),
            container: self.container.clone(),
            s3: self.s3.clone(),
            ..Host::default()
        }
    }
}

/// Why the text isn't a target.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TargetError {
    /// Nothing was typed.
    #[error("type a host, user@host:port or a URL")]
    Empty,
    /// A quote isn't closed.
    #[error("a quote is not closed")]
    Unbalanced,
    /// No host was given.
    #[error("no host was given")]
    MissingHost,
    /// More than one host was given.
    #[error("unexpected {0:?}: give one host")]
    Extra(String),
    /// An option needs a value.
    #[error("{0} needs a value")]
    MissingValue(String),
    /// An option that isn't supported here.
    #[error("unknown option {0}")]
    UnknownOption(String),
    /// A URL scheme that isn't a protocol.
    #[error("unknown scheme {0}://")]
    UnknownScheme(String),
    /// A port that isn't 1 to 65535.
    #[error("{0:?} is not a port (1 to 65535)")]
    BadPort(String),
    /// A host or user that can't be used.
    #[error("{0:?} is not a valid host or user")]
    BadName(String),
    /// A serial setting that can't be used.
    #[error("serial: {0}")]
    BadSerial(String),
    /// A container or pod that can't be used.
    #[error("{0}")]
    BadContainer(String),
    /// An S3 address or setting that can't be used.
    #[error("s3: {0}")]
    BadS3(String),
}

impl fmt::Display for Target {
    /// The canonical quick-connect text (parses back to the same target).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.protocol == Protocol::Serial {
            write!(f, "serial://{}", self.host)?;
            let mut query = Vec::new();
            if let Some(baud) = self.serial.baud {
                query.push(format!("baud={baud}"));
            }
            if let Some(bits) = self.serial.data_bits {
                query.push(format!("data_bits={bits}"));
            }
            if let Some(parity) = self.serial.parity {
                query.push(format!("parity={}", parity_name(parity)));
            }
            if let Some(bits) = self.serial.stop_bits {
                query.push(format!("stop_bits={bits}"));
            }
            if let Some(flow) = self.serial.flow_control {
                query.push(format!("flow={}", flow_name(flow)));
            }
            if !query.is_empty() {
                write!(f, "?{}", query.join("&"))?;
            }
            return Ok(());
        }
        if matches!(self.protocol, Protocol::Docker | Protocol::Kube) {
            return self.fmt_container(f);
        }
        if self.protocol == Protocol::S3 {
            return self.fmt_s3(f);
        }
        if self.protocol != Protocol::Ssh {
            write!(f, "{}://", self.protocol.as_str())?;
        }
        if let Some(user) = &self.user {
            write!(f, "{user}@")?;
        }
        f.write_str(&bracket_ipv6(&self.host))?;
        if let Some(port) = self.port {
            write!(f, ":{port}")?;
        }
        if !self.jump.is_empty() {
            write!(f, " -J {}", self.jump.join(","))?;
        }
        Ok(())
    }
}

/// `host` in brackets when it is an IPv6 address (so a port can follow).
#[must_use]
pub fn bracket_ipv6(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_owned()
    }
}

const fn parity_name(parity: Parity) -> &'static str {
    match parity {
        Parity::None => "none",
        Parity::Even => "even",
        Parity::Odd => "odd",
    }
}

const fn flow_name(flow: FlowControl) -> &'static str {
    match flow {
        FlowControl::None => "none",
        FlowControl::Software => "software",
        FlowControl::Hardware => "hardware",
    }
}

/// Splits like a POSIX shell: whitespace separates, single and double quotes group, a
/// backslash escapes the next character (outside single quotes).
fn split_words(text: &str) -> Result<Vec<String>, TargetError> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(inner) => current.push(inner),
                        None => return Err(TargetError::Unbalanced),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(escaped) => current.push(escaped),
                            None => return Err(TargetError::Unbalanced),
                        },
                        Some(inner) => current.push(inner),
                        None => return Err(TargetError::Unbalanced),
                    }
                }
            }
            '\\' => {
                in_word = true;
                if let Some(escaped) = chars.next() {
                    current.push(escaped);
                }
            }
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            c => {
                in_word = true;
                current.push(c);
            }
        }
    }
    if in_word {
        words.push(current);
    }
    Ok(words)
}

/// A host name: no whitespace, `@` or `/`, and nothing `ssh` would read as an option.
pub(crate) fn check_name(name: &str) -> Result<(), TargetError> {
    let bad = name.is_empty()
        || name.starts_with('-')
        || name
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '@' || c == '/');
    if bad {
        Err(TargetError::BadName(name.to_owned()))
    } else {
        Ok(())
    }
}

/// A user name: like a host, but `@` is allowed (OpenSSH splits at the last one).
pub(crate) fn check_user(name: &str) -> Result<(), TargetError> {
    let bad = name.is_empty()
        || name.starts_with('-')
        || name
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '/');
    if bad {
        Err(TargetError::BadName(name.to_owned()))
    } else {
        Ok(())
    }
}

fn parse_port(text: &str) -> Result<u16, TargetError> {
    text.parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or_else(|| TargetError::BadPort(text.to_owned()))
}

/// Decodes `%XX` escapes (user names in URLs); anything malformed is kept as written.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    let hex = |byte: u8| {
        char::from(byte)
            .to_digit(16)
            .and_then(|digit| u8::try_from(digit).ok())
    };
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2]))
        {
            out.push(high * 16 + low);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_owned())
}

/// `[user@]host[:port]`, with IPv6 in brackets (or bare, without a port).
pub(crate) fn parse_endpoint(
    text: &str,
) -> Result<(Option<String>, String, Option<u16>), TargetError> {
    let (user, rest) = match text.rsplit_once('@') {
        Some((user, rest)) => {
            // `user;fingerprint=...` in ssh URLs (RFC draft): the parameters are ignored.
            let user = percent_decode(user.split(';').next().unwrap_or_default());
            check_user(&user)?;
            (Some(user), rest)
        }
        None => (None, text),
    };
    let (host, port) = if let Some(inner) = rest.strip_prefix('[') {
        let (host, after) = inner
            .split_once(']')
            .ok_or_else(|| TargetError::BadName(rest.to_owned()))?;
        let port = match after {
            "" => None,
            after => Some(parse_port(
                after
                    .strip_prefix(':')
                    .ok_or_else(|| TargetError::BadName(rest.to_owned()))?,
            )?),
        };
        (host.to_owned(), port)
    } else if rest.matches(':').count() > 1 {
        // A bare IPv6 address: no port.
        (rest.to_owned(), None)
    } else if let Some((host, port)) = rest.rsplit_once(':') {
        (host.to_owned(), Some(parse_port(port)?))
    } else {
        (rest.to_owned(), None)
    };
    if host.is_empty() {
        return Err(TargetError::MissingHost);
    }
    check_name(&host)?;
    Ok((user, host, port))
}

fn parse_jump(list: &str, jump: &mut Vec<String>) -> Result<(), TargetError> {
    for item in list
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        parse_endpoint(item)?;
        jump.push(item.to_owned());
    }
    Ok(())
}

impl Target {
    /// `docker://[user@]container`, `podman://...` or `kube://[namespace/]pod[?container=&context=]`.
    fn fmt_container(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let options = &self.container;
        if self.protocol == Protocol::Kube {
            f.write_str("kube://")?;
            if let Some(namespace) = &options.namespace {
                write!(f, "{namespace}/")?;
            }
            f.write_str(&self.host)?;
            let mut query = Vec::new();
            if let Some(container) = &options.pod_container {
                query.push(format!("container={container}"));
            }
            if let Some(context) = &options.context {
                query.push(format!("context={}", percent_encode(context)));
            }
            if !query.is_empty() {
                write!(f, "?{}", query.join("&"))?;
            }
            return Ok(());
        }
        write!(f, "{}://", options.engine.unwrap_or_default().program())?;
        if let Some(user) = &self.user {
            write!(f, "{user}@")?;
        }
        f.write_str(&self.host)
    }
}

impl Target {
    /// `s3://[user@]host[:port][path][?region=&path_style=]` (`s3+http://` without TLS).
    fn fmt_s3(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (scheme, server) = match self.host.split_once("://") {
            Some((scheme, server)) if scheme.eq_ignore_ascii_case("http") => ("s3+http", server),
            Some((_, server)) => ("s3", server),
            None => ("s3", self.host.as_str()),
        };
        write!(f, "{scheme}://")?;
        if let Some(user) = &self.user {
            write!(f, "{user}@")?;
        }
        f.write_str(server)?;
        f.write_str(&self.path)?;
        let mut query = Vec::new();
        if let Some(region) = &self.s3.region {
            query.push(format!("region={region}"));
        }
        if let Some(path_style) = self.s3.path_style {
            query.push(format!("path_style={path_style}"));
        }
        if !query.is_empty() {
            write!(f, "?{}", query.join("&"))?;
        }
        Ok(())
    }
}

/// `s3://` (`tls`) or `s3+http://` text after the scheme: `[access_key@]host[:port][/bucket/folder]
/// [?region=name&path_style=false]`.
fn parse_s3(rest: &str, tls: bool) -> Result<Target, TargetError> {
    let bad = |what: &str| TargetError::BadS3(what.to_owned());
    let (rest, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (authority, path) = match rest.split_once('/') {
        Some((authority, path)) => (authority, path.trim_end_matches('/')),
        None => (rest, ""),
    };
    let (user, host, port) =
        parse_endpoint(authority).map_err(|_| bad("no server (s3://access_key@host:port)"))?;
    let mut s3 = S3Options::default();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = percent_decode(value);
        match key {
            "region" if super::is_region(&value) => s3.region = Some(value),
            "path_style" => {
                s3.path_style = Some(match value.as_str() {
                    "true" | "1" | "on" => true,
                    "false" | "0" | "off" => false,
                    _ => return Err(bad(&format!("path_style={value}"))),
                });
            }
            "region" => return Err(bad(&format!("not a region: {value:?}"))),
            _ => return Err(bad(&format!("unknown setting {key}"))),
        }
    }
    let mut endpoint = format!(
        "{}://{}",
        if tls { "https" } else { "http" },
        bracket_ipv6(&host)
    );
    if let Some(port) = port {
        endpoint.push_str(&format!(":{port}"));
    }
    Ok(Target {
        protocol: Protocol::S3,
        user,
        host: endpoint,
        s3,
        path: if path.is_empty() {
            String::new()
        } else {
            format!("/{}", percent_decode(path))
        },
        ..Target::default()
    })
}

/// `text` with `%`, `&`, `#` and spaces percent-encoded (a query value).
fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '%' | '&' | '#' | ' ' | '?' | '=' => out.push_str(&format!("%{:02X}", u32::from(c))),
            c => out.push(c),
        }
    }
    out
}

/// `docker://[user@]container`, `podman://[user@]container`, or
/// `kube://[namespace/]pod[?container=name&context=name]`.
fn parse_container(scheme: &str, rest: &str) -> Result<Target, TargetError> {
    let bad = |what: &str| TargetError::BadContainer(what.to_owned());
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let path = path.trim_end_matches('/');
    let mut container = ContainerOptions::default();
    let mut user = None;
    let host = if scheme == "kube" {
        let (namespace, pod) = match path.split_once('/') {
            Some((namespace, pod)) => (Some(percent_decode(namespace)), percent_decode(pod)),
            None => (None, percent_decode(path)),
        };
        if let Some(namespace) = namespace {
            if !containers::is_name(&namespace) {
                return Err(bad("not a namespace (kube://namespace/pod)"));
            }
            container.namespace = Some(namespace);
        }
        for pair in query.split('&').filter(|pair| !pair.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let value = percent_decode(value);
            match key {
                "container" if containers::is_name(&value) => container.pod_container = Some(value),
                "context" if containers::is_context(&value) => container.context = Some(value),
                "container" | "context" => {
                    return Err(bad(&format!("not a {key} name: {value:?}")));
                }
                _ => return Err(bad(&format!("unknown setting {key}"))),
            }
        }
        pod
    } else {
        if !query.is_empty() {
            return Err(bad("a container takes no settings"));
        }
        if scheme == "podman" {
            container.engine = Some(Engine::Podman);
        }
        match path.rsplit_once('@') {
            Some((name, rest)) => {
                check_user(name)?;
                user = Some(name.to_owned());
                percent_decode(rest)
            }
            None => percent_decode(path),
        }
    };
    if !containers::is_name(&host) {
        return Err(bad(if scheme == "kube" {
            "no pod (kube://namespace/pod)"
        } else {
            "no container (docker://name)"
        }));
    }
    Ok(Target {
        protocol: if scheme == "kube" {
            Protocol::Kube
        } else {
            Protocol::Docker
        },
        user,
        host,
        container,
        ..Target::default()
    })
}

fn parse_serial(rest: &str) -> Result<Target, TargetError> {
    let (device, query) = rest.split_once('?').unwrap_or((rest, ""));
    let device = percent_decode(device.trim());
    if device.is_empty() || device.starts_with('-') {
        return Err(TargetError::BadSerial(
            "no device (serial:///dev/ttyUSB0 or serial://COM3)".to_owned(),
        ));
    }
    let mut serial = SerialOptions::default();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let bad = || TargetError::BadSerial(format!("{key}={value}"));
        match key {
            "baud" => {
                serial.baud = Some(
                    value
                        .parse()
                        .ok()
                        .filter(|baud| *baud > 0)
                        .ok_or_else(bad)?,
                )
            }
            "data_bits" => {
                serial.data_bits = Some(
                    value
                        .parse()
                        .ok()
                        .filter(|bits| (5..=8).contains(bits))
                        .ok_or_else(bad)?,
                );
            }
            "stop_bits" => {
                serial.stop_bits = Some(
                    value
                        .parse()
                        .ok()
                        .filter(|bits| *bits == 1 || *bits == 2)
                        .ok_or_else(bad)?,
                );
            }
            "parity" => {
                serial.parity = Some(match value {
                    "none" => Parity::None,
                    "even" => Parity::Even,
                    "odd" => Parity::Odd,
                    _ => return Err(bad()),
                });
            }
            "flow" | "flow_control" => {
                serial.flow_control = Some(match value {
                    "none" => FlowControl::None,
                    "software" | "xonxoff" => FlowControl::Software,
                    "hardware" | "rtscts" => FlowControl::Hardware,
                    _ => return Err(bad()),
                });
            }
            _ => return Err(TargetError::BadSerial(format!("unknown setting {key}"))),
        }
    }
    Ok(Target {
        protocol: Protocol::Serial,
        host: device,
        serial,
        ..Target::default()
    })
}

fn parse_url(scheme: &str, rest: &str) -> Result<Target, TargetError> {
    let protocol = Protocol::parse(scheme)
        .filter(|protocol| protocol.is_network())
        .ok_or_else(|| TargetError::UnknownScheme(scheme.to_owned()))?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let (user, host, port) = parse_endpoint(authority)?;
    Ok(Target {
        protocol,
        user,
        host,
        port,
        ..Target::default()
    })
}

/// Parses quick-connect text.
///
/// # Errors
///
/// [`TargetError`] says what is wrong, for the popup to show.
pub fn parse(text: &str) -> Result<Target, TargetError> {
    let words = split_words(text.trim())?;
    let mut words = words.into_iter().peekable();
    let mut protocol = None;
    if let Some(first) = words.peek() {
        let program = first.to_ascii_lowercase();
        if ["ssh", "mosh", "telnet"].contains(&program.as_str()) {
            protocol = Protocol::parse(&program);
            words.next();
        }
    }
    let mut jump = Vec::new();
    let mut port = None;
    let mut user = None;
    let mut positional: Vec<String> = Vec::new();
    while let Some(word) = words.next() {
        if let Some(option) = word.strip_prefix('-').filter(|_| word.len() > 1) {
            let mut chars = option.chars();
            let name: String = chars.next().map(String::from).unwrap_or_default();
            let inline = chars.as_str();
            let mut value = || {
                if inline.is_empty() {
                    words
                        .next()
                        .ok_or_else(|| TargetError::MissingValue(format!("-{name}")))
                } else {
                    Ok(inline.to_owned())
                }
            };
            match name.as_str() {
                "J" => parse_jump(&value()?, &mut jump)?,
                "p" => port = Some(parse_port(&value()?)?),
                "l" => {
                    let name = value()?;
                    check_user(&name)?;
                    user = Some(name);
                }
                _ => return Err(TargetError::UnknownOption(word)),
            }
        } else {
            positional.push(word);
        }
    }
    let mut positional = positional.into_iter();
    let Some(first) = positional.next() else {
        return Err(if text.trim().is_empty() {
            TargetError::Empty
        } else {
            TargetError::MissingHost
        });
    };
    let container = ["docker", "podman", "kube"].into_iter().find_map(|scheme| {
        first
            .strip_prefix(scheme)
            .and_then(|rest| rest.strip_prefix("://"))
            .map(|rest| (scheme, rest))
    });
    let mut target = if let Some(rest) = first.strip_prefix("serial:") {
        parse_serial(rest.trim_start_matches("//"))?
    } else if let Some(rest) = first.strip_prefix("s3://") {
        parse_s3(rest, true)?
    } else if let Some(rest) = first.strip_prefix("s3+http://") {
        parse_s3(rest, false)?
    } else if let Some((scheme, rest)) = container {
        parse_container(scheme, rest)?
    } else if let Some((scheme, rest)) = first.split_once("://") {
        parse_url(scheme, rest)?
    } else {
        let (user, host, port) = parse_endpoint(&first)?;
        Target {
            protocol: protocol.unwrap_or_default(),
            user,
            host,
            port,
            ..Target::default()
        }
    };
    // `telnet host 23`: the port as a second word.
    if target.protocol == Protocol::Telnet
        && target.port.is_none()
        && let Some(word) = positional.next()
    {
        target.port = Some(parse_port(&word)?);
    }
    // `vnc://host:1`: a display number, as VNC viewers read it (display 1 is port 5901).
    if target.protocol == Protocol::Vnc
        && let Some(display) = target.port.filter(|port| *port < 100)
    {
        target.port = Some(5900 + display);
    }
    if let Some(extra) = positional.next() {
        return Err(TargetError::Extra(extra));
    }
    if !matches!(
        target.protocol,
        Protocol::Serial | Protocol::Docker | Protocol::Kube | Protocol::S3
    ) {
        if port.is_some() {
            target.port = port;
        }
        if user.is_some() {
            target.user = user;
        }
        target.jump = jump;
    }
    Ok(target)
}

/// The OpenSSH options of a connection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SshArgs {
    /// User name.
    pub user: Option<String>,
    /// Host name or address.
    pub host: String,
    /// Port, when not 22.
    pub port: Option<u16>,
    /// Jump hosts as `[user@]host[:port]`.
    pub jump: Vec<String>,
    /// Private key file.
    pub identity_file: Option<String>,
    /// Keepalive interval in seconds (0: none).
    pub keepalive_secs: u32,
    /// Compression.
    pub compression: bool,
    /// Agent forwarding.
    pub agent_forwarding: bool,
    /// X11 forwarding.
    pub x11: X11Forwarding,
}

impl SshArgs {
    /// The arguments after `ssh`.
    #[must_use]
    pub fn to_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if let Some(port) = self.port.filter(|port| *port != 22) {
            args.push("-p".to_owned());
            args.push(port.to_string());
        }
        if let Some(file) = self
            .identity_file
            .as_deref()
            .filter(|file| !file.trim().is_empty())
        {
            args.push("-i".to_owned());
            args.push(file.trim().to_owned());
        }
        if !self.jump.is_empty() {
            args.push("-J".to_owned());
            args.push(self.jump.join(","));
        }
        if self.keepalive_secs > 0 {
            args.push("-o".to_owned());
            args.push(format!("ServerAliveInterval={}", self.keepalive_secs));
        }
        if self.compression {
            args.push("-C".to_owned());
        }
        if self.agent_forwarding {
            args.push("-A".to_owned());
        }
        match self.x11 {
            X11Forwarding::Off => {}
            X11Forwarding::Untrusted => args.push("-X".to_owned()),
            X11Forwarding::Trusted => args.push("-Y".to_owned()),
        }
        args.push(match &self.user {
            Some(user) => format!("{user}@{}", self.host),
            None => self.host.clone(),
        });
        args
    }

    /// The command as one line for a shell (`ssh -p 2222 deploy@web`), quoted where needed.
    #[must_use]
    pub fn to_command_line(&self) -> String {
        std::iter::once("ssh".to_owned())
            .chain(self.to_args().iter().map(|arg| shell_quote(arg)))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Checks what goes on the command line: nothing may look like an option.
    ///
    /// # Errors
    ///
    /// [`TargetError::BadName`] for a host, user or jump that `ssh` would misread.
    pub fn check(&self) -> Result<(), TargetError> {
        check_name(&self.host)?;
        if let Some(user) = &self.user {
            check_user(user)?;
        }
        for hop in &self.jump {
            parse_endpoint(hop)?;
        }
        if let Some(file) = &self.identity_file
            && file.trim_start().starts_with('-')
        {
            return Err(TargetError::BadName(file.clone()));
        }
        Ok(())
    }
}

impl Target {
    /// The OpenSSH options of an SSH target.
    #[must_use]
    pub fn ssh_args(&self) -> SshArgs {
        SshArgs {
            user: self.user.clone(),
            host: self.host.clone(),
            port: self.port,
            jump: self.jump.clone(),
            ..SshArgs::default()
        }
    }
}

impl HostsFile {
    /// The OpenSSH options of a saved host, with everything it inherits; saved jump hosts
    /// become their addresses.
    #[must_use]
    pub fn ssh_args(&self, host: &super::Host) -> SshArgs {
        let resolved = self.resolve(host);
        SshArgs {
            user: resolved.user().map(str::to_owned),
            host: host.address.clone(),
            port: resolved.port(),
            jump: resolved
                .jump()
                .iter()
                .map(|reference| self.jump_spec(reference))
                .collect(),
            identity_file: resolved.string("identity_file").map(str::to_owned),
            keepalive_secs: resolved.keepalive_secs(),
            compression: resolved.flag("ssh.compression"),
            agent_forwarding: resolved.flag("ssh.agent_forwarding"),
            x11: resolved.x11(),
        }
    }
}

/// The kind of a proxy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyKind {
    /// SOCKS5.
    Socks5,
    /// HTTP CONNECT.
    Http,
}

/// A proxy written as `socks5://[user@]host[:port]` or `http://[user@]host[:port]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyUrl {
    /// SOCKS5 or HTTP.
    pub kind: ProxyKind,
    /// The proxy host.
    pub host: String,
    /// The proxy port (1080 for SOCKS5 and 8080 for HTTP when not given).
    pub port: u16,
    /// A user name, if given.
    pub user: Option<String>,
}

/// Parses a proxy URL (see [`ProxyUrl`]).
///
/// # Errors
///
/// [`TargetError::UnknownScheme`] for another scheme, and the errors of a host and port.
pub fn parse_proxy(text: &str) -> Result<ProxyUrl, TargetError> {
    let text = text.trim();
    let (scheme, rest) = text
        .split_once("://")
        .ok_or_else(|| TargetError::UnknownScheme(text.to_owned()))?;
    let (kind, default_port) = match scheme.to_ascii_lowercase().as_str() {
        "socks5" | "socks5h" | "socks" => (ProxyKind::Socks5, 1080),
        "http" => (ProxyKind::Http, 8080),
        other => return Err(TargetError::UnknownScheme(other.to_owned())),
    };
    let rest = rest.trim_end_matches('/');
    let (user, address) = match rest.rsplit_once('@') {
        Some((user, address)) => (Some(user.to_owned()), address),
        None => (None, rest),
    };
    if let Some(user) = &user {
        check_user(user)?;
    }
    let (host, port) = split_host_port(address)?;
    check_name(&host)?;
    Ok(ProxyUrl {
        kind,
        host,
        port: port.unwrap_or(default_port),
        user,
    })
}

/// `host`, `host:port`, `[v6]` or `[v6]:port`.
fn split_host_port(text: &str) -> Result<(String, Option<u16>), TargetError> {
    let port_of = |port: &str| {
        port.parse::<u16>()
            .ok()
            .filter(|port| *port > 0)
            .ok_or_else(|| TargetError::BadPort(port.to_owned()))
    };
    if let Some(inner) = text.strip_prefix('[') {
        let (host, after) = inner
            .split_once(']')
            .ok_or_else(|| TargetError::BadName(text.to_owned()))?;
        let port = match after.strip_prefix(':') {
            Some(port) => Some(port_of(port)?),
            None if after.is_empty() => None,
            None => return Err(TargetError::BadName(text.to_owned())),
        };
        return Ok((host.to_owned(), port));
    }
    match text.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => Ok((host.to_owned(), Some(port_of(port)?))),
        _ if text.is_empty() => Err(TargetError::MissingHost),
        _ => Ok((text.to_owned(), None)),
    }
}

/// Quotes `arg` for a POSIX shell when it has anything but safe characters.
#[must_use]
pub fn shell_quote(arg: &str) -> String {
    let safe = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "@%+=:,./-_[]~".contains(c));
    if safe {
        arg.to_owned()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(text: &str) -> Target {
        parse(text).unwrap_or_else(|error| panic!("{text}: {error}"))
    }

    #[test]
    fn proxies() {
        let socks = parse_proxy("socks5://me@proxy.lan").unwrap();
        assert_eq!(socks.kind, ProxyKind::Socks5);
        assert_eq!((socks.host.as_str(), socks.port), ("proxy.lan", 1080));
        assert_eq!(socks.user.as_deref(), Some("me"));
        let http = parse_proxy("http://[::1]:3128/").unwrap();
        assert_eq!(http.kind, ProxyKind::Http);
        assert_eq!((http.host.as_str(), http.port), ("::1", 3128));
        assert!(parse_proxy("ftp://x").is_err());
        assert!(parse_proxy("proxy:1080").is_err());
        assert!(parse_proxy("socks5://proxy:0").is_err());
        assert!(parse_proxy("http://-oProxyCommand=x").is_err());
    }

    #[test]
    fn plain_forms() {
        let t = target("deploy@10.0.1.21:2222");
        assert_eq!(
            (t.user.as_deref(), t.host.as_str(), t.port),
            (Some("deploy"), "10.0.1.21", Some(2222))
        );
        assert_eq!(t.protocol, Protocol::Ssh);
        assert_eq!(target("web-01").host, "web-01");
        assert_eq!(target("  web:22 ").port, Some(22));
        let v6 = target("root@[fe80::1]:2200");
        assert_eq!((v6.host.as_str(), v6.port), ("fe80::1", Some(2200)));
        assert_eq!(target("fe80::1").host, "fe80::1");
        assert_eq!(target("[::1]").host, "::1");
        assert_eq!(target("fe80::1").to_string(), "[fe80::1]");
    }

    #[test]
    fn options_and_pasted_commands() {
        let t = target("ssh -p 2222 -J bastion,ops@hop:2200 -l deploy web");
        assert_eq!(t.port, Some(2222));
        assert_eq!(t.user.as_deref(), Some("deploy"));
        assert_eq!(t.jump, vec!["bastion", "ops@hop:2200"]);
        assert_eq!(t.to_string(), "deploy@web:2222 -J bastion,ops@hop:2200");
        assert_eq!(parse(&t.to_string()), Ok(t));
        let t = target("web -Jbastion -p2222");
        assert_eq!((t.port, t.jump.len()), (Some(2222), 1));
        let t = target("telnet router 2323");
        assert_eq!((t.protocol, t.port), (Protocol::Telnet, Some(2323)));
        assert!(matches!(parse("'my host'@a"), Err(TargetError::BadName(_))));
        assert!(matches!(
            parse("web -\u{fc}"),
            Err(TargetError::UnknownOption(_))
        ));
    }

    #[test]
    fn urls() {
        let t = target("ssh://deploy;fingerprint=SHA256-abc@web:2222/");
        assert_eq!(
            (t.user.as_deref(), t.host.as_str(), t.port),
            (Some("deploy"), "web", Some(2222))
        );
        let t = target("rdp://Administrator@win-01");
        assert_eq!((t.protocol, t.host.as_str()), (Protocol::Rdp, "win-01"));
        assert_eq!(t.to_string(), "rdp://Administrator@win-01");
        assert_eq!(target("vnc://desk:5901").port, Some(5901));
        // A display number, as VNC viewers take it.
        assert_eq!(target("vnc://desk:1").port, Some(5901));
        assert_eq!(
            target("sftp://me%40corp@files").user.as_deref(),
            Some("me@corp")
        );
        assert_eq!(target("mosh://m").protocol, Protocol::Mosh);
    }

    #[test]
    fn s3_urls() {
        let t =
            target("s3://AKIA123@minio.lan:9000/backups/2026?region=eu-west-1&path_style=false");
        assert_eq!(
            (
                t.protocol,
                t.user.as_deref(),
                t.host.as_str(),
                t.path.as_str(),
                t.s3.region.as_deref(),
                t.s3.path_style
            ),
            (
                Protocol::S3,
                Some("AKIA123"),
                "https://minio.lan:9000",
                "/backups/2026",
                Some("eu-west-1"),
                Some(false)
            )
        );
        assert_eq!(target(&t.to_string()), t);
        let t = target("s3+http://127.0.0.1:9000");
        assert_eq!(
            (t.host.as_str(), t.user.as_deref()),
            ("http://127.0.0.1:9000", None)
        );
        assert_eq!(t.to_string(), "s3+http://127.0.0.1:9000");
        let host = t.to_host();
        assert_eq!(
            (host.protocol, host.address.as_str()),
            (Protocol::S3, "http://127.0.0.1:9000")
        );
        for bad in [
            "s3://",
            "s3://h?x=1",
            "s3://h?path_style=maybe",
            "s3://h?region=a%20b",
        ] {
            assert!(matches!(parse(bad), Err(TargetError::BadS3(_))), "{bad}");
        }
    }

    #[test]
    fn container_urls() {
        let t = target("docker://web");
        assert_eq!(
            (t.protocol, t.host.as_str(), t.container.engine),
            (Protocol::Docker, "web", None)
        );
        let t = target("podman://postgres@db");
        assert_eq!(
            (t.user.as_deref(), t.container.engine),
            (Some("postgres"), Some(Engine::Podman))
        );
        assert_eq!(t.to_string(), "podman://postgres@db");
        let t = target(
            "kube://shop/api-7d9f?container=app&context=arn%3Aaws%3Aeks%3A1%3Acluster%2Fprod",
        );
        assert_eq!(
            (
                t.protocol,
                t.host.as_str(),
                t.container.namespace.as_deref(),
                t.container.pod_container.as_deref(),
                t.container.context.as_deref()
            ),
            (
                Protocol::Kube,
                "api-7d9f",
                Some("shop"),
                Some("app"),
                Some("arn:aws:eks:1:cluster/prod")
            )
        );
        assert_eq!(target(&t.to_string()), t);
        assert_eq!(t.to_host().container, t.container);
        for bad in [
            "docker://",
            "docker://-rm",
            "kube://a/b/c",
            "kube://p?x=1",
            "docker://a?b",
        ] {
            assert!(
                matches!(parse(bad), Err(TargetError::BadContainer(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn serial_urls() {
        let t = target("serial:///dev/ttyUSB0?baud=115200");
        assert_eq!(
            (t.protocol, t.host.as_str(), t.serial.baud),
            (Protocol::Serial, "/dev/ttyUSB0", Some(115_200))
        );
        let t = target("serial://COM3?baud=9600&parity=even&data_bits=7&stop_bits=2&flow=rtscts");
        assert_eq!(t.host, "COM3");
        assert_eq!(t.serial.parity, Some(Parity::Even));
        assert_eq!(t.serial.flow_control, Some(FlowControl::Hardware));
        assert_eq!(parse(&t.to_string()), Ok(t));
        assert!(matches!(
            parse("serial://COM3?baud=fast"),
            Err(TargetError::BadSerial(_))
        ));
        assert!(matches!(parse("serial://"), Err(TargetError::BadSerial(_))));
    }

    #[test]
    fn mistakes_are_explained() {
        assert_eq!(parse("  "), Err(TargetError::Empty));
        assert_eq!(parse("web:0"), Err(TargetError::BadPort("0".into())));
        assert_eq!(
            parse("web:99999"),
            Err(TargetError::BadPort("99999".into()))
        );
        assert_eq!(parse("web -p"), Err(TargetError::MissingValue("-p".into())));
        assert_eq!(
            parse("web -x"),
            Err(TargetError::UnknownOption("-x".into()))
        );
        assert_eq!(parse("a b"), Err(TargetError::Extra("b".into())));
        assert_eq!(
            parse("gopher://x"),
            Err(TargetError::UnknownScheme("gopher".into()))
        );
        assert_eq!(parse("'open"), Err(TargetError::Unbalanced));
        assert_eq!(parse("ssh"), Err(TargetError::MissingHost));
        // Nothing that ssh would read as an option.
        assert!(matches!(
            parse("-oProxyCommand=x@h"),
            Err(TargetError::UnknownOption(_))
        ));
        assert!(parse("'-oProxyCommand=x'@h").is_err());
        assert!(matches!(parse("ssh://-oX@h"), Err(TargetError::BadName(_))));
        assert!(matches!(parse("h -l -oX"), Err(TargetError::BadName(_))));
        assert!(matches!(parse("web -J -oX"), Err(TargetError::BadName(_))));
        assert!(matches!(parse("u@-h"), Err(TargetError::BadName(_))));
    }

    #[test]
    fn openssh_command_lines() {
        let args = SshArgs {
            user: Some("deploy".into()),
            host: "10.0.1.21".into(),
            port: Some(2222),
            jump: vec!["jump@bastion:2200".into()],
            identity_file: Some("~/.ssh/id deploy".into()),
            keepalive_secs: 30,
            compression: true,
            agent_forwarding: false,
            x11: X11Forwarding::Untrusted,
        };
        assert_eq!(
            args.to_args(),
            vec![
                "-p",
                "2222",
                "-i",
                "~/.ssh/id deploy",
                "-J",
                "jump@bastion:2200",
                "-o",
                "ServerAliveInterval=30",
                "-C",
                "-X",
                "deploy@10.0.1.21"
            ]
        );
        assert_eq!(
            args.to_command_line(),
            "ssh -p 2222 -i '~/.ssh/id deploy' -J jump@bastion:2200 -o ServerAliveInterval=30 -C -X deploy@10.0.1.21"
        );
        assert!(args.check().is_ok());
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
        let plain = target("web").ssh_args();
        assert_eq!(plain.to_args(), vec!["web"]);
        let bad = SshArgs {
            host: "-oProxyCommand=evil".into(),
            ..SshArgs::default()
        };
        assert!(bad.check().is_err());
    }

    #[test]
    fn saved_hosts_become_ssh_arguments() {
        let (file, _) = HostsFile::from_toml_str(
            r#"
            [[group]]
            id = "G"
            name = "G"
            [group.defaults]
            user = "deploy"
            jump = ["bastion"]
            [[host]]
            id = "B"
            name = "bastion"
            address = "b.example.com"
            [[host]]
            id = "W"
            name = "web"
            group = "G"
            address = "10.0.0.5"
            [host.ssh]
            agent_forwarding = true
            "#,
        )
        .unwrap();
        let args = file.ssh_args(file.host("W").unwrap());
        assert_eq!(
            args.to_args(),
            vec![
                "-J",
                "b.example.com",
                "-o",
                "ServerAliveInterval=30",
                "-A",
                "deploy@10.0.0.5"
            ]
        );
    }
}
