# OpenSesh user guide

OpenSesh is one window for your remote work: local terminals, SSH with SFTP and tunnels, telnet, serial ports, mosh, containers, S3 storage, and remote desktops over RDP and VNC. This guide walks through it from the first start. Installing is covered in the [README](../README.md#install).

## Contents

- [The first start](#the-first-start)
- [The window](#the-window)
- [Hosts](#hosts)
- [Terminals](#terminals)
- [SSH](#ssh)
- [The keychain](#the-keychain)
- [Files: SFTP and S3](#files-sftp-and-s3)
- [Tunnels](#tunnels)
- [Snippets, macros and recordings](#snippets-macros-and-recordings)
- [Other connections](#other-connections)
- [Remote desktops](#remote-desktops)
- [The server monitor](#the-server-monitor)
- [Import, export and sync](#import-export-and-sync)
- [Settings](#settings)
- [Keyboard shortcuts](#keyboard-shortcuts)
- [The command line](#the-command-line)
- [Where OpenSesh keeps your data](#where-opensesh-keeps-your-data)
- [When something goes wrong](#when-something-goes-wrong)
- [Uninstalling](#uninstalling)

## The first start

With no saved hosts, the Hosts view offers four ways in, one click each:

- **Import hosts** from `~/.ssh/config`, MobaXterm, PuTTY, Remmina, a CSV file or an OpenSesh bundle;
- **New host**, to fill in a host yourself;
- **Local terminal**, a shell on this computer;
- **Quick connect**, to type an address and go.

OpenSesh never goes on the internet by itself: no telemetry, and the update check is off until you turn it on.

## The window

- **The rail** on the left switches between the views: **Hosts**, **Terminal**, **SFTP**, **Tunnels**, **Snippets**, **Keychain**, **History**, and **Settings** at the bottom.
- **Tabs** along the top hold terminals and remote desktops. Drop a tab outside the window to move it to another one (or "Move to a new window" in its menu). Right-click a tab to rename it, give it a color, pin it or duplicate it.
- **The command palette** (`Ctrl+Shift+P`) finds any action, host or tunnel by typing a few letters of its name.
- **The side panel** (`Ctrl+Shift+E`) shows the current session's files, the server's info and its tunnels next to the terminal.
- **The status bar** shows the current session, the server monitor, running tunnels and the notifications. Messages that pop up also stay in the notifications panel. When something fails, the message says what happened in plain words, and **Details** shows the technical text, which you can copy.

## Hosts

A host is a saved connection: an address, a user, a protocol, and how to log in.

- **Add one** with **Host** in the Hosts view, `Ctrl+Shift+P` then "New host", or from quick connect.
- **Organize** hosts in groups and subgroups, with tags and favorites. A group sets defaults for its hosts and subgroups (user, port, jump hosts, key file, terminal profile, SSH and SFTP options), which a host can override. Search finds hosts by name, address, user, tag or group from a few letters.
- **Quick connect** (`Ctrl+Shift+O`) takes what you would type in a terminal: `host`, `user@host`, `host:port`, `user@[::1]:2222`, `ssh deploy@web-01 -J bastion`, or a URL: `ssh://`, `sftp://`, `telnet://`, `mosh://`, `rdp://`, `vnc://`, `serial://COM3` or `serial:///dev/ttyUSB0?baud=115200`, `docker://name`, `podman://name`, `kube://namespace/pod`, `s3://key@host/bucket`. Enter opens it in a new tab, Shift+Enter in a pane to the right, Ctrl+Enter in a pane below.
- **History** lists your recent connections (a click connects again), your session recordings and the log folders.

## Terminals

- **Local shells:** `Ctrl+Shift+T` opens your default shell. The **+** menu next to the tabs lists every shell the computer has: PowerShell, cmd, Git Bash and the WSL distributions on Windows, the shells in `/etc/shells` on Linux.
- **Split** a tab into panes (`Alt+Shift+=` to the right, `Alt+Shift+-` down). Move between panes with `Alt+arrows`, resize them with `Alt+Shift+arrows`, and maximize one with `Ctrl+Shift+Z`.
- **Broadcast** (`Ctrl+Shift+B`) types into every pane of the tab at once; each pane can opt out.
- **Workspaces** save the tabs of every window under a name (their split layouts, each pane's profile and folder, tab names and colors), to open them again later with new shells in the same folders.
- **Copy and paste** with `Ctrl+Shift+C` and `Ctrl+Shift+V` ("Copy on select" in Settings > Terminal copies what you select). A paste that deserves a look is shown for review first, where it can be edited: several lines that would run at once, characters that hide what is seen (escape sequences, invisible or look-alike letters), or commands such as a download piped to a shell or `rm -rf /`.
- **Find** (`Ctrl+Shift+F`) searches the scrollback. `Ctrl+click` opens a link.
- **Make it yours** in Settings:
  - **Profiles:** font, size, cursor, scrollback and colors. Each host can use its own profile.
  - **Themes:** built in, or imported from iTerm2, Windows Terminal, Alacritty, kitty and base16 files.
  - **Keyword highlighting:** log levels, network addresses, status words, paths and URLs, and your own rules.

## SSH

OpenSesh has its own SSH client. Its questions (a new host key, a password, a one-time code) are asked inside the terminal pane, so other tabs keep working.

- **Host keys** are checked before anything is sent, on every hop. A new key is shown with its fingerprint for you to accept. A key that changed stops the connection.
- **Logging in:** a password, a key from the keychain, the keys of a running SSH agent (`SSH_AUTH_SOCK`, the Windows OpenSSH agent, Pageant), one-time codes, or a mix, as the server asks.
- **Jump hosts and proxies:** a host can go through one or several jump hosts, or a SOCKS or HTTP proxy.
- **Reconnection:** when a connection drops, OpenSesh tries again and says when. The pane keeps its scrollback.
- **Install your key on a server** from the host's menu in the Hosts view: it is added to the server's `authorized_keys`, and the password isn't needed after that.
- **The system's OpenSSH** can be used for a host instead, in the host's settings.
- **Remote graphical programs** show here through X11 forwarding or Waypipe: see [Remote graphical programs](remote-graphics.md).

## The keychain

The keychain holds identities (a user with a password or a key), passwords and SSH keys.

- **Where secrets live:** in an encrypted vault. Its key is kept in your system's keyring (Windows Credential Manager; on Linux the Secret Service: GNOME Keyring, or KWallet with its Secret Service interface on), or comes from a master password you choose in **Settings > Security**, which can also lock the vault after a while. Without a keyring, set a master password.
- **SSH keys:** generate Ed25519, ECDSA or RSA 4096 keys, import them from OpenSSH or PuTTY files, and export them. The keys of running SSH agents are listed too.
- **Known hosts:** the host keys you accepted, which you can check and remove.

Secrets are never written to disk in clear text, never logged, and never shown in error messages.

## Files: SFTP and S3

- **The SFTP view** has two panes, each of them this computer or a server. Drag files between them, or use the arrows. The transfer queue pauses, resumes and retries.
- **The side panel's Files tab** follows the terminal's current folder on the server.
- **Editing a server's file** opens it in your editor. When you save it, OpenSesh uploads it, and asks first if someone changed it on the server meanwhile.
- **Permissions, owners and file info** are in each file's menu.
- **S3 storage** (AWS, MinIO, RustFS and other S3 servers) is browsed the same way: multipart uploads, temporary links to share a file, and the secret key kept in the keychain.

## Tunnels

The Tunnels view lists your port forwards, with their traffic and state.

- **Local:** a port here reaches a port behind the server.
- **Remote:** a port on the server reaches one here.
- **Dynamic:** a SOCKS proxy here that goes out through the server.

A tunnel runs on its own connection (with reconnection) or rides along a host's terminal sessions. Tunnels can be imported from `~/.ssh/config`.

## Snippets, macros and recordings

- **Snippets** are saved commands with variables (`{{service}}`) that OpenSesh asks for, and secrets taken from the keychain. Run one in the current terminal or in several at once with `Ctrl+Shift+Space`.
- **Macros** are snippets in steps that wait for the server's output before going on (a prompt, a word).
- **Recordings:** a session can be recorded and played back later from History. The files are asciicast v2, so `asciinema play` opens them too.
- **Session logs:** Settings > SSH can keep a log of each session's output.

## Other connections

- **Telnet**, for routers and old devices.
- **Serial ports**, with the speed, data bits, parity, stop bits and flow control, and a hexadecimal view.
- **Mosh:** OpenSesh starts `mosh-server` over its SSH client, and `mosh-client` runs in the pane (it must be installed).
- **Containers:** a shell in a Docker or Podman container, or in a Kubernetes pod, through `docker`, `podman` or `kubectl`.

## Remote desktops

RDP and VNC desktops open in tabs. The bar above a desktop has Ctrl+Alt+Del, full screen and a menu (scaling, the keyboard, reconnecting, splitting: a local terminal next to the desktop).

- **RDP:** Network Level Authentication, the server's certificate checked like a host key, the clipboard both ways, a desktop that follows the pane's size, and jump hosts.
- **VNC:** VeNCrypt with certificates, the Tight, ZRLE and Hextile encodings, and view-only hosts.
- **The keyboard:** while the desktop has it, every key goes to the remote computer, OpenSesh's shortcuts included. **Ctrl+Alt+Home** gives it back.

## The server monitor

While a terminal is connected to a server over SSH, the status bar shows the server's CPU, memory, network, disk and load. It reads them every few seconds without installing anything there, on Linux (busybox too), FreeBSD and macOS. The side panel's **Info** tab has the details: the system, kernel, uptime, disks, addresses and logged-in users. **Settings > SSH** turns it off, or picks what is shown.

## Import, export and sync

- **Import** hosts from `~/.ssh/config` (linked or copied), MobaXterm, PuTTY, Remmina, CSV files and OpenSesh bundles. You see what will be added before anything changes.
- **Export** hosts as an OpenSesh bundle (hosts, profiles, themes and snippets; the keychain's secrets only if you choose, encrypted with an export password), or as an OpenSSH config.
- **Sync** your settings between computers through a folder synced by Syncthing, or a Git repository. Changes made on both computers are merged, not overwritten: see [Syncing settings between computers](sync.md).

## Settings

| Page | What is there |
|---|---|
| General | Language, start-up and closing, updates, the settings file |
| Appearance | Theme (system, dark or light), contrast, accent color, density, text size, the window's layout |
| Terminal | The profile being edited: font, colors, cursor, background, scrolling, selection and clipboard, shell, bell, paste checks, keyword highlighting |
| Profiles | Your terminal profiles |
| Themes | Terminal color themes: importing and exporting them |
| Shortcuts | Every shortcut, to change, clear or reset |
| SSH | The SSH client, authentication order, keepalive, reconnection, the server monitor, session logs, known hosts |
| SFTP | Transfers, the file panes, the editor for server files |
| Security | The vault, the master password, locking |
| Data and sync | Import and export, the settings folder, Git, conflicts |
| About | Version, system details, third-party notices |

**High contrast:** with Contrast on "System", OpenSesh follows your desktop's high-contrast setting (Qt 6.10 or newer). "High" turns it on always: stronger text, outlines and a thicker focus ring.

Settings are plain TOML files that you can read, back up and sync.

## Keyboard shortcuts

OpenSesh's shortcuts use `Ctrl+Shift` or `Alt` so that terminal programs keep their keys (`Ctrl+A`, `Ctrl+R`, `Ctrl+K`...). They can all be changed in **Settings > Shortcuts**.

| Shortcut | Action |
|---|---|
| `Ctrl+Shift+P` | Command palette |
| `Ctrl+Shift+O` | Quick connect |
| `Ctrl+Shift+T` | New local terminal tab |
| `Ctrl+Shift+D` | Duplicate tab |
| `Ctrl+Alt+Shift+T` | Reopen closed tab |
| `Ctrl+Tab`, `Ctrl+Shift+Tab` | Switch to the previously used tab, the least recently used |
| `Ctrl+PgDown`, `Ctrl+PgUp` | Next tab, previous tab |
| `Ctrl+Shift+PgUp`, `Ctrl+Shift+PgDown` | Move tab left, right |
| `Alt+1` ... `Alt+9` | Go to tab 1 to 9 |
| `Alt+Shift+=`, `Alt+Shift+-` | Split right, split down |
| `Alt+arrows`, `Alt+Shift+arrows` | Focus, resize panes (with several panes) |
| `Ctrl+Shift+Z` | Maximize pane, restore |
| `Ctrl+Shift+B` | Broadcast input to all panes |
| `Ctrl+Shift+W` | Close pane |
| `Ctrl+Shift+C`, `Ctrl+Shift+V` | Copy, paste |
| `Ctrl+Shift+F` | Find in terminal |
| `Ctrl+=`, `Ctrl+-`, `Ctrl+0` | Terminal text bigger, smaller, reset |
| `Ctrl+Shift+Space` | Run a snippet |
| `Ctrl+Shift+E` | Side panel |
| `Ctrl+,` | Settings |
| `Ctrl+Alt+Home` | Give the keyboard back to OpenSesh (from a remote desktop) |
| `F11` | Full screen |
| `F6`, `Shift+F6` (or `Ctrl+F6`, `Ctrl+Shift+F6`) | Move the focus between the window's regions |
| `Ctrl+Shift+Q` | Quit |

## The command line

`opensesh` works with the running OpenSesh, and starts it when needed:

```sh
opensesh list [--json]                  # saved hosts, with the ones linked from ~/.ssh/config
opensesh connect web-01                 # connect to a saved host, by name or id
opensesh open deploy@10.0.1.21:2222     # quick connect (also ssh://, rdp://...); OpenSesh asks first
```

On Windows it is `bin\opensesh.exe` in the install or portable folder: add that `bin` folder to `PATH` to use it anywhere. Starting OpenSesh while it runs brings the open window to the front instead of opening a second one.

## Where OpenSesh keeps your data

| | Linux | Windows | Portable |
|---|---|---|---|
| Settings, hosts, snippets, tunnels | `~/.config/opensesh` | `%APPDATA%\OpenSesh` | `data\` next to `OpenSesh.exe` |
| The vault, logs, recordings | `~/.local/share/opensesh` | `%LOCALAPPDATA%\OpenSesh` | `data\` |
| Cache | `~/.cache/opensesh` | `%LOCALAPPDATA%\OpenSesh\cache` | `data\cache` |

The settings folder can be moved (to a synced folder, for example) in **Settings > Data and sync**. Files are replaced in one step, so a crash never leaves half a file, and the settings files keep backups of their previous versions.

## When something goes wrong

- **The message's Details** show the technical text, which you can copy into a bug report.
- **Logs** are in the logs folder: History has a button that opens it. They never contain passwords or keys.
- **If OpenSesh crashes,** a dialog shows the crash report (also saved next to the logs), for you to read and, if you want, attach to an issue. Nothing is sent by itself.
- **Linux, no keyring:** without GNOME Keyring, or KWallet with its Secret Service interface on, set a master password in Settings > Security.
- **Linux, Wayland:** install Qt's Wayland plugin (`qt6-wayland`) for native Wayland windows. The install script does it for you.
- **Windows SmartScreen** may warn the first time, since the packages aren't code-signed yet.
- Bugs and ideas: [github.com/caixax/opensesh/issues](https://github.com/caixax/opensesh/issues).

## Uninstalling

- **Windows:** from "Installed apps" (the installer), or delete the portable folder.
- **Linux:** `curl -fsSL https://raw.githubusercontent.com/caixax/opensesh/main/install.sh | bash -s -- --uninstall`, or remove the `opensesh` package with your package manager.

Your settings and data stay where the table above says, in case you come back. Delete those folders to remove them too.
