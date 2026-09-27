//! Running snippets and macros on terminal panes, and the macro recorder (Sprint 10).
//!
//! A run types a snippet's text, or goes through a macro's steps, in each target pane at the
//! same time (one task per pane, on the SSH runtime). `{{name}}` takes the values asked once for
//! the run; `{{secret:identity}}` the password of a keychain identity, fetched by the keychain
//! worker and wiped after typing, never shown to QML. A newline is typed as Enter.
//!
//! A step that waits for a pattern looks at what the pane printed since the run started (or
//! since the previous wait matched), without its escape sequences: a session tap collects it
//! from the start, so a prompt that comes back quickly isn't missed.
//!
//! The recorder is a tap too: what is typed in a pane, with the pauses between, becomes send
//! and pause steps.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};

use opensesh_core::snippets::{Part, Snippet, Step, parse};
use opensesh_ssh::log::Cleaner;
use opensesh_term::session::{Tap, TapId};
use secrecy::{ExposeSecret, SecretString};
use zeroize::Zeroizing;

use crate::keychain::{self, Job};
use crate::terminal::registry;

/// How a run ended in one pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Everything was typed.
    Done,
    /// It stopped: a code (`gone`, `timeout`, `secret-locked`, `secret-unknown`, `no-secret`,
    /// `missing`) and a detail (the pattern, the identity, the variable).
    Stopped {
        /// Why.
        code: &'static str,
        /// About what.
        detail: String,
    },
}

/// Where a run reports, per pane (pane 0 for what stops the whole run, like a locked vault).
pub type ReportSink = Arc<dyn Fn(i32, Outcome) + Send + Sync>;

/// Output a run's panes printed, without escape sequences, for its waits.
struct Collect {
    cleaner: Cleaner,
    sender: tokio::sync::mpsc::UnboundedSender<String>,
}

impl Tap for Collect {
    fn output(&mut self, bytes: &[u8]) {
        let text = self.cleaner.clean(bytes);
        if !text.is_empty() {
            let _ = self
                .sender
                .send(String::from_utf8_lossy(&text).into_owned());
        }
    }
}

/// The most text a wait keeps (what came before is dropped).
const WAIT_BUFFER: usize = 64 * 1024;

/// Runs `snippet` in `panes`, with `values` for its variables.
pub fn run(snippet: Snippet, values: HashMap<String, String>, panes: Vec<i32>, report: ReportSink) {
    let Some(runtime) = opensesh_ssh::runtime() else {
        return;
    };
    runtime.spawn(async move {
        let secrets = match fetch_secrets(&snippet.secrets()).await {
            Ok(secrets) => Arc::new(secrets),
            Err(outcome) => {
                report(0, outcome);
                return;
            }
        };
        let values = Arc::new(values);
        let steps = Arc::new(if snippet.is_macro() {
            snippet.steps.clone()
        } else {
            vec![Step::Send(snippet.text.clone())]
        });
        for pane in panes {
            let (steps, values, secrets, report) = (
                Arc::clone(&steps),
                Arc::clone(&values),
                Arc::clone(&secrets),
                Arc::clone(&report),
            );
            tokio::spawn(async move {
                let outcome = run_in(pane, &steps, &values, &secrets).await;
                report(pane, outcome);
            });
        }
    });
}

/// The passwords of `names` (keychain identities), from the keychain worker.
async fn fetch_secrets(names: &[String]) -> Result<HashMap<String, SecretString>, Outcome> {
    let mut out = HashMap::new();
    for name in names {
        let (reply, answer) = tokio::sync::oneshot::channel();
        if !keychain::request(Job::ConnectionSecrets {
            identity: name.clone(),
            reply,
        }) {
            return Err(Outcome::Stopped {
                code: "secret-locked",
                detail: name.clone(),
            });
        }
        let secrets = answer.await.map_err(|_| Outcome::Stopped {
            code: "secret-locked",
            detail: name.clone(),
        })?;
        match secrets {
            Ok(secrets) => match secrets.password {
                Some(password) => {
                    out.insert(name.clone(), password);
                }
                None => {
                    return Err(Outcome::Stopped {
                        code: "no-secret",
                        detail: name.clone(),
                    });
                }
            },
            Err(why) => {
                return Err(Outcome::Stopped {
                    code: if why == "locked" {
                        "secret-locked"
                    } else {
                        "secret-unknown"
                    },
                    detail: name.clone(),
                });
            }
        }
    }
    Ok(out)
}

