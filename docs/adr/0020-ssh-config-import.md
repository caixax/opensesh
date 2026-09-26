# ADR 0020: Reading `~/.ssh/config` with our own parser

- **Status:** accepted
- **Date:** 2026-09-26
- **Sprint:** 5

## Context

PLAN Sprint 5 asks to import `~/.ssh/config`, including `Include`, `ProxyJump`, `IdentityFile`, `Port` and `User`, skipping wildcard patterns with a warning, in two modes: linked read-only, or an imported copy. It asks for an ADR choosing between an existing crate and our own parser, and for fixtures.

## Options

1. **`ssh2-config`** (crates.io): parses the file and answers "what applies to host X", as OpenSSH does. It resolves settings per query rather than listing the `Host` blocks, which is what an import needs, and its handling of includes and of what it can't parse is outside our control.
2. **`ssh_config`** and similar small crates: parse lines into blocks, without `Include`.
3. **Our own parser**, reading only what the import uses, with OpenSSH's rules for those keywords.

## Decision

Option 3, in the new `opensesh-import` crate (`ssh_config` module), where Sprint 16's importers will join it.

- **What is read:** `Host` blocks with plain names (each name is a host), and in them `HostName` (with `%h`), `User`, `Port`, `IdentityFile` (all of them, in order) and `ProxyJump` (a list, or `none`). Keywords are case-insensitive, `Keyword=value` works, double quotes group arguments, `#` starts a comment. As in OpenSSH, the first value found for an option wins, and a name in several blocks takes what each block adds.
- **`Include`:** paths with `~` or `%d`, relative ones in `~/.ssh`, globs (`*`, `?`, sorted as glob(3) does, hidden files only with a leading dot); an `Include` inside a `Host` block adds to that block. At most 16 levels, and a file never includes itself in the same chain.
- **Skipped with a warning (file and line):** wildcard and negated patterns (`Host *`, `*.corp`, `!bastion`), `Match` blocks, options before the first `Host`, unreadable or missing files, bad ports, and names starting with `-`. Every other keyword is left alone: OpenSSH still reads the file when it connects (ADR 0022).
- **Linked mode** stores `[[source]] kind = "ssh_config", path = "~/.ssh/config"` in `hosts.toml`; the file and every file it includes are watched, and their hosts get `ssh_config:<name>` ids, so recent connections and jump references survive reloads. Linked hosts are read-only; "Duplicate" makes an editable copy. **Copy mode** adds the hosts to a new group, skipping names already saved.
- **Fixtures** (`crates/opensesh-import/tests/fixtures/home/.ssh/`) and unit tests cover includes by glob and inside a block, an include loop, a missing file, first-value-wins across files, `%h`, `ProxyJump none`, wildcard patterns and `Match`.

## Consequences

- The import lists exactly the hosts a user sees in their config, with the reason for anything left out.
- Options the parser ignores keep working for SSH hosts, because OpenSSH reads the same file; once the built-in client arrives (Sprint 7) it will read the options it supports from the resolved host.
- A future need (`Match` support, `CanonicalizeHostname`) extends our parser rather than waiting on a crate.
