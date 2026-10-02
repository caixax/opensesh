//! The containers and pods that are running (PLAN Sprint 12), for the host editor and quick
//! connect: what `docker ps`, `podman ps` and `kubectl get pods` print as JSON. The commands run
//! on the caller's thread (call [`list`] from a background one) and are given 15 seconds.
//!
//! Entering a container is a local terminal running the command
//! [`opensesh_core::hosts::containers::command`] gives.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use opensesh_core::hosts::containers::Engine;
use serde_json::Value;

/// How long a listing may take.
const TIMEOUT: Duration = Duration::from_secs(15);

/// Where to list from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Docker's or Podman's running containers.
    Engine(Engine),
    /// Running pods.
    Kube {
        /// The kubeconfig context (`None`: the current one).
        context: Option<String>,
        /// The namespace (`None`: every namespace).
        namespace: Option<String>,
    },
}

/// A running container or pod.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Running {
    /// The container or pod name.
    pub name: String,
    /// The image (containers) or the namespace (pods).
    pub detail: String,
    /// Pods: the namespace.
    pub namespace: Option<String>,
    /// Pods: their containers.
    pub containers: Vec<String>,
}

/// The program and arguments that list `source`.
#[must_use]
pub fn arguments(source: &Source) -> (&'static str, Vec<String>) {
    let words = |words: &[&str]| words.iter().map(|word| (*word).to_owned()).collect();
    match source {
        Source::Engine(Engine::Docker) => ("docker", words(&["ps", "--format", "{{json .}}"])),
        Source::Engine(Engine::Podman) => ("podman", words(&["ps", "--format", "json"])),
        Source::Kube { context, namespace } => {
            let mut args: Vec<String> = Vec::new();
            if let Some(context) = context {
                args.extend(["--context".to_owned(), context.clone()]);
            }
            args.extend(words(&[
                "get",
                "pods",
                "--field-selector=status.phase=Running",
                "--request-timeout=10s",
                "-o",
                "json",
            ]));
            match namespace {
                Some(namespace) => args.extend(["--namespace".to_owned(), namespace.clone()]),
                None => args.push("--all-namespaces".to_owned()),
            }
            ("kubectl", args)
        }
    }
}

/// The containers in what `docker ps --format '{{json .}}'` (a JSON object a line) or
/// `podman ps --format json` (one JSON list) printed.
#[must_use]
pub fn parse_containers(output: &str) -> Vec<Running> {
    let objects: Vec<Value> = match serde_json::from_str::<Value>(output.trim()) {
        Ok(Value::Array(items)) => items,
        _ => output
            .lines()
            .filter_map(|line| serde_json::from_str(line.trim()).ok())
            .collect(),
    };
    let mut running: Vec<Running> = objects
        .iter()
        .filter_map(|object| {
            // Docker: "web" (several are comma-separated); Podman: ["web"].
            let name = match object.get("Names")? {
                Value::String(names) => names.split(',').next()?.trim().to_owned(),
                Value::Array(names) => names.first()?.as_str()?.trim().to_owned(),
                _ => return None,
            };
            (!name.is_empty()).then(|| Running {
                name,
                detail: object
                    .get("Image")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                namespace: None,
                containers: Vec::new(),
            })
        })
        .collect();
    running.sort_by(|a, b| a.name.cmp(&b.name));
    running
}

/// The running pods in what `kubectl get pods -o json` printed.
///
/// # Errors
///
/// When it isn't that JSON.
pub fn parse_pods(output: &str) -> Result<Vec<Running>, String> {
    let value: Value = serde_json::from_str(output.trim())
        .map_err(|error| format!("kubectl printed something else: {error}"))?;
    let items = value
        .get("items")
        .and_then(Value::as_array)
        .ok_or("kubectl printed no list of pods")?;
    let text = |value: Option<&Value>| value.and_then(Value::as_str).map(str::to_owned);
    let mut pods: Vec<Running> = items
        .iter()
        .filter(|item| text(item.pointer("/status/phase")).is_none_or(|phase| phase == "Running"))
        .filter_map(|item| {
            let name = text(item.pointer("/metadata/name"))?;
            let namespace = text(item.pointer("/metadata/namespace"));
            let containers = item
                .pointer("/spec/containers")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|container| text(container.get("name")))
                        .collect()
                })
                .unwrap_or_default();
            Some(Running {
                name,
                detail: namespace.clone().unwrap_or_default(),
                namespace,
                containers,
            })
        })
        .collect();
    pods.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));
    Ok(pods)
}

/// Lists `source` (blocks for as long as the command takes, at most 15 seconds).
///
/// # Errors
///
/// Why there is no list, for people: the program isn't installed, it failed (what it said), or
/// it gave no answer in time.
pub fn list(source: &Source) -> Result<Vec<Running>, String> {
    let (program, args) = arguments(source);
    let output = run(program, &args)?;
    match source {
        Source::Engine(_) => Ok(parse_containers(&output)),
        Source::Kube { .. } => parse_pods(&output),
    }
}

