//! Port forwarding (PLAN Sprint 9) over the connections of [`crate::connect`]:
//!
//! - **local** (`-L`): a listener here; each connection opens a `direct-tcpip` channel, and the
//!   server connects to the destination;
//! - **remote** (`-R`): the server listens (`tcpip-forward`); each connection comes back as a
//!   `forwarded-tcpip` channel, and this side connects to the destination;
//! - **dynamic** (`-D`): a SOCKS5 server here ([`socks`]); each connection opens a
//!   `direct-tcpip` channel to where the program asked.
//!
//! A forward runs on a connection that can change: [`start`] takes a `watch` of it. The caller
//! replaces a lost connection (a terminal session that reconnects, or [`keep_connected`]) and
//! the forward carries on over the new one: listeners here stay bound, and a connection that
//! arrives meanwhile waits for the new one up to 10 s; remote forwards are asked for again.
//! [`Traffic`] counts bytes each way and connections.

pub mod socks;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use russh::client::Msg;
use russh::{Channel, ChannelOpenFailure};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

use crate::SshError;
use crate::connect::Connection;
use socks::Reply;

/// How long a new connection waits for the SSH connection while it is being replaced.
const WAIT_FOR_CONNECTION: Duration = Duration::from_secs(10);
/// How long a SOCKS client has to say where it goes.
const HANDSHAKE: Duration = Duration::from_secs(10);
/// Waits between reconnection attempts; the last one repeats.
const BACKOFF: [u64; 6] = [1, 2, 4, 8, 16, 30];

/// A host (a name or an address) and a port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// The host.
    pub host: String,
    /// The port (0: picked when listening).
    pub port: u16,
}

impl Endpoint {
    /// `host` and `port`.
    #[must_use]
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }
}

impl std::fmt::Display for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.host.contains(':') {
            write!(f, "[{}]:{}", self.host, self.port)
        } else {
            write!(f, "{}:{}", self.host, self.port)
        }
    }
}

/// What to forward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Forward {
    /// Listen at `bind` here; connections go to `to` from the server.
    Local {
        /// Where to listen here.
        bind: Endpoint,
        /// The destination, as the server reaches it.
        to: Endpoint,
    },
    /// The server listens at `bind`; connections go to `to` from here.
    Remote {
        /// Where the server listens.
        bind: Endpoint,
        /// The destination, as this computer reaches it.
        to: Endpoint,
    },
    /// A SOCKS5 proxy at `bind` here; connections go where the program asks, from the server.
    Dynamic {
        /// Where to listen here.
        bind: Endpoint,
    },
}

/// The traffic of a forward: bytes this side sent through the tunnel and received from it, and
/// its connections.
#[derive(Debug, Default)]
pub struct Traffic {
    sent: AtomicU64,
    received: AtomicU64,
    open: AtomicU64,
    total: AtomicU64,
}

/// A copy of [`Traffic`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    /// Bytes sent through the tunnel.
    pub sent: u64,
    /// Bytes that came back.
    pub received: u64,
    /// Connections open now.
    pub open: u64,
    /// Connections so far.
    pub total: u64,
}

impl Traffic {
    /// The counts now.
    #[must_use]
    pub fn counts(&self) -> Counts {
        Counts {
            sent: self.sent.load(Ordering::Relaxed),
            received: self.received.load(Ordering::Relaxed),
            open: self.open.load(Ordering::Relaxed),
            total: self.total.load(Ordering::Relaxed),
        }
    }
}

/// What a forward says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Report {
    /// Listening at this port: here, or on the server for a remote forward.
    Listening(u16),
    /// A remote forward between connections.
    Waiting,
    /// It can't run: the address is in use, the server refused to listen...
    Failed(String),
}

/// Where a forward reports.
pub type ReportSink = Arc<dyn Fn(Report) + Send + Sync>;

/// The connection a forward runs on; `None` while there is none.
pub type Connections = watch::Receiver<Option<Arc<Connection>>>;

