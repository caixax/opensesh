//! The terminal kinds that aren't a local shell or the built-in SSH client (Sprint 12): telnet,
//! serial ports and mosh. A pane's saved host, or its quick-connect target, says which; this
//! module turns it into what the backend needs, and starts the session.
//!
//! A test run (a smoke test or screenshots) never reaches the network or a device: telnet goes
//! to its in-process test server.

use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;

use opensesh_core::hosts::{Host, Protocol, target};
use opensesh_proto_misc::telnet::{self, TelnetSpec};

use crate::bridge::app_info::is_test_run;
use crate::services;
use crate::terminal::registry::{self, LocalOptions, SessionEntry, StartError};

/// How long connecting may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// The port of the smoke test's telnet server (0 until it starts).
static TELNET_TEST_SERVER: AtomicU16 = AtomicU16::new(0);

/// Sends every telnet connection of a test run to its server on `port`.
pub fn set_telnet_test_server(port: u16) {
    TELNET_TEST_SERVER.store(port, Ordering::Relaxed);
}

/// What a pane of one of these kinds starts.
#[derive(Debug, Clone)]
pub enum Start {
    /// A telnet connection.
    Telnet(TelnetSpec),
}

/// What the pane of saved host `host_id`, or of quick-connect `target_text`, starts, when it is
/// one of these kinds; `None` for the others. `term` is the profile's `TERM`.
#[must_use]
pub fn for_pane(host_id: &str, target_text: &str, term: &str) -> Option<Result<Start, String>> {
    let library = crate::hosts::current();
    let host = if host_id.is_empty() {
        let parsed = target::parse(target_text).ok()?;
        // An unsaved host: no group, so the app's defaults apply.
        Host {
            name: parsed.host.clone(),
            protocol: parsed.protocol,
            address: parsed.host,
            port: parsed.port,
            user: parsed.user,
            serial: parsed.serial,
            ..Host::default()
        }
    } else {
        library.file.host(host_id)?.clone()
    };
    match host.protocol {
        Protocol::Telnet => Some(telnet_for(&host, &library.file, term)),
        _ => None,
    }
}

fn telnet_for(
    host: &Host,
    file: &opensesh_core::hosts::HostsFile,
    term: &str,
) -> Result<Start, String> {
    let resolved = file.resolve(host);
    let mut spec = TelnetSpec {
        host: host.address.trim().to_owned(),
        port: resolved.port().unwrap_or(23),
        term: term.to_owned(),
        connect_timeout: CONNECT_TIMEOUT,
        log: services::get().and_then(|services| {
            crate::ssh::session_log(
                services.paths.data_dir(),
                &host.name,
                resolved.session_log(),
            )
        }),
    };
    if spec.host.is_empty() {
        return Err("the host has no address".to_owned());
    }
    if is_test_run() {
        let port = TELNET_TEST_SERVER.load(Ordering::Relaxed);
        if port == 0 {
            return Err("a test run connects only to its own telnet server".to_owned());
        }
        "127.0.0.1".clone_into(&mut spec.host);
        spec.port = port;
        spec.log = None;
    }
    Ok(Start::Telnet(spec))
}

/// Starts the session of pane `id`.
///
/// # Errors
///
/// [`StartError`] when the backend or the engine thread can't start.
pub fn open(id: i32, start: Start, options: LocalOptions) -> Result<Arc<SessionEntry>, StartError> {
    match start {
        Start::Telnet(spec) => {
            registry::open_backend(id, options, |size| Ok(telnet::start(spec, size)?))
        }
    }
}
