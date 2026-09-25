//! Desktop environment / compositor detection, used to resolve the `auto` window decoration
//! mode (PLAN §5.3): on tiling compositors the window buttons are hidden.

use crate::config::Decorations;

/// What we know about the running desktop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopInfo {
    /// Best-effort desktop/compositor name (e.g. `Hyprland`, `KDE`, `GNOME`), empty if unknown.
    pub name: String,
    /// Whether it is a tiling compositor / window manager.
    pub tiling: bool,
}

/// Compositors and window managers that tile windows, as they appear in `XDG_CURRENT_DESKTOP`
/// (compared case-insensitively).
const TILING_DESKTOPS: &[&str] = &[
    "hyprland",
    "sway",
    "river",
    "niri",
    "i3",
    "bspwm",
    "dwm",
    "qtile",
    "awesome",
    "xmonad",
    "herbstluftwm",
    "leftwm",
    "spectrwm",
];

/// Socket variables that only tiling compositors set, with the name to report. Sway also sets
/// `I3SOCK`, so it is checked after `SWAYSOCK`. These variables can be stale (Hyprland and niri
/// export them to the systemd/D-Bus activation environment), so a socket only counts if it
/// exists.
const TILING_SOCKETS: &[(&str, &str)] = &[
    ("SWAYSOCK", "sway"),
    ("NIRI_SOCKET", "niri"),
    ("I3SOCK", "i3"),
];

/// Detects the desktop from environment variables. `env` returns a variable's value and
/// `path_exists` checks a socket path.
#[must_use]
pub fn detect(
    env: impl Fn(&str) -> Option<String>,
    path_exists: impl Fn(&str) -> bool,
) -> DesktopInfo {
    // Hyprland's variable is a signature, not a path; its socket lives under XDG_RUNTIME_DIR.
    if let Some(signature) = env("HYPRLAND_INSTANCE_SIGNATURE").filter(|s| !s.is_empty()) {
        let socket = env("XDG_RUNTIME_DIR")
            .map(|runtime| format!("{runtime}/hypr/{signature}/.socket.sock"));
        if socket.is_none_or(|socket| path_exists(&socket)) {
            return DesktopInfo {
                name: "Hyprland".to_owned(),
                tiling: true,
            };
        }
    }
    for (variable, name) in TILING_SOCKETS {
        if env(variable).is_some_and(|path| !path.is_empty() && path_exists(&path)) {
            return DesktopInfo {
                name: (*name).to_owned(),
                tiling: true,
            };
        }
    }
    let current = env("XDG_CURRENT_DESKTOP")
        .or_else(|| env("XDG_SESSION_DESKTOP"))
        .or_else(|| env("DESKTOP_SESSION"))
        .unwrap_or_default();
    // XDG_CURRENT_DESKTOP is a colon-separated list, e.g. "ubuntu:GNOME".
    let tiling = current.split(':').any(|part| {
        TILING_DESKTOPS
            .iter()
            .any(|tiling| part.eq_ignore_ascii_case(tiling))
    });
    let name = current
        .split(':')
        .next_back()
        .unwrap_or_default()
        .to_owned();
    DesktopInfo { name, tiling }
}

/// Detects the desktop of the current process.
#[must_use]
pub fn detect_current() -> DesktopInfo {
    detect(
        |name| std::env::var(name).ok(),
        |path| std::path::Path::new(path).exists(),
    )
}

/// Resolves the configured decoration mode for this desktop: `auto` becomes `none` on tiling
/// compositors (tabs stay, window buttons go) and `custom` elsewhere.
#[must_use]
pub fn effective_decorations(configured: Decorations, desktop: &DesktopInfo) -> Decorations {
    match configured {
        Decorations::Auto if desktop.tiling => Decorations::None,
        Decorations::Auto => Decorations::Custom,
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn detect_with(vars: &[(&str, &str)]) -> DesktopInfo {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        // Every socket "exists" except the ones under /stale.
        detect(
            |name| map.get(name).cloned(),
            |path| !path.starts_with("/stale"),
        )
    }

    #[test]
    fn tiling_compositors_are_detected() {
        assert!(detect_with(&[("HYPRLAND_INSTANCE_SIGNATURE", "abc")]).tiling);
        assert!(detect_with(&[("SWAYSOCK", "/run/user/1000/sway.sock")]).tiling);
        assert!(detect_with(&[("XDG_CURRENT_DESKTOP", "river")]).tiling);
        assert!(detect_with(&[("XDG_CURRENT_DESKTOP", "niri")]).tiling);
        assert!(detect_with(&[("XDG_CURRENT_DESKTOP", "Hyprland")]).tiling);
        assert!(detect_with(&[("XDG_SESSION_DESKTOP", "i3")]).tiling);
        assert_eq!(
            detect_with(&[("HYPRLAND_INSTANCE_SIGNATURE", "x")]).name,
            "Hyprland"
        );
    }

    #[test]
    fn floating_desktops_are_not_tiling() {
        for desktop in ["KDE", "GNOME", "ubuntu:GNOME", "XFCE", "X-Cinnamon", ""] {
            let info = detect_with(&[("XDG_CURRENT_DESKTOP", desktop)]);
            assert!(!info.tiling, "{desktop}");
        }
        assert_eq!(
            detect_with(&[("XDG_CURRENT_DESKTOP", "ubuntu:GNOME")]).name,
            "GNOME"
        );
        assert!(!detect_with(&[]).tiling);
    }

    #[test]
    fn empty_or_stale_sockets_are_ignored() {
        assert!(!detect_with(&[("SWAYSOCK", "")]).tiling);
        assert!(!detect_with(&[("NIRI_SOCKET", "/stale/niri.sock")]).tiling);
        assert!(
            !detect_with(&[
                ("HYPRLAND_INSTANCE_SIGNATURE", "abc"),
                ("XDG_RUNTIME_DIR", "/stale/run")
            ])
            .tiling
        );
        assert!(
            detect_with(&[
                ("HYPRLAND_INSTANCE_SIGNATURE", "abc"),
                ("XDG_RUNTIME_DIR", "/run/user/1000")
            ])
            .tiling
        );
    }

    #[test]
    fn auto_decorations_follow_the_desktop() {
        let tiling = DesktopInfo {
            name: "sway".into(),
            tiling: true,
        };
        let floating = DesktopInfo {
            name: "KDE".into(),
            tiling: false,
        };
        assert_eq!(
            effective_decorations(Decorations::Auto, &tiling),
            Decorations::None
        );
        assert_eq!(
            effective_decorations(Decorations::Auto, &floating),
            Decorations::Custom
        );
        assert_eq!(
            effective_decorations(Decorations::Native, &tiling),
            Decorations::Native
        );
    }
}