/// A running forward. Stopping it (or dropping it) closes its listener and its connections, and
/// cancels a remote forward on the server.
#[derive(Debug)]
pub struct Running {
    stop: watch::Sender<bool>,
    traffic: Arc<Traffic>,
}

impl Running {
    /// Its traffic.
    #[must_use]
    pub fn traffic(&self) -> Arc<Traffic> {
        Arc::clone(&self.traffic)
    }

    /// Stops it.
    pub fn stop(&self) {
        self.stop.send_replace(true);
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop.send_replace(true);
    }
}

/// Starts `forward` on `runtime`, over the connections `connections` gives.
#[must_use]
pub fn start(
    runtime: &tokio::runtime::Handle,
    forward: Forward,
    connections: Connections,
    report: ReportSink,
) -> Running {
    start_counting(
        runtime,
        forward,
        connections,
        Arc::new(Traffic::default()),
        report,
    )
}

/// [`start`], counting into `traffic` (which outlives this run: a tunnel that starts again keeps
/// its counts).
#[must_use]
pub fn start_counting(
    runtime: &tokio::runtime::Handle,
    forward: Forward,
    connections: Connections,
    traffic: Arc<Traffic>,
    report: ReportSink,
) -> Running {
    let (stop, stopped) = watch::channel(false);
    let shared = Arc::clone(&traffic);
    match forward {
        Forward::Local { bind, to } => {
            runtime.spawn(listen(bind, Some(to), connections, shared, report, stopped));
        }
        Forward::Dynamic { bind } => {
            runtime.spawn(listen(bind, None, connections, shared, report, stopped));
        }
        Forward::Remote { bind, to } => {
            runtime.spawn(remote(bind, to, connections, shared, report, stopped));
        }
    }
    Running { stop, traffic }
}

/// Where a remote forward's connections go, by the port the server listens on.
#[derive(Clone, Default)]
pub(crate) struct Routes(Arc<Mutex<HashMap<u32, Route>>>);

#[derive(Clone)]
pub(crate) struct Route {
    to: Endpoint,
    traffic: Arc<Traffic>,
    stopped: watch::Receiver<bool>,
}

impl Routes {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u32, Route>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn get(&self, port: u32) -> Option<Route> {
        self.lock().get(&port).cloned()
    }

    fn insert(&self, port: u32, route: Route) {
        self.lock().insert(port, route);
    }

    fn remove(&self, port: u32) {
        self.lock().remove(&port);
    }
}

/// Serves a `forwarded-tcpip` channel the server opened for a remote forward.
pub(crate) async fn serve_forwarded(channel: Channel<Msg>, route: Route) {
    match TcpStream::connect((route.to.host.as_str(), route.to.port)).await {
        Ok(socket) => {
            let _ = socket.set_nodelay(true);
            pump(socket, channel.into_stream(), route.traffic, route.stopped).await;
        }
        Err(error) => {
            tracing::debug!(to = %route.to, "a remote forward's destination refused: {error}");
            let _ = channel.close().await;
        }
    }
}

/// A local or dynamic forward (`to` is `None` for dynamic): the listener and its connections.
async fn listen(
    bind: Endpoint,
    to: Option<Endpoint>,
    connections: Connections,
    traffic: Arc<Traffic>,
    report: ReportSink,
    mut stopped: watch::Receiver<bool>,
) {
    let listener = match TcpListener::bind((bind.host.as_str(), bind.port)).await {
        Ok(listener) => listener,
        Err(error) => {
            report(Report::Failed(format!("can't listen on {bind}: {error}")));
            return;
        }
    };
    let port = listener
        .local_addr()
        .map_or(bind.port, |address| address.port());
    report(Report::Listening(port));
    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((socket, peer)) => {
                    let _ = socket.set_nodelay(true);
                    tokio::spawn(serve_local(
                        socket,
                        peer,
                        to.clone(),
                        connections.clone(),
                        Arc::clone(&traffic),
                        stopped.clone(),
                    ));
                }
                Err(error) => {
                    // Out of file descriptors and the like: don't spin.
                    tracing::debug!("accepting a forwarded connection failed: {error}");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            },
            _ = stopped.changed() => return,
        }
    }
}

