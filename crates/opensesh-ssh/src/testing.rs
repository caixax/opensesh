//! A tiny SSH server for tests, and for the app's smoke test (which must not reach the network):
//! a user [`USER`] with the password [`PASSWORD`] and/or keys, an optional one-time code after a
//! key ([`CODE`]), `direct-tcpip` for jump hosts, and a shell that prints `test$ ` and echoes what
//! it gets ("exit" ends it with status 3, "drop" drops the connection). It also answers the OS
//! detection command. Nothing here runs unless a test or the smoke test starts it.

use std::sync::Arc;
use std::time::Duration;

use russh::keys::{PrivateKey, PublicKey};
use russh::server::{Auth, Handler, Msg, Session};
use russh::{Channel, ChannelId, MethodKind, MethodSet};
use tokio::net::TcpListener;

use crate::osdetect;

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
}

#[derive(Clone)]
struct Server {
    rules: Rules,
    key_accepted: bool,
    code_asked: bool,
    typed: Vec<u8>,
    /// Session channels (the tiny shell only answers on these, not on jump tunnels).
    sessions: Vec<ChannelId>,
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
        reply.accept().await;
        Ok(())
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
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
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
        session.channel_success(channel)?;
        if command == osdetect::COMMAND.as_bytes() {
            session.data(channel, b"ID=debian\n__uname__\nLinux\n".to_vec())?;
            session.exit_status_request(channel, 0)?;
            session.eof(channel)?;
            session.close(channel)?;
        }
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
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
        Ok(())
    }
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
