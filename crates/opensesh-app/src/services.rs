//! Process-wide services shared by the QML singletons (which QML constructs with no arguments,
//! so they can't receive them any other way). Set once by `main` before QML is loaded.

use std::sync::OnceLock;
use std::time::Duration;

use opensesh_core::AppPaths;
use opensesh_core::desktop::{self, DesktopInfo};
use opensesh_core::writer::FileWriter;

/// How long a file must stay unchanged in memory before the writer saves it.
const SAVE_DEBOUNCE: Duration = Duration::from_millis(300);

/// Shared services.
#[derive(Debug)]
pub struct Services {
    /// Resolved directories.
    pub paths: AppPaths,
    /// Background writer for settings and state files.
    pub writer: FileWriter,
    /// Detected desktop environment.
    pub desktop: DesktopInfo,
}

static SERVICES: OnceLock<Services> = OnceLock::new();

/// Creates the services. Only the first call has an effect.
///
/// # Errors
///
/// Fails if the writer thread can't start.
pub fn init(paths: AppPaths) -> std::io::Result<()> {
    let services = Services {
        paths,
        writer: FileWriter::spawn(SAVE_DEBOUNCE)?,
        desktop: desktop::detect_current(),
    };
    if SERVICES.set(services).is_err() {
        tracing::warn!("services were already initialized");
    }
    Ok(())
}

/// The services, if [`init`] ran.
#[must_use]
pub fn get() -> Option<&'static Services> {
    SERVICES.get()
}

/// Writes every pending file (call before exiting).
pub fn flush() {
    if let Some(services) = get() {
        services.writer.flush();
    }
}
