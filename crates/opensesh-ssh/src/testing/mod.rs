//! A tiny SSH server for tests, and for the app's smoke test (which must not reach the network):
//! a user [`USER`] with the password [`PASSWORD`] and/or keys, an optional one-time code after a
//! key ([`CODE`]), `direct-tcpip` (jump hosts, local and dynamic forwards) and `tcpip-forward`
//! (remote forwards) when [`Rules::jump`] allows them, and a shell that prints `test$ ` and
//! echoes what it gets ("exit" ends it with status 3, "drop" drops the connection, "cd /path"
//! reports the folder with OSC 7). It also answers the OS detection command, "install my key"
//! (into [`Rules::authorized_keys`]), and the remote monitor and host info commands (the readings
//! of a made-up Debian server, one a second, its counters growing), `mosh-server new` with
//! [`Rules::mosh`], and serves SFTP over a folder when [`Rules::sftp_root`] names one. Nothing here
//! runs unless a test or the smoke test starts it.

mod sftp;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use russh::keys::{PrivateKey, PublicKey};
use russh::server::{Auth, Handler, Msg, Session};
use russh::{Channel, ChannelId, MethodKind, MethodSet};
use tokio::net::TcpListener;

use crate::{copy_id, monitor, mosh, osdetect};

/// What `mosh-server new` prints here (with [`Rules::mosh`]): a session on UDP port 60001.
pub const MOSH_OUTPUT: &str = "\r\nMOSH CONNECT 60001 T3BlblNlc2ggdGVzdCBrZQ\r\n\r\n\
    mosh-server (mosh 1.4.0) [build mosh 1.4.0]\r\n\
    [mosh-server detached, pid = 4242]\r\n";

/// The user name the server knows.
pub const USER: &str = "tester";
/// The password it accepts.
pub const PASSWORD: &str = "right password";
/// The one-time code it asks for after a key when [`Rules::mfa`] is set.
pub const CODE: &str = "123456";

/// What a test server accepts.
#[derive(Debug, Clone, Default)]
pub struct Rules {
    /// Accept [`PASSWORD`] for [`USER`].
    pub password: bool,
    /// Accept these public keys for [`USER`].
    pub keys: Vec<PublicKey>,
    /// After a key, ask for [`CODE`] (keyboard-interactive).
    pub mfa: bool,
    /// Allow `direct-tcpip` (a jump host).
    pub jump: bool,
    /// Drop the connection when "drop" is typed.
    pub droppable: bool,
    /// The host key (a new random Ed25519 key when not given).
    pub host_key: Option<PrivateKey>,
    /// The `authorized_keys` lines "install my key" added (shared with the test).
    pub authorized_keys: Arc<Mutex<Vec<String>>>,
    /// Serve SFTP with this folder as `/` (none: no SFTP subsystem).
    pub sftp_root: Option<PathBuf>,
    /// Answer the remote monitor and host info commands (else they fail, as on a server without
    /// `sh`).
    pub monitor: bool,
    /// Answer `mosh-server new` with a session (else it isn't installed).
    pub mosh: bool,
}

struct Server {
    rules: Rules,
    key_accepted: bool,
    code_asked: bool,
    typed: Vec<u8>,
    /// Session channels (the tiny shell only answers on these, not on jump tunnels).
    sessions: Vec<ChannelId>,
    /// Channels running the "install my key" script.
    installs: Vec<ChannelId>,
    /// Session channels that may still ask for the SFTP subsystem.
    channels: HashMap<ChannelId, Channel<Msg>>,
    /// Remote forwards (`tcpip-forward`): the listener tasks, by port.
    forwards: HashMap<u32, tokio::task::JoinHandle<()>>,
    /// Channels running the remote monitor: the tasks printing their readings.
    monitors: HashMap<ChannelId, tokio::task::JoinHandle<()>>,
}

impl Drop for Server {
    fn drop(&mut self) {
        // The connection is gone: so are its remote forwards and monitors.
        for task in self.forwards.values().chain(self.monitors.values()) {
            task.abort();
        }
    }
}

fn methods(rules: &Rules, key_accepted: bool) -> MethodSet {
    let mut kinds = Vec::new();
    if rules.mfa && key_accepted {
        kinds.push(MethodKind::KeyboardInteractive);
    } else {
        if !rules.keys.is_empty() {
            kinds.push(MethodKind::PublicKey);
        }
        if rules.password {
            kinds.push(MethodKind::Password);
        }
    }
    MethodSet::from(kinds.as_slice())
}