/// Made-up containers or pods, for screenshots and test runs (which never run these programs).
#[must_use]
pub fn samples(source: &Source) -> Vec<Running> {
    let container = |name: &str, image: &str| Running {
        name: name.to_owned(),
        detail: image.to_owned(),
        namespace: None,
        containers: Vec::new(),
    };
    let pod = |name: &str, namespace: &str, containers: &[&str]| Running {
        name: name.to_owned(),
        detail: namespace.to_owned(),
        namespace: Some(namespace.to_owned()),
        containers: containers.iter().map(|name| (*name).to_owned()).collect(),
    };
    match source {
        Source::Engine(_) => vec![
            container("postgres", "postgres:16"),
            container("redis", "redis:7-alpine"),
            container("web", "nginx:1.27"),
        ],
        Source::Kube { .. } => vec![
            pod("api-7d9f8c6b5-x2k4p", "shop", &["app", "envoy"]),
            pod("worker-5c4b9-qq7rt", "shop", &["worker"]),
        ],
    }
}

/// Runs `program` with `args`: its standard output, or why not.
fn run(program: &str, args: &[String]) -> Result<String, String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window flashing up.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!("{program} isn't installed here (or isn't on PATH)")
        } else {
            format!("{program} didn't start: {error}")
        }
    })?;
    // Both pipes are read on threads of their own: a full one would stop the program.
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::Builder::new()
            .name("opensesh-list".to_owned())
            .spawn(move || {
                let mut bytes = Vec::new();
                if let Some(mut pipe) = pipe {
                    let _ = pipe.read_to_end(&mut bytes);
                }
                bytes
            })
    };
    let stdout = child
        .stdout
        .take()
        .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>);
    let stderr = child
        .stderr
        .take()
        .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>);
    let readers = drain(stdout).and_then(|out| drain(stderr).map(|err| (out, err)));
    let (out, err) = match readers {
        Ok(readers) => readers,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("could not start a thread: {error}"));
        }
    };
    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{program} gave no answer in {} s",
                    TIMEOUT.as_secs()
                ));
            }
            Err(error) => return Err(format!("{program}: {error}")),
        }
    };
    let out = out.join().unwrap_or_default();
    let err = err.join().unwrap_or_default();
    if status.success() {
        Ok(String::from_utf8_lossy(&out).into_owned())
    } else {
        let said = String::from_utf8_lossy(&err);
        let line = said
            .lines()
            .map(str::trim)
            .rfind(|line| !line.is_empty())
            .unwrap_or("it failed");
        Err(format!("{program}: {line}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_lines_and_podman_lists() {
        let docker = r#"{"Command":"\"nginx -g\"","ID":"1a2b","Image":"nginx:1.27","Names":"web","Status":"Up 3 hours"}
{"Command":"\"redis\"","ID":"3c4d","Image":"redis:7","Names":"redis,cache","Status":"Up 2 days"}
"#;
        let names: Vec<(String, String)> = parse_containers(docker)
            .into_iter()
            .map(|running| (running.name, running.detail))
            .collect();
        assert_eq!(
            names,
            [
                ("redis".to_owned(), "redis:7".to_owned()),
                ("web".to_owned(), "nginx:1.27".to_owned())
            ]
        );
        let podman = r#"[{"Id":"9f","Image":"docker.io/library/postgres:16","Names":["db"],"State":"running"}]"#;
        assert_eq!(parse_containers(podman)[0].name, "db");
        assert!(parse_containers("").is_empty());
        assert!(parse_containers("Cannot connect to the Docker daemon").is_empty());
    }

    #[test]
    fn pods() {
        let output = r#"{"apiVersion":"v1","items":[
            {"metadata":{"name":"worker-1","namespace":"shop"},"spec":{"containers":[{"name":"worker"}]},"status":{"phase":"Running"}},
            {"metadata":{"name":"api-1","namespace":"shop"},"spec":{"containers":[{"name":"app"},{"name":"envoy"}]},"status":{"phase":"Running"}},
            {"metadata":{"name":"job-1","namespace":"batch"},"spec":{"containers":[{"name":"job"}]},"status":{"phase":"Succeeded"}}
        ],"kind":"List"}"#;
        let pods = parse_pods(output).unwrap_or_default();
        assert_eq!(pods.len(), 2);
        assert_eq!(pods[0].name, "api-1");
        assert_eq!(pods[0].namespace.as_deref(), Some("shop"));
        assert_eq!(pods[0].containers, ["app", "envoy"]);
        assert!(parse_pods("error: You must be logged in").is_err());
    }

    #[test]
    fn listing_commands() {
        assert_eq!(
            arguments(&Source::Kube {
                context: Some("prod".into()),
                namespace: None
            })
            .1
            .join(" "),
            "--context prod get pods --field-selector=status.phase=Running --request-timeout=10s \
             -o json --all-namespaces"
        );
        assert_eq!(arguments(&Source::Engine(Engine::Podman)).0, "podman");
    }

    #[test]
    fn a_program_that_isnt_there() {
        let error = run("opensesh-no-such-program", &[])
            .err()
            .unwrap_or_default();
        assert!(error.contains("isn't installed"), "{error}");
    }
}
