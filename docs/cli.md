# Command line

`sheprd` is also a command line for the running Sheprd app. Herdr's own
`herdr` CLI drives the daemon: workspaces, terminal tabs, panes, agents,
worktrees, and notifications. `sheprd COMMAND` covers what only this
client knows or draws: its windows, its own tabs (pages, code, reviews,
Devices), the activity bar's sections, worktree notes, checkpoints, forwarded
ports, and its settings.

Commands reach the app over a socket of its own,
`$XDG_STATE_HOME/herdr/gpui/control.sock` (default `~/.local/state/`), which
only your user can open; on Windows, over a named pipe (see
[Windows](#windows)). An agent on an SSH host has no path to it; there,
the app's shim at `~/.local/state/herdr/gpui/bin/sheprd` offers
`browser open` and `browser reload` through Herdr workspace metadata (see
[remote agents](../crates/sheprd-gpui/README.md#letting-agents-open-pages)),
and every other command exits 8. A few commands work from files alone and
need no running app; they say so below.

On Windows, `sheprd.exe` is a console program, so a shell waits for it and
sees its exit code; the installers put its directory on `PATH`. The Start menu
opens `sheprd-launcher.exe` instead, which starts it without a console
window.

```sh
sheprd --help              # every command
sheprd COMMAND --help      # one command
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
| 3 | Sheprd is not running |
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
| `ui agent-group [NAME\|all]` | List the agents panel's coordinator groups, marking the one shown, or show one group's sessions with its coordinator first, or `all` of them, for this window and session. Exits 5 for a group no coordinator names and 8 unless `[sidebar] coordinator_suffix` is set and the agents panel is shown |
| `ui panel workspaces\|overview\|pull-requests\|issues\|tasks\|security\|automations\|ports\|usage` | Show an activity bar section: Workspaces with the sidebar expanded, Overview of what needs you on every host, the focused workspace's pull requests or issues across the window, Beads tasks across the window's projects, the vulnerability reports and Dependabot alerts of your repositories, every host's automation scripts and runs, every port this computer's user listens on, or every host's plan usage |
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

When no Sheprd is running and no `--window` is given, `open` starts the app,
as a plain `sheprd` would, then asks again every quarter second until the
window has connected to the local Herdr and accepts the folder. It waits at
most 20 seconds after starting the app, then exits 7. Explorer's **Open in
Sheprd** entry, which both Windows installers add, runs this command through
`sheprd-launcher.exe`, so no console window appears.

### The app's own tabs

| Command | What it does |
| --- | --- |
| `tab [list]` | Pages, code, reviews, and Devices tabs with IDs |
| `tab focus\|close ID` | |
| `browser open\|reload\|feedback` | See [browser tabs](../crates/sheprd-gpui/README.md#browser-tabs) |
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
| `orchestrator items\|issues\|prs\|tasks` | What it last synced, from its local database (no app needed); `tasks` is the repository's Beads tasks, with each one's assignee, priority, and blockers in `--json` |
| `review` | The review of your workspace's uncommitted changes |
| `px0 [open]` | Open or focus px0 on your local workspace's checkout (macOS and Windows) |

`px0 open` reuses one editor tab per workspace. It discovers a local px0
workbench or starts an installed px0; `config set px0.url http://127.0.0.1:7777/`
selects an existing workbench, and `config unset px0.url` restores automatic
discovery. `--json` returns the opened tab ID and workspace. Exit 0 means the
tab opened; startup errors appear in that tab with Retry. Remote workspaces
and builds without embedded pages return exit 8. See [px0](../crates/sheprd-gpui/README.md#px0).

### Worktrees and agents

| Command | What it does |
| --- | --- |
| `notes [list]` | Every worktree note |
| `notes get\|clear`, `notes set TEXT` | The note on your checkout (its repository and branch) |
| `worktree [status]`, `worktree mute\|unmute` | Whether your worktree's notifications are muted; silence them or let them through again |
| `feedback [--wait S]` | Notes the user sent your pane from pages, reviews, or terminals |
| `checkpoint [list]` | Checkpoints of the current directory's checkout, newest first (no app needed) |
| `checkpoint create [LABEL]` | Take one now (no app needed) |
| `checkpoint diff A [B]` | What changed between two, or since one (no app needed) |
| `checkpoint restore [ID]` | Open the checkpoints dialog; you confirm there |
| `scripts [list]` | The repository's `.herdr/worktree.toml` scripts (no app needed) |
| `scripts setup\|run` | Run one in a new tab; an unseen script still asks first. Archive removes the worktree, so it stays behind the app's delete dialog |
| `teleport --to HOST` | Open the teleport dialog for your linked worktree, reviewing HOST; you confirm there |
| `fan-out PROMPT [--agent KIND[:N]]...` | Open the fan-out dialog prefilled; you start it there |
| `chat [status]` | The `sheprd-acp` chat panes of the window's host, this machine or an SSH host: agent, status, model, context window use, and whether the chat view or the terminal shows |
| `chat convert [--pane ID]` | Move the terminal agent in your pane (or ID, else the focused one) into a `sheprd-acp` chat in the same pane, keeping its conversation (see [Moving a terminal agent into a chat](rich-chat.md#moving-a-terminal-agent-into-a-chat)). Returns once the move has started; `chat` shows the chat when it attaches. Exits 6 while the agent works, 5 when the pane has no terminal agent, and 8 for an agent or pane that cannot move |
| `chat show\|hide\|toggle [--pane ID]` | Show the native chat view over your pane (or ID, else the focused one), or its terminal |
| `chat new AGENT [--pane ID]` | Open a tab running a `sheprd-acp` chat with a built-in agent (`claude`, `codex`, `copilot`, `gemini`, `opencode`, `pi`) in the workspace and folder of your pane (or ID). On an SSH host the tab runs that host's `sheprd-acp` (see [Chats on SSH hosts](#chats-on-ssh-hosts)). With no Herdr pane to go by, as when called from a Herdr 2 pane, a Herdr 2 tab in the active group splits its pane in use with the chat instead (see [Chats in Herdr 2 panes](rich-chat.md#chats-in-herdr-2-panes)); its pane id in `chat` is `core:` and the terminal's. Exits 8 when `sheprd-acp` is missing here, the host cannot be reached, or the agent is not installed on it (`sheprd-acp agents` lists those that are) |

### Hosts

A focus asked for on a connected host the window is not showing, such as
`herdr --machine Homelab agent focus NAME`, switches the window to that host
when it moves the host to another tab. The daemon reports nothing when the
target is already its focused tab, or when the focus only moves between panes
of one tab, so a script that must end up on the host runs `sheprd host select`
as well. A focus waits until you stop typing or composing, so keys meant for
one host do not reach another, and choosing a host yourself in the meantime
cancels it. It leaves the host restored at the next launch as it was.
It does not take the window while a dialog is open, or from another window
already showing the host. A tab or workspace opening on a background host does
not take it either.

| Command | What it does |
| --- | --- |
| `host [list]`, `host select ID` | The window's hosts, with their tags; show another one |
| `host mute\|unmute [HOST]` | Silence a host's notifications as a whole, or let them through again, by ID or label; absent, your host. Works while it is offline |
| `host tag ID [TAG...]` | Replace the tags that group a host in the sidebar and on paired phones; with no tags, clear them. Each computer's daemon keeps its own tags, so a host away gets the edit once it is back |
| `pr [status]\|checks\|comments\|open` | Your branch's pull request, on the window's selected host. `comments` needs your workspace focused |
| `pr quota` | Last reported GraphQL points used, limit, and remaining, plus rate-limit retry delays for the window's GitHub accounts. `--json` returns records merged by login: known observations are preferred, then the latest reset window and greatest usage within it; retry delays retain the longest pause. Usage is never summed across credentials. Unavailable quotas are `null`. No network request is made. |
| `ports [list]` | Ports your workspace listens on, and forwards |
| `ports forward\|unforward PORT` | Forward a port of the SSH host to this machine |
| `ports open PORT` | `localhost:PORT` in a tab, forwarded first when remote |
| `ports all` | Every TCP port this computer's user listens on, as the Ports section lists them, whoever started it: bind address, a clear name (a macOS app's name, or what an interpreter runs, such as `vite` or `manage.py runserver`), process, pid, command line, project and working directory, CPU and resident memory, uptime, whether it is paused or a system listener, and the Herdr workspace that started it. Keeps the section's scan running for a minute and waits up to 20 seconds for its first listing. macOS and Linux only (exit 8 elsewhere); exits 4 when nothing listens |
| `ports stop\|pause PORT` | Ask in the app to stop (SIGTERM, then SIGKILL a few seconds later) or pause (SIGSTOP) the processes listening on `PORT` of this computer. Nothing is signalled until the user confirms there; exits 5 when nothing of yours listens on it, 6 while a dialog is open |
| `ports resume PORT` | Resume (SIGCONT) the paused processes listening on `PORT` of this computer; exits 5 when none is paused |
| `usage`, `system` | Plan usage and load of the selected host: CPU, memory, disk, uptime, and its GPU and video memory when it has one and `show_gpu_load` or `show_sidebar_gpu_load` is on |
| `security` | Private vulnerability reports in triage (titles and metadata only, never their descriptions) and open Dependabot alerts of the github.com repositories the window's workspaces hold, most severe first, as the Security section last read them. Exits 4 when there are none |
| `security open REPOSITORY ALERT\|GHSA` | Show alert `ALERT` (its number) or private report `GHSA` (its GHSA ID) of `REPOSITORY` (`owner/name`) in the Security section, beside its list: the fix, the advisory, and the dependency for an alert; the report, its advisory draft, and its conversation for a report. Only items the section lists open; nothing on GitHub changes |
| `automation approve FILE [--host ID]` | Open the approval of the automation script `FILE` in the app: its plan, caps, and content hash, for the user to approve or cancel. The command approves nothing. `FILE` is the script's path on the host as `sheprd-automate status` lists it, resolved here when it exists; `--host` names the host by ID or label (default: your workspace's host). Exits 5 when the host does not list the script, 6 while a dialog is open or the host was not read yet |
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
| `language [status]`, `language list`, `language set system\|TAG` | The language set and the locale Sheprd shows, the languages there are, each with its name in itself as Settings > General lists it, or save the `language` setting (a BCP 47 tag; `en-XA` and `en-XB` are pseudo-locales). Only `status` asks the app, and without it reports what it would show |
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
sheprd config set boat.auto_stop never
sheprd config set boat.auto_stop '"7200"'  # seconds, stored as a string
sheprd config get boat.auto_stop --json
sheprd cloud add                         # open Cloud devices, then choose Boat
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
sheprd config set cloud.agents '["claude", "opencode"]'
sheprd config set cloud.setup '"git config --global pull.rebase true"'
sheprd config set cloud.coder.agents '[]'   # nothing extra on Coder
sheprd cloud setup herdr-tidy-lark-aa
```

An agent that needs machines of its own asks for them with `cloud create`,
then follows the batch with `cloud wait` until each is ready:

```sh
sheprd cloud create coder --count 10 --template dev --json
sheprd cloud wait 3 --timeout 600 --json   # again while it exits 4
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
boat, boxd, coder, container, daytona, opensandbox, railway, sprites. The device
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
sheprd cloud exec herdr-tidy-lark-aa --cwd ~/code -- git clone https://github.com/me/app
sheprd cloud exec herdr-tidy-lark-aa --cwd ~/code/app -- sh -c 'git switch -c fix && make test'
```

An agent that worked on cloud machines removes them with `cloud destroy`,
naming each by its machine ID, host ID, or label (a label two machines share
is refused), or with `--batch` every machine a batch named. The window asks
once, listing each machine and what happens to it: Azure, Boat, boxd, Coder,
Container, Daytona, OpenSandbox, and Sprites delete the machine with what
goes with it and then forget it here;
Railway VMs cannot be deleted from this app, so they are only forgotten and
expire on their own. A dialog already open refuses the request (exit 6).

```sh
sheprd cloud destroy coder:5d1c… herdr-tidy-lark-aa --json
sheprd cloud wait 3 --timeout 600 --json
```

`cloud wait` exits 0 once every machine went as asked, 1 once the batch
settled otherwise (a machine failed, with why, or the user said no), and 4
while it is still waiting for the user or running. A batch keeps going if the
command or the window that asked goes away; the app keeps the last 32.

Focus-on-hover between split terminal panes is on by default. Disable it with
`sheprd config set focus_follows_mouse false`.
The change applies live; `config get focus_follows_mouse` reads it back.

An unnamed tab shows its number. To show its agent's terminal title instead,
run `sheprd config set tab_labels.agent_title true`.

### Remote control

| Command | What it does |
| --- | --- |
| `remote [status]` | Whether remote control is on and its daemon runs: its port, version, the addresses it serves with network labels and daemon-host interface names, its certificate authority fingerprint, and paired and connected devices. JSON includes optional `status.address_details` keyed by origin; older daemons omit it |
| `remote devices` | The paired devices with their IDs, when each was last seen, whether it gets notifications, and the other machines it was paired with from here or is being paired with. JSON includes each device's optional `via` (the computer that vouched for it) and `hosts`, each with a `state` of `offered`, `allowed`, `asking`, `pending`, `paired`, or `failed`. Exits 4 when there are none |
| `remote pair [--address HTTPS_ORIGIN]` | Open the pairing dialog, preferring Tailscale by default. Choose an address in the dialog, or pass an exact origin from `remote status` (for example `https://100.101.102.103:7443`). The QR and its token show only there, and the user approves the phone there. An unavailable address is reported in the dialog |
| `remote restart` | Restart the daemon from the binary beside the app, as after a rebuild or update; phones reconnect on their own. Exits 6 while the service is already changing |
| `remote revoke DEVICE` | Ask in the window to revoke a paired device, named by its ID from `remote devices` or its name (a name two devices share is refused); the user confirms there |

Remote control is turned on in Settings > Remote control, or with
`config set remote.enabled true`; the app then installs the `sheprd-remote-control`
daemon as a user service and keeps it in line with the `[remote]` settings.
While it is off, `remote status` says so and exits 0; `pair`, `devices`,
`revoke`, and `restart` exit 1. A dialog already open in the window refuses `pair` and
`revoke` (exit 6). `--json` gives `{"enabled", "running", "status", "error"}`
for `status`, where `status` is the daemon's own report, and
`{"devices": [...]}` for `devices`.

```sh
sheprd remote status --json
sheprd remote revoke d_6f1c
```

### Mods

| Command | What it does |
| --- | --- |
| `mod [list]` | The mods the window runs: where each draws, its palette commands, and its last error. Exits 4 when there are none |
| `mod reload NAME` | Read a mod's files again now, starting it over. Exits 5 when no mod of that name runs |
| `mod open NAME` | Show a mod's activity bar app in the window's body. Exits 5 when no running mod of that name has one, 8 while the activity bar is off |
| `mod panel [show\|hide\|toggle]` | The panel of mod sections on the window's right: whether it shows and which mods have a section, or show, hide, or toggle it (also the `toggle_mod_panel` command) |
| `mod logs NAME [--lines N]` | The last lines (default 50, at most 200) the mod wrote with `herdr.log`. Exits 4 when there are none |
| `mod check DIR` | Load a mod folder, run its `init` and `view` once against an empty state, and report what it draws. Runs here; Sheprd need not run. Exits 1 with the error and its line when the mod is invalid |

Mods are folders of sandboxed Luau in `<config>/herdr/gpui-mods/` that add
status bar items, sidebar sections, and palette commands; saving a file
reloads its mod. See [Mods](mods.md). `--json` gives `{"dir", "mods": [...]}`
for `list`, each with `name`, `places`, `commands`, `drawn`, and `error`, and
`{"name", "valid", "elements", "places", "hash", "log"}` for `check`.

```sh
sheprd mod check ~/.config/herdr/gpui-mods/agents
sheprd mod logs agents --lines 20
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
sheprd events --follow --kind tab_closed
sheprd wait tab-closed 12 --timeout 600
sheprd wait window            # also waits for the app to start
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
work, run on a machine the user saved. Launching the app or requesting a new
terminal can start the appropriate local daemon: installed Herdr 1, or a
user-installed/bundled herdr-core. Existing Herdr 1 sessions are not migrated,
and no command upgrades a personal daemon. `update` never installs the app;
`cloud create` installs Herdr only on the machines the user confirmed, as
creating them in Settings does.

Adding an SSH or local network device has no command: the Add Device dialog
runs it. Its setup workspace runs `sheprd __device-setup-attach SOCKET`,
an internal helper that shows the setup the app runs (see the Native Shell
README's "Local Network access"); it is not a command to call yourself, and
the socket it names is single use.

Herdr 1 scrollback search is not repeated here: `herdr pane read` covers it. Moving a
window and capturing a whole window have no command because the app cannot
do them.

## Links

`sheprd URL`, with a `herdr-gpui://` link as the only argument, hands the
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
- `sheprd COMMAND` opens the pipe at identification level, so the server
  can learn who called but cannot act as you, and checks that the pipe's
  owner is you before it sends anything. A pipe someone else owns is refused
  with exit 1.

A pipe has no receive or send timeout. Reads poll with `PeekNamedPipe`, as
the daemon client does; writes have no deadline, so the app's out buffer
holds a whole answer and the connection bound limits what a stalled caller
can hold.

## Chats on SSH hosts

Native chat views are on by default: turn off **ACP chats** in Settings >
General (`acp_chats = false` under `[features]`) to go without them. While it
is off, chat panes show their terminal and `chat new` exits 8.

Set the reading width with `sheprd config set chat.max_width 1200`, or use
`0` for the full available width. Bounded widths range from 320 to 4000 logical
pixels; the default is 800. Settings > General offers the same width slider and
**Full-width chat** switch.

A chat on an SSH host shows in the native chat view like a local one. Sheprd
runs `sheprd-acp connect ID` on the host over the same SSH connection it uses
for Herdr, and the program there sees an ordinary client on its own socket.
Starting a chat there (`chat new`, or the workspace menu) runs the host's own
`sheprd-acp`, looked for in `~/.herdr/bin`, `~/.cargo/bin`, `~/.local/bin`, and
then the remote `PATH`. Install it on the host with:

```sh
cargo install --locked --git https://github.com/sheprd-labs/sheprd sheprd-acp --root ~/.herdr
```

The workspace menu offers only the agents whose own ACP command is installed
on the host, as `sheprd-acp agents` lists them; an agent `npx` would have to
download first is left out.

Files dropped on a remote chat are copied to the host first, into private
temporary files under its `${TMPDIR:-/tmp}`, and the copies are attached.

### Existing agent rendering

Set `chat.render_existing = true`, with `features.acp_chats` left on, to
render existing Linux Codex/Claude conversations automatically. The original
terminal input remains interactive in the same pane. `chat status --json`
reports `input_mode` (`chat`, `terminal`, or `read_only`) and `session_id`;
`blocked` status means the running agent needs input. `chat show/hide/toggle`
still switches the pane presentation, without starting or stopping its agent.
