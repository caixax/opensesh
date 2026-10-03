# Security policy

## Reporting a vulnerability

Please report security problems **privately**, through GitHub's private vulnerability reporting: open the repository's **Security** tab and choose **Report a vulnerability** ([direct link](https://github.com/caixax/opensesh/security/advisories/new)). Please don't open a public issue for them.

A useful report says which version of OpenSesh and which system it was (Windows, or the Linux distribution and desktop), what an attacker needs (a malicious server, a crafted file to import, access to the computer...), and how to reproduce it.

The report stays private until a fix is released; you are credited in the advisory unless you'd rather not be.

## Supported versions

Security fixes go into the latest release. Updating to it is the way to get them.

## What OpenSesh protects, and how

- [Threat model](docs/threat-model.md): what OpenSesh defends against, and what it doesn't.
- [Security review](docs/security-review.md): what was checked before 1.0, what was found, and what is accepted.
- [Vault format](docs/vault-format.md): how passwords and keys are stored.