impl Handler for Server {
    type Error = russh::Error;

    async fn auth_none(&mut self, _user: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Reject {
            proceed_with_methods: Some(methods(&self.rules, false)),
            partial_success: false,
        })
    }

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        if self.rules.password && user == USER && password == PASSWORD {
            return Ok(Auth::Accept);
        }
        Ok(Auth::Reject {
            proceed_with_methods: Some(methods(&self.rules, false)),
            partial_success: false,
        })
    }

    async fn auth_publickey_offered(
        &mut self,
        _user: &str,
        key: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        if self
            .rules
            .keys
            .iter()
            .any(|allowed| allowed.key_data() == key.key_data())
        {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::reject())
        }
    }

    async fn auth_publickey(&mut self, user: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        let allowed = user == USER
            && self
                .rules
                .keys
                .iter()
                .any(|allowed| allowed.key_data() == key.key_data());
        if !allowed {
            return Ok(Auth::Reject {
                proceed_with_methods: Some(methods(&self.rules, false)),
                partial_success: false,
            });
        }
        if self.rules.mfa {
            self.key_accepted = true;
            return Ok(Auth::Reject {
                proceed_with_methods: Some(methods(&self.rules, true)),
                partial_success: true,
            });
        }
        Ok(Auth::Accept)
    }

    async fn auth_keyboard_interactive<'a>(
        &'a mut self,
        _user: &str,
        _submethods: &str,
        response: Option<russh::server::Response<'a>>,
    ) -> Result<Auth, Self::Error> {
        if !(self.rules.mfa && self.key_accepted) {
            return Ok(Auth::reject());
        }
        if !self.code_asked {
            self.code_asked = true;
            return Ok(Auth::Partial {
                name: "Two-factor".into(),
                instructions: "Enter the code from your app.".into(),
                prompts: vec![("Verification code: ".into(), false)].into(),
            });
        }
        let answer = response.and_then(|mut response| response.next());
        if answer.as_deref() == Some(CODE.as_bytes()) {
            Ok(Auth::Accept)
        } else {
            self.code_asked = false;
            Ok(Auth::Reject {
                proceed_with_methods: Some(methods(&self.rules, true)),
                partial_success: false,
            })
        }
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: russh::server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.sessions.push(channel.id());
        if self.rules.sftp_root.is_some() {
            self.channels.insert(channel.id(), channel);
        }
        reply.accept().await;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        match (
            name,
            self.rules.sftp_root.clone(),
            self.channels.remove(&channel),
        ) {
            ("sftp", Some(root), Some(stream)) => {
                // Its bytes are SFTP's from now on, not the tiny shell's.
                self.sessions.retain(|id| *id != channel);
                session.channel_success(channel)?;
                russh_sftp::server::run(stream.into_stream(), sftp::Server::new(root)).await;
            }
            _ => session.channel_failure(channel)?,
        }
        Ok(())
    }

    /// Remote forwarding, where jumps are allowed: listens where asked (port 0: any) and opens a
    /// `forwarded-tcpip` channel back for each connection.
    async fn tcpip_forward(
        &mut self,
        address: &str,
        port: &mut u32,
        session: &mut Session,
    ) -> Result<bool, Self::Error> {
        let Ok(wanted) = u16::try_from(*port) else {
            return Ok(false);
        };
        if !self.rules.jump {
            return Ok(false);
        }
        let host = if address.is_empty() {
            "0.0.0.0"
        } else {
            address
        };
        let Ok(listener) = TcpListener::bind((host, wanted)).await else {
            return Ok(false);
        };
        let bound = listener
            .local_addr()
            .map_or(u32::from(wanted), |local| u32::from(local.port()));
        *port = bound;
        let handle = session.handle();
        let address = address.to_owned();
        let task = tokio::spawn(async move {
            while let Ok((mut socket, peer)) = listener.accept().await {
                let channel = handle
                    .channel_open_forwarded_tcpip(
                        address.clone(),
                        bound,
                        peer.ip().to_string(),
                        u32::from(peer.port()),
                    )
                    .await;
                if let Ok(channel) = channel {
                    tokio::spawn(async move {
                        let mut stream = channel.into_stream();
                        let _ = tokio::io::copy_bidirectional(&mut stream, &mut socket).await;
                    });
                }
            }
        });
        self.forwards.insert(bound, task);
        Ok(true)
    }

    async fn cancel_tcpip_forward(
        &mut self,
        _address: &str,
        port: u32,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        Ok(self.forwards.remove(&port).is_some_and(|task| {
            task.abort();
            true
        }))
    }

    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<Msg>,
        host: &str,
        port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: russh::server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if !self.rules.jump {
            reply
                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                .await;
            return Ok(());
        }
        let target = format!("{host}:{port}");
        reply.accept().await;
        tokio::spawn(async move {
            if let Ok(mut tcp) = tokio::net::TcpStream::connect(target).await {
                let mut stream = channel.into_stream();
                let _ = tokio::io::copy_bidirectional(&mut stream, &mut tcp).await;
            }
        });
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.remove(&channel);
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.remove(&channel);
        session.channel_success(channel)?;
        session.data(channel, b"test$ ".to_vec())?;
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        command: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.remove(&channel);
        session.channel_success(channel)?;
        if command == osdetect::COMMAND.as_bytes() {
            session.data(channel, b"ID=debian\n__uname__\nLinux\n".to_vec())?;
            session.exit_status_request(channel, 0)?;
            session.eof(channel)?;
            session.close(channel)?;
        } else if command == copy_id::SCRIPT.as_bytes() {
            self.installs.push(channel);
        } else if self.rules.monitor && is_monitor_command(command) {
            // A reading a second, whatever the interval asked for: tests don't wait.
            let handle = session.handle();
            let task = tokio::spawn(async move {
                for tick in 0_u64.. {
                    if handle.data(channel, sample_reading(tick)).await.is_err() {
                        return;
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            });
            self.monitors.insert(channel, task);
        } else if self.rules.mosh && command == mosh::SERVER_COMMAND.as_bytes() {
            session.data(channel, MOSH_OUTPUT.as_bytes().to_vec())?;
            session.exit_status_request(channel, 0)?;
            session.eof(channel)?;
            session.close(channel)?;
        } else if self.rules.monitor && command == monitor::info_command().as_bytes() {
            session.data(channel, format!("{SAMPLE_INFO}{}", sample_reading(0)))?;
            session.exit_status_request(channel, 0)?;
            session.eof(channel)?;
            session.close(channel)?;
        } else {
            // No other command exists here, as in a shell without it.
            session.extended_data(channel, 1, b"sh: command not found\n".to_vec())?;
            session.exit_status_request(channel, 127)?;
            session.eof(channel)?;
            session.close(channel)?;
        }
        Ok(())
    }

    async fn channel_close(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // The client stopped watching: so does the monitor's loop, as `sh` would.
        if let Some(task) = self.monitors.remove(&channel) {
            task.abort();
        }
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        // The key line of "install my key": added unless the same key is there.
        if self.installs.contains(&channel) {
            let text = String::from_utf8_lossy(data);
            let line = text.lines().next().unwrap_or_default().trim().to_owned();
            let key: Vec<&str> = line.split_whitespace().take(2).collect();
            let answer = {
                let mut keys = self
                    .rules
                    .authorized_keys
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                if keys
                    .iter()
                    .any(|known| known.split_whitespace().take(2).collect::<Vec<_>>() == key)
                {
                    "__present__\n"
                } else {
                    keys.push(line);
                    "__added__\n"
                }
            };
            session.data(channel, answer.as_bytes().to_vec())?;
            session.exit_status_request(channel, 0)?;
            session.eof(channel)?;
            session.close(channel)?;
            return Ok(());
        }
        // A tiny shell: echo, "exit" ends it, "drop" drops the connection.
        if !self.sessions.contains(&channel) {
            return Ok(());
        }
        session.data(channel, data.to_vec())?;
        self.typed.extend_from_slice(data);
        if self.typed.ends_with(b"exit\r") {
            session.exit_status_request(channel, 3)?;
            session.eof(channel)?;
            session.close(channel)?;
        }
        if self.rules.droppable && self.typed.ends_with(b"drop\r") {
            self.typed.clear();
            return Err(russh::Error::Disconnect);
        }
        // "cd <absolute path>" says where the shell went (OSC 7), as an integrated shell does.
        if self.typed.ends_with(b"\r") {
            let line = self.typed[..self.typed.len() - 1]
                .rsplit(|byte| *byte == b'\r')
                .next()
                .unwrap_or_default()
                .to_vec();
            if let Some(path) = line.strip_prefix(b"cd /") {
                let mut reply = b"\r\n\x1b]7;file://test/".to_vec();
                reply.extend_from_slice(path);
                reply.extend_from_slice(b"\x1b\\test$ ");
                session.data(channel, reply)?;
            }
        }
        Ok(())
    }
}

