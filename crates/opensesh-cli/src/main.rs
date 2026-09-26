//! `opensesh`: OpenSesh from the command line (PLAN Sprint 5, ADR 0021).
//!
//! `list` reads the saved hosts itself. `connect` and `open` hand the request to the running
//! OpenSesh over its local socket, or start OpenSesh with the request when none runs.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command-line tool answers on stdout and stderr"
)]

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use opensesh_core::AppPaths;
use opensesh_core::hosts::{HOSTS_FILE, Host, HostsFile, target};
use opensesh_core::ipc::{self, Endpoint, IpcError, Reply, Request};
use opensesh_import::ssh_config;

const USAGE: &str = "\
Usage: opensesh [COMMAND]

Commands:
  list [--json]      List the saved hosts (with the ones linked from ~/.ssh/config)
  connect <HOST>     Connect to a saved host, by name or id, in OpenSesh
  open <TARGET>      Connect to user@host:port, ssh://, rdp://, serial://... in OpenSesh
                     (OpenSesh asks before connecting)
  (none)             Start OpenSesh, or bring it to the front

Options:
  -V, --version      Print the version
  -h, --help         Print this help

OpenSesh starts when it isn't running; a running one takes the request.";

/// How long the running instance has to answer.
const TIMEOUT: Duration = Duration::from_secs(5);

