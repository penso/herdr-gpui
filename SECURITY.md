# Security Policy

## Supported Versions

Herdr GPUI is pre-1.0 and has no published releases yet. Only the current
`main` branch receives fixes. Pin a commit if you need a stable reference.

## Reporting a Vulnerability

Report privately through GitHub's
[security advisory form](https://github.com/penso/herdr-gpui/security/advisories/new).
Please do not open a public issue for a suspected vulnerability.

Include the affected commit, your macOS and Rust versions, what an attacker
would gain, and a reproduction if you have one. Expect an initial reply within
a week; this is a small project without a paid on-call rotation.

## Scope

This repository is a **client**. It connects over a Unix domain socket to a
Herdr daemon that you already run, and it owns no terminal processes, no
server state, and no network listener.

In scope:

- Parsing and framing of daemon messages in `crates/herdr-protocol` — decoder
  limits, bounds, and malformed or hostile payloads.
- Socket discovery and connection handling in `crates/herdr-client`, including
  which socket paths are trusted and how a session is selected.
- Anything in `crates/herdr-gpui` that lets terminal content escape its pane:
  unintended command execution, clipboard or notification writes, or path
  handling from daemon-supplied strings.

Out of scope here, and belonging upstream at
[herdrdev/herdr](https://github.com/herdrdev/herdr):

- The daemon itself, its socket permissions, and its session model.
- Anything requiring an attacker who can already run code as your user — at
  that point they can talk to the daemon directly, with or without this client.

Dependency advisories are handled by Dependabot and the `cargo-deny` CI job;
open a normal issue for those rather than a private advisory.

## Verifying a Release

Every release asset carries three independent claims. They fail in different
ways, so check the ones you care about:

| Files | Claim |
| --- | --- |
| `.sha256`, `.sha512` | The bytes are intact |
| `.sig` + `.crt` | Sigstore: this repository's workflow produced them |
| GitHub attestation | Provenance: which workflow, commit, and runner built them |
| `.asc` | GPG: the maintainer signed off, using a key CI never holds |

```sh
# Checksums and maintainer GPG signature
./scripts/verify-release.sh --version 20260920.01 --checksums

# Build provenance
gh attestation verify herdr-gpui-20260920.01-macos-universal.app.zip \
  --repo penso/herdr-gpui
```

The maintainer key is published at <https://pen.so/gpg.asc>, fingerprint
`3103 20A8 CC1C 5BA8 6AD0 9040 C045 1BAD F764 9BBF`. `verify-release.sh`
checks that fingerprint before importing anything, because HTTPS proves the
host, not that the host served the right key.

The `.asc` signatures appear shortly after a release is published rather than
with it: the signing key is hardware-resident and is never available to CI.

## Build and Release Hardening

- Workflows are audited by [zizmor](https://docs.zizmor.sh) on every run, and
  it gates the jobs that handle secrets.
- Every action is pinned to a commit SHA; Dependabot updates them with a
  seven-day cooldown so an automated PR cannot pull in a freshly compromised
  release on day one.
- Workflows grant no permissions by default; each job requests only what it
  needs, and checkouts do not persist credentials.
- Release builds restore no dependency cache — a poisoned cache entry would
  otherwise be signed and notarized along with everything else.
- The macOS bundle is signed with a Developer ID certificate under the
  hardened runtime with no entitlements, then notarized and stapled.
- A CycloneDX SBOM of the dependency graph ships with each release.
