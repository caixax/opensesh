//! The local shells this computer has (PLAN Sprint 12), for the new tab menu and the `shell`
//! terminal setting.
//!
//! - **Linux and macOS:** `$SHELL` first, then `/etc/shells` (programs that exist, each once:
//!   `/bin/bash` and `/usr/bin/bash` are the same file on merged-`/usr` systems; `nologin`,
//!   `false`, `git-shell` and the restricted `rbash` are left out).
//! - **Windows:** PowerShell 7, Windows PowerShell, the Command Prompt, Git Bash, MSYS2 and
//!   Cygwin where they are installed, and the WSL distributions from `wsl.exe -l -q`, whose
//!   output is UTF-16 (or UTF-8 with `WSL_UTF8=1`).
//!
//! A shell is a command line ([`opensesh_core::command_line`]): the setting stores it as text,
//! so a user can also write their own. Discovering reads files and runs `wsl.exe`: call it off the GUI
//! thread.

use std::path::{Path, PathBuf};

use opensesh_core::command_line::join;

/// A shell to offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shell {
    /// A stable id (`bash`, `pwsh`, `wsl:Ubuntu`).
    pub id: String,
    /// What the menu shows (`bash`, `PowerShell 7`, `Ubuntu (WSL)`).
    pub name: String,
    /// The command line that starts it (see [`opensesh_core::command_line::split`]).
    pub command: String,
    /// It is the user's own shell (`$SHELL`), what a new tab runs without a choice.
    pub default: bool,
}

/// The shells of this computer, the user's own first (blocking: reads files, runs `wsl.exe`).
#[must_use]
pub fn discover() -> Vec<Shell> {
    #[cfg(windows)]
    {
        windows::discover()
    }
    #[cfg(not(windows))]
    {
        let listed = std::fs::read_to_string("/etc/shells").unwrap_or_default();
        unix_shells(
            std::env::var_os("SHELL").map(PathBuf::from).as_deref(),
            &listed,
            &|path| path.canonicalize().ok().filter(|path| is_executable(path)),
        )
    }
}

/// Programs in `/etc/shells` that aren't interactive shells.
const NOT_SHELLS: [&str; 5] = ["nologin", "false", "true", "git-shell", "rbash"];

