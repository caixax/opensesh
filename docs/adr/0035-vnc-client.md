# ADR 0035: The VNC client

- **Status:** accepted
- **Date:** 2026-10-02
- **Sprint:** 14

## Context

PLAN Sprint 14 asks for VNC in a tab:
- a spike and an ADR between libvncclient and a Rust crate;
- the encodings Raw, CopyRect, Tight, ZRLE and Hextile at least, VNC authentication, and VeNCrypt/TLS if it can be done;
- scaling, the clipboard, a read-only mode and the SSH tunnel;
- reusing Sprint 13's `FramebufferItem` and input.

"Done when": it connects to TigerVNC, x11vnc and wayvnc.

## Options

The spike (`spikes/vnc-client`) has the details.

1. **libvncclient** (LibVNCServer): C, GPL-2.0-or-later, the most complete. A CMake build with zlib, libjpeg, libpng and a TLS library, which the Windows build doesn't have (ADR 0033 and ADR 0034 avoided CMake for the same reason), and bindings to a callback API.
2. **`vnc-rs` 0.6.0:** Rust, MIT or Apache-2.0, async on tokio. Its source has the security types None and VNC authentication only, and no Hextile; VeNCrypt can't be added from outside, as its negotiation is internal.
3. **An RFB client of our own** (`opensesh-vnc`): RFB 3.3 to 3.8 is small and documented (RFC 6143 and the community `rfbproto`), and every crate it needs is already locked but one.

## Decision

**Our own client, `opensesh-vnc`,** in the app's workspace (no helper program: its dependencies fit the app's lock file).

### What it does

- **Versions:** 3.3, 3.7 and 3.8 (unknown ones as 3.3, newer as 3.8).
- **Security types:**
  - none;
  - VNC authentication (DES from `des` 0.9, already locked through `russh`): it only proves the password, the session isn't encrypted, and the pane's bar says so ("Not encrypted", from an `Unencrypted` event the session sends);
  - **VeNCrypt 0.2 with its X509 subtypes** (X509None, X509Vnc, X509Plain): TLS on rustls with `ring`, as the RDP helper's (no key logging, no resumption), and the server's certificate trusted on first use in `trusted_certificates.toml`, decided by the app before any password goes.
  - The anonymous TLS subtypes (TLSNone, TLSVnc, TLSPlain) need anonymous Diffie-Hellman, which rustls doesn't have and which can't tell the server from someone in the middle; a server that only offers them gets an explanation. RSA-AES (RA2) can come later.
- **Encodings,** asked for in this order: CopyRect, Tight (its zlib streams, the copy, palette and gradient filters, JPEG through `zune-jpeg`), ZRLE, Hextile, Raw; and the cursor, desktop size, extended desktop size and last rectangle pseudo-encodings, with Tight's JPEG quality and zlib level.
- **Pixels:** 32 bits, red in the low byte, so ZRLE's and Tight's 3-byte pixels are the first three bytes. The session keeps the whole desktop (CopyRect and the cursor need it) and hands out the rectangles that changed.
- **Input:** keys as X keysyms (Latin-1 and Unicode keysyms for characters, so the server's layout doesn't change what lands; a key goes up with the keysym it went down with), the pointer's buttons and the wheel as RFB's button mask.
- **The clipboard:** text both ways in Latin-1 (RFB's own); the extended clipboard (UTF-8) can come later.
- **Resizing:** the desktop follows the server's size; where the server takes SetDesktopSize (TigerVNC), the pane's size is asked for. A size asked for before the server said whether it can is kept until it does.
- **Read-only:** no keys, pointer or clipboard go to the server.
- **Limits:** desktops up to 8192 pixels a side, cursors up to 256, compressed rectangles up to 64 MiB, texts up to 1 MiB.

### How it fits the app

**The same messages as the RDP helper** (`opensesh-rdp-protocol`, ADR 0034): a VNC session is a task in the app (`opensesh_vnc::drive`) that reads the same control messages and answers with the same events, pixels, pointer pictures and clipboard text. So the remote desktop pane (`DesktopView`, its `FramebufferItem`, the questions in the overlay, the certificate store, the jump host's tunnel, reconnecting) is one for both protocols. The protocol gained a keysym control and VNC's connect options (read-only, quality, shared), which the RDP helper ignores.

### Tests

- **Unit tests:** the canvas, every decoder (zlib streams that go on across rectangles, Hextile's subrectangles, ZRLE's palettes and runs, Tight's filters), VNC authentication against an answer computed with OpenSSL's DES, VeNCrypt's negotiation, the keysyms, the messages.
- **An in-process RFB server** (`opensesh_vnc::testing`): every version and security type, a test certificate, its updates in Tight, ZRLE, Hextile and Raw in turn, the cursor, SetDesktopSize, the clipboard both ways. The session tests, the drive test, the smoke test and the screenshots use it.
- **CI:** TigerVNC (VNC authentication, and VeNCrypt X509Vnc with a certificate), x11vnc and wayvnc (VeNCrypt with a user name and password) on Ubuntu (`scripts/vnc-test-servers.sh`).

## Consequences

- One more protocol on crates the app already had, plus `zune-jpeg` (MIT, Apache-2.0 or Zlib) and `x509-cert` 0.3 (for the certificate's subject).
- Servers that only offer anonymous TLS or RSA-AES need VNC authentication allowed (TigerVNC's default list has it) or certificates.
- No H.264 (wayvnc's open-h264), no QEMU extended keys, no extended clipboard yet.
