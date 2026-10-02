# Spike: the VNC client (Sprint 14)

**Question:** libvncclient, `vnc-rs`, or an RFB client of our own?

PLAN Sprint 14 asks for Raw, CopyRect, Tight, ZRLE and Hextile at least, VNC authentication, VeNCrypt/TLS if it can be done, scaling, the clipboard, a read-only mode and the SSH tunnel, and "done when it connects to TigerVNC, x11vnc and wayvnc".

## What the servers offer

- **TigerVNC** (`Xvnc`, `x0vncserver`): `SecurityTypes` defaults to `TLSVnc,VncAuth`. The full list is None, VncAuth, Plain, TLSNone, TLSVnc, TLSPlain, X509None, X509Vnc, X509Plain and the RSA-AES types (RA2, RA2ne, RA2_256, RA2ne_256). The X509 types need `X509Cert` and `X509Key`.
- **x11vnc:** no authentication unless a password is given (`-passwd`, `-rfbauth`: VncAuth); `-ssl` adds TLS.
- **wayvnc:** no authentication unless `enable_auth` is set, which needs `certificate_file`, `private_key_file` and a password (VeNCrypt with X509 and a user name and password); `rsa_private_key_file` adds RSA-AES; DES (VncAuth) only with `allow_broken_crypto`.
- **TLS without certificates** (TLSNone, TLSVnc, TLSPlain, "anonymous TLS") uses anonymous Diffie-Hellman cipher suites. rustls has none, and they protect against eavesdroppers only, not against someone in the middle. **The X509 types** are ordinary TLS: rustls on `ring`, with the server's certificate trusted on first use as for RDP (ADR 0034).

So **None, VncAuth and VeNCrypt's X509None, X509Vnc and X509Plain** reach all three servers in their usual setups (TigerVNC through VncAuth by default, or X509 when certificates are set; wayvnc with or without `enable_auth`). RSA-AES would add encryption without certificates; it can come later.

## The options

### libvncclient (LibVNCServer)

- C, GPL-2.0-or-later (compatible with ours, ADR 0001); the most complete client: every encoding, VeNCrypt and anonymous TLS through GnuTLS or OpenSSL, SASL.
- **The build:** CMake, with zlib, libjpeg(-turbo), libpng and a TLS library. Our Windows build has no CMake or NASM (ADR 0033 and ADR 0034 avoided them for the same reason). Linux distros package it (`libvncclient1`), Windows would need it built and shipped.
- **Bindings:** `libvnc-sys` 0.1.6 (bindgen over a CMake build) or our own; a callback API driven by its own loop.

### `vnc-rs` 0.6.0

- Rust, MIT or Apache-2.0, async on tokio, maintained (0.6.0 on 2026-09-21).
- **Read from its source** (`src/client/auth.rs`, `src/config.rs`): security types None and VncAuth only (the others are named, not implemented); encodings Raw, CopyRect, Tight, TRLE and ZRLE, the cursor, desktop size, last rectangle and extended desktop size. **No Hextile** ("obsolescent", by choice), **no VeNCrypt**, and Tight's JPEG rectangles are handed out undecoded.
- Its security negotiation is internal: VeNCrypt couldn't be added from outside.

### Our own client (`opensesh-vnc`)

- RFB is small and well documented (RFC 6143 for 3.8, the community `rfbproto` for the rest).
- **On crates the workspace already locks:** `des` 0.9.0 (VncAuth), `flate2` (ZRLE and Tight's zlib streams), `tokio-rustls` with `ring` (VeNCrypt X509). One new crate: a JPEG decoder for Tight (`zune-jpeg` 0.5.15, MIT, Apache-2.0 or Zlib, MSRV 1.77.1).
- **Tests:** an in-process RFB server is small too (the handshake, the security types and a few encodings), so the tests and the smoke test need no network, as for SSH, S3 and RDP.
- **Effort:** the handshake and five decoders (Hextile and Tight are the larger ones), a few days.

## Result

An RFB client of our own: it is the only option that covers every PLAN item (Hextile, VeNCrypt) without a C build, and it reuses what Sprint 13 built: the `FramebufferItem`, the remote desktop pane, the certificate store and the jump host tunnel. See ADR 0035.
