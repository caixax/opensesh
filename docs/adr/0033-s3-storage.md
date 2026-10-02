# ADR 0033: S3 storage

- **Status:** accepted
- **Date:** 2026-10-02
- **Sprint:** 12

## Context

The owner asked for a new connection kind, S3, for AWS and compatible servers such as their own RustFS:
- **Fields:** an endpoint, a region (`us-east-1` by default), an access key, a secret key, and path-style addressing (on by default).
- **The SDK:** the official AWS SDK.
- **Browsing:** buckets, then folders inside them through `ListObjectsV2` with the `/` delimiter.
- **Operations:**
  - upload, multipart in 32 MB parts;
  - download, delete, rename (copy and delete);
  - new folder (an empty object ending in `/`);
  - temporary links (presigned URLs).
- **The secret key** is stored encrypted, never in clear.

The files view (ADR 0028) shows any `Fs` and copies between any two through the transfer queue.

## Options and decisions

### The SDK and its TLS

**Decision:** `aws-sdk-s3` 1.122.0, the newest that builds with Rust 1.89 (1.123 needs 1.91), without its default features:
- **TLS:** rustls on `ring`, through `aws-smithy-http-client` 1.1.9 (`rustls-ring`), as the SSH client uses ring. The default TLS is `aws-lc`, which needs CMake and NASM on Windows.
- **No `aws-config`:** credentials come only from OpenSesh's vault, never from `~/.aws`, the environment or instance metadata, so nothing is read or sent that the user didn't set.

**Checksums:** only sent where the API requires them (`WhenRequired`). The SDK's newer default (checksums on every upload) is refused by several S3-compatible servers.

**Timeouts:** 15 s to connect and 120 s per read, with the SDK's standard retries.

### Where the code lives

**`opensesh-s3`** (Qt-free) is the client:
- buckets and listings, with pages;
- reads with ranges;
- a writer that sends a small file in one `PutObject` and a bigger one in a multipart upload of 32 MB parts;
- copies (`UploadPartCopy` in 512 MB parts above 5 GiB), deletes (a thousand to a request), buckets;
- presigned `GET` links, from a second to the seven days SigV4 allows.

**`Fs::S3`** in `opensesh-ssh::sftp` puts it behind the files view's operations:
- `/` lists the buckets, and `/bucket/a/b` is an object or a folder;
- folders are prefixes, and an empty one keeps a marker object (`a/b/`), which listings don't show;
- renaming a folder copies, then deletes, every object under it;
- copies inside the same storage run on the server.

### Uploads that stop

S3 can't write at an offset: a multipart upload is finished, or abandoned.

**The writer** (`AsyncWrite`) passes the bytes through a pipe to an upload task:
- **Shutting it down** finishes the upload and waits for it, so the transfer reports the server's answer.
- **Dropping it without a shutdown** aborts the multipart upload, so no parts are left on the server.

**The transfer queue** knows that S3 can't resume (`Fs::can_resume`):
- **A pause** abandons the upload, and resuming starts that file over.
- **A cancel** leaves what was there before untouched.
- **"Continue"** for a shorter object already there replaces it instead.
- **Times and permissions** aren't set on objects (`Fs::keeps_metadata`).

### The keys

**Decision:** an S3 host's keys are a keychain identity, user name and password:
- **The access key** is the identity's user name (or the host's user). It isn't a secret.
- **The secret key** is the identity's password, kept in the vault (ADR 0023) like any password.

**In the host editor:** an Access key and a Secret key field. A typed secret key is saved encrypted as the password of the host's identity, or of a new identity named after the host; `hosts.toml` keeps only the identity's id.

**When the pane opens:**
- **A locked vault** is offered to be unlocked, as for SSH.
- **A host without a secret key** asks for it in the pane (the password card). It is used for that pane only, and asked again (up to three times) when the server refuses it.

### The host and quick connect

**`Protocol::S3`:**
- **The address** is the endpoint (`http://` or `https://` host and port, no path); empty is AWS.
- **`s3.region`** and **`s3.path_style`** are new host fields.

**Quick connect:** `s3://[access_key@]host[:port][/bucket/folder][?region=&path_style=]`, and `s3+http://` for a server without TLS.

**Where they open:** an S3 host, or such a target, opens in the files view; its panes can also choose S3 hosts as their source.

**In the files view:**
- permissions, owners and links are hidden;
- at the top, "New folder" makes a bucket;
- an object's menu copies a link that lasts an hour, a day or a week.

### Tests

**In-process:** `opensesh-s3::testing` is a small S3 server on hyper's HTTP/1 server, in memory, path-style, on 127.0.0.1:
- it checks that requests are signed with its access key (not the signature itself);
- it answers listings with pages, multipart uploads, copies, ranges and presigned links.

The crate's tests, the files view's tests and the smoke test use it; a test run never reaches the network.

**Against a real server:** `scripts/s3-test-server.sh` runs RustFS 1.0.0 (Docker or Podman). The ignored `real_s3` test sends 1 GiB through the transfer queue and back with the same SHA-256, in CI.

## Consequences

- S3 storage is one more source in the files view, with the same queue, conflicts and drag and drop.
- **The keys stay in the vault.** Temporary links are signed here, without a request; anyone with a link can download that object until it expires.
- **No resume into S3:** a paused or interrupted upload starts over. Downloads resume as before.
- **Dependencies:**
  - The SDK brings a large tree (the `aws-smithy-*` crates, hyper, rustls); `cargo deny` accepts all its licenses.
  - `cargo audit` warns about `lru` 0.16.4 (RUSTSEC-2026-0253, unsound `pop()` when a key's code panics), which the SDK uses for its S3 Express session cache with `String` keys; no fixed 0.16 release exists. Reviewed each sprint.
