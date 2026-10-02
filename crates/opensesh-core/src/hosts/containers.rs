//! Containers and pods as hosts (PLAN Sprint 12): a docker host's address is a container (run by
//! Docker or Podman), a kube host's a pod. Its pane is a local terminal running `docker exec -it`,
//! `podman exec -it` or `kubectl exec -it`, so their own configuration applies (the Docker
//! context, the kubeconfig).

use serde::{Deserialize, Serialize};

use super::{Host, Protocol};
use crate::command_line;

/// What runs a docker host's container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    /// Docker.
    #[default]
    Docker,
    /// Podman.
    Podman,
}

impl Engine {
    /// The program, and the quick-connect scheme.
    #[must_use]
    pub const fn program(self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
        }
    }
}

/// How a container or pod is entered.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerOptions {
    /// Docker hosts: Docker (unset) or Podman.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<Engine>,
    /// The shell to run, as a command line; unset starts bash where there is one, else sh.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    /// Kube hosts: the namespace (unset: the context's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// Kube hosts: the container in the pod (unset: the pod's default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pod_container: Option<String>,
    /// Kube hosts: the kubeconfig context (unset: the current one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

impl ContainerOptions {
    /// Whether nothing is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// What runs when no shell is set: bash where the container has it, else sh.
pub const SHELL_FALLBACK: &str =
    "if command -v bash >/dev/null 2>&1; then exec bash; else exec sh; fi";

/// Whether `name` can be a container, pod, namespace or container-in-a-pod name on a command
/// line and in a URL.
#[must_use]
pub fn is_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && !name.chars().any(|c| {
            c.is_whitespace() || c.is_control() || matches!(c, '/' | '?' | '#' | '&' | '@')
        })
}

/// Whether `context` can be a kubeconfig context on a command line (they may hold `/`, `:` and
/// `@`).
#[must_use]
pub fn is_context(context: &str) -> bool {
    !context.is_empty()
        && !context.starts_with('-')
        && !context.chars().any(|c| c.is_whitespace() || c.is_control())
}

fn set(value: Option<&String>) -> Option<&str> {
    value
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
}

/// The program and its arguments that open a shell in `host`, a docker or kube host.
///
/// # Errors
///
/// What is wrong with the host, for people.
pub fn command(host: &Host) -> Result<Vec<String>, String> {
    let name = host.address.trim();
    if !is_name(name) {
        return Err(format!("{name:?} isn't a container or pod name"));
    }
    let options = &host.container;
    let shell = match set(options.shell.as_ref()) {
        Some(line) => {
            let words = command_line::split(line).map_err(|error| format!("the shell: {error}"))?;
            if words
                .first()
                .is_some_and(|program| program.starts_with('-'))
            {
                return Err("the shell can't start with -".to_owned());
            }
            words
        }
        None => vec!["sh".to_owned(), "-c".to_owned(), SHELL_FALLBACK.to_owned()],
    };
    let word = |text: &str| text.to_owned();
    match host.protocol {
        Protocol::Docker => {
            let engine = options.engine.unwrap_or_default();
            let mut args = vec![
                word(engine.program()),
                word("exec"),
                word("-it"),
                word("--env"),
                word("TERM=xterm-256color"),
            ];
            if let Some(user) = set(host.user.as_ref()) {
                args.extend([word("--user"), word(user)]);
            }
            args.push(word(name));
            args.extend(shell);
            Ok(args)
        }
        Protocol::Kube => {
            let mut args = vec![word("kubectl")];
            if let Some(context) = set(options.context.as_ref()) {
                if !is_context(context) {
                    return Err(format!("{context:?} isn't a context name"));
                }
                args.extend([word("--context"), word(context)]);
            }
            args.extend([word("exec"), word("-it")]);
            if let Some(namespace) = set(options.namespace.as_ref()) {
                if !is_name(namespace) {
                    return Err(format!("{namespace:?} isn't a namespace"));
                }
                args.extend([word("--namespace"), word(namespace)]);
            }
            args.push(word(name));
            if let Some(container) = set(options.pod_container.as_ref()) {
                if !is_name(container) {
                    return Err(format!("{container:?} isn't a container name"));
                }
                args.extend([word("--container"), word(container)]);
            }
            args.push(word("--"));
            args.extend(shell);
            Ok(args)
        }
        other => Err(format!("a {} host isn't a container", other.as_str())),
    }
}

