# Sandboxes

How Sheprd gives agents isolated machines: containers on this computer,
Fly Sprites, a self-hosted OpenSandbox server, and the existing cloud
providers. User-facing behavior is in the
crate README's "Containers (Docker or Podman)", "OpenSandbox", "Fly Sprites",
and "Smart Dispatch" sections; this page records the design and is for working on it.

## Decision: a sandbox is a device

Herdr owns terminals, sessions, and agents, and this client never changes
Herdr. So a sandbox is not a process the GUI supervises: it is a Linux machine
running its own Herdr daemon, saved as a cloud device and reached the way an
SSH host is, by running a command whose standard streams carry Herdr's bridge
(`herdr bridge`). Everything Herdr already does for a remote host then holds
inside the sandbox:

- PTYs, resize, signals, exit status, and output replay are the sandbox's own
  daemon's. A window that closes or reconnects never stops an agent there.
- Reconnecting runs the same command again; nothing replays commands.
- Workspaces, worktrees, and agents are created by that daemon, through the
  `herdr` CLI that Teleport's scripts already drive.

The GUI's part is the machine's lifecycle (create, start, stop, delete),
getting Herdr onto it, and moving Git state in and out with Teleport.

## What the original RFC asked for, and where it lives

Paths are in `crates/sheprd-remote/src/`, which has no GPUI, unless they name
`crates/sheprd-gpui/src/`: the jobs, batches, and views the window drives.

| RFC component | Here |
| --- | --- |
| Provider registry, `SandboxProvider` | `cloud/providers.rs`: one `providers!` row per provider generates the name, logo, connector, background command, deletion, on-demand provisioning, and `cloud::ProviderError`. No async trait: everything blocks on workers. |
| Session manager (PTY, reconnect) | The sandbox's own Herdr daemon, through `sheprd_client::connect_command`. |
| Exec / attach | The provider's command: `docker exec -i`, `ssh`, or this app as a relay (`azure/sandbox/relay.rs`, `sprites/relay.rs`, `opensandbox/relay.rs`, sharing `cloud/socket.rs`). |
| Git sync | `teleport/`: `fresh.rs` (new worktree from a base commit, cloned or bundled), `carry.rs`, and Teleport itself, untracked files included. |
| Workspace lifecycle | `crates/sheprd-gpui/src/cloud/jobs.rs` (create with progress), `cloud/catalog.rs` (saved devices), provider `setup.rs` (create, delete, roll back a failed create). |
| Dirty-state warning before delete | `cloud/unsynced.rs`, shown by the sidebar's Remove dialog. |
| Lane-to-sandbox | `crates/sheprd-gpui/src/dispatch/`: the **New container** / **New sprite** tile; `dispatch/job.rs` provisions, then dispatches. |
| Prebuilt environments | `cloud/image/`: a project's recipe (`.herdr/sandbox/Dockerfile`, else a dev container) read from its host, a key hashed from what the build reads, a catalog of built images, and one shared build registry; containers and Daytona hold images (`container/image.rs`, `daytona/image.rs`). |
| Idle timers, quotas | `container/idle.rs`, `[container] max_containers`; Azure's `cleanup.rs`, each checked every minute by the app (`crates/sheprd-gpui/src/cloud.rs`); sprites sleep on their own. |
| Credential broker | `teleport/credentials.rs` sends the `gh` token to a host that cannot reach GitHub. No scoping or trust tiers yet (see below). |
| Port proxy | `crates/sheprd-gpui/src/port_forward/`, SSH hosts only. |

## Adding a provider

1. A `CloudProvider` variant and key in `sheprd-client`, behind a feature.
2. A module with `connect(account, id, session, stop)`, `command(account,
   id, stop)` (a `Command` the bridge, installer, and scripts append one
   shell string to, as `ssh` does), and an `Error`.
3. A row in `cloud/providers.rs`. `deletion` makes Remove delete the machine
   after the unpushed-work check; `provision` makes the machine available to
   dispatch; `image` (a `cloud::image::Support`) lets it start from a
   project's sandbox image, which `provision` then receives.
