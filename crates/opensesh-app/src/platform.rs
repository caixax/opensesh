//! Small operating-system integrations outside Qt.
//!
//! Release builds on Windows use the GUI subsystem (no console window), so stdout and stderr go
//! nowhere. These helpers keep command-line output and fatal startup errors visible there; on
//! every other build they do nothing.

/// Removes the current directory from the DLL search order (Windows). ConPTY is loaded by bare
/// name (`conpty.dll`): without the copy bundled next to the executable (ADR 0014), a DLL
/// planted in the folder the app was started from would otherwise be loaded.
pub fn harden_dll_search() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::LibraryLoader::SetDllDirectoryW;
        let empty = [0_u16];
        // SAFETY: `empty` is a valid NUL-terminated wide string that outlives the call; an
        // empty string only removes the current directory from the search order.
        unsafe {
            SetDllDirectoryW(empty.as_ptr());
        }
    }
}

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

/// This process's resident memory in bytes: the working set on Windows, `VmRSS` on Linux; `None`
/// elsewhere or when it can't be read (Sprint 17's leak check).
#[must_use]
pub fn resident_memory() -> Option<u64> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::ProcessStatus::{
            K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;

        let size = u32::try_from(std::mem::size_of::<PROCESS_MEMORY_COUNTERS>()).ok()?;
        let mut counters = PROCESS_MEMORY_COUNTERS {
            cb: size,
            ..PROCESS_MEMORY_COUNTERS::default()
        };
        // SAFETY: the pseudo-handle of this process, and a counters struct of the size given.
        let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &raw mut counters, size) };
        (ok != 0).then(|| u64::try_from(counters.WorkingSetSize).unwrap_or(u64::MAX))
    }
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        let kilobytes: u64 = status
            .lines()
            .find_map(|line| line.strip_prefix("VmRSS:"))?
            .trim()
            .trim_end_matches("kB")
            .trim()
            .parse()
            .ok()?;
        Some(kilobytes * 1024)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        None
    }
}
