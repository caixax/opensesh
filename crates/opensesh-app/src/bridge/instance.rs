//! `Instance` QML singleton (Sprint 5, ADR 0021): requests that reach the running OpenSesh from a
//! second start or the `opensesh` CLI (over the local socket of `opensesh_core::ipc`), and the
//! request this process was started with (`--connect`, `--open`).
//!
//! Requests wait in a queue until QML calls `takePending()` (once the main window is ready);
//! from then on each one is emitted as it arrives.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt string type from cxx-qt-lib.
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// Requests from other processes.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(bool, listening, READ, CONSTANT)]
        type Instance = super::InstanceRust;

        /// Bring the main window to the front.
        #[qsignal]
        #[cxx_name = "activateRequested"]
        fn activate_requested(self: Pin<&mut Self>);

        /// Connect to saved host `host` (id or name).
        #[qsignal]
        #[cxx_name = "connectRequested"]
        fn connect_requested(self: Pin<&mut Self>, host: QString);

        /// Connect to a quick-connect target or URL; ask the user first (PLAN §8).
        #[qsignal]
        #[cxx_name = "openRequested"]
        fn open_requested(self: Pin<&mut Self>, url: QString);

        /// QML is ready: emits the waiting requests, and the next ones as they come.
        #[qinvokable]
        #[cxx_name = "takePending"]
        fn take_pending(self: Pin<&mut Self>);

        /// Queues a request as if it came from another process (`activate`, `connect` with a
        /// host, `open` with a target): the smoke test's way in.
        #[qinvokable]
        fn simulate(self: &Self, op: &QString, value: &QString);
    }

    impl cxx_qt::Initialize for Instance {}
    impl cxx_qt::Threading for Instance {}
}

use core::pin::Pin;
use std::sync::{LazyLock, Mutex, PoisonError};

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;
use opensesh_core::ipc::{Reply, Request};

/// Rust state behind `Instance`.
#[derive(Debug, Default)]
pub struct InstanceRust {
    listening: bool,
}

struct Shared {
    pending: Vec<Request>,
    thread: Option<CxxQtThread<qobject::Instance>>,
    ready: bool,
}

static SHARED: LazyLock<Mutex<Shared>> = LazyLock::new(|| {
    Mutex::new(Shared {
        pending: Vec::new(),
        thread: None,
        ready: false,
    })
});

static LISTENING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// This process took the instance socket.
pub fn set_listening() {
    LISTENING.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Queues `request` and wakes QML if it is ready.
pub fn push(request: Request) {
    let mut shared = SHARED.lock().unwrap_or_else(PoisonError::into_inner);
    shared.pending.push(request);
    if shared.ready {
        if let Some(thread) = &shared.thread {
            let _ = thread.queue(|object| object.drain());
        }
    }
}

/// The socket handler: checks the request and queues it. Runs on the listener's thread.
pub fn handle(request: Request) -> Reply {
    match &request {
        Request::Connect { host } if host.trim().is_empty() => Reply::Error {
            message: "no host given".to_owned(),
        },
        Request::Open { url } => match opensesh_core::hosts::target::parse(url) {
            Ok(_) => {
                push(request);
                Reply::Ok
            }
            Err(error) => Reply::Error {
                message: error.to_string(),
            },
        },
        _ => {
            push(request);
            Reply::Ok
        }
    }
}

impl qobject::Instance {
    fn drain(mut self: Pin<&mut Self>) {
        let requests = std::mem::take(
            &mut SHARED
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .pending,
        );
        for request in requests {
            match request {
                Request::Activate => self.as_mut().activate_requested(),
                Request::Connect { host } => self.as_mut().connect_requested(QString::from(&host)),
                Request::Open { url } => self.as_mut().open_requested(QString::from(&url)),
            }
        }
    }

    /// See the bridge declaration.
    pub fn take_pending(self: Pin<&mut Self>) {
        SHARED.lock().unwrap_or_else(PoisonError::into_inner).ready = true;
        self.drain();
    }

    /// See the bridge declaration.
    pub fn simulate(&self, op: &QString, value: &QString) {
        let value = value.to_string();
        let request = match op.to_string().as_str() {
            "connect" => Request::Connect { host: value },
            "open" => Request::Open { url: value },
            _ => Request::Activate,
        };
        // Delivered on the next turn of the event loop, as a real request would be.
        let _ = handle(request);
    }
}

impl cxx_qt::Initialize for qobject::Instance {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().listening = LISTENING.load(std::sync::atomic::Ordering::Relaxed);
        SHARED.lock().unwrap_or_else(PoisonError::into_inner).thread = Some(self.qt_thread());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_checked_before_they_are_queued() {
        assert!(matches!(
            handle(Request::Open {
                url: "web:99999".into()
            }),
            Reply::Error { .. }
        ));
        assert!(matches!(
            handle(Request::Connect { host: " ".into() }),
            Reply::Error { .. }
        ));
        assert_eq!(
            handle(Request::Open {
                url: "deploy@web".into()
            }),
            Reply::Ok
        );
        let pending = SHARED.lock().unwrap().pending.clone();
        assert!(pending.contains(&Request::Open {
            url: "deploy@web".into()
        }));
    }
}
