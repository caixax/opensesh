//! Detecting the remote operating system (PLAN Sprint 7) for the host's icon: one command on a
//! separate exec channel, never in the user's shell. Hosts can opt out.

use std::time::Duration;

use russh::ChannelMsg;

use crate::connect::Connection;

/// What runs on the server: the `ID` of `/etc/os-release` (Linux and the BSDs that have it), then
/// the kernel name (macOS, FreeBSD). A server without `sh` (Windows) fails, which says Windows.
pub const COMMAND: &str = "cat /etc/os-release 2>/dev/null; echo __uname__; uname -s 2>/dev/null";

/// How long the detection may take.
const TIMEOUT: Duration = Duration::from_secs(5);

/// The icon for what [`COMMAND`] printed (`os-debian`, `os-apple`...), `None` when unknown.
#[must_use]
pub fn icon_for(output: &str, succeeded: bool) -> Option<&'static str> {
    let (release, uname) = output.split_once("__uname__").unwrap_or((output, ""));
    let field = |name: &str| {
        release.lines().find_map(|line| {
            line.strip_prefix(name)
                .and_then(|rest| rest.strip_prefix('='))
                .map(|value| value.trim().trim_matches('"').to_ascii_lowercase())
        })
    };
    let ids: Vec<String> = field("ID")
        .into_iter()
        .chain(
            field("ID_LIKE")
                .map(|like| {
                    like.split_whitespace()
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        )
        .collect();
    for id in &ids {
        let icon = match id.as_str() {
            "debian" => "os-debian",
            "ubuntu" => "os-ubuntu",
            "fedora" => "os-fedora",
            "arch" | "archarm" => "os-archlinux",
            "manjaro" | "manjaro-arm" => "os-manjaro",
            "alpine" => "os-alpinelinux",
            "almalinux" => "os-almalinux",
            "rocky" => "os-rockylinux",
            "rhel" | "centos" => "os-redhat",
            "opensuse" | "opensuse-leap" | "opensuse-tumbleweed" | "suse" | "sles" => "os-opensuse",
            "gentoo" => "os-gentoo",
            "nixos" => "os-nixos",
            "linuxmint" => "os-linuxmint",
            "raspbian" => "os-raspberrypi",
            "freebsd" => "os-freebsd",
            _ => continue,
        };
        return Some(icon);
    }
    match uname.trim() {
        "Darwin" => Some("os-apple"),
        "FreeBSD" => Some("os-freebsd"),
        "Linux" => Some("os-linux"),
        _ if !succeeded && output.trim().is_empty() => None,
        _ if !succeeded => Some("os-windows"),
        _ => None,
    }
}

/// Runs [`COMMAND`] on `connection` and returns the icon, `None` when unknown or on any error.
pub async fn detect(connection: &Connection) -> Option<&'static str> {
    let run = async {
        let target = connection.target().ok()?;
        let mut channel = target.channel_open_session().await.ok()?;
        channel.exec(true, COMMAND).await.ok()?;
        let mut output = Vec::new();
        let mut status = None;
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                    if output.len() < 64 * 1024 {
                        output.extend_from_slice(&data);
                    }
                }
                ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        icon_for(&String::from_utf8_lossy(&output), status == Some(0))
    };
    tokio::time::timeout(TIMEOUT, run).await.ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn releases() {
        let debian = "PRETTY_NAME=\"Debian GNU/Linux 13 (trixie)\"\nID=debian\n__uname__\nLinux\n";
        assert_eq!(icon_for(debian, true), Some("os-debian"));
        let mint = "ID=linuxmint\nID_LIKE=\"ubuntu debian\"\n__uname__\nLinux\n";
        assert_eq!(icon_for(mint, true), Some("os-linuxmint"));
        let unknown_like_ubuntu = "ID=pop\nID_LIKE=\"ubuntu debian\"\n__uname__\nLinux\n";
        assert_eq!(icon_for(unknown_like_ubuntu, true), Some("os-ubuntu"));
        assert_eq!(icon_for("__uname__\nDarwin\n", true), Some("os-apple"));
        assert_eq!(
            icon_for("ID=\"rocky\"\n__uname__\nLinux\n", true),
            Some("os-rockylinux")
        );
        assert_eq!(
            icon_for("ID=weird\n__uname__\nLinux\n", true),
            Some("os-linux")
        );
        // cmd.exe doesn't know the command.
        assert_eq!(
            icon_for(
                "'cat' is not recognized as an internal or external command",
                false
            ),
            Some("os-windows")
        );
        assert_eq!(icon_for("", false), None);
    }
}
