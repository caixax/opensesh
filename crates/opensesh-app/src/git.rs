//! The Git helper of the settings folder (Sprint 16): status, commit, pull and push with the
//! `git` program, only when the user asks (pull and push reach the network). Runs on a worker
//! thread; `git` never waits for a password here (`GIT_TERMINAL_PROMPT=0`): credentials come
//! from the user's Git setup (an SSH agent, a credential helper) or the command fails.

use std::path::Path;
use std::process::{Command, Output};

/// The repository's state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    /// The folder is (in) a Git repository.
    pub repo: bool,
    /// The branch, empty when detached.
    pub branch: String,
    /// The branch it follows, if any (`origin/main`).
    pub upstream: String,
    /// Commits here that aren't there.
    pub ahead: u32,
    /// Commits there that aren't here.
    pub behind: u32,
    /// Changed, new or deleted files.
    pub changes: u32,
}

fn git(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "")
        .env("LC_ALL", "C");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// What a command printed, or why it failed (its error output).
fn finish(output: std::io::Result<Output>) -> Result<String, String> {
    match output {
        Ok(output) if output.status.success() => {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
        }
        Ok(output) => {
            let error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            let out = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            Err(if error.is_empty() { out } else { error })
        }
        Err(error) => Err(format!("git could not run: {error}")),
    }
}

/// Runs `git -C dir args...`.
///
/// # Errors
///
/// Its error output when it fails or can't run.
pub fn run(dir: &Path, args: &[&str]) -> Result<String, String> {
    finish(git(dir).args(args).output())
}

/// Whether the `git` program is installed.
#[must_use]
pub fn available() -> bool {
    finish(git(Path::new(".")).arg("--version").output()).is_ok()
}

/// The repository's state (`repo: false` when the folder isn't in one).
#[must_use]
pub fn status(dir: &Path) -> Status {
    match run(dir, &["status", "--porcelain=v1", "--branch"]) {
        Ok(text) => parse_status(&text),
        Err(_) => Status::default(),
    }
}

/// `git status --porcelain=v1 --branch`'s output read.
#[must_use]
pub fn parse_status(text: &str) -> Status {
    let mut status = Status {
        repo: true,
        ..Status::default()
    };
    for line in text.lines() {
        let Some(branch) = line.strip_prefix("## ") else {
            if !line.trim().is_empty() {
                status.changes += 1;
            }
            continue;
        };
        // `main...origin/main [ahead 1, behind 2]`, `No commits yet on main`, `HEAD (no branch)`.
        let (names, counts) = match branch.split_once(" [") {
            Some((names, counts)) => (names, counts.trim_end_matches(']')),
            None => (branch, ""),
        };
        let names = names.strip_prefix("No commits yet on ").unwrap_or(names);
        match names.split_once("...") {
            Some((local, upstream)) => {
                local.clone_into(&mut status.branch);
                upstream.clone_into(&mut status.upstream);
            }
            None if names.starts_with("HEAD (") => {}
            None => names.clone_into(&mut status.branch),
        }
        for count in counts.split(", ") {
            if let Some(n) = count.strip_prefix("ahead ") {
                status.ahead = n.parse().unwrap_or(0);
            } else if let Some(n) = count.strip_prefix("behind ") {
                status.behind = n.parse().unwrap_or(0);
            }
        }
    }
    status
}

/// Commits every change of the folder with `message`.
///
/// # Errors
///
/// Git's error; `nothing to commit` when there is nothing.
pub fn commit(dir: &Path, message: &str) -> Result<String, String> {
    run(dir, &["add", "--all", "."])?;
    if run(dir, &["diff", "--cached", "--quiet"]).is_ok() {
        return Err("nothing to commit".to_owned());
    }
    run(dir, &["commit", "--quiet", "-m", message])
}

/// Brings the other computers' commits (a merge: conflicts are left as markers for the conflict
/// dialog).
///
/// # Errors
///
/// Git's error.
pub fn pull(dir: &Path) -> Result<String, String> {
    run(dir, &["pull", "--no-rebase", "--no-edit"])
}

/// Sends the commits here.
///
/// # Errors
///
/// Git's error.
pub fn push(dir: &Path) -> Result<String, String> {
    run(dir, &["push"])
}

/// Makes the folder a repository, with a `.gitignore` for OpenSesh's backups, temporary files
/// and lock (added to an existing one).
///
/// # Errors
///
/// Git's error, or when `.gitignore` can't be written.
pub fn init(dir: &Path) -> Result<String, String> {
    let out = run(dir, &["init", "--quiet"])?;
    let ignore = dir.join(".gitignore");
    let current = std::fs::read_to_string(&ignore).unwrap_or_default();
    if !current.contains(".opensesh.lock") {
        let mut text = current;
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(opensesh_core::sync::GIT_IGNORE);
        opensesh_core::fsutil::atomic_write(&ignore, text.as_bytes(), 0)
            .map_err(|error| error.to_string())?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_lines() {
        let status = parse_status(
            "## main...origin/main [ahead 1, behind 2]\n M hosts.toml\n?? themes/x.toml\n",
        );
        assert_eq!(
            status,
            Status {
                repo: true,
                branch: "main".to_owned(),
                upstream: "origin/main".to_owned(),
                ahead: 1,
                behind: 2,
                changes: 2,
            }
        );
        let fresh = parse_status("## No commits yet on main\n?? hosts.toml\n");
        assert_eq!((fresh.branch.as_str(), fresh.changes), ("main", 1));
        let detached = parse_status("## HEAD (no branch)\n");
        assert_eq!(detached.branch, "");
        let plain = parse_status("## work\n");
        assert_eq!(
            (plain.branch.as_str(), plain.upstream.as_str()),
            ("work", "")
        );
    }

    #[test]
    fn a_settings_repository() {
        if !available() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        assert!(!status(dir.path()).repo);
        init(dir.path()).unwrap();
        let ignore = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(ignore.contains("*.bak.*"));
        std::fs::write(dir.path().join("hosts.toml"), "schema_version = 1\n").unwrap();
        std::fs::write(dir.path().join("hosts.toml.bak.1"), "old").unwrap();
        let status = status(dir.path());
        assert!(status.repo);
        // The backup is ignored.
        assert_eq!(status.changes, 2);
        // A commit needs a name; the test's own, never the user's settings.
        run(dir.path(), &["config", "user.name", "Test"]).unwrap();
        run(
            dir.path(),
            &["config", "user.email", "test@example.invalid"],
        )
        .unwrap();
        run(dir.path(), &["config", "commit.gpgsign", "false"]).unwrap();
        // No hooks of the user's own setup run here.
        let hooks = dir.path().join(".git").join("no-hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        run(
            dir.path(),
            &["config", "core.hooksPath", &hooks.display().to_string()],
        )
        .unwrap();
        commit(dir.path(), "Settings").unwrap();
        assert_eq!(super::status(dir.path()).changes, 0);
        assert_eq!(
            commit(dir.path(), "Again"),
            Err("nothing to commit".to_owned())
        );
    }
}