/// Types `text` (filled in) into pane `pane`; `Err` with the missing part. The filled-in text,
/// which may hold a secret, is built once in a buffer of its final size and wiped after.
fn type_text(
    pane: i32,
    text: &str,
    values: &HashMap<String, String>,
    secrets: &HashMap<String, SecretString>,
) -> Result<(), Outcome> {
    let bytes = typed_bytes(text, values, secrets)?;
    let entry = registry::get(pane).ok_or(Outcome::Stopped {
        code: "gone",
        detail: String::new(),
    })?;
    entry.session().write(&bytes);
    Ok(())
}

/// What typing `text` sends: its variables and secrets filled in, and a newline (or `\r\n`) as
/// Enter. No copy of a secret is left behind: the pieces are borrowed and the buffer, sized once,
/// is wiped when dropped.
fn typed_bytes(
    text: &str,
    values: &HashMap<String, String>,
    secrets: &HashMap<String, SecretString>,
) -> Result<Zeroizing<Vec<u8>>, Outcome> {
    let parts = parse(text);
    let mut pieces: Vec<&[u8]> = Vec::with_capacity(parts.len());
    for part in &parts {
        let piece = match part {
            Part::Text(plain) => Some(plain.as_bytes()),
            Part::Variable(name) => values.get(name).map(String::as_bytes),
            Part::Secret(name) => secrets
                .get(name)
                .map(|secret| secret.expose_secret().as_bytes()),
        };
        match piece {
            Some(piece) => pieces.push(piece),
            None => {
                return Err(Outcome::Stopped {
                    code: "missing",
                    detail: match part {
                        Part::Variable(name) | Part::Secret(name) | Part::Text(name) => {
                            name.clone()
                        }
                    },
                });
            }
        }
    }
    // Never longer than the pieces: no reallocation leaves a copy behind.
    let mut bytes = Zeroizing::new(Vec::with_capacity(
        pieces.iter().map(|piece| piece.len()).sum(),
    ));
    let mut after_cr = false;
    for byte in pieces.into_iter().flatten().copied() {
        match byte {
            b'\n' if after_cr => {}
            b'\n' => bytes.push(b'\r'),
            other => bytes.push(other),
        }
        after_cr = byte == b'\r';
    }
    Ok(bytes)
}

async fn run_in(
    pane: i32,
    steps: &[Step],
    values: &HashMap<String, String>,
    secrets: &HashMap<String, SecretString>,
) -> Outcome {
    // Output is collected from the start only when something waits for it.
    let waits = steps
        .iter()
        .any(|step| matches!(step, Step::WaitFor { .. }));
    let mut output = None;
    let mut tap = None;
    if waits {
        let Some(entry) = registry::get(pane) else {
            return Outcome::Stopped {
                code: "gone",
                detail: String::new(),
            };
        };
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        tap = Some(entry.session().add_tap(Box::new(Collect {
            cleaner: Cleaner::default(),
            sender,
        })));
        output = Some(receiver);
    }
    let outcome = steps_in(pane, steps, values, secrets, output.as_mut()).await;
    if let (Some(id), Some(entry)) = (tap, registry::get(pane)) {
        entry.session().remove_tap(id);
    }
    outcome
}

async fn steps_in(
    pane: i32,
    steps: &[Step],
    values: &HashMap<String, String>,
    secrets: &HashMap<String, SecretString>,
    mut output: Option<&mut tokio::sync::mpsc::UnboundedReceiver<String>>,
) -> Outcome {
    let mut seen = String::new();
    for step in steps {
        match step {
            Step::Send(text) => {
                if let Err(outcome) = type_text(pane, text, values, secrets) {
                    return outcome;
                }
            }
            Step::Delay(ms) => tokio::time::sleep(Duration::from_millis(*ms)).await,
            Step::WaitFor {
                pattern,
                timeout_ms,
            } => {
                let Ok(regex) = regex::Regex::new(pattern) else {
                    return Outcome::Stopped {
                        code: "timeout",
                        detail: pattern.clone(),
                    };
                };
                let deadline = tokio::time::Instant::now() + Duration::from_millis(*timeout_ms);
                loop {
                    if let Some(found) = regex.find(&seen) {
                        // The next wait looks after this match.
                        seen.drain(..found.end());
                        break;
                    }
                    let Some(receiver) = output.as_deref_mut() else {
                        break;
                    };
                    match tokio::time::timeout_at(deadline, receiver.recv()).await {
                        Ok(Some(text)) => {
                            seen.push_str(&text);
                            if seen.len() > WAIT_BUFFER {
                                let cut = seen.len() - WAIT_BUFFER;
                                let cut = (cut..seen.len())
                                    .find(|at| seen.is_char_boundary(*at))
                                    .unwrap_or(seen.len());
                                seen.drain(..cut);
                            }
                        }
                        Ok(None) => {
                            return Outcome::Stopped {
                                code: "gone",
                                detail: String::new(),
                            };
                        }
                        Err(_) => {
                            return Outcome::Stopped {
                                code: "timeout",
                                detail: pattern.clone(),
                            };
                        }
                    }
                }
            }
        }
    }
    Outcome::Done
}

