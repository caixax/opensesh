# Import, export and sync

How to bring hosts from other programs, move them to another computer, and keep the settings of several computers in step with Git or Syncthing. Decisions behind it: [ADR 0037](adr/0037-importers-bundles-and-sync.md).

## Import

Hosts view, the "…" menu (or the command palette): **Import hosts…**. Choose where from; the dialog shows what it found and what it left out, and adds the hosts to a new group, with the other program's folders as groups inside it. Hosts you have already (same name, protocol and address) are skipped. Passwords are never imported.

| From | What to choose | What comes |
|---|---|---|
| MobaXterm | an `.mxtsessions` export (right-click "User sessions", "Export all sessions to file"), a `.moba` file, or `MobaXterm.ini` (`%APPDATA%\MobaXterm`, or next to a portable `MobaXterm.exe`) | SSH, SFTP, RDP and VNC sessions: host, port, user, SSH gateways (as jump hosts), the key, the proxy, agent forwarding, the comment as notes |
| PuTTY | on Windows, your saved sessions (the registry, the default) or a `.reg` file exported with `regedit /e putty.reg HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions` on another computer; on Linux, `~/.putty/sessions` | SSH, Telnet and serial sessions: host, port, user, key, proxy, agent and X11 forwarding, compression, remote command; the serial line's settings |
| Remmina | `~/.local/share/remmina` (or one `.remmina` file) | RDP, VNC, SSH and SFTP profiles: server, user, domain, SSH tunnel (as a jump host), view-only, clipboard, labels (as tags), notes |
| CSV file | any spreadsheet saved as CSV (comma, semicolon or tab) | what you map: name, address (`user@host:port` works), port, user, protocol, group (`Parent/Child`), tags, notes, key file, jump hosts |
| OpenSesh bundle | an `.opensesh` file | hosts and groups, snippets, profiles and themes, and with its export password the keychain |
| ~/.ssh/config | OpenSSH's config file | imported, or linked so it follows the file |

**PuTTY keys** (`.ppk`) are left out of the hosts with a note: import them in the Keychain view, which converts them, and pick them in the host.

## Export

Hosts view, "…", **Export…** (or Settings, Data and sync):

- **OpenSesh bundle:** your hosts and groups, snippets, terminal profiles and themes in one file, to import on another computer or keep as a backup. **Include the keychain** adds your identities and keys with their passwords, sealed with an export password of your choice (the same protection as the vault). Keep that file as safe as the password is strong.
- **OpenSSH config file:** your SSH hosts (or the selected ones, from a host's menu) as `Host` blocks for `ssh` and the tools that read `~/.ssh/config`. Keychain identities stay in OpenSesh: export the key from the Keychain view if `ssh` needs it.

## Sync between computers

Settings, **Data and sync**, **Settings folder**: choose a folder that Git or Syncthing syncs. OpenSesh uses it from its next start.

- **What lives there:** hosts and groups, profiles, themes, snippets, tunnels, shortcuts, highlight rules, settings, workspaces, known hosts and trusted certificates.
- **What stays on each computer:** the vault (passwords and keys), the keychain's list of identities and keys, logs, recordings and the window's state. Bring identities to another computer once with a bundle exported **with its keychain**; hosts synced afterwards find them.
- **Choosing a folder that already has settings** (from your other computer) lets you use them as they are, or add this computer's first (nothing there is overwritten).
- **"Use the default folder"** goes back, from the next start.

### Changes on two computers

When a file changed on disk since OpenSesh read it (Syncthing brought the other computer's version, or a `git pull`), saving merges both: hosts, groups, snippets and tunnels record by record, other settings value by value. If both computers changed the same value, this computer's is kept.

### Conflicts

When Syncthing keeps two versions of a file (`hosts.sync-conflict-….toml`) or a `git pull` leaves conflict markers, OpenSesh says so and lists the file under **Conflicts**. **Resolve…** shows what differs, by host, group or snippet, and lets you keep either side of each; what only one side has is kept unless you say otherwise. Resolving writes the file and removes Syncthing's copy.

### With Syncthing

Share the folder between your computers. Optionally add these lines to the folder's `.stignore`, so backups and temporary files stay local:

```
*.bak.*
.*.tmp-*
.*.bak-*
.opensesh.lock
```

### With Git

Under **Git**: **Make it a repository** (with a `.gitignore` for OpenSesh's own files), then add a remote with Git (`git remote add origin <url>`, then a first `git push -u origin main`). From then on, **Commit**, **Pull** and **Push** do what they say. Only these buttons reach the network. Git asks for no password inside OpenSesh: use an SSH agent or your credential helper, as for any repository.
