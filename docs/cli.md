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
| `ui panel workspaces\|overview\|pull-requests\|issues\|usage` | Show an activity bar section: Workspaces with the sidebar expanded, Overview of what needs you on every host, the focused workspace's pull requests or issues across the window, or every host's plan usage |
| `run COMMAND` | Any app command by its `[keybindings]` name, as the palette runs it |
| `commands` | The names `run` accepts (no app needed) |

### The app's own tabs

| Command | What it does |
| --- | --- |
| `tab [list]` | Pages, code, reviews, and Devices tabs with IDs |
| `tab focus\|close ID` | |
| `browser open\|reload\|feedback` | See [browser tabs](../crates/herdr-gpui/README.md#browser-tabs) |
| `browser list` | Page tabs only |
| `browser navigate ID URL` | Load another address in a page tab |
| `browser back\|forward ID` | |
| `browser text ID` | The page's visible text |
| `browser screenshot ID PATH` | A PNG of the visible page, at most 2000 px a side (macOS) |
| `code open FILE[:LINE]` | A read-only code tab at a line |
| `code edit FILE[:LINE]` | Your terminal editor, in a new pane beside yours |
| `devices` | The Devices overview |
| `orchestrator [--repo PATH]` | Show the Issues section narrowed to your checkout's project, or PATH's |
| `orchestrator items\|issues\|prs` | What it last synced, from its local database (no app needed) |
| `review` | The review of your workspace's uncommitted changes |

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

### Hosts

| Command | What it does |
| --- | --- |
| `host [list]`, `host select ID` | The window's hosts; show another one |
| `pr [status]\|checks\|comments\|open` | Your branch's pull request, on the window's selected host. `comments` needs your workspace focused |
| `ports [list]` | Ports your workspace listens on, and forwards |
| `ports forward\|unforward PORT` | Forward a port of the SSH host to this machine |
| `ports open PORT` | `localhost:PORT` in a tab, forwarded first when remote |
| `usage`, `system` | Plan usage and load of the selected host |
| `cloud [list]` | Saved cloud machines (no app needed) |
| `cloud add` | Open the settings that add one |
| `cloud setup ID` | Run the `[cloud]` setup script on a saved machine again, in a new tab once the window shows it. `ID` is the machine ID from `cloud list`, its host ID, or its label |
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

Focus-on-hover between split terminal panes is on by default. Disable it with
`herdr-gpui config set focus_follows_mouse false`.
The change applies live; `config get focus_follows_mouse` reads it back.

### Events

`events` prints what happened in the app: windows opening, closing, and
gaining focus; the app's tabs opening and closing; notes kept for a pane;
worktree notes changing; and host status changes. Each event has a sequence
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
seen, and adding a cloud machine open the app's own dialog (archiving a
worktree has no command at all), already pointed
at your checkout, and the user confirms there. Nothing here installs,
starts, stops, or upgrades Herdr, and `update` never installs the app.

Adding a device has no command either: the Add Device dialog runs it. Its
setup workspace runs `herdr-gpui __device-setup-attach SOCKET`, an internal
helper that shows the setup the app runs (see the Native Shell README's
"Local Network access"); it is not a command to call yourself, and the socket
it names is single use.

Scrollback search is not repeated here: `herdr pane read` covers it. Moving a
window, capturing a whole window, and destroying a cloud machine have no
command because the app cannot do them.

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
currently 4. Requests and responses are bounded to 256 KiB, a request that
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