/// What was typed in a pane while its macro is recorded, with when.
type Typed = Arc<Mutex<Vec<(Instant, Vec<u8>)>>>;

/// What is typed in a pane while its macro is recorded, with when.
struct Recording {
    typed: Typed,
}

impl Tap for Recording {
    fn input(&mut self, bytes: &[u8]) {
        self.typed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((Instant::now(), bytes.to_vec()));
    }
}

/// Macros being recorded: the pane, its tap and what it collected.
static RECORDINGS: LazyLock<Mutex<HashMap<i32, (TapId, Typed)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// A pause at least this long becomes a pause step.
const PAUSE: Duration = Duration::from_millis(800);

/// Starts recording what is typed in pane `pane`; false when it has no session.
pub fn record_start(pane: i32) -> bool {
    let Some(entry) = registry::get(pane) else {
        return false;
    };
    let typed = Arc::new(Mutex::new(Vec::new()));
    let id = entry.session().add_tap(Box::new(Recording {
        typed: Arc::clone(&typed),
    }));
    let old = RECORDINGS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(pane, (id, typed));
    if let Some((old, _)) = old {
        entry.session().remove_tap(old);
    }
    true
}

/// Whether pane `pane` records a macro.
#[must_use]
pub fn recording(pane: i32) -> bool {
    RECORDINGS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .contains_key(&pane)
}

/// Stops recording pane `pane`: what was typed, as steps (empty when nothing was).
pub fn record_stop(pane: i32) -> Vec<Step> {
    let Some((id, typed)) = RECORDINGS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&pane)
    else {
        return Vec::new();
    };
    if let Some(entry) = registry::get(pane) {
        entry.session().remove_tap(id);
    }
    let typed = std::mem::take(&mut *typed.lock().unwrap_or_else(PoisonError::into_inner));
    steps_of(&typed)
}

/// Typed input with its times as steps: text, with a pause step where the typing paused.
fn steps_of(typed: &[(Instant, Vec<u8>)]) -> Vec<Step> {
    let mut steps = Vec::new();
    let mut text: Vec<u8> = Vec::new();
    let mut last: Option<Instant> = None;
    let flush = |text: &mut Vec<u8>, steps: &mut Vec<Step>| {
        if !text.is_empty() {
            // Enter is stored as a newline, as snippets write it.
            let typed = String::from_utf8_lossy(text).replace('\r', "\n");
            steps.push(Step::Send(typed));
            text.clear();
        }
    };
    for (when, bytes) in typed {
        if let Some(last) = last {
            let pause = when.saturating_duration_since(last);
            if pause >= PAUSE {
                flush(&mut text, &mut steps);
                // To the tenth of a second, at most ten minutes.
                let ms = u64::try_from(pause.as_millis()).unwrap_or(u64::MAX);
                let ms = (ms / 100 * 100).clamp(100, opensesh_core::snippets::MAX_WAIT_MS);
                steps.push(Step::Delay(ms));
            }
        }
        text.extend_from_slice(bytes);
        last = Some(*when);
    }
    flush(&mut text, &mut steps);
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_input_becomes_steps() {
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        let typed = vec![
            (at(0), b"en".to_vec()),
            (at(100), b"able\r".to_vec()),
            (at(2_450), b"show run".to_vec()),
            (at(2_500), b"\r".to_vec()),
        ];
        assert_eq!(
            steps_of(&typed),
            [
                Step::Send("enable\n".into()),
                Step::Delay(2_300),
                Step::Send("show run\n".into()),
            ]
        );
        assert!(steps_of(&[]).is_empty());
    }

    #[test]
    fn typed_text_fills_in_its_values_and_presses_enter_for_newlines() {
        let values = HashMap::from([("service".to_owned(), "nginx".to_owned())]);
        let secrets = HashMap::from([("db".to_owned(), SecretString::from("s3cret"))]);
        let typed = |text: &str| typed_bytes(text, &values, &secrets).map(|bytes| bytes.to_vec());
        assert_eq!(
            typed("restart {{service}}\r\n{{secret:db}}\nlast\r").ok(),
            Some(b"restart nginx\rs3cret\rlast\r".to_vec())
        );
        assert!(matches!(
            typed("{{other}}"),
            Err(Outcome::Stopped { code: "missing", detail }) if detail == "other"
        ));
        assert!(matches!(
            typed("{{secret:web}}"),
            Err(Outcome::Stopped { code: "missing", detail }) if detail == "web"
        ));
    }
}
