//! Waypipe (Sprint 15, ADR 0036): Wayland programs on the server shown on this computer's Wayland
//! desktop, as `waypipe ssh` does it:
//!
//! - **here**, `waypipe --socket <local> client` runs for the session ([`Local::start`]): it
//!   listens on a Unix socket and shows what comes through it;
//! - **there**, the session's command becomes `waypipe --socket <remote> --unlink-socket server --
//!   <the shell or command>` ([`server_command`]): the programs it starts talk to its Wayland
//!   display, and it sends their windows to its socket. The display is made in
//!   `XDG_RUNTIME_DIR`, which servers without a login session (no logind) often lack: a private
//!   one is made for the session then, and removed after it;
//! - **between them**, the server's socket is forwarded to this one
//!   (`streamlocal-forward@openssh.com`, as `ssh -R remote:local`).
//!
//! Only on Unix with a Wayland session here; `waypipe` must be installed on both sides.

use std::path::{Path, PathBuf};
use std::time::Duration;

use russh::ChannelMsg;

use crate::connect::Connection;

/// Asks whether the server has `waypipe` (`command -v waypipe`, on a channel of its own).
pub async fn on_server(connection: &Connection) -> bool {
    let check = async {
        let target = connection.target().ok()?;
        let mut channel = target.channel_open_session().await.ok()?;
        channel.exec(true, "command -v waypipe").await.ok()?;
        let mut status = None;
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        status
    };
    tokio::time::timeout(Duration::from_secs(10), check)
        .await
        .ok()
        .flatten()
        == Some(0)
}

/// The local `waypipe client` of a session, ended (and its socket removed) when dropped.
#[derive(Debug)]
pub struct Local {
    socket: PathBuf,
    remote: String,
    child: tokio::process::Child,
}

impl Drop for Local {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        let _ = std::fs::remove_file(&self.socket);
    }
}

/// A name no other session uses.
fn unique() -> String {
    let mut bytes = [0_u8; 8];
    rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

impl Local {
    /// Starts `waypipe client` here.
    ///
    /// # Errors
    ///
    /// Why it can't run: not Unix, no Wayland session here, `waypipe` not installed, or it didn't
    /// start listening.
    pub async fn start() -> Result<Self, String> {
        if !cfg!(unix) {
            return Err(
                "Waypipe needs a Wayland desktop here, which this system doesn't have".into(),
            );
        }
        if std::env::var_os("WAYLAND_DISPLAY").is_none() {
            return Err("this session isn't a Wayland desktop (WAYLAND_DISPLAY isn't set)".into());
        }
        let folder =
            std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
        let id = unique();
        let socket = folder.join(format!("opensesh-waypipe-{id}.sock"));
        let child = tokio::process::Command::new("waypipe")
            .arg("--socket")
            .arg(&socket)
            .arg("client")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    "waypipe isn't installed here (install the waypipe package)".to_owned()
                } else {
                    format!("waypipe didn't start: {error}")
                }
            })?;
        let local = Self {
            socket,
            remote: format!("/tmp/opensesh-waypipe-{id}.sock"),
            child,
        };
        // Listening within a few seconds, or not at all.
        for _ in 0..30 {
            if local.socket.exists() {
                return Ok(local);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Err("waypipe didn't start listening".to_owned())
    }

    /// Its socket here.
    #[must_use]
    pub fn socket(&self) -> &Path {
        &self.socket
    }

    /// The server's socket, forwarded here.
    #[must_use]
    pub fn remote(&self) -> &str {
        &self.remote
    }
}

/// Single-quotes `text` for a POSIX shell.
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// The session's command on the server with `waypipe server` around it: `command`, or the user's
/// login shell. It runs in `sh` whatever the user's shell is, with a private `XDG_RUNTIME_DIR`
/// when theirs is missing.
#[must_use]
pub fn server_command(remote_socket: &str, command: Option<&str>) -> String {
    let (inner, argument) = match command {
        Some(command) => (r#"sh -c "$1""#, format!(" sh {}", quote(command))),
        None => (r#""${SHELL:-/bin/sh}" -l"#, String::new()),
    };
    let script = format!(
        r#"d=; if [ ! -d "${{XDG_RUNTIME_DIR:-}}" ] || [ ! -w "${{XDG_RUNTIME_DIR:-}}" ]; then d=$(mktemp -d) || exit 1; XDG_RUNTIME_DIR=$d; export XDG_RUNTIME_DIR; fi; waypipe --socket {} --unlink-socket server -- {inner}; s=$?; if [ -n "$d" ]; then rm -rf "$d"; fi; exit $s"#,
        quote(remote_socket)
    );
    format!("sh -c {}{argument}", quote(&script))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_on_the_server() {
        let shell = server_command("/tmp/w.sock", None);
        assert!(shell.starts_with("sh -c 'd=; if [ ! -d"), "{shell}");
        assert!(
            shell.contains(r#"waypipe --socket '\''/tmp/w.sock'\'' --unlink-socket server -- "${SHELL:-/bin/sh}" -l; s=$?;"#),
            "{shell}"
        );
        let command = server_command("/tmp/w.sock", Some("gedit 'my file'"));
        assert!(
            command.contains(r#"server -- sh -c "$1"; s=$?;"#),
            "{command}"
        );
        assert!(
            command.ends_with(r#"exit $s' sh 'gedit '\''my file'\'''"#),
            "{command}"
        );
        assert_ne!(unique(), unique());
    }

    /// The command run by `sh` with a stand-in `waypipe` that runs what follows `--`.
    #[cfg(unix)]
    fn run_on_a_server(runtime: &Path, command: &str) -> String {
        use std::os::unix::fs::PermissionsExt;

        let bin = tempfile::tempdir().unwrap();
        let fake = bin.path().join("waypipe");
        std::fs::write(
            &fake,
            "#!/bin/sh\nwhile [ \"$1\" != -- ]; do shift; done; shift\necho \"runtime=$XDG_RUNTIME_DIR\"\nexec \"$@\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = format!(
            "{}:{}",
            bin.path().display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(server_command("/tmp/w.sock", Some(command)))
            .env("PATH", path)
            .env("XDG_RUNTIME_DIR", runtime)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        String::from_utf8(output.stdout).unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn a_missing_runtime_directory_is_made_for_the_session() {
        // The user's own directory is used when it is there.
        let runtime = tempfile::tempdir().unwrap();
        let text = run_on_a_server(runtime.path(), "echo \"it's\" 'here'");
        assert_eq!(
            text,
            format!("runtime={}\nit's here\n", runtime.path().display())
        );
        // Else a private one, removed when the session ends.
        let missing = runtime.path().join("missing");
        let text = run_on_a_server(&missing, "echo \"$XDG_RUNTIME_DIR\"");
        let mut lines = text.lines();
        let made = lines.next().unwrap().strip_prefix("runtime=").unwrap();
        assert_ne!(made, missing.display().to_string());
        assert_eq!(lines.next(), Some(made));
        assert!(!Path::new(made).exists(), "{made}");
    }
}
