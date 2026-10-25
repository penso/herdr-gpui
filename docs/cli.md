# Command line

`herdr-gpui` is also a command line for the running Herdr UI. Herdr's own
`herdr` CLI drives the daemon: workspaces, terminal tabs, panes, agents,
worktrees, and notifications. `herdr-gpui COMMAND` covers what only this
client knows or draws: its windows, its own tabs (pages, code, reviews,
Devices), the activity bar's sections, worktree notes, checkpoints, forwarded
ports, and its settings.

Commands reach the app over a socket of its own,
`$XDG_STATE_HOME/herdr/gpui/control.sock` (default `~/.local/state/`), which
only your user can open; on Windows, over a named pipe (see
[Windows](#windows)). An agent on an SSH host has no path to it; there,
the app's shim at `~/.local/state/herdr/gpui/bin/herdr-gpui` offers
`browser open` and `browser reload` through Herdr workspace metadata (see
[remote agents](../crates/herdr-gpui/README.md#letting-agents-open-pages)),
and every other command exits 8. A few commands work from files alone and
need no running app; they say so below.

On Windows, `herdr-gpui.exe` is a console program, so a shell waits for it and
sees its exit code; the installers put its directory on `PATH`. The Start menu
opens `herdr-gpui-launcher.exe` instead, which starts it without a console
window.

```sh
herdr-gpui --help              # every command
herdr-gpui COMMAND --help      # one command
```

## Options and output

Every command accepts:

- `--json`: print one JSON document instead of text. Errors become
  `{"error": {"code", "message"}}` on standard output.
- `--window ID`: act in that window (IDs come from `window list`). By default
  a command acts in the window showing the caller's workspace
  (`HERDR_WORKSPACE_ID`, which Herdr sets in every pane). When no window shows
  it, as for a session the app does not show, the command exits 5 rather than
  act on another session's window. Outside Herdr, it acts in the frontmost
  window.

| Exit | Meaning |
| --- | --- |
| 0 | Done |
| 1 | Refused or failed |
| 2 | Invalid arguments, or an address or file that cannot be shown |
| 3 | Herdr UI is not running |
| 4 | Nothing yet: no notes, no event, no pull request, condition not met |
| 5 | The window, workspace, tab, host, or file is not there |
| 6 | A dialog is open in the window, or the app is busy |
| 7 | The app did not answer in time |
| 8 | Not available on this platform or in this version of the app |

## Commands

### App and windows

| Command | What it does |
| --- | --- |
| `status` | Whether the app runs, its version and control protocol, windows, and hosts |
| `app focus\|hide\|quit` | Bring the app forward, hide it, or quit it (Herdr keeps running) |
| `window [list]` | Windows with their IDs, size, workspace, and hosts |
| `window new [--session NAME]` | Another window, of your own Herdr session or a named local session |
| `window focus ID` | Bring a window forward |
| `window fullscreen [on\|off\|toggle]` | |
| `window size [WIDTHxHEIGHT]` | Print or set the window's size; windows cannot be moved |
| `window detach\|reconnect` | Detach from, or reconnect to, the selected host |
| `ui sidebar\|statusbar [on\|off\|toggle]` | For this window and session |
| `ui font-size +\|-\|reset\|POINTS` | The terminal font size for this session |
| `ui panel workspaces\|overview\|pull-requests\|issues\|security\|automations\|usage` | Show an activity bar section: Workspaces with the sidebar expanded, Overview of what needs you on every host, the focused workspace's pull requests or issues across the window, the vulnerability reports and Dependabot alerts of your repositories, every host's automation scripts and runs, or every host's plan usage |
| `run COMMAND` | Any app command by its `[keybindings]` name, as the palette runs it |
| `commands` | The names `run` accepts (no app needed) |
| `open DIR` | Open a folder as **Open Project** in the palette does, starting the app if needed (below) |

`open DIR` checks the folder where you run it and resolves it to an absolute,
canonical path. A path that does not exist exits 5; a file, or a folder in a
device namespace (`\\.\…`, `\\?\GLOBALROOT\…`, and on Unix `/dev`, `/proc`,
`/sys`), exits 1. The app then opens the command palette on
its projects and opens the folder there, which focuses the workspace already
rooted in it or starts one, on the window's local Herdr, and brings the window
forward. A dialog open in the window refuses it (exit 6); the palette shows
anything that goes wrong after that, such as the folder disappearing.

When no Herdr UI is running and no `--window` is given, `open` starts the app,
as a plain `herdr-gpui` would, then asks again every quarter second until the
window has connected to the local Herdr and accepts the folder. It waits at
most 20 seconds after starting the app, then exits 7. Explorer's **Open in
Herdr UI** entry, which both Windows installers add, runs this command through
`herdr-gpui-launcher.exe`, so no console window appears.

### The app's own tabs

| Command | What it does |
| --- | --- |
| `tab [list]` | Pages, code, reviews, and Devices tabs with IDs |
| `tab focus\|close ID` | |
| `browser open\|reload\|feedback` | See [browser tabs](../crates/herdr-gpui/README.md#browser-tabs) |
| `browser list` | Page tabs only |
| `browser navigate ID URL` | Load another address in a page tab |
| `browser back\|forward ID` | |
| `browser zoom ID +\|-\|reset\|PERCENT` | Zoom a page tab a step, back to 100%, or to 50-300%; the tab keeps it |
| `browser text ID` | The page's visible text |
| `browser screenshot ID PATH` | A PNG of the visible page, at most 2000 px a side (macOS) |
| `code open FILE[:LINE]` | A read-only code tab at a line |
| `code edit FILE[:LINE]` | Your terminal editor, in a new pane beside yours |
| `devices` | The Devices overview |
| `orchestrator [--repo PATH]` | Show the Issues section narrowed to your checkout's project, or PATH's |
| `orchestrator items\|issues\|prs` | What it last synced, from its local database (no app needed) |
| `review` | The review of your workspace's uncommitted changes |
| `px0 [open]` | Open or focus px0 on your local workspace's checkout (macOS and Windows) |

`px0 open` reuses one editor tab per workspace. It discovers a local px0
workbench or starts an installed px0; `config set px0.url http://127.0.0.1:7777/`
selects an existing workbench, and `config unset px0.url` restores automatic
discovery. `--json` returns the opened tab ID and workspace. Exit 0 means the
tab opened; startup errors appear in that tab with Retry. Remote workspaces
and builds without embedded pages return exit 8. See [px0](../crates/herdr-gpui/README.md#px0).

### Worktrees and agents

| Command | What it does |
| --- | --- |
| `notes [list]` | Every worktree note |
| `notes get\|clear`, `notes set TEXT` | The note on your checkout (its repository and branch) |
| `feedback [--wait S]` | Notes the user sent your pane from pages, reviews, or terminals |
| `checkpoint [list]` | Checkpoints of the current directory's checkout, newest first (no app needed) |
| `checkpoint create [LABEL]` | Take one now (no app needed) |
| `checkpoint diff A [B]` | What changed between two, or since one (no app needed) |
| `checkpoint restore [ID]` | Open the checkpoints dialog; you confirm there |
| `scripts [list]` | The repository's `.herdr/worktree.toml` scripts (no app needed) |
| `scripts setup\|run` | Run one in a new tab; an unseen script still asks first. Archive removes the worktree, so it stays behind the app's delete dialog |
| `teleport --to HOST` | Open the teleport dialog for your linked worktree, reviewing HOST; you confirm there |
| `fan-out PROMPT [--agent KIND[:N]]...` | Open the fan-out dialog prefilled; you start it there |
| `chat [status]` | The `herdr-acp` chat panes of the window's host, this machine or an SSH host: agent, status, model, context window use, and whether the chat view or the terminal shows |
| `chat show\|hide\|toggle [--pane ID]` | Show the native chat view over your pane (or ID, else the focused one), or its terminal |
| `chat new AGENT [--pane ID]` | Open a tab running a `herdr-acp` chat with a built-in agent (`claude`, `codex`, `copilot`, `gemini`, `opencode`, `pi`) in the workspace and folder of your pane (or ID). On an SSH host the tab runs that host's `herdr-acp` (see [Chats on SSH hosts](#chats-on-ssh-hosts)). Exits 8 when `herdr-acp` is missing here, the host cannot be reached, or the agent is not installed on it (`herdr-acp agents` lists those that are) |

### Hosts

| Command | What it does |
| --- | --- |
| `host [list]`, `host select ID` | The window's hosts; show another one |
| `pr [status]\|checks\|comments\|open` | Your branch's pull request, on the window's selected host. `comments` needs your workspace focused |
| `ports [list]` | Ports your workspace listens on, and forwards |
| `ports forward\|unforward PORT` | Forward a port of the SSH host to this machine |
| `ports open PORT` | `localhost:PORT` in a tab, forwarded first when remote |
| `usage`, `system` | Plan usage and load of the selected host: CPU, memory, disk, uptime, and its GPU and video memory when it has one and `show_gpu_load` or `show_sidebar_gpu_load` is on |
| `security` | Private vulnerability reports in triage (titles and metadata only, never their descriptions) and open Dependabot alerts of the github.com repositories the window's workspaces hold, most severe first, as the Security section last read them. Exits 4 when there are none |
| `security open REPOSITORY ALERT\|GHSA` | Show alert `ALERT` (its number) or private report `GHSA` (its GHSA ID) of `REPOSITORY` (`owner/name`) in the Security section, beside its list: the fix, the advisory, and the dependency for an alert; the report, its advisory draft, and its conversation for a report. Only items the section lists open; nothing on GitHub changes |
| `automation approve FILE [--host ID]` | Open the approval of the automation script `FILE` in the app: its plan, caps, and content hash, for the user to approve or cancel. The command approves nothing. `FILE` is the script's path on the host as `herdr-automate status` lists it, resolved here when it exists; `--host` names the host by ID or label (default: your workspace's host). Exits 5 when the host does not list the script, 6 while a dialog is open or the host was not read yet |
| `cloud [list]` | Saved cloud machines (no app needed) |
| `cloud add` | Open the settings that add one |
| `cloud images [list]` | Sandbox images built from projects' recipes (no app needed) |
| `cloud images build [PROVIDER]` | Build the current repository's sandbox images ahead of a dispatch |
| `cloud setup ID` | Run the `[cloud]` setup script on a saved machine again, in a new tab once the window shows it. `ID` is the machine ID from `cloud list`, its host ID, or its label |
| `cloud create PROVIDER [--count N] [--template T] [--preset P] [--sandbox] [--no-install] [--no-setup]` | Ask in the window to create up to 16 machines; the user confirms there, once for all of them. Prints the batch to follow (below) |
| `cloud exec ID [--timeout S] [--cwd DIR] -- COMMAND...` | Run a command on a saved machine through the app's connection to it, and print its output (below) |
| `cloud destroy ID... \| --batch BATCH` | Ask in the window to delete saved machines at their providers and forget them; the user confirms there, once for all of them. Prints the batch to follow (below) |
| `cloud wait BATCH [--timeout S]` | Each machine of a batch once all are settled, or after S seconds (default 60, at most 600) |
| `cloud batches` | The batches the app keeps, newest last; the oldest are left out when they would not fit in one answer |
| `processes [--pane ID]` | The processes under a pane, with CPU and memory (no app needed) |
| `search symbol\|file QUERY` | Go to Symbol or Go to File over the current checkout (no app needed) |

### Settings and services

| Command | What it does |
| --- | --- |
| `config [path]` | The settings files (no app needed) |
| `config get\|set\|unset KEY [VALUE]` | Read or change `config-gpui.local.toml`, keeping its comments. A value that would not load, or an unknown key, is refused and nothing is written; the app applies the rest at once (no app needed) |
| `config check` | Report invalid settings, exit 1 if any |
| `config reload` | Reread Herdr's config and this app's |
| `theme [list]`, `theme set NAME` | The app theme. A device's terminal theme is `config set devices.ID.theme NAME`, with the ID from `host list` minus its `ssh:` prefix |
| `font [list]`, `font set NAME`, `font reset` | |
| `keybinds [list]`, `keybinds check` | Shortcuts; custom Herdr commands this client cannot reach |
| `caffeine [status]`, `caffeine on\|off\|toggle [--for 30m]` | Keep the machine awake |
| `sound [list]`, `sound play [done\|request]`, `bell` | Sounds play at `sound.volume` percent (`config set sound.volume 30`) |
| `update [status]`, `update check` | Never downloads or installs |
| `integrations [list]` | What Herdr offers the selected host |
| `integrations install-skill` | Install the agent skill (no app needed) |
| `logs [--lines N] [--path]` | The app's log (no app needed) |
| `diagnostics bundle PATH` | A redacted report for a bug, with the newest crash reports (no app needed) |

Boat's sandbox lifetime uses the existing config commands:

```sh
herdr-gpui config set boat.auto_stop never
herdr-gpui config set boat.auto_stop '"7200"'  # seconds, stored as a string
herdr-gpui config get boat.auto_stop --json
herdr-gpui cloud add                         # open Cloud devices, then choose Boat
```

`never` is the default: billing continues until the sandbox is stopped in Boat.
Numeric deadlines are fixed, even during active SSH work; trials require at
most 7200 seconds. The setting applies on create and resume. In Boat settings,
**Finish setup** extends a running sandbox from now or disables its deadline.

**Recover creation** retries a persisted creation request with its original
options and key; forgetting an unresolved request requires confirmation there.

What a new cloud machine installs and runs when it first connects is set the
same way, and `cloud setup` runs it again on one already saved:

```sh
herdr-gpui config set cloud.agents '["claude", "opencode"]'
herdr-gpui config set cloud.setup '"git config --global pull.rebase true"'
herdr-gpui config set cloud.coder.agents '[]'   # nothing extra on Coder
herdr-gpui cloud setup herdr-tidy-lark-aa
```

An agent that needs machines of its own asks for them with `cloud create`,
then follows the batch with `cloud wait` until each is ready:

```sh
herdr-gpui cloud create coder --count 10 --template dev --json
herdr-gpui cloud wait 3 --timeout 600 --json   # again while it exits 4
```

The window asks once, saying how many machines of which provider, how they
are made, and that the provider bills each until it is destroyed. Once
confirmed, each machine is made on its own, saved as a device, given Herdr
when it lacks it (unless `--no-install`), and has the `[cloud]` setup script
run on it in a login shell from its home directory, without a terminal
(unless `--no-setup`). Only then is it `ready`; `cloud wait` shows each
machine's step meanwhile, and its device ID once saved. A failed setup script
fails the machine with the end of its output, but the machine stays saved;
destroy it like any other. Coder takes `--template` and `--preset` by name
(without them, the deployment's only template and its default preset);
Azure makes a Container Apps sandbox with `--sandbox`. Boat and Railway make
their machines one at a time. Providers are named by key or name: azure,
boat, coder, container, daytona, opensandbox, railway, sprites. The device
list keeps at most 64 cloud devices, counting those being made.

`cloud exec` runs one command on a saved cloud machine, named as for
`cloud setup`, through the same authenticated command the app connects with
(`coder ssh`, a provider's SSH gateway, and so on), so the agent needs no
credentials of its own. Everything after `--` is the command, word for word;
it gets no input. `--cwd` runs it in a directory, `~/` being the machine's
home. `--timeout` (default 120 seconds, at most 600) counts everything,
including waiting for a stopped machine to start; past it the command is
stopped. Its stdout comes back on stdout and its stderr on stderr, each cut
to its last 64 or 32 KiB and with terminal controls removed; `--json` gives
`{"device", "status", "stdout", "stderr", "truncated"}`. It exits 0 when the
command exited 0, and 1 otherwise. It asks nothing: like the agent's own
shell, it acts on the user's machine at once. Git pushes from a machine need
credentials there, such as Coder's external auth or a `gh auth login` in the
`[cloud]` setup script.

```sh
herdr-gpui cloud exec herdr-tidy-lark-aa --cwd ~/code -- git clone https://github.com/me/app
herdr-gpui cloud exec herdr-tidy-lark-aa --cwd ~/code/app -- sh -c 'git switch -c fix && make test'
```

An agent that worked on cloud machines removes them with `cloud destroy`,
naming each by its machine ID, host ID, or label (a label two machines share
is refused), or with `--batch` every machine a batch named. The window asks
once, listing each machine and what happens to it: Azure, Boat, Coder,
Container, Daytona, OpenSandbox, and Sprites delete the machine with what
goes with it and then forget it here;
Railway VMs cannot be deleted from this app, so they are only forgotten and
expire on their own. A dialog already open refuses the request (exit 6).

```sh
herdr-gpui cloud destroy coder:5d1c… herdr-tidy-lark-aa --json
herdr-gpui cloud wait 3 --timeout 600 --json
```

`cloud wait` exits 0 once every machine went as asked, 1 once the batch
settled otherwise (a machine failed, with why, or the user said no), and 4
while it is still waiting for the user or running. A batch keeps going if the
command or the window that asked goes away; the app keeps the last 32.

Focus-on-hover between split terminal panes is on by default. Disable it with
`herdr-gpui config set focus_follows_mouse false`.
The change applies live; `config get focus_follows_mouse` reads it back.

### Remote control

| Command | What it does |
| --- | --- |
| `remote [status]` | Whether remote control is on and its daemon runs: its port, version, the addresses it serves, its certificate authority fingerprint, and paired and connected devices |
| `remote devices` | The paired devices with their IDs, when each was last seen, and whether it gets notifications. Exits 4 when there are none |
| `remote pair` | Open the pairing dialog in the window. The QR and its token show only there, and the user approves the phone there |
| `remote restart` | Restart the daemon from the binary beside the app, as after a rebuild or update; phones reconnect on their own. Exits 6 while the service is already changing |
| `remote revoke DEVICE` | Ask in the window to revoke a paired device, named by its ID from `remote devices` or its name (a name two devices share is refused); the user confirms there |

Remote control is turned on in Settings > Remote control, or with
`config set remote.enabled true`; the app then installs the `herdr-remote-control`
daemon as a user service and keeps it in line with the `[remote]` settings.
While it is off, `remote status` says so and exits 0; `pair`, `devices`,
`revoke`, and `restart` exit 1. A dialog already open in the window refuses `pair` and
`revoke` (exit 6). `--json` gives `{"enabled", "running", "status", "error"}`
for `status`, where `status` is the daemon's own report, and
`{"devices": [...]}` for `devices`.

```sh
herdr-gpui remote status --json
herdr-gpui remote revoke d_6f1c
```

### Events

`events` prints what happened in the app: windows opening, closing, and
gaining focus; the app's tabs opening and closing; notes kept for a pane;
worktree notes changing; host status changes; and cloud batches moving on
(`cloud_batch`, with the batch and its phase: `confirming`, `running`,
`done`, or `cancelled`). Each event has a sequence
number; pass back the `INSTANCE:SEQ` cursor a call prints with `--after` to
get only newer events. A cursor from an earlier run of the app gets every
event the new run has kept, so a restart never hides events.

```sh
herdr-gpui events --follow --kind tab_closed
herdr-gpui wait tab-closed 12 --timeout 600
herdr-gpui wait window            # also waits for the app to start
```

## What stays in the app

Commands never bypass a confirmation. Restoring a checkpoint, teleporting a
worktree, fanning out a prompt, running a worktree script the app has not
seen, creating or destroying cloud machines, adding a cloud machine, pairing
a phone for remote control, and revoking one open the app's own dialog
(archiving a worktree and approving a phone have no command at all), already
pointed at your checkout, the machines, or the device named, and the user
confirms there.
`cloud exec` asks nothing because it confirms nothing: it is the agent's own
work, run on a machine the user saved. Nothing here installs, starts, stops,
or upgrades Herdr on this computer, and `update` never installs the app;
`cloud create` installs Herdr only on the machines the user confirmed, as
creating them in Settings does.

Adding an SSH or local network device has no command: the Add Device dialog
runs it. Its setup workspace runs `herdr-gpui __device-setup-attach SOCKET`,
an internal helper that shows the setup the app runs (see the Native Shell
README's "Local Network access"); it is not a command to call yourself, and
the socket it names is single use.

Scrollback search is not repeated here: `herdr pane read` covers it. Moving a
window and capturing a whole window have no command because the app cannot
do them.

## Links

`herdr-gpui URL`, with a `herdr-gpui://` link as the only argument, hands the
link to the running app, or starts the app and follows it once it is
connected. On Windows the installer registers the scheme, so the shell runs
this for jump list entries, notification clicks, and links in other apps.

| Link | What the app does |
| --- | --- |
| `herdr-gpui://open?path=PATH` | Focus the workspace showing the Git checkout or worktree at `PATH`, or ask in the window before opening one there |
| `herdr-gpui://device?id=ID` | Show the device `host list` names `ID` |
| `herdr-gpui://notification?tag=TAG` | Open the pane of a notification this app posted |

A link is untrusted input, checked by the command and again by the app: only
those shapes, each parameter once, bounded lengths, no control characters, and
an absolute local path without `..` (on Windows a drive path, never a UNC or
`\\?\` one). A link only ever shows something; it never runs a command. A
refused link exits 2, an unknown device or notification 5, as for commands.
The request is `{"method": "link", "caller", "op": {"url"}}`.

## The protocol

One JSON request line, one JSON response line, then the socket closes. A
request is `{"method", "caller", "op"}`, where `caller` carries the pane,
workspace, daemon socket, and window, and `op` is tagged with `"op"`. Browser
requests keep their original flat form (`browser.open`, `browser.reload`,
`browser.feedback`, `notes.send`). `status` reports the protocol version,
currently 6. Requests and responses are bounded to 256 KiB, a request that
waits says for how long (at most 600 seconds), and the app answers at most a
few requests at a time.

### Windows

On Windows the same lines travel over a named pipe,
`\\.\pipe\herdr-gpui-control-SID`, where `SID` is your Windows user's
security identifier. When `XDG_STATE_HOME` is set, a hash of the state
directory is appended, so a test or side-by-side install gets a pipe of its
own; without it, every shell of yours reaches the same app whatever `HOME` it
has. Pipe names are shared by all users of the machine, so:

- The app creates the pipe owned by you, with a DACL that grants only your
  SID and inherits nothing, so no other user can connect or add an instance.
- It creates it as the first instance and rejects remote clients
  (`PIPE_REJECT_REMOTE_CLIENTS`), so it never joins a pipe someone else made
  and nothing off this machine can reach it. If another user made the name
  first, the app does not listen, and logs why.
- `herdr-gpui COMMAND` opens the pipe at identification level, so the server
  can learn who called but cannot act as you, and checks that the pipe's
  owner is you before it sends anything. A pipe someone else owns is refused
  with exit 1.

A pipe has no receive or send timeout. Reads poll with `PeekNamedPipe`, as
the daemon client does; writes have no deadline, so the app's out buffer
holds a whole answer and the connection bound limits what a stalled caller
can hold.

## Chats on SSH hosts

Native chat views are opt-in: turn on **ACP chats** in Settings > General
(`acp_chats = true` under `[features]`). While it is off, chat panes show
their terminal and `chat new` exits 8.

A chat on an SSH host shows in the native chat view like a local one. Herdr UI
runs `herdr-acp connect ID` on the host over the same SSH connection it uses
for Herdr, and the program there sees an ordinary client on its own socket.
Starting a chat there (`chat new`, or the workspace menu) runs the host's own
`herdr-acp`, looked for in `~/.herdr/bin`, `~/.cargo/bin`, `~/.local/bin`, and
then the remote `PATH`. Install it on the host with:

```sh
cargo install --locked --git https://github.com/penso/herdr-gpui herdr-acp --root ~/.herdr
```

The workspace menu offers only the agents whose own ACP command is installed
on the host, as `herdr-acp agents` lists them; an agent `npx` would have to
download first is left out.

Files dropped on a remote chat are copied to the host first, into private
temporary files under its `${TMPDIR:-/tmp}`, and the copies are attached.