/// The shells of `/etc/shells` (`listed`) and `$SHELL` (`user`), the latter first; `resolve`
/// gives a program's real path when it exists and runs (to list each program once).
#[cfg_attr(windows, allow(dead_code, reason = "Unix only; tested everywhere"))]
fn unix_shells(
    user: Option<&Path>,
    listed: &str,
    resolve: &dyn Fn(&Path) -> Option<PathBuf>,
) -> Vec<Shell> {
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut out: Vec<Shell> = Vec::new();
    let candidates = user
        .into_iter()
        .map(|path| (path.to_path_buf(), true))
        .chain(
            listed
                .lines()
                .map(str::trim)
                .filter(|line| line.starts_with('/'))
                .map(|line| (PathBuf::from(line), false)),
        );
    for (path, default) in candidates {
        let Some(name) = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
        else {
            continue;
        };
        if NOT_SHELLS.contains(&name.as_str()) {
            continue;
        }
        let Some(real) = resolve(&path) else {
            continue;
        };
        if seen.contains(&real) {
            continue;
        }
        seen.push(real);
        // Two different programs with one name (`/bin/sh` and `/usr/local/bin/sh`) keep apart.
        let id = if out.iter().any(|shell| shell.id == name) {
            path.display().to_string()
        } else {
            name.clone()
        };
        out.push(Shell {
            id,
            name,
            command: join(&[path.display().to_string()]),
            default,
        });
    }
    if let Some(first) = out.first_mut().filter(|_| user.is_some()) {
        first.default = true;
    }
    let (mine, mut others): (Vec<Shell>, Vec<Shell>) =
        out.into_iter().partition(|shell| shell.default);
    others.sort_by(|a, b| a.name.cmp(&b.name).then(a.command.cmp(&b.command)));
    mine.into_iter().chain(others).collect()
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

/// The names in the output of `wsl.exe -l -q`: UTF-16 (little endian, maybe with a byte order
/// mark), or UTF-8 when `WSL_UTF8=1`. Docker Desktop's own distributions are left out.
#[must_use]
pub fn wsl_distributions(output: &[u8]) -> Vec<String> {
    let utf16 =
        output.len() >= 2 && output.len() % 2 == 0 && output.chunks(2).any(|pair| pair[1] == 0);
    let text = if utf16 {
        let units: Vec<u16> = output
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(output).into_owned()
    };
    text.trim_start_matches('\u{feff}')
        .lines()
        .map(|line| line.trim_matches(|c: char| c.is_whitespace() || c == '\0'))
        .filter(|name| !name.is_empty() && !name.starts_with("docker-desktop"))
        .map(str::to_owned)
        .collect()
}

#[cfg(windows)]
mod windows {
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};

    use super::{Shell, join, wsl_distributions};

    /// No console window flashes for `wsl.exe`.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn env_path(name: &str) -> Option<PathBuf> {
        std::env::var_os(name).map(PathBuf::from)
    }

    fn in_path(name: &str) -> Option<PathBuf> {
        std::env::split_paths(&std::env::var_os("PATH")?)
            .filter(|dir| dir.is_absolute())
            .map(|dir| dir.join(name))
            .find(|path| path.is_file())
    }

    /// The first of `candidates` that exists.
    fn first(candidates: impl IntoIterator<Item = Option<PathBuf>>) -> Option<PathBuf> {
        candidates.into_iter().flatten().find(|path| path.is_file())
    }

    fn under(root: Option<PathBuf>, relative: &str) -> Option<PathBuf> {
        root.map(|root| root.join(relative))
    }

    fn shell(id: &str, name: &str, program: &Path, args: &[&str]) -> Shell {
        let mut words = vec![program.display().to_string()];
        words.extend(args.iter().map(|arg| (*arg).to_owned()));
        Shell {
            id: id.to_owned(),
            name: name.to_owned(),
            command: join(&words),
            default: false,
        }
    }

    pub(super) fn discover() -> Vec<Shell> {
        let system = env_path("SystemRoot");
        let program_files = env_path("ProgramFiles");
        let program_files_x86 = env_path("ProgramFiles(x86)");
        let local_programs = env_path("LOCALAPPDATA").map(|dir| dir.join("Programs"));
        let mut out = Vec::new();
        if let Some(pwsh) = first([
            in_path("pwsh.exe"),
            under(program_files.clone(), r"PowerShell\7\pwsh.exe"),
        ]) {
            out.push(shell("pwsh", "PowerShell 7", &pwsh, &[]));
        }
        if let Some(powershell) = first([under(
            system.clone(),
            r"System32\WindowsPowerShell\v1.0\powershell.exe",
        )]) {
            out.push(shell("powershell", "Windows PowerShell", &powershell, &[]));
        }
        if let Some(cmd) = first([
            under(system.clone(), r"System32\cmd.exe"),
            env_path("ComSpec"),
        ]) {
            out.push(shell("cmd", "Command Prompt", &cmd, &[]));
        }
        if let Some(bash) = first([
            under(program_files.clone(), r"Git\bin\bash.exe"),
            under(program_files_x86, r"Git\bin\bash.exe"),
            under(local_programs, r"Git\bin\bash.exe"),
        ]) {
            out.push(shell("git-bash", "Git Bash", &bash, &["--login", "-i"]));
        }
        // MSYS2's own launcher, in this terminal (`-defterm -no-start`), in the folder it is
        // started in (`-here`), with the UCRT64 environment its installer recommends.
        if let Some(msys2) = first([Some(PathBuf::from(r"C:\msys64\msys2_shell.cmd"))]) {
            out.push(shell(
                "msys2",
                "MSYS2",
                &msys2,
                &["-defterm", "-here", "-no-start", "-ucrt64"],
            ));
        }
        if let Some(cygwin) = first([Some(PathBuf::from(r"C:\cygwin64\bin\bash.exe"))]) {
            out.push(shell("cygwin", "Cygwin", &cygwin, &["--login", "-i"]));
        }
        if let Some(wsl) = first([under(system, r"System32\wsl.exe")]) {
            let listed = std::process::Command::new(&wsl)
                .args(["-l", "-q"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
            match listed {
                Ok(output) if output.status.success() => {
                    for distro in wsl_distributions(&output.stdout) {
                        let mut entry = shell(
                            &format!("wsl:{distro}"),
                            &format!("{distro} (WSL)"),
                            &wsl,
                            &["-d"],
                        );
                        entry.command = format!("{} {}", entry.command, join(&[distro.as_str()]));
                        out.push(entry);
                    }
                }
                // No distribution installed (or WSL turned off): nothing to offer.
                Ok(_) => {}
                Err(error) => tracing::debug!("wsl.exe -l -q didn't run: {error}"),
            }
        }
        // What a new tab runs without a choice (see `shell::default_shell`).
        if let Some(first) = out.first_mut() {
            first.default = true;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests")]

    use super::*;

    #[test]
    fn wsl_lists_in_utf16_and_utf8() {
        let names = "Ubuntu-22.04\r\nDebian\r\ndocker-desktop\r\narchlinux\r\n";
        let mut utf16: Vec<u8> = vec![0xff, 0xfe];
        utf16.extend(names.encode_utf16().flat_map(u16::to_le_bytes));
        let expected = ["Ubuntu-22.04", "Debian", "archlinux"];
        assert_eq!(wsl_distributions(&utf16), expected);
        assert_eq!(wsl_distributions(&utf16[2..]), expected);
        assert_eq!(wsl_distributions(names.as_bytes()), expected);
        assert!(wsl_distributions(b"").is_empty());
    }

    #[test]
    fn etc_shells() {
        let listed = "# /etc/shells: valid login shells\n/bin/sh\n/usr/bin/sh\n/bin/bash\n/usr/bin/bash\n\
                      /bin/rbash\n/usr/bin/zsh\n/usr/bin/fish\n/usr/sbin/nologin\n/usr/bin/tmux\n/opt/gone/ksh\n";
        // Merged /usr: /bin is /usr/bin; /bin/sh is dash; ksh isn't installed.
        let resolve = |path: &Path| -> Option<PathBuf> {
            let text = path
                .to_str()?
                .replace("/bin/", "/usr/bin/")
                .replace("/usr/usr/", "/usr/");
            let real = if text.ends_with("/sh") {
                "/usr/bin/dash".to_owned()
            } else {
                text
            };
            (!real.contains("gone")).then(|| PathBuf::from(real))
        };
        let shells = unix_shells(Some(Path::new("/usr/bin/zsh")), listed, &resolve);
        let names: Vec<&str> = shells.iter().map(|shell| shell.name.as_str()).collect();
        assert_eq!(names, ["zsh", "bash", "fish", "sh", "tmux"]);
        assert!(shells[0].default && shells.iter().skip(1).all(|shell| !shell.default));
        assert_eq!(shells[0].command, "/usr/bin/zsh");
        assert_eq!(shells[1].command, "/bin/bash");
        // Without $SHELL nothing is the default.
        assert!(
            unix_shells(None, listed, &resolve)
                .iter()
                .all(|shell| !shell.default)
        );
    }

    #[test]
    fn this_computer_has_a_shell() {
        let shells = discover();
        assert!(!shells.is_empty());
        for shell in &shells {
            assert!(
                !opensesh_core::command_line::split(&shell.command)
                    .unwrap()
                    .is_empty(),
                "{shell:?}"
            );
        }
    }
}
