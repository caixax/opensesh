# ADR 0029: Tunnels

- **Status:** accepted
- **Date:** 2026-09-27
- **Sprint:** 9

## Context

PLAN §5.4 and Sprint 9 ask for a clear port forwarding manager: local (`-L`), remote (`-R`) and
dynamic SOCKS5 (`-D`) forwards, a view with a state and a switch each, starting with the app,
tied to a host or independent, reconnection, traffic counters, duplicating, a warning when a
tunnel listens beyond localhost (PLAN §8), importing the forwards of `~/.ssh/config`, and the
command palette. The SSH client of Sprint 7 (ADR 0027) gives the connections.

## Options and decisions

### Forwarding

`russh` 0.63.3 has what is needed: `direct-tcpip` channels (local and dynamic), the
`tcpip-forward` global request and the server's `forwarded-tcpip` channels (remote). No new
crate. The SOCKS5 server is small (no authentication, `CONNECT`, IPv4, IPv6 and names) and is
written next to the SOCKS5 client the proxies use. Names are resolved by the server, like
`ssh -D` (and `curl --socks5-hostname`), so they resolve inside the network the tunnel reaches.

A remote forward's channels come to the client's handler, which only accepts them for a port
this client asked for (a route per port on the connection); others are refused.

### Tunnels on a connection that changes

The engine (`opensesh-ssh::tunnel`, no Qt) runs a forward on a `watch` of the connection
rather than on one connection. Whoever owns the connection replaces it when it is lost, and the
forward carries on: listeners here stay bound (a connection that arrives meanwhile waits up to
10 s), and remote forwards are asked for again on the new connection. The alternative, stopping
and starting the forward with each connection, would give the local port away while
reconnecting.

### Tied or independent

"Tied to a host or independent" (PLAN) is read as two ways to run:

- **Independent:** a connection of its own, through a saved host or `user@host` text, kept up
  with backoff (1, 2, 4, 8, 16, then every 30 s) when reconnecting is on. Errors that need the
  user (a refused key, a failed authentication, a cancelled question, a locked vault) stop it
  instead. Its questions wait in its row of the Tunnels view.
- **Tied:** it runs while a terminal session to its saved host is connected, on that session's
  connection (no second login), and stops when the last one ends: OpenSSH's `LocalForward`
  semantics. This is what the forwards imported from `~/.ssh/config` become.

The terminal registry tells the tunnel service when a session of a saved host is up or gone.

### Where the state lives

The tunnels run in a service of the app (`crate::tunnels`) rather than in the QML singleton,
because the terminal registry and the SSH runtime report to it from their own threads. The
`Tunnels` singleton shows the list as JSON (state, port, counts, a waiting question) and saves
`tunnels.toml` through the background writer; the file is reloaded when it changes on disk.
Whether a tunnel is switched on is not saved: `autostart` says what starts with the app.

### Listening beyond localhost

A tunnel whose listening address isn't a loopback one (`127.0.0.0/8`, `::1`, `localhost`) is
marked in the list, and the editor asks once before saving it: anyone on the network could use
it to reach the server's network (local, dynamic), or reach this computer through the server
(remote, when the server's `GatewayPorts` allows it). The default address is `127.0.0.1`.

### Test runs

The smoke test and the screenshots start without the user's tunnels, never write the file, and
connect only to the in-process test server, which now also serves `tcpip-forward`. A small HTTP
server of the test run lets the smoke test fetch a page through the tunnels.

## Consequences

- Every kind is covered with real traffic: in-process with our own clients and `curl`, and
  against OpenSSH with `curl`, including a tunnel that comes back after the server kills its
  session.
- A tied tunnel only listens while its host has a session; a user who wants the port always
  there makes it independent.
- A tunnel that can't listen (its port is taken) or whose remote forward the server refuses
  stops, with the reason in its row; switching it on again tries again.
- Forwarding Unix sockets (`-L port:/path`, `streamlocal`) and remote dynamic forwards
  (`RemoteForward port`) are not supported yet; the importer skips them with a warning.