fn main() -> ExitCode {
    match run(std::env::args_os().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("opensesh: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<OsString>) -> Result<()> {
    let args: Vec<String> = args
        .into_iter()
        .map(|arg| {
            arg.into_string()
                .map_err(|arg| anyhow::anyhow!("{arg:?} is not valid Unicode"))
        })
        .collect::<Result<_>>()?;
    let command = args.first().map(String::as_str);
    match command {
        None => deliver(Request::Activate, &[]),
        Some("-h" | "--help" | "help") => {
            println!("{USAGE}");
            Ok(())
        }
        Some("-V" | "--version") => {
            println!("opensesh {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("list") => {
            let json = match &args[1..] {
                [] => false,
                [flag] if flag == "--json" => true,
                other => bail!(
                    "unexpected {:?} (try: opensesh list [--json])",
                    other.join(" ")
                ),
            };
            list(json)
        }
        Some("connect") => {
            let [name] = &args[1..] else {
                bail!("give one host: opensesh connect <HOST>");
            };
            let (file, _) = load_hosts()?;
            let host = file.find_host(name).with_context(|| {
                format!("no saved host is called {name:?} (see: opensesh list)")
            })?;
            deliver(
                Request::Connect {
                    host: host.id.clone(),
                },
                &["--connect".to_owned(), host.id.clone()],
            )
        }
        Some("open") => {
            let text = args[1..].join(" ");
            let parsed = target::parse(&text).with_context(|| format!("{text:?}"))?;
            let url = parsed.to_string();
            deliver(
                Request::Open { url: url.clone() },
                &["--open".to_owned(), url],
            )
        }
        Some(other) => bail!("unknown command {other:?}\n\n{USAGE}"),
    }
}

/// The saved hosts, with the linked ones.
fn load_hosts() -> Result<(HostsFile, Vec<String>)> {
    let paths = AppPaths::resolve().context("could not find the OpenSesh folders")?;
    let path = paths.config_dir().join(HOSTS_FILE);
    let (mut file, warnings) = HostsFile::load(&path)?;
    let mut notes: Vec<String> = warnings.iter().map(ToString::to_string).collect();
    if !file.sources.is_empty() {
        let home = opensesh_core::paths::home_dir().unwrap_or_default();
        let linked = ssh_config::load_sources(&file.sources, &home);
        notes.extend(linked.warnings.iter().map(ToString::to_string));
        file.hosts.extend(linked.to_hosts(true, None));
    }
    Ok((file, notes))
}

fn address(file: &HostsFile, host: &Host) -> String {
    let resolved = file.resolve(host);
    let mut text = String::new();
    if let Some(user) = resolved.user() {
        text.push_str(user);
        text.push('@');
    }
    text.push_str(&target::bracket_ipv6(&host.address));
    if let Some(port) = resolved
        .port()
        .filter(|port| Some(*port) != host.protocol.default_port())
    {
        text.push(':');
        text.push_str(&port.to_string());
    }
    text
}

fn list(json: bool) -> Result<()> {
    let (file, notes) = load_hosts()?;
    let mut hosts: Vec<&Host> = file.hosts.iter().collect();
    hosts.sort_by(|a, b| opensesh_core::hosts::search::natural_cmp(&a.name, &b.name));
    if json {
        let list: Vec<serde_json::Value> = hosts
            .iter()
            .map(|host| {
                serde_json::json!({
                    "id": host.id,
                    "name": host.name,
                    "protocol": host.protocol.as_str(),
                    "address": host.address,
                    "target": address(&file, host),
                    "group": host.group.as_deref().map(|id| file.group_path(id)).unwrap_or_default(),
                    "tags": host.tags,
                    "favorite": host.favorite,
                    "linked": host.is_linked(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&list)?);
    } else if hosts.is_empty() {
        println!("No saved hosts. Add them in OpenSesh, or link ~/.ssh/config in the Hosts view.");
    } else {
        let rows: Vec<[String; 4]> = hosts
            .iter()
            .map(|host| {
                [
                    host.name.clone(),
                    host.protocol.as_str().to_owned(),
                    address(&file, host),
                    host.group
                        .as_deref()
                        .map(|id| file.group_path(id))
                        .unwrap_or_else(|| {
                            if host.is_linked() {
                                "~/.ssh/config".to_owned()
                            } else {
                                String::new()
                            }
                        }),
                ]
            })
            .collect();
        let header = ["NAME", "PROTOCOL", "TARGET", "GROUP"];
        let widths: Vec<usize> = (0..4)
            .map(|column| {
                rows.iter()
                    .map(|row| row[column].chars().count())
                    .chain(std::iter::once(header[column].len()))
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        let print = |cells: [&str; 4]| {
            let line: Vec<String> = cells
                .iter()
                .zip(&widths)
                .map(|(cell, width)| format!("{cell:<width$}"))
                .collect();
            println!("{}", line.join("  ").trim_end());
        };
        print(header);
        for row in &rows {
            print([&row[0], &row[1], &row[2], &row[3]]);
        }
    }
    for note in notes {
        eprintln!("note: {note}");
    }
    Ok(())
}

/// Gives `request` to the running OpenSesh, or starts OpenSesh with `start_args`.
fn deliver(request: Request, start_args: &[String]) -> Result<()> {
    let paths = AppPaths::resolve().context("could not find the OpenSesh folders")?;
    match ipc::send(&Endpoint::for_paths(&paths), &request, TIMEOUT) {
        Ok(Reply::Ok) => Ok(()),
        Ok(Reply::Error { message }) => bail!("OpenSesh refused: {message}"),
        Err(IpcError::NotRunning(_)) => start_app(start_args),
        Err(error) => Err(error.into()),
    }
}

/// The GUI next to this program or in the folder above (the Windows packages put the CLI in
/// `bin\` under `OpenSesh.exe`), else `opensesh-app` from `PATH`.
fn app_path() -> PathBuf {
    let names: &[&str] = if cfg!(windows) {
        &["OpenSesh.exe", "opensesh-app.exe"]
    } else {
        &["opensesh-app"]
    };
    let here = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from));
    let dirs: Vec<PathBuf> = here
        .into_iter()
        .flat_map(|dir| {
            let parent = dir.parent().map(PathBuf::from);
            std::iter::once(dir).chain(parent)
        })
        .collect();
    dirs.iter()
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from(names[names.len() - 1]))
}

fn start_app(args: &[String]) -> Result<()> {
    let app = app_path();
    Command::new(&app)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("could not start {}", app.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_args(args: &[&str]) -> Result<()> {
        run(args.iter().map(OsString::from).collect())
    }

    #[test]
    fn bad_commands_explain_themselves() {
        let error = run_args(&["frobnicate"]).unwrap_err().to_string();
        assert!(error.contains("unknown command"));
        let error = run_args(&["connect"]).unwrap_err().to_string();
        assert!(error.contains("give one host"));
        let error = run_args(&["open", "web:99999"]).unwrap_err();
        assert!(format!("{error:#}").contains("not a port"));
        let error = run_args(&["list", "--yaml"]).unwrap_err().to_string();
        assert!(error.contains("--json"));
        assert!(run_args(&["--version"]).is_ok());
        assert!(run_args(&["--help"]).is_ok());
    }
}