4. A Settings card, a config table (`sheprd-remote`'s `config/`, with its
   `Sections` field, and the app's `save_*` beside its other tables), its
   feature forwarded from sheprd-gpui, and the per-provider `match` arms the
   compiler asks for (Settings tabs, Add Device). Keep the setup job's
   failure path deleting what it made.

A command must carry bytes unchanged and report the remote exit code, or 255
when the connection failed, as `ssh` does. Teleport's scripts need stdin's
end of file to reach the remote command: Git bundles travel on stdin.

## Providers evaluated

Checked 2026-10-17 against each provider's published API description; see
the sources. "Stream" is whether a long-lived, binary-safe stdin/stdout
stream to an arbitrary command exists.

| Provider | Stream | Status here |
| --- | --- | --- |
| Docker / Podman | Yes: `exec -i` without a TTY | Shipped. Live-tested with Docker (OrbStack). Podman follows the same CLI; not live-tested. |
| Fly Sprites | Yes: exec WebSocket, one tag byte per frame, explicit stdin EOF | Shipped as experimental. Tested against a scripted stream; no live account. |
| E2B | Only through envd's Connect RPC (base64 JSON, one POST per stdin chunk); its CLI decodes output as text, which breaks binary data | Not built: costlier than Sprites for the same result. |
| OpenSandbox | Yes: execd's PTY session runs a command, and its WebSocket in pipe mode carries stdin, stdout, and stderr unchanged; it has no stdin EOF, so the relay frames input for a wrapper that ends it | Shipped as experimental. Live-tested against `opensandbox-server` (execd v1.1.0) on Docker: binary input with EOF, exit codes, install, entrypoint server, delete. |
| Azure Container Apps sandboxes | Exec WebSocket on a terminal only | Already shipped, with a relay that works around the terminal. |

Sources: [OpenSandbox execd PTY sessions](https://github.com/opensandbox-group/OpenSandbox/blob/main/components/execd/PTY.md),
[Sprites exec WebSocket](https://docs.fly.io/sprites/api/websockets/execute-command.md),
[Sprites lifecycle](https://docs.fly.io/sprites/concepts/lifecycle.md),
[E2B process API](https://github.com/e2b-dev/infra/blob/main/packages/envd/spec/process/process.proto),
[E2B CLI exec](https://docs.e2b.dev/cli/exec-command.md),
[OpenSandbox](https://github.com/opensandbox-group/OpenSandbox),
[podman exec](https://docs.podman.io/en/v5.0.3/markdown/podman-exec.1.html).

## Sandbox images

A project's recipe is read through Teleport's script runner from the host
its checkout is on, so a project on an SSH host or cloud machine builds like
a local one. Only what Git would commit is read (`git ls-files` with ignored
files excluded), never a symbolic link, and only the files the Dockerfile's
`COPY` and `ADD` lines name, plus `.dockerignore`: a recipe whose dev
container uses the whole repository as its context still rebuilds only when
what it copies changes. The key is a SHA-256 over a format version, the
platform, the composed Dockerfile (the app's Herdr layer included), and each
file's path, mode, and bytes, length-prefixed.

`cloud/image/builds.rs` runs at most one build per project and provider, on
the worker that asked; another asking waits for it, then reuses the result.
Builds started from Settings or a workspace's menu belong to the main window,
so closing Settings does not cancel them. The catalog (`sandbox-images.json`)
is a record for display and for Rebuild, never the truth: before reuse the
provider is asked whether it still holds the image.

The Herdr layer installs the latest release when the image is built, and the
key does not include Herdr's version, so an image keeps the Herdr it was built
with until its recipe changes or it is reset.

## Threat model

- **Containers** share this computer's kernel. They keep cooperating agents
  apart (no host files, no engine socket, no host network, an unprivileged
  user, `no-new-privileges`, CPU, memory, and process limits) but are not a
  boundary against hostile code. The card and README say so.
- **VMs and microVMs** (Azure VMs, sprites) isolate the kernel. They do not
  stop a credential deliberately given to them from leaving.
- **Credentials**: a provider's API token stays on this computer; relays read
  it from the credential store and send it only to the configured API root.
  Teleport sends the `gh` token to a host that cannot reach GitHub, for every
  provider alike.
- **Deleting** a machine is the default answer only where the provider can
  delete it, and only after the unpushed-work search; a machine that cannot be
  searched is called unchecked, never clean.

## Not done yet

- **Agent credentials in new sandboxes.** A fresh container or sprite has no
  signed-in agent, so fan-out does not offer them: every lane would fail its
  agent check. Doing this safely means choosing what may be copied into a
  sandbox (API keys, not session files) and per-device trust, which the `gh`
  token handling would follow too.
- **Orphans beyond containers.** A crash between a provider creating a
  machine and the device being saved leaves the machine unlisted. The
  Container card finds such containers by their `dev.herdr-gpui.device` label
  and offers to add them back. OpenSandbox sandboxes carry a
  `herdr-gpui-device` metadata key and could get the same; sprites only have
  a `herdr-` name prefix, which the user's own sprites may share, so they are
  left alone.
- **Ports** for containers and sprites: forwarding needs an SSH host.
- **Images elsewhere.** Azure sandboxes and OpenSandbox start from registry
  images, so each needs a registry setting (one per provider) to push to.
  Daytona builds cannot copy files in yet: its SDK uploads the context to
  Daytona's object storage with signed S3 requests first.
