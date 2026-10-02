//! OpenSesh's RDP helper (ADR 0034): one IronRDP session, driven by the app over standard input
//! and output with `opensesh-rdp-protocol`'s messages. It is a program of its own because
//! IronRDP's dependencies can't share the app's lock file.
//!
//! - [`session`]: connecting (TCP, TLS, the certificate the app decides on, NLA) and the session.
//! - [`drive`]: the conversation with the app: connect, reconnect with a new password, resize.
//! - [`tls`]: TLS on rustls with `ring`, without key logging.
//! - [`certificate`]: the server's certificate, read for the app's question.
//!
//! Passwords, clipboard text and typed keys are never logged.

pub mod certificate;
mod clipboard;
pub mod session;
pub mod tls;

use opensesh_rdp_protocol::{Connect, Control, Event, FromHelper, Status, ToHelper};
use tokio::sync::mpsc::Receiver;

use crate::session::{Ended, Memory, Out};

/// Waits for what comes before a connection: the password and [`Control::Connect`]. `None` when
/// the app ends first.
async fn first_connect(
    inbox: &mut Receiver<ToHelper>,
    password: &mut String,
    memory: &mut Memory,
) -> Option<Connect> {
    loop {
        match inbox.recv().await? {
            ToHelper::Password(given) => *password = given,
            ToHelper::Clipboard(text) => memory.clipboard = Some(text),
            ToHelper::Control(Control::Connect(settings)) => return Some(settings),
            ToHelper::Control(Control::Disconnect) => return None,
            ToHelper::Control(_) => {}
        }
    }
}

/// After a disconnection: waits for [`Control::Reconnect`] (`true`; a new password may come
/// first) or the end (`false`).
async fn wait_for_reconnect(
    inbox: &mut Receiver<ToHelper>,
    password: &mut String,
    settings: &mut Connect,
    memory: &mut Memory,
) -> bool {
    loop {
        match inbox.recv().await {
            Some(ToHelper::Password(given)) => *password = given,
            Some(ToHelper::Clipboard(text)) => memory.clipboard = Some(text),
            Some(ToHelper::Control(Control::Reconnect)) => return true,
            Some(ToHelper::Control(Control::Resize { width, height })) => {
                settings.width = width;
                settings.height = height;
            }
            Some(ToHelper::Control(Control::Disconnect)) | None => return false,
            Some(ToHelper::Control(_)) => {}
        }
    }
}

/// The conversation with the app, until it ends it.
pub async fn drive(mut inbox: Receiver<ToHelper>, out: Out) {
    let mut password = String::new();
    let mut memory = Memory::default();
    let Some(mut settings) = first_connect(&mut inbox, &mut password, &mut memory).await else {
        return;
    };
    loop {
        let ended =
            session::connect_and_run(&mut settings, &password, &mut inbox, &mut memory, &out).await;
        let (code, reason) = match ended {
            Ended::Shutdown => return,
            Ended::Resize(width, height) => {
                settings.width = width;
                settings.height = height;
                continue;
            }
            Ended::Refused(reason) => ("auth", reason),
            Ended::Lost(code, reason) => (code, reason),
        };
        if !out
            .send(FromHelper::Event(Event::Status(Status::Disconnected {
                code: code.to_owned(),
                reason,
            })))
            .await
        {
            return;
        }
        if !wait_for_reconnect(&mut inbox, &mut password, &mut settings, &mut memory).await {
            return;
        }
    }
}
