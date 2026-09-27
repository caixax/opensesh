//! The questions a connection asks the user: whether to trust a host key, a password, a key's
//! passphrase, keyboard-interactive (MFA) prompts.
//!
//! A connection hands each [`Request`] to the app's [`Asker`] and waits for the answer. The app
//! shows the question in the pane and calls [`Request::answer`]; dropping a request without
//! answering counts as "cancel". Answers carrying secrets are `SecretString`s: they print
//! nothing in `Debug` and are wiped on drop.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use secrecy::SecretString;
use tokio::sync::oneshot;

/// A question to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prompt {
    /// Should this host key be trusted?
    HostKey(HostKeyQuestion),
    /// A password for `target` (`user@host`).
    Password {
        /// `user@host`.
        target: String,
        /// A previous attempt was refused.
        retry: bool,
    },
    /// The passphrase of a key file.
    Passphrase {
        /// The file.
        key: String,
        /// A previous attempt was wrong.
        retry: bool,
    },
    /// Keyboard-interactive prompts from the server (one-time codes, PAM questions).
    KeyboardInteractive {
        /// `user@host`.
        target: String,
        /// The server's title for the prompts (may be empty).
        name: String,
        /// The server's instructions (may be empty).
        instructions: String,
        /// The fields to fill.
        fields: Vec<Field>,
    },
}

/// One keyboard-interactive field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// The server's prompt text.
    pub label: String,
    /// Whether the answer may be shown as typed (otherwise it is masked).
    pub echo: bool,
}

/// A host key to decide about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostKeyQuestion {
    /// The host.
    pub host: String,
    /// The port.
    pub port: u16,
    /// Key type (`ssh-ed25519`, ...).
    pub key_type: String,
    /// `SHA256:...`.
    pub fingerprint: String,
    /// Why the question is asked.
    pub kind: HostKeyKind,
}

/// Why a host key needs a decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyKind {
    /// First connection to this host (with this key type).
    New {
        /// Keys of other types already known for the host.
        other_types: Vec<String>,
    },
    /// The host presents a different key than the one remembered: a possible attack.
    Changed {
        /// The remembered key's fingerprint.
        known_fingerprint: String,
        /// The file it is in.
        file: String,
        /// The line.
        line: usize,
    },
}

/// The user's answer.
#[derive(Debug)]
pub enum Answer {
    /// Trust the host key for this connection only.
    TrustOnce,
    /// Trust the host key and remember it in OpenSesh's `known_hosts`.
    TrustAndRemember,
    /// Secrets typed in (one per field of the question).
    Secrets(Vec<SecretString>),
    /// No.
    Cancel,
}

/// A question waiting for its answer.
#[derive(Debug)]
pub struct Request {
    /// Tells requests apart.
    pub id: u64,
    /// The question.
    pub prompt: Prompt,
    reply: oneshot::Sender<Answer>,
}

impl Request {
    /// Answers the question (the connection goes on at once).
    pub fn answer(self, answer: Answer) {
        // The connection may have ended meanwhile; nothing waits for the answer then.
        let _ = self.reply.send(answer);
    }
}

/// Where a connection sends its questions. It must return at once (show the question, store the
/// request, answer later).
pub type Asker = Arc<dyn Fn(Request) + Send + Sync>;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Asks `prompt` and waits for the answer (a dropped request is [`Answer::Cancel`]).
pub async fn ask(asker: &Asker, prompt: Prompt) -> Answer {
    let (reply, answer) = oneshot::channel();
    asker(Request {
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        prompt,
        reply,
    });
    answer.await.unwrap_or(Answer::Cancel)
}

/// An asker that cancels every question (tests, and connections with no one to ask).
#[must_use]
pub fn never() -> Asker {
    Arc::new(|request: Request| request.answer(Answer::Cancel))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn answers_and_drops() {
        let asker: Asker = Arc::new(|request: Request| {
            let answer = match request.prompt {
                Prompt::Password { .. } => Answer::Secrets(vec![SecretString::from("pw")]),
                _ => Answer::Cancel,
            };
            request.answer(answer);
        });
        let answer = ask(
            &asker,
            Prompt::Password {
                target: "a@b".into(),
                retry: false,
            },
        )
        .await;
        assert!(matches!(answer, Answer::Secrets(ref secrets) if secrets.len() == 1));
        // An asker that drops the request cancels.
        let dropper: Asker = Arc::new(|_request: Request| {});
        assert!(matches!(
            ask(
                &dropper,
                Prompt::Passphrase {
                    key: "k".into(),
                    retry: false
                }
            )
            .await,
            Answer::Cancel
        ));
        assert!(!format!("{answer:?}").contains("pw\""));
    }
}