/// The connection to use now; waits for one while it is being replaced.
async fn current(connections: &mut Connections) -> Option<Arc<Connection>> {
    let wait = async {
        connections
            .wait_for(|connection| {
                connection
                    .as_ref()
                    .is_some_and(|connection| !connection.is_closed())
            })
            .await
            .ok()
            .and_then(|connection| connection.clone())
    };
    tokio::time::timeout(WAIT_FOR_CONNECTION, wait)
        .await
        .ok()
        .flatten()
}

/// One connection of a local forward (to `to`) or a dynamic one (where SOCKS says).
async fn serve_local(
    mut socket: TcpStream,
    peer: SocketAddr,
    to: Option<Endpoint>,
    mut connections: Connections,
    traffic: Arc<Traffic>,
    stopped: watch::Receiver<bool>,
) {
    let dynamic = to.is_none();
    let target = match to {
        Some(to) => to,
        None => match tokio::time::timeout(HANDSHAKE, socks::accept(&mut socket)).await {
            Ok(Ok((host, port))) => Endpoint::new(host, port),
            Ok(Err(error)) => {
                tracing::debug!(%peer, "a SOCKS client was turned away: {error}");
                return;
            }
            Err(_) => return,
        },
    };
    let Some(connection) = current(&mut connections).await else {
        if dynamic {
            let _ = socks::reply(&mut socket, Reply::GeneralFailure).await;
        }
        return;
    };
    let channel = match connection.target() {
        Ok(handle) => {
            handle
                .channel_open_direct_tcpip(
                    target.host.clone(),
                    u32::from(target.port),
                    peer.ip().to_string(),
                    u32::from(peer.port()),
                )
                .await
        }
        Err(_) => Err(russh::Error::Disconnect),
    };
    match channel {
        Ok(channel) => {
            if dynamic && socks::reply(&mut socket, Reply::Succeeded).await.is_err() {
                return;
            }
            pump(socket, channel.into_stream(), traffic, stopped).await;
        }
        Err(error) => {
            tracing::debug!(to = %target, "the server didn't open a forwarded channel: {error}");
            if dynamic {
                let reply = match error {
                    russh::Error::ChannelOpenFailure(
                        ChannelOpenFailure::AdministrativelyProhibited,
                    ) => Reply::NotAllowed,
                    russh::Error::ChannelOpenFailure(ChannelOpenFailure::ConnectFailed) => {
                        Reply::HostUnreachable
                    }
                    _ => Reply::GeneralFailure,
                };
                let _ = socks::reply(&mut socket, reply).await;
            }
        }
    }
}

/// A remote forward: asked for on each connection, cancelled when it is replaced or stopped.
async fn remote(
    bind: Endpoint,
    to: Endpoint,
    mut connections: Connections,
    traffic: Arc<Traffic>,
    report: ReportSink,
    mut stopped: watch::Receiver<bool>,
) {
    loop {
        let connection = connections
            .borrow_and_update()
            .clone()
            .filter(|connection| !connection.is_closed());
        let mut registered = None;
        match connection {
            Some(connection) => match register(&connection, &bind, &to, &traffic, &stopped).await {
                Ok(port) => {
                    report(Report::Listening(port));
                    registered = Some((connection, port));
                }
                Err(message) => report(Report::Failed(message)),
            },
            None => report(Report::Waiting),
        }
        let done = tokio::select! {
            changed = connections.changed() => changed.is_err(),
            _ = stopped.changed() => true,
        };
        if let Some((connection, port)) = registered {
            unregister(&connection, &bind, port).await;
        }
        if done {
            return;
        }
    }
}

