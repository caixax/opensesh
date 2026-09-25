//! Small operating-system integrations outside Qt.
//!
//! Release builds on Windows use the GUI subsystem (no console window), so stdout and stderr go
//! nowhere. These helpers keep command-line output and fatal startup errors visible there; on
//! every other build they do nothing.

/// Attaches to the console of the parent process, if there is one, so `--help`, `--version` and
/// early errors are visible when a Windows release build is started from a terminal.
pub fn attach_parent_console() {
    #[cfg(all(windows, not(debug_assertions)))]
    {
        use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
        // SAFETY: AttachConsole has no memory-safety preconditions. It fails harmlessly when
        // there is no parent console (e.g. started from Explorer), which is the normal case.
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

/// Shows an error that stops the app. On Windows release builds a message box is the only
/// visible output; elsewhere the error has already been written to stderr.
pub fn show_fatal_error(message: &str) {
    #[cfg(all(windows, not(debug_assertions)))]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};

        let text = wide(&format!("OpenSesh could not start.\n\n{message}"));
        let title = wide(opensesh_core::identity::APP_NAME);
        // SAFETY: both buffers are NUL-terminated UTF-16 strings that outlive the call, and a
        // null owner window is allowed.
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }
    #[cfg(not(all(windows, not(debug_assertions))))]
    let _ = message;
}

/// NUL-terminated UTF-16 copy of `text` for Win32 APIs.
#[cfg(all(windows, not(debug_assertions)))]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