/// Whether `command` is the remote monitor's loop, at any interval.
fn is_monitor_command(command: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(command) else {
        return false;
    };
    text.strip_prefix("sh -c 'n=")
        .and_then(|rest| rest.split_once(';'))
        .and_then(|(seconds, _)| seconds.parse::<u64>().ok())
        .is_some_and(|seconds| text == monitor::command(Duration::from_secs(seconds)))
}

/// What the host info command prints about the made-up server, before its reading.
const SAMPLE_INFO: &str = "@release\nPRETTY_NAME=\"Debian GNU/Linux 13 (trixie)\"\nID=debian\n\
                           @uname\nLinux 6.12.48+deb13-amd64 x86_64\n@hostname\ntest-server\n\
                           @ncpu\n4\n@addr\n1: lo    inet 127.0.0.1/8 scope host lo\n\
                           2: eth0    inet 10.0.0.5/24 brd 10.0.0.255 scope global eth0\n\
                           2: eth0    inet6 fe80::5054:ff:fe12:3456/64 scope link\n";

/// Reading `tick` of the made-up server: 12 % CPU, 1.25 MB/s in and 80 kB/s out a second, 5 of
/// 8 GB free, `/` at 40 %, up 12 days.
fn sample_reading(tick: u64) -> String {
    let (busy, idle) = (1000 + tick * 12, 9000 + tick * 88);
    let (received, sent) = (5_000_000 + tick * 1_250_000, 700_000 + tick * 80_000);
    let uptime = 1_036_800 + tick;
    format!(
        "@os Linux\n@cpu\ncpu  {busy} 0 0 {idle} 0 0 0 0 0 0\n\
         @meminfo\nMemTotal:        7812500 kB\nMemAvailable:    4882812 kB\n\
         SwapTotal:       1953125 kB\nSwapFree:        1953125 kB\n\
         @netdev\nInter-|   Receive |  Transmit\n face |bytes packets|bytes packets\n\
         lo: 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0\n\
         eth0: {received} 0 0 0 0 0 0 0 {sent} 0 0 0 0 0 0 0\n\
         @route\nIface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\tMTU\tWindow\tIRTT\n\
         eth0\t00000000\t0100000A\t0003\t0\t0\t0\t00000000\t0\t0\t0\n\
         @uptime\n{uptime}.00 0.00\n@loadavg\n0.12 0.25 0.50 1/100 1000\n\
         @df\nFilesystem     1024-blocks      Used Available Capacity Mounted on\n\
         /dev/vda1         41152736  16461094  24691642      41% /\n\
         tmpfs                 1000         0      1000       0% /run\n\
         @who\n{USER}   pts/0        2026-09-28 09:00 (127.0.0.1)\n@end\n"
    )
}