async fn register(
    connection: &Connection,
    bind: &Endpoint,
    to: &Endpoint,
    traffic: &Arc<Traffic>,
    stopped: &watch::Receiver<bool>,
) -> Result<u16, String> {
    let handle = connection.target().map_err(|error| error.to_string())?;
    let route = Route {
        to: to.clone(),
        traffic: Arc::clone(traffic),
        stopped: stopped.clone(),
    };
    // A fixed port is routed before asking, so no early connection is refused.
    let routes = connection.routes();
    if bind.port != 0 {
        routes.insert(u32::from(bind.port), route.clone());
    }
    match handle
        .tcpip_forward(bind.host.clone(), u32::from(bind.port))
        .await
    {
        Ok(port) => {
            // The server says which port only when it picked one.
            let port = if port == 0 {
                u32::from(bind.port)
            } else {
                port
            };
            routes.insert(port, route);
            u16::try_from(port).map_err(|_| format!("the server picked port {port}"))
        }
        Err(error) => {
            routes.remove(u32::from(bind.port));
            Err(match error {
                russh::Error::RequestDenied => format!("the server refused to listen on {bind}"),
                error => format!("asking the server to listen on {bind} failed: {error}"),
            })
        }
    }
}

async fn unregister(connection: &Connection, bind: &Endpoint, port: u16) {
    connection.routes().remove(u32::from(port));
    if let Ok(handle) = connection.target() {
        // A dead connection doesn't answer: don't wait for it.
        let cancel = handle.cancel_tcpip_forward(bind.host.clone(), u32::from(port));
        let _ = tokio::time::timeout(Duration::from_secs(5), cancel).await;
    }
}

/// Copies both ways between a socket here and a channel, counting, until both ends are done
/// (or one fails, or the forward stops).
async fn pump<R>(
    local: TcpStream,
    remote: R,
    traffic: Arc<Traffic>,
    mut stopped: watch::Receiver<bool>,
) where
    R: AsyncRead + AsyncWrite + Unpin + Send,
{
    if *stopped.borrow() {
        return;
    }
    traffic.open.fetch_add(1, Ordering::Relaxed);
    traffic.total.fetch_add(1, Ordering::Relaxed);
    let (mut local_read, mut local_write) = local.into_split();
    let (mut remote_read, mut remote_write) = tokio::io::split(remote);
    let up = copy_counting(&mut local_read, &mut remote_write, &traffic.sent);
    let down = copy_counting(&mut remote_read, &mut local_write, &traffic.received);
    tokio::pin!(up, down);
    // One way ending cleanly (EOF) waits for the other; an error ends both.
    tokio::select! {
        result = &mut up => if result.is_ok() {
            tokio::select! {
                _ = &mut down => {}
                _ = stopped.changed() => {}
            }
        },
        result = &mut down => if result.is_ok() {
            tokio::select! {
                _ = &mut up => {}
                _ = stopped.changed() => {}
            }
        },
        _ = stopped.changed() => {}
    }
    traffic.open.fetch_sub(1, Ordering::Relaxed);
}

async fn copy_counting<R, W>(
    reader: &mut R,
    writer: &mut W,
    counter: &AtomicU64,
) -> std::io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buffer = vec![0_u8; 32 * 1024];
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            writer.shutdown().await?;
            return Ok(());
        }
        writer.write_all(&buffer[..read]).await?;
        counter.fetch_add(read as u64, Ordering::Relaxed);
    }
}

/// How an independent tunnel's own connection is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    /// Connecting (or answering its questions).
    Connecting,
    /// Up.
    Connected,
    /// Lost or failed; trying again in this many seconds.
    Retrying {
        /// Seconds until the next attempt.
        in_secs: u64,
        /// Why the last one ended.
        reason: String,
    },
    /// Given up: an error that needs the user (a refused key, a wrong password, a cancelled
    /// question), or the connection was lost and reconnecting is off.
    Ended {
        /// [`SshError::code`], or `lost`.
        code: &'static str,
        /// Why.
        reason: String,
    },
}

