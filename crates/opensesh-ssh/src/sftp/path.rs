//! Paths: POSIX on servers (SFTP always uses `/`), native on this computer. Paths are strings
//! everywhere so the views and the transfers handle both alike. On Windows the local root, above
//! the drives, is the empty path.

use std::path::{Component, Path, PathBuf};

/// How a file system writes paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// A server: `/` separated.
    Posix,
    /// This computer.
    Local,
}

/// `/a//b/./c/../d` becomes `/a/b/d`. `..` above the root stays at the root.
#[must_use]
pub fn normalize_posix(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|last| *last != "..") {
                    parts.pop();
                } else if !absolute {
                    parts.push("..");
                }
            }
            other => parts.push(other),
        }
    }
    let joined = parts.join("/");
    match (absolute, joined.is_empty()) {
        (true, _) => format!("/{joined}"),
        (false, true) => ".".to_owned(),
        (false, false) => joined,
    }
}

/// `name` inside `dir`.
#[must_use]
pub fn join(style: Style, dir: &str, name: &str) -> String {
    match style {
        Style::Posix => {
            if dir.is_empty() {
                name.to_owned()
            } else if dir.ends_with('/') {
                format!("{dir}{name}")
            } else {
                format!("{dir}/{name}")
            }
        }
        Style::Local => {
            // A drive picked at the root (Windows): `C:` becomes `C:\`.
            if dir.is_empty() && cfg!(windows) && name.ends_with(':') {
                return format!("{name}\\");
            }
            PathBuf::from(dir).join(name).display().to_string()
        }
    }
}

/// The folder holding `path`; `None` at the top.
#[must_use]
pub fn parent(style: Style, path: &str) -> Option<String> {
    match style {
        Style::Posix => {
            let normal = normalize_posix(path);
            if normal == "/" || normal == "." {
                return None;
            }
            match normal.rsplit_once('/') {
                Some(("", _)) => Some("/".to_owned()),
                Some((dir, _)) => Some(dir.to_owned()),
                None => Some(".".to_owned()),
            }
        }
        Style::Local => {
            if path.is_empty() {
                return None;
            }
            match Path::new(path).parent() {
                Some(dir) => Some(dir.display().to_string()),
                // A drive root on Windows goes up to the list of drives.
                None if cfg!(windows) => Some(String::new()),
                None => None,
            }
        }
    }
}

/// The last component (the whole path when it has none, as `/`).
#[must_use]
pub fn file_name(style: Style, path: &str) -> String {
    match style {
        Style::Posix => {
            let normal = normalize_posix(path);
            normal
                .rsplit('/')
                .find(|part| !part.is_empty())
                .unwrap_or(normal.as_str())
                .to_owned()
        }
        Style::Local => Path::new(path).file_name().map_or_else(
            || path.to_owned(),
            |name| name.to_string_lossy().into_owned(),
        ),
    }
}

/// The breadcrumbs of `path`: each ancestor as (label, path), the root first. The computer's
/// root on Windows has an empty label and path (the UI words it).
#[must_use]
pub fn crumbs(style: Style, path: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    match style {
        Style::Posix => {
            let normal = normalize_posix(path);
            let mut current = String::new();
            if normal.starts_with('/') {
                out.push(("/".to_owned(), "/".to_owned()));
                current.push('/');
            }
            for part in normal.split('/').filter(|part| !part.is_empty()) {
                current = join(Style::Posix, &current, part);
                out.push((part.to_owned(), current.clone()));
            }
        }
        Style::Local => {
            if cfg!(windows) {
                out.push((String::new(), String::new()));
            }
            let mut current = PathBuf::new();
            for component in Path::new(path).components() {
                current.push(component.as_os_str());
                let label = match component {
                    Component::RootDir if !cfg!(windows) => "/".to_owned(),
                    Component::RootDir => continue,
                    Component::Prefix(prefix) => prefix.as_os_str().to_string_lossy().into_owned(),
                    other => other.as_os_str().to_string_lossy().into_owned(),
                };
                let mut shown = current.display().to_string();
                // `C:` alone means "the current folder of drive C"; the crumb is its root.
                if matches!(component, Component::Prefix(_)) {
                    shown.push('\\');
                    current.push("\\");
                }
                out.push((label, shown));
            }
        }
    }
    out
}

/// A name that doesn't collide: `name (1).ext`, `name (2).ext`...
#[must_use]
pub fn numbered(name: &str, n: u32) -> String {
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => format!("{stem} ({n}).{extension}"),
        _ => format!("{name} ({n})"),
    }
}

/// `text` quoted for a POSIX shell (single quotes).
#[must_use]
pub fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posix_paths() {
        assert_eq!(normalize_posix("/a//b/./c/../d/"), "/a/b/d");
        assert_eq!(normalize_posix("/../x"), "/x");
        assert_eq!(normalize_posix("a/../.."), "..");
        assert_eq!(normalize_posix(""), ".");
        assert_eq!(join(Style::Posix, "/", "etc"), "/etc");
        assert_eq!(join(Style::Posix, "/etc", "ssh"), "/etc/ssh");
        assert_eq!(parent(Style::Posix, "/etc/ssh"), Some("/etc".into()));
        assert_eq!(parent(Style::Posix, "/etc"), Some("/".into()));
        assert_eq!(parent(Style::Posix, "/"), None);
        assert_eq!(file_name(Style::Posix, "/etc/ssh/"), "ssh");
        assert_eq!(file_name(Style::Posix, "/"), "/");
        assert_eq!(
            crumbs(Style::Posix, "/home/me"),
            vec![
                ("/".to_owned(), "/".to_owned()),
                ("home".to_owned(), "/home".to_owned()),
                ("me".to_owned(), "/home/me".to_owned()),
            ]
        );
    }

    #[test]
    fn names_and_quotes() {
        assert_eq!(numbered("report.pdf", 2), "report (2).pdf");
        assert_eq!(numbered(".bashrc", 1), ".bashrc (1)");
        assert_eq!(numbered("folder", 3), "folder (3)");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }

    #[cfg(windows)]
    #[test]
    fn windows_paths() {
        assert_eq!(join(Style::Local, "", "C:"), r"C:\");
        assert_eq!(join(Style::Local, r"C:\Users", "me"), r"C:\Users\me");
        assert_eq!(parent(Style::Local, r"C:\Users"), Some(r"C:\".into()));
        assert_eq!(parent(Style::Local, r"C:\"), Some(String::new()));
        assert_eq!(parent(Style::Local, ""), None);
        let crumbs = crumbs(Style::Local, r"C:\Users\me");
        let paths: Vec<&str> = crumbs.iter().map(|(_, path)| path.as_str()).collect();
        assert_eq!(paths, ["", r"C:\", r"C:\Users", r"C:\Users\me"]);
    }

    #[cfg(unix)]
    #[test]
    fn unix_paths() {
        assert_eq!(join(Style::Local, "/home", "me"), "/home/me");
        assert_eq!(parent(Style::Local, "/home"), Some("/".into()));
        assert_eq!(parent(Style::Local, "/"), None);
        let crumbs = crumbs(Style::Local, "/home/me");
        let paths: Vec<&str> = crumbs.iter().map(|(_, path)| path.as_str()).collect();
        assert_eq!(paths, ["/", "/home", "/home/me"]);
    }
}
