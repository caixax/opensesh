# SSH against real servers

The SSH client (`crates/opensesh-ssh`) has three layers of tests:

- **Unit tests** in the crate: algorithm lists, proxy handshakes against fake proxies, the
  ProxyCommand parser, the session log's text cleaner, OS release parsing.
- **An in-process server** (`opensesh_ssh::testing`, `tests/server.rs`): every authentication
  method, new, changed and revoked host keys, keyboard-interactive, three hops, the agent,
  "install my key" and the terminal backend's reconnection. They run everywhere with
  `cargo test`, and the app's smoke test uses the same server.
- **Real servers** (`tests/real_servers.rs`): OpenSSH and Dropbear started by
  `scripts/ssh-test-servers.sh`. They are ignored by default.

## The servers

`scripts/ssh-test-servers.sh start` (as root) installs the packages (apt, pacman or dnf) and
starts, on 127.0.0.1 only, for the user `opensesh-test`:

| Port | Server   | Authentication                                     | Role             |
|------|----------|----------------------------------------------------|------------------|
| 2221 | OpenSSH  | public key or a user certificate; X11 forwarding   | first jump host  |
| 2222 | Dropbear | public key or password                             | second jump host |
| 2223 | OpenSSH  | public key, then a one-time code (TOTP, PAM)       | target with MFA  |
| 2224 | Dropbear | password                                           | password target  |

The servers use their own host keys and configuration files in `$OPENSESH_SSH_SERVERS`
(default `/tmp/opensesh-ssh-servers`); the system's sshd configuration is left alone. Two system
changes are made and undone by `stop`: the user `opensesh-test`, and a marked block at the top
of `/etc/pam.d/sshd` that asks that user, and only that user, for the one-time code.

## Running the tests

```sh
sudo scripts/ssh-test-servers.sh start
eval "$(ssh-agent -s)" && ssh-add /tmp/opensesh-ssh-servers/client_ed25519
cargo test -p opensesh-ssh --test real_servers -- --ignored --test-threads 1
sudo scripts/ssh-test-servers.sh stop
```

The tests cover:

- OpenSSH with a key file, Dropbear with a password, a wrong password refused;
- a key that isn't in `authorized_keys` getting in with its certificate (`<key>-cert.pub`, from
  a user CA the server trusts);
- agent forwarding and the environment: `ssh-add -l` on the server lists the local agent's key,
  and a variable the server accepts (`AcceptEnv`) arrives;
- OpenSSH → Dropbear → OpenSSH (two jump hosts), every hop authenticated by the agent, and the
  target asking for a one-time code after the key (the code comes from `oathtool`);
- the terminal backend through Dropbear to the MFA target: the session's `sshd-session` is
  killed, the pane says the connection was lost, Enter reconnects (with a new code), and
  `exit 0` ends the session with code 0.

CI runs them in the `ssh` job on Ubuntu 24.04. Locally they run in the `archlinux` WSL distro,
whose default user is root (Docker is not needed).