/// Field problems of a docker or kube host's options, as (field, code) pairs.
pub(super) fn check(options: &ContainerOptions, problems: &mut Vec<(&'static str, &'static str)>) {
    if set(options.namespace.as_ref()).is_some_and(|name| !is_name(name)) {
        problems.push(("container.namespace", "invalid"));
    }
    if set(options.pod_container.as_ref()).is_some_and(|name| !is_name(name)) {
        problems.push(("container.pod_container", "invalid"));
    }
    if set(options.context.as_ref()).is_some_and(|context| !is_context(context)) {
        problems.push(("container.context", "invalid"));
    }
    if set(options.shell.as_ref()).is_some_and(|line| {
        command_line::split(line).map_or(true, |words| {
            words.first().is_some_and(|word| word.starts_with('-'))
        })
    }) {
        problems.push(("container.shell", "invalid"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(protocol: Protocol, address: &str, container: ContainerOptions) -> Host {
        Host {
            protocol,
            address: address.to_owned(),
            container,
            ..Host::default()
        }
    }

    #[test]
    fn docker_and_podman() {
        let web = host(Protocol::Docker, "web", ContainerOptions::default());
        assert_eq!(
            command(&web),
            Ok(vec![
                "docker".to_owned(),
                "exec".to_owned(),
                "-it".to_owned(),
                "--env".to_owned(),
                "TERM=xterm-256color".to_owned(),
                "web".to_owned(),
                "sh".to_owned(),
                "-c".to_owned(),
                SHELL_FALLBACK.to_owned(),
            ])
        );
        let db = Host {
            user: Some("postgres".to_owned()),
            ..host(
                Protocol::Docker,
                "db",
                ContainerOptions {
                    engine: Some(Engine::Podman),
                    shell: Some("bash -l".to_owned()),
                    ..ContainerOptions::default()
                },
            )
        };
        assert_eq!(
            command(&db).map(|args| args.join(" ")),
            Ok("podman exec -it --env TERM=xterm-256color --user postgres db bash -l".to_owned())
        );
    }

    #[test]
    fn kubectl() {
        let pod = host(
            Protocol::Kube,
            "api-7d9f",
            ContainerOptions {
                namespace: Some("shop".to_owned()),
                pod_container: Some("app".to_owned()),
                context: Some("arn:aws:eks:eu-west-1:1:cluster/prod".to_owned()),
                shell: Some("/bin/ash".to_owned()),
                ..ContainerOptions::default()
            },
        );
        assert_eq!(
            command(&pod).map(|args| args.join(" ")),
            Ok(
                "kubectl --context arn:aws:eks:eu-west-1:1:cluster/prod exec -it --namespace shop \
                api-7d9f --container app -- /bin/ash"
                    .to_owned()
            )
        );
    }

    #[test]
    fn names_that_cant_be_used() {
        assert!(command(&host(Protocol::Docker, "-rm", ContainerOptions::default())).is_err());
        assert!(command(&host(Protocol::Docker, "a b", ContainerOptions::default())).is_err());
        assert!(command(&host(Protocol::Ssh, "web", ContainerOptions::default())).is_err());
        let bad_shell = ContainerOptions {
            shell: Some("-c 'x'".to_owned()),
            namespace: Some("a/b".to_owned()),
            context: Some("has space".to_owned()),
            ..ContainerOptions::default()
        };
        let mut problems = Vec::new();
        check(&bad_shell, &mut problems);
        assert_eq!(
            problems,
            [
                ("container.namespace", "invalid"),
                ("container.context", "invalid"),
                ("container.shell", "invalid"),
            ]
        );
    }
}
