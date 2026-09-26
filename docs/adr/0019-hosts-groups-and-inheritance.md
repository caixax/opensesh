# ADR 0019: Saved hosts, nested groups, inheritance and search

- **Status:** accepted
- **Date:** 2026-09-26
- **Sprint:** 5

## Context

PLAN Sprint 5 asks for hosts and nested groups with tags, favorites, color, icon, markdown notes and group defaults, in `hosts.toml` (§4.3) with hot reload; a host editor that shows inherited values as "inherited from <group>"; fuzzy search with `nucleo` on name, address, user and tags under 16 ms for 1000 hosts (§9); and recent connections. §3.4 describes the `ResolvedHost` as global defaults, then the nested groups, then the host, then the tab. Secrets go to the vault in Sprint 6 and must never reach `hosts.toml`.

## Options

1. **A generic key-value store per host**, the UI deciding what the keys mean. Flexible, but nothing checks the file, and inheritance needs a schema anyway.
2. **Typed structs with serde**, each inheritable field an `Option` (unset means inherited), groups holding the same fields as `[group.defaults]`.
3. **Flattened hosts** (every value copied from the group when saving). Simple to read back, but a group change doesn't reach its hosts.

For the search: `nucleo` (the high-level crate, a background worker with its own thread pool, meant for huge lists that update while matching) or `nucleo-matcher` (the same project's matcher and pattern parser, synchronous).

## Decision

Option 2, with `nucleo-matcher`.

- **Model** (`opensesh-core::hosts`): `Group` (id, name, `parent`, color, notes, `defaults`) and `Host` (id, name, group, protocol, address, port, user, `identity_file`, `jump`, profile, tags, favorite, color, icon, notes, `[host.ssh]`, `[host.sftp]`, `[host.serial]`, `[host.terminal]`). Ids are ULIDs. Keys this version doesn't know are kept and written back. Colors are tab color names (ADR 0017) or `#RRGGBB`.
- **Loading is lenient per entry:** an entry that doesn't parse is skipped with a warning; a missing or repeated id gets a new one; references to unknown groups and parent loops are cut; invalid ports are dropped; tags are cleaned. A file that isn't TOML, or that a newer OpenSesh wrote, is shown but never saved over (like `config.toml`, ADR 0007).
- **Inheritance:** the inheritable keys (user, port, jump, identity file, profile, and the SSH and SFTP options) resolve from the host, else the nearest group in its chain that sets them, else a built-in default (the protocol's port, keepalive 30 s, forwarding off...). Each resolved value carries its origin (`Default`, `Group(id)`, `Host`); the editor shows it as "deploy (from Production)" and an empty field means inheriting. An empty jump list is an explicit "no jump hosts". Terminal options join the profile chain of ADR 0016: each group from the outermost (its profile and `[terminal]` table), then the host, then the pane's own profile if the user picked one.
- **Validation** returns field codes (`required`, `invalid`, `unknown`) that QML words; hosts and users starting with `-` are refused, since `ssh` would read them as options.
- **Search:** each host is one line (name, address, user, tags, group path) matched by `nucleo-matcher`'s `Pattern`, so several words can match different fields; a name match, the text found as typed, and an exact name or address rank higher. Filters (scope, protocol, tag) and orders (name with numbers in numeric order, address, recently used, group) apply without text. With 1000 hosts the slowest query takes under 1 ms in release builds (`docs/perf.md`), so the synchronous matcher is enough and no background worker is needed; `nucleo`'s worker can replace it if lists grow by orders of magnitude.
- **Recent connections** go to `recent.toml` in the data folder (the last 30, saved hosts and quick-connect targets).
- **In the app**, the `Hosts` singleton publishes an immutable library (like the terminal profiles) that terminals read for their host's levels; each host's summary JSON is built once per change, so a search is the match plus a join.

## Consequences

- A group change reaches every host that doesn't set the value itself, and the editor always says where a value comes from.
- `hosts.toml` stays readable and diffable: only what is set is written.
- The search budget has two orders of magnitude of headroom; the QML side (parsing the result list) is the larger cost and stays under a frame for 1000 hosts.
- Passwords and `vault:` identities wait for Sprint 6; until then a host names a key file.