/// Starts a server with `rules` on a free port of 127.0.0.1 and returns the port. It runs until
/// the runtime ends.
///
/// # Errors
///
/// When no port can be bound, or no host key can be made.
pub async fn serve(rules: Rules) -> std::io::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let host_key = match rules.host_key.clone() {
        Some(key) => key,
        None => random_host_key()?,
    };
    let config = Arc::new(russh::server::Config {
        keys: vec![host_key],
        auth_rejection_time: Duration::from_millis(1),
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..russh::server::Config::default()
    });
    tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                return;
            };
            let handler = Server {
                rules: rules.clone(),
                key_accepted: false,
                code_asked: false,
                typed: Vec::new(),
                sessions: Vec::new(),
                installs: Vec::new(),
                monitors: HashMap::new(),
                channels: HashMap::new(),
                forwards: HashMap::new(),
            };
            let config = Arc::clone(&config);
            tokio::spawn(async move {
                if let Ok(session) = russh::server::run_stream(config, socket, handler).await {
                    let _ = session.await;
                }
            });
        }
    });
    Ok(port)
}

/// A random Ed25519 host key.
fn random_host_key() -> std::io::Result<PrivateKey> {
    let seed = opensesh_vault::crypto::random_array::<32>()
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let keypair = russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&seed);
    PrivateKey::new(
        russh::keys::ssh_key::private::KeypairData::Ed25519(keypair),
        "opensesh test server",
    )
    .map_err(|error| std::io::Error::other(error.to_string()))
}