/// Makes a new connection.
pub type Connector = Arc<
    dyn Fn() -> Pin<Box<dyn Future<Output = Result<Connection, SshError>> + Send>> + Send + Sync,
>;

/// Where [`keep_connected`] says how the connection is doing.
pub type LinkSink = Arc<dyn Fn(Link) + Send + Sync>;

/// An independent tunnel's own connection, kept up by [`keep_connected`]. Dropping it (or
/// [`Kept::stop`]) closes the connection.
#[derive(Debug)]
pub struct Kept {
    /// The connection now, for [`start`].
    pub connections: Connections,
    stop: watch::Sender<bool>,
}

impl Kept {
    /// Closes the connection and stops reconnecting.
    pub fn stop(&self) {
        self.stop.send_replace(true);
    }
}

impl Drop for Kept {
    fn drop(&mut self) {
        self.stop.send_replace(true);
    }
}

/// Whether an error needs the user rather than another attempt.
fn needs_the_user(error: &SshError) -> bool {
    matches!(
        error,
        SshError::HostKey { .. }
            | SshError::Auth { .. }
            | SshError::Cancelled
            | SshError::SecretsLocked
    )
}

/// Connects with `connector` and keeps the connection up: a lost one is replaced after 1, 2,
/// 4, 8, 16, then every 30 s when `reconnect` is on. Errors that need the user end it.
#[must_use]
pub fn keep_connected(
    runtime: &tokio::runtime::Handle,
    connector: Connector,
    reconnect: bool,
    link: LinkSink,
) -> Kept {
    let (sender, connections) = watch::channel(None::<Arc<Connection>>);
    let (stop, mut stopped) = watch::channel(false);
    runtime.spawn(async move {
        let mut attempt = 0_usize;
        loop {
            link(Link::Connecting);
            let result = tokio::select! {
                result = connector() => result,
                _ = stopped.changed() => return,
            };
            let reason = match result {
                Ok(connection) => {
                    attempt = 0;
                    let connection = Arc::new(connection);
                    sender.send_replace(Some(Arc::clone(&connection)));
                    link(Link::Connected);
                    loop {
                        tokio::select! {
                            () = tokio::time::sleep(Duration::from_secs(1)) => {
                                if connection.is_closed() {
                                    break;
                                }
                            }
                            _ = stopped.changed() => {
                                sender.send_replace(None);
                                connection.close().await;
                                return;
                            }
                        }
                    }
                    sender.send_replace(None);
                    "the connection was lost".to_owned()
                }
                Err(error) if needs_the_user(&error) => {
                    link(Link::Ended {
                        code: error.code(),
                        reason: error.to_string(),
                    });
                    return;
                }
                Err(error) => error.to_string(),
            };
            if !reconnect {
                link(Link::Ended {
                    code: "lost",
                    reason,
                });
                return;
            }
            let wait = BACKOFF[attempt.min(BACKOFF.len() - 1)];
            attempt += 1;
            link(Link::Retrying {
                in_secs: wait,
                reason,
            });
            tokio::select! {
                () = tokio::time::sleep(Duration::from_secs(wait)) => {}
                _ = stopped.changed() => return,
            }
        }
    });
    Kept { connections, stop }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints() {
        assert_eq!(Endpoint::new("db", 5432).to_string(), "db:5432");
        assert_eq!(Endpoint::new("::1", 80).to_string(), "[::1]:80");
    }

    #[test]
    fn errors_that_need_the_user() {
        assert!(needs_the_user(&SshError::Cancelled));
        assert!(needs_the_user(&SshError::Auth {
            target: "a@b".into(),
            tried: "password".into()
        }));
        assert!(!needs_the_user(&SshError::Network {
            target: "b:22".into(),
            message: "refused".into()
        }));
        assert!(!needs_the_user(&SshError::Timeout {
            what: "connecting".into()
        }));
    }
}
