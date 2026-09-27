//! What a folder listing holds: one [`Entry`] per file, sorting, and permissions as text.

use std::cmp::Ordering;

use opensesh_core::hosts::search::natural_cmp;

/// The kind of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A regular file.
    File,
    /// A folder.
    Dir,
    /// A symbolic link.
    Symlink,
    /// A device, a socket, a pipe...
    Other,
}

impl Kind {
    /// From the type bits of a POSIX mode.
    #[must_use]
    pub fn from_mode(mode: u32) -> Self {
        match mode & 0o170_000 {
            0o040_000 => Self::Dir,
            0o100_000 => Self::File,
            0o120_000 => Self::Symlink,
            _ => Self::Other,
        }
    }

    /// The name in the UI's data.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Dir => "dir",
            Self::Symlink => "symlink",
            Self::Other => "other",
        }
    }
}

/// One file of a listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The name in its folder.
    pub name: String,
    /// What it is (a link is `Symlink`; see `target_kind`).
    pub kind: Kind,
    /// Size in bytes.
    pub size: u64,
    /// Modified, in seconds since the Unix epoch.
    pub modified: Option<i64>,
    /// Permission bits (`0o7777`).
    pub mode: u32,
    /// Owner and group ids, when known.
    pub uid: Option<u32>,
    /// Group id.
    pub gid: Option<u32>,
    /// Where a link points.
    pub link_target: Option<String>,
    /// What a link points to (`None` when it is broken or unknown).
    pub target_kind: Option<Kind>,
}

impl Entry {
    /// A folder, or a link to one: opening it lists it.
    #[must_use]
    pub fn is_dir_like(&self) -> bool {
        self.kind == Kind::Dir || self.target_kind == Some(Kind::Dir)
    }

    /// Hidden by convention (a leading dot).
    #[must_use]
    pub fn is_hidden(&self) -> bool {
        self.name.starts_with('.')
    }
}

/// How a listing is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    /// Natural order of names (`file2` before `file10`).
    #[default]
    Name,
    /// Size.
    Size,
    /// Modification time.
    Modified,
    /// Kind, then extension.
    Kind,
    /// Permission bits.
    Permissions,
    /// Owner id.
    Owner,
}

impl SortKey {
    /// From the UI's name; unknown names sort by name.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text {
            "size" => Self::Size,
            "modified" => Self::Modified,
            "kind" => Self::Kind,
            "permissions" => Self::Permissions,
            "owner" => Self::Owner,
            _ => Self::Name,
        }
    }
}

fn extension(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => extension,
        _ => "",
    }
}

/// Sorts `entries` by `key`, folders first. Ties go by name, so the order is stable for any key.
pub fn sort(entries: &mut [Entry], key: SortKey, ascending: bool) {
    entries.sort_by(|a, b| {
        let folders = b.is_dir_like().cmp(&a.is_dir_like());
        if folders != Ordering::Equal {
            return folders;
        }
        let by_key = match key {
            SortKey::Name => Ordering::Equal,
            SortKey::Size => a.size.cmp(&b.size),
            SortKey::Modified => a.modified.cmp(&b.modified),
            SortKey::Kind => natural_cmp(extension(&a.name), extension(&b.name)),
            SortKey::Permissions => a.mode.cmp(&b.mode),
            SortKey::Owner => a.uid.cmp(&b.uid),
        };
        let ordering = by_key.then_with(|| natural_cmp(&a.name, &b.name));
        if ascending {
            ordering
        } else {
            ordering.reverse()
        }
    });
}

/// `drwxr-xr-x`, like `ls -l` (setuid, setgid and sticky included).
#[must_use]
pub fn permissions_text(kind: Kind, mode: u32) -> String {
    let mut text = String::with_capacity(10);
    text.push(match kind {
        Kind::Dir => 'd',
        Kind::Symlink => 'l',
        Kind::Other => '?',
        Kind::File => '-',
    });
    let bit = |mask: u32, yes: char| if mode & mask != 0 { yes } else { '-' };
    let exec = |exec: u32, special: u32, set: char, unset: char| match (
        mode & exec != 0,
        mode & special != 0,
    ) {
        (true, true) => set,
        (false, true) => unset,
        (true, false) => 'x',
        (false, false) => '-',
    };
    text.push(bit(0o400, 'r'));
    text.push(bit(0o200, 'w'));
    text.push(exec(0o100, 0o4000, 's', 'S'));
    text.push(bit(0o040, 'r'));
    text.push(bit(0o020, 'w'));
    text.push(exec(0o010, 0o2000, 's', 'S'));
    text.push(bit(0o004, 'r'));
    text.push(bit(0o002, 'w'));
    text.push(exec(0o001, 0o1000, 't', 'T'));
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, kind: Kind, size: u64) -> Entry {
        Entry {
            name: name.into(),
            kind,
            size,
            modified: Some(i64::try_from(size).unwrap_or(0)),
            mode: 0o644,
            uid: Some(1000),
            gid: Some(1000),
            link_target: None,
            target_kind: None,
        }
    }

    fn names(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.name.as_str()).collect()
    }

    #[test]
    fn folders_first_then_natural_names() {
        let mut list = vec![
            entry("file10", Kind::File, 1),
            entry("b", Kind::Dir, 0),
            entry("file2", Kind::File, 3),
            entry("a", Kind::Dir, 0),
            Entry {
                target_kind: Some(Kind::Dir),
                ..entry("link", Kind::Symlink, 0)
            },
        ];
        sort(&mut list, SortKey::Name, true);
        assert_eq!(names(&list), ["a", "b", "link", "file2", "file10"]);
        sort(&mut list, SortKey::Size, false);
        assert_eq!(names(&list), ["link", "b", "a", "file2", "file10"]);
        assert_eq!(SortKey::parse("modified"), SortKey::Modified);
        assert_eq!(SortKey::parse("nonsense"), SortKey::Name);
    }

    #[test]
    fn kinds_and_permissions() {
        assert_eq!(Kind::from_mode(0o040_755), Kind::Dir);
        assert_eq!(Kind::from_mode(0o120_777), Kind::Symlink);
        assert_eq!(Kind::from_mode(0o100_644), Kind::File);
        assert_eq!(Kind::from_mode(0o020_600), Kind::Other);
        assert_eq!(permissions_text(Kind::Dir, 0o755), "drwxr-xr-x");
        assert_eq!(permissions_text(Kind::File, 0o4755), "-rwsr-xr-x");
        assert_eq!(permissions_text(Kind::Dir, 0o1777), "drwxrwxrwt");
        assert_eq!(permissions_text(Kind::File, 0o2644), "-rw-r-Sr--");
        assert!(entry(".bashrc", Kind::File, 1).is_hidden());
    }
}
