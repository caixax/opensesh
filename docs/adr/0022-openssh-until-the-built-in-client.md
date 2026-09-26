# ADR 0022: SSH hosts connect through OpenSSH until the built-in client

- **Status:** accepted
- **Date:** 2026-09-26
- **Sprint:** 5

## Context

Sprint 5 brings saved hosts, quick connect, "connect in a split" and the CLI's `connect`, but the built-in SSH client (`russh`, `opensesh-ssh`) arrives in Sprint 7. PLAN §2 already lists the system `ssh` as an optional alternative backend (`[host.ssh] backend = "openssh"`). Without a way to connect, the Hosts view could only be tested against placeholders.

## Options

1. **Connect nothing yet:** save hosts, and show "arrives in Sprint 7".
2. **Run the system `ssh`** in a terminal pane (a PTY like a local shell), with the host's resolved options as arguments.
3. **Pull the SSH sprint forward.**

## Decision

Option 2 for every SSH host until Sprint 7, whatever its `backend` says; afterwards `openssh` stays as the opt-in backend.

- The resolved host becomes `ssh [-p port] [-i key] [-J jump,...] [-o ServerAliveInterval=N] [-C] [-A] [-X|-Y] [user@]address`; jump hosts that name saved hosts become their resolved `user@address:port`. Arguments go to the program directly (no shell); hosts, users, jump hosts and key files that start with `-` are refused before anything runs. "Copy the ssh command" gives the same line, quoted for a POSIX shell.
- A pane of kind `ssh` remembers its host (or its quick-connect target) in workspaces; its command is worked out again when it opens, so a restored workspace uses the host as it is now. The exit banner says the connection ended and offers Reconnect; if `ssh` isn't installed, it says so.
- OpenSSH reads `~/.ssh/config` and `known_hosts` itself, so host keys are checked by OpenSSH for now (PLAN §8's own verification comes with the built-in client).
- The smoke test never runs `ssh`: program sessions run the hermetic test shell there, and the test checks the command line instead.
- Other protocols are saved normally; connecting to them says which sprint brings them.

## Consequences

- The Hosts view, quick connect, splits and the CLI are usable end to end now, on Windows (OpenSSH ships with Windows 10 and 11) and on Linux.
- Behaviour follows the user's OpenSSH (its config, agent and prompts) until Sprint 7 switches the default to the built-in client.
- Passwords are typed into OpenSSH's own prompt; OpenSesh stores none (the vault arrives in Sprint 6).
