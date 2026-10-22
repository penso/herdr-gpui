# Herdr Native Shell

A GPUI 0.3.6 (`gpui-pre`) client for a Local daemon and saved SSH hosts, with macOS
support, Linux x86_64/ARM64 builds, and a Windows build with
native CI coverage. See [Windows](#windows) for what is unavailable there.
It starts an installed local `herdr server` when absent; explicit socket and
development targets remain attach-only. On Unix, GUI launches resolve the login-shell
environment once on the connection worker so daemon plugins can find tools such as
`node`. Terminal launches skip the shell probe. Failed probes fall back to standard
per-user and Homebrew bin directories; the probe has a five-second timeout.
It does not link or install Herdr, stop
daemons, spawn a local PTY, or emulate a terminal. Herdr's remote bridge may start
the named remote session. SSH requires an installed POSIX Herdr, noninteractive authentication,
and an already trusted host key. For hosts that need MFA or a password, configure
`ControlMaster auto` with a `ControlPath` in `~/.ssh/config` and authenticate once
with `ssh HOST` in a terminal: the app reuses that master connection while it lives,
but never creates or keeps one itself. Set `ForwardAgent yes` only for trusted hosts;
Herdr servers that support it then keep remote panes' `SSH_AUTH_SOCK` working
across reconnects. A failed host is retried with backoff capped at 30 seconds.
Runtime dependencies include GPUI, `herdr-client`, `serde_json` for API parameters,
`ureq` for background GitHub owner avatar downloads, and `serde`/`config` (aliased
as `config_loader`, TOML-only) for GUI configuration. `toml` preserves strict
field types during deserialization; `toml_edit` preserves comments on settings saves.

Solid light/heavy box-drawing characters, block elements (including fractional
blocks and quadrants), and the four solid prompt separators (U+E0B0, U+E0B2,
U+E0B4, and U+E0B6) are drawn on the terminal cell grid, with device-pixel-aligned
edges. Borders, block-art logos, and prompt caps reach the full cell height
regardless of font line spacing. The sub-cell remainder below the grid continues
the last row's backgrounds, so a full-screen app's background reaches the pane
edge, unless that row holds one of these separators, so a prompt on the last row
stays cell-high. Backgrounds, highlights, underlines, and separators share snapped
absolute cell edges to prevent one-pixel seams at fractional cell widths. This
rendering is independent of the program producing the terminal cells.
Dashed, double, rounded and diagonal box lines, shading characters, and
graphemes with combining marks continue to use font rendering.

```sh
cargo run -p herdr-gpui
cargo run -p herdr-gpui -- --session default
cargo run -p herdr-gpui -- --session my-project --dev
cargo run -p herdr-gpui -- --socket /absolute/path/to/herdr-client.sock
```

Without flags, discovery follows `herdr-client`'s environment and release-session
rules. `--socket` must name the binary **client** socket, not the JSON API socket.
`--dev` selects the `herdr-dev` config directory. Connection failure is displayed
in the single-row status bar and host rows. Endpoints reconnect independently with
bounded backoff; Terminal > Reconnect retries the selected endpoint immediately,
without input replay. The command palette's **Detach** disconnects the selected
endpoint without closing the window or stopping the daemon, and pauses retries
until Reconnect. It follows Herdr's `[keys].detach` binding (default: `Ctrl-B`,
then `Q`); set `detach = ""` under `[keys]` to leave it unbound.
A selected host that drops stays selected while it reconnects: its last terminal
picture stays up, dimmed, under a card with the reason and a Reconnect now button,
until the new connection presents its own frame. Keys typed meanwhile are not sent,
and a flash says so. Only a surface activation that fails on a live connection
falls back to Local. The card names why SSH refused a host: an untrusted host key,
failed authentication, an unreachable host, or no Herdr installed. The
first two and the last need the user, so those hosts wait the full 30 seconds
between retries rather than backing off from half a second. After the machine
sleeps (the wall clock runs ahead of the monotonic one, which stops while macOS
and Linux are suspended), every live SSH link must answer a ping within three
seconds or reconnect, and every dropped endpoint is dialled at once instead of
at the end of its backoff.
The status dot pulses amber during local daemon startup and is red when disconnected.
Healthy connections leave the status bar quiet; connection indicators live in the
device picker. Startup, disconnection, and operation errors remain in the status bar.

For split terminal panes within a tab, **Focus follows mouse** is on by default.
Disable it in **Settings > Tabs & panes > Panes**, or run
`herdr-gpui config set focus_follows_mouse false`.
Hovering a pane focuses it when the window is active and
terminal input has keyboard focus. Held mouse buttons, modifiers, dragging,
IME composition, menus, and terminal popups suppress hover focus.

Normal launches restore the main windows left open at quit, including each
window's size and screen position, using the current launch's connection options.
Closing an individual window removes it from the saved set; closing the final
main window retains its geometry for next time. Additional windows cascade from
the last open main window. Fullscreen windows restore to their normal rectangle.
Each window reopens on the display it was on; if that display is disconnected,
it opens on the primary display, resized and moved to fit. Logs windows
are not restored. Geometry is stored in `window-state.json` under
`$XDG_STATE_HOME/herdr/gpui`, or `~/.local/state/herdr/gpui` by default
(`%LOCALAPPDATA%\herdr\gpui` on Windows when `HOME` is unset). Native
test modes skip this state. Up to 64 main windows are restored.

The Rust GitHub updater verifies signed archive manifests and presents a shared
GPUI panel through **app updates** in the sidebar menu or **Herdr > Check for
Updates...**. Background offers change the status version label without taking
focus. **Download** and **Install Update** are separate approvals; closing the
panel does not cancel work. Explicit **Cancel** requests cancellation.
The panel shows archive download progress and an animated bar for work without a
known percentage. Homebrew-managed installs use `brew upgrade --cask herdr-gpui`;
if the installed version is behind the offer, `brew update` refreshes metadata
before one retry. Linux distribution installs download the signed package matching
the owning package manager and invoke APT, DNF, or pacman through PolicyKit.
Package-manager upgrades cannot be cancelled mid-install and offer **Restart**
on completion. Standalone installs restart as part of installation.
The existing UI timer polls the updater mailbox; workers own blocking work, and
the restart helper is dispatched before CLI parsing or GPUI startup.

Only normal launches with a calendar release version `YYYYMMDD.COUNTER` (tag
`vYYYYMMDD.COUNTER`) and embedded public signing key start the service. Test
fixtures use a disabled, worker-free updater. Update targets are macOS app
bundles, published Linux `.deb`, `.rpm`, and Arch packages, and user-owned Linux
executables under `HOME` on x86_64/aarch64 GNU systems.
**QA > Show app update available** and the sidebar's **preview app update** use
independent synthetic version `9999.0.0`: Download becomes Ready and Install Update
only dismisses the panel. No preview action reaches the updater service or quits.
**QA > Show update download progress (50%)** displays a half-filled bar;
**QA > Show Homebrew update progress** displays the animated activity bar.
Both remain visible until dismissed and never run a real update.
See [update setup and QA](../../docs/updating.md).
The protected release workflow builds both macOS and both Linux architectures
with the required public key. Linux manual `Herdr-VERSION-TARGET.tar.gz` archives
include desktop integration and notices; updater-only
`herdr-gpui-VERSION-TARGET-update.tar.gz` archives contain one executable.
Native two-version update/restart QA remains pending.

The macOS **QA** menu requires the opt-in `qa-menu` Cargo feature and is excluded
from default Cargo builds and published releases. `just run` enables it
automatically, or use
`cargo run --locked --release -p herdr-gpui --features qa-menu`.
The menu offers **Show NeedsAttention toast**, **Show Finished
toast**, **Show UpdateInstalled toast**, and **Show Custom toast**. Each adds a
synthetic in-app toast for the selected endpoint, even when disconnected. Each
preview replaces the visible card immediately, bypassing disabled delivery,
delay, active-target suppression, and agent evidence checks. It uses the configured
corner, normal kind-specific lifetime, and dismiss button.
NeedsAttention and Finished previews retain the current target,
when available, so clicking them tests normal navigation; other previews are inert.
Creating previews does not contact the daemon or updater. Close any in-app
panel first: toasts remain hidden while a panel is open.

**Send notification in 3 seconds** waits long enough to switch to another app,
then posts a real OS notification (see [System notifications](#system-notifications))
for the selected host's focused pane, whatever the delivery setting. Clicking it
tests the normal focus and navigation path. On macOS it needs the `just run`
bundle, and the first one asks for notification permission.

Spaces lists Local first, then saved hosts in the upstream catalog's order.
Enabled hosts connect in the background with inactive terminal surfaces; disabled
hosts remain visible. Host and repository collapse state is endpoint-scoped, and
Agents aggregates all connected endpoints with host labels. The catalog is read
through `herdr-client` every two seconds; changes to targets, sessions, enablement,
and ordering are reflected without restarting. Catalog errors preserve the last
valid list. Saved-host edits and remote provisioning are delegated to Herdr's CLI.
An explicit `--socket` is isolated: it never loads or connects saved hosts, or
reads/writes saved selection. Other launches read `client/endpoint-selection.json`
once, under the same release/dev state root as the catalog. Local remains usable
while the desired host connects; restoration waits for its first snapshot rather
than timing out during SSH startup. Explicit host/workspace/agent choices cancel
pending restoration and persist selection asynchronously, without editing hosts.
Other running clients' choices never change this window's selection. Missing,
malformed, disabled or removed saved choices fall back to the valid legacy
catalog selection (normally Local), matching upstream. Live removal/disable
returns to Local and cancels pending restoration; re-enabling does not steal focus.
Automatic activation failure returns to Local without overwriting the saved
preference or repeatedly attempting the same handoff. Write failures are shown
in the status bar and do not undo the UI choice. Rename, remove, enable, and
disable remain available through `herdr machine`.

The fixed bottom-left device picker offers **All Devices**, **Local**, saved SSH
devices, and **Add Device…**. All Devices shows Spaces and Agents across hosts
without changing the active terminal. Choosing a device filters both lists and
switches the terminal through the existing surface handoff. While filtered,
navigation to another device follows that device; removal or disable falls back
to Local. The filter is window-local and starts at All Devices. The adjacent gear
opens the existing Settings page (also available with `Cmd-,`). Beside it, the
sessions icon lists this machine's sessions and each saved device's own, grouped
under the device, with the device this window is on leading (also available with
`Cmd-Shift-S`). A device's list is asked for over SSH, at most once every 30
seconds while the popup is open; until it answers, and if it never does, the
device still offers the session it was saved with. Choosing a session attaches
this window to it on that device.

Set `device_footer = "auto"` under `[sidebar]` to draw this footer only while
there are two or more devices (or one is being added), or `"never"` to leave it
out; the default is `"always"`. Without the footer, the Agents list takes its
height, Settings and Sessions keep their shortcuts and palette entries, and
**Devices Overview** in the palette still offers **Add Device**.

The current session has an accent highlight; status dots and delete icons stay
aligned across rows. The **Add session…** row has no status dot.
Session and worktree context-menu actions share right-aligned 14-pixel icons
in 24-pixel slots. Trash buttons have a separate high-contrast hover background,
including on highlighted session rows.

Each section offers **Add session…**, opening a centered modal. Enter a name and choose **Create and connect**
to start a named headless server locally, or attach through the saved device's
SSH bridge (which starts its named server if absent). This never opens a nested
interactive Herdr TUI. Names use 1–64 ASCII letters, digits, dots, underscores or
hyphens, excluding `.` and `..`. Names already in the cached list are refused;
if another client creates that name meanwhile, Herdr connects to it rather than
overwriting it. Startup failures appear in the connection status.

Choose a row's **trash icon**, or select it with the arrow keys and press Delete /
Backspace, then confirm **Delete permanently** in the centered approval modal.
Confirmation stops the named session, terminates its running processes, and
deletes its saved state. Matching endpoints in this window move to `default`
before the operation, including when deleting the currently displayed session.
Cancel does not stop or switch anything. Herdr cannot delete `default`.
The picker stays visible beneath the confirmation modal. After successful
deletion, the row fades over 260 ms without moving or resizing the picker;
it is removed after the fade finishes. Progress stays inside the row rather
than adding a header. Failed deletions keep the row and show the error.
A remote session still
referenced by a saved device profile must have that profile removed or
reconfigured first, so a later launch cannot silently recreate it.

Deletion runs `herdr session stop --json -- NAME` followed by
`herdr session delete --json -- NAME`, locally or over noninteractive SSH.
Local stopping has a 20-second deadline and deletion a 15-second deadline; the
whole SSH operation has a 45-second deadline. There are no automatic retries.
An already-stopped session can still be deleted: the delete command rechecks
running state and refuses any daemon that remains live or restarted meanwhile.
The picker refreshes after success or
failure, including timeouts whose result is uncertain. File, process and SSH
work runs off the UI thread, with one deletion outstanding per window.
Local add/delete is unavailable with explicit sockets or development catalogs;
saved-device management is unavailable for disabled devices.
Remote hosts need an installed Herdr supporting the session commands and
noninteractive SSH authentication. The GUI does not install or update it.

**Add Device…** opens one dialog for every kind of device. A rail on its left
lists SSH hosts (then WSL distributions on Windows) and, under **Cloud**, each cloud
provider the build includes, shown by its logo: a green dot marks a provider
whose account is configured, and **Set up** one that is not. Each entry keeps
its own form, so switching between them keeps what was typed. **Ctrl-Tab** and
**Ctrl-Shift-Tab** move through the rail from any field. In a narrow window the
rail shows only the logos.

The SSH form accepts an SSH target, label, and optional remote session (default:
`default`). Herdr's `machine add` saves a new profile every time and takes no
lock, so the dialog keeps a host from being saved twice itself. A host counts as
already saved when a profile with the same session reaches the same user, host
name, and port as `ssh -G` resolves them, so an SSH alias, `user@address`, and
`ssh://` spellings of one machine all match. The host is claimed for this app
before anything else, so a second add from any window is refused while the
first runs. The catalog file is read again right before `machine add` runs and
right after: if another client saved the same host in between, the profile added
second is removed, so exactly one remains. A terminal setup keeps the claim
while it runs and releases it when it ends, saved or not, so a failed setup
never locks the host out; if the setup workspace never runs, the claim is
released after two minutes. Another client adding the host while a terminal
setup runs can still create a duplicate. A host refused as already being added
is still in another Add Device dialog or in a running "Set up" workspace.

Under the SSH target, **Nearby devices** suggests hosts to add. Opening the
dialog starts one search, shown by a sliding bar, which lasts about ten seconds;
**Search again** repeats it. Suggestions come from three places, and a machine
two of them name is listed once:

- **SSH config**: concrete `Host` aliases in `~/.ssh/config` and the files it
  `Include`s. Patterns, `Match` blocks, and git hosting services are skipped.
- **Tailscale**: online peers from the local `tailscale status --json`, by their
  MagicDNS name, on Linux, macOS, and BSD peers. Phones, TVs, and Windows peers
  are left out. Tailscale not being installed is not an error.
- **Bonjour**: hosts that advertise `_ssh._tcp` on the local network, such as a
  Mac with Remote Login on or a Linux host whose Avahi publishes SSH. This
  machine is skipped.

Choosing a suggestion fills in the target with the user's own alias when there
is one, then the Tailscale name, then the Bonjour host. Devices already saved
are not suggested. Nothing is scanned: a host that runs SSH without announcing
it, or that is on another subnet, appears only through SSH config or Tailscale.
On macOS 15 and later the first search asks for Local Network access; see
[Local Network access](#local-network-access).

Typing in the SSH target filters the suggestions by name or target, those that
start with what was typed first. When a target extends what was typed, the rest
is drawn dimmed after the cursor and **Tab** fills it in, along with an empty
label. **Up** and **Down** pick any row, and **Tab** or **Enter** then fills it.
A typed `user@` is kept. **Enter** otherwise adds exactly what the field shows:
a row that matches only by the name a host announces is never chosen for you.

Each device's header in the Spaces list starts with a mark for where it runs:
a monitor for this machine, the provider's logo for a cloud device, and a
terminal for an SSH host or WSL distribution. An SSH host shows the Coder logo
when its name ends in `.coder`, as `coder config-ssh` names workspaces, and the
Tailscale logo when it is a `*.ts.net` name or a Tailscale address
(`100.64.0.0/10` or `fd7a:115c:a1e0::/48`). Hover the mark to see what it means.
Orca and Minimal draw no icons on their rows and draw no mark either. Set
top-level `show_device_icons = false`, or turn off **Show device icons** under
Settings > Sidebar, to leave it out everywhere; the name
then starts where the mark was.

Right-click a saved SSH device's header in the Spaces list to **Rename** it or
choose **Remove device…** to forget it. Renaming runs `herdr machine rename`;
an empty name falls back to the SSH target, as when adding. Removal runs the installed `herdr machine remove`, which
edits only the local catalog: the host's own Herdr keeps running. When the
device has its own GitHub sign-in, the confirmation also offers to delete it,
since its account panel goes away with the device. Local has no such menu.

A cloud device's header (Azure, Boat, Coder, Daytona, or Railway) has a menu
too: **Rename…** changes the name the sidebar shows, falling back to the
machine's name when left empty; **Cloud device settings…** opens its provider
under Settings > Cloud devices; and **Remove device…** removes it from this
computer. For an Azure machine, a Boat or Daytona sandbox, or a Coder
workspace, it deletes the machine at the provider by default, with **Remove
from Herdr only** beside it; a Railway VM is left as it is and expires on its
own. `herdr-gpui cloud destroy` asks the same for several devices at once,
and `herdr-gpui cloud create` asks to make several (see `docs/cli.md`). Port forwards and server keybindings need a saved SSH
host, so a cloud device does not offer them.

**Forward port…** in the same menu forwards one of the host's ports, such as a
dev server an agent started there, to this computer. Each forward has its own
`ssh -N` master on a private control socket, which is then asked to listen with
`ssh -O forward -L 127.0.0.1:<local>:localhost:<remote>`. That request succeeds
only once this forward's own SSH holds the port, so a port another local
process took first is never shown as forwarded. The local port keeps the remote
number when it can, privileged ports move up by 10000 (80 becomes 10080), and a
refused port falls back once to one the system picks. The menu lists the host's
forwards as connecting, listening (with **Open**, which shows
`http://127.0.0.1:<local>/` in a browser tab; the address rather than
`localhost`, since SSH listens on IPv4 loopback only), or ended with the
reason, and a flash reports each change. Nothing reconnects: a forward ends
when you stop it, when its host is disabled or removed, when the window closes,
or when the app quits, and stays ended until you forward the port again. A
dropped Herdr connection leaves it running. The master uses the bridge's
noninteractive SSH policy but never a master of yours, which would keep the
forward after it is stopped, so hosts that authenticate only through a master
cannot forward. It still applies `LocalForward`/`RemoteForward` entries from
your SSH config, as `ssh -N` would. Forwarding needs a Unix client, like every
saved SSH device.

The label is optional: an empty one names the device after its SSH target as
typed, such as `user@host` or an address. Once the device is saved, the dialog
closes by itself unless there is recovery advice to read. That warning wraps
inside the scrollable dialog and stays until you choose **Done**, close the
dialog, or press Enter or Escape; the device is already saved.

**Add device** first checks the host over non-interactive SSH, using
the same executable search and compatibility rules as the connection bridge, and
never installs or starts anything while checking:

- Herdr running, or installed but stopped: the installed `herdr machine add`
  runs without a terminal and saves the device. It starts a stopped server
  itself. Any approval it would need fails instead of waiting for input.
  If it refuses to save because the existing server needs a restart, the app
  checks the host again. A still-running host that passes the app's compatibility
  probe is saved locally with a warning about version/startup differences and
  SSH recovery. Adding it does not require restarting the server or its panes.
- Herdr missing: the dialog asks "Herdr was not detected on the host. Should we
  install it?" An outdated Herdr asks to update it instead.
- SSH needs a prompt (unknown host key, password, passphrase), the check failed,
  or saving without a terminal failed: the dialog offers to continue in a
  terminal.

Accepting creates a new workspace on this device's Herdr and shows the setup
there. The setup itself, `herdr machine add` (after `ssh-copy-id` when the host
refused every key), runs as a child of this app on a terminal the app owns; the
workspace's shell only runs `herdr-gpui __device-setup-attach SOCKET`, which
relays the pane's keys and size to it over a private, single-use socket in the
app's state directory. It runs with your login `PATH` and agent, so it uses
your own `ssh` (Homebrew's or any other), SSH config, and security key, exactly
as a terminal would. Herdr handles SSH prompts, approval to install/update
remote software, server startup, and saving the machine only after successful
setup; complete any prompts in that workspace. The saved device appears
automatically on the next catalog refresh. No credentials are collected by the
GUI. Closing the workspace hangs the setup up, and so does quitting the app,
since the setup belongs to it. Explicit-socket and development-catalog windows
do not offer setup. Windows has no terminal setup yet: there the dialog names
what needs a terminal and asks you to run `herdr machine add TARGET` in one
(see [Windows](#ssh-hosts)).

#### Local Network access

macOS 15 and later ask before an app reaches devices on the local network, and
judge every process by the app responsible for it. Everything the app runs for
devices (the Bonjour search, host checks, saving, connecting, and the setup
above) runs as Herdr's own child, so it shares the one **Herdr** entry in
System Settings › Privacy & Security › Local Network, with whichever `ssh` you
use. The first time, macOS shows its alert and refuses the attempt until you
answer; the search and the host check wait up to a minute for it and continue
on their own once you allow it. If you denied it, **Open Local Network
Settings** in the dialog goes to the setting; turn on Herdr and search or add
again, without restarting the app.

A refused connection reads as "no route to host", which an offline device or a
wrong route also produces, so the dialog only blames the permission when the
app's own multicast check is refused too. Otherwise it reports no route to the
host.

Commands you type into a terminal are a different matter. Their processes
belong to the Herdr daemon, not to this app, and a daemon started outside the
app, as from another terminal, leaves each of them responsible for itself: macOS
then lets Apple's own tools such as `/usr/bin/ssh` and `nc` reach the LAN but
refuses third-party ones such as Homebrew's `ssh` with "no route to host",
without an alert and without an entry you could turn on. This app cannot change
how the daemon's processes are judged; that needs a change to how Herdr
launches its daemon on macOS. Device setup is not affected, since it runs in
the app.

### Coder workspaces

Settings > **Cloud devices** holds accounts with providers that create machines
to use as devices: Coder, Daytona, and Railway. Coder's card edits the `[coder]`
keys (see `config-gpui.example.toml`), saves the OAuth client secret to the
credential store rather than the config file, signs in and out, and lists the
saved Coder devices with a Remove action. A secret set in the config file or
`HERDR_CODER_OAUTH_CLIENT_SECRET` takes precedence over the saved one.

Once Coder is configured, **Coder** in **Add Device…** shows its form; before
that, it points to this Settings card. The form signs in with Coder's
OAuth2 provider (authorization code with PKCE; the browser returns to the
configured loopback redirect), then lists the account's templates, their presets,
and existing workspaces. It opens with the first template, its default preset,
and a random `prefix-adjective-noun` name already chosen, so creating takes one
click; choosing an existing workspace attaches it instead.

Creating or attaching runs in the background: the dialog closes at once and can
start another while earlier ones are still building. The devices icon in the
footer counts the workspaces being added, the device picker lists each under
**Adding** with its latest status (creating, building, installing Herdr), and a
toast reports each one ready to use, or why it was not added. Closing the window
cancels unfinished jobs.

Templates need not include Herdr. The dialog's **Install Herdr if it is missing**
switch, on by default, is the approval to run Herdr's published installer
(`curl -fsSL https://herdr.dev/install.sh | sh`) inside the new workspace, which
verifies the release checksum and installs to `~/.local/bin`. This is the only
remote installation the GUI performs, and only with that approval; reconnects
never install anything. With the switch off, a workspace without Herdr is not
added. A workspace without `curl` must get Herdr another way.

Cloud devices, from every provider, are stored in the GUI's own
`cloud-devices.json` in its state directory, because Herdr's `endpoints.json`
schema is SSH-only. Each entry names its provider, so adding another provider
needs no new file. They connect by
running Herdr's remote bridge through `coder ssh`, with the deployment URL and
access token passed only in that child's environment; Coder's tunnel
authenticates the connection, so no SSH config or host key is involved. The
`coder` CLI is required, found on PATH, in `/opt/homebrew/bin`, `/usr/local/bin`,
`~/.local/bin`, or at `[coder] cli`. A stopped workspace (for example after
autostop) is started before connecting. Selecting a saved workspace in the dialog
offers **Remove device**, which forgets it without touching the workspace.

The sign-in is saved under the same rules as the GitHub one: the Keychain in
signed macOS release builds, the Secret Service on Linux, otherwise the private
`coder-credentials` file beside the GUI config. It is tied
to the deployment and OAuth client; changing either signs out. Tokens are renewed
before expiry and on rejection. File drops are refused on Coder panes, and
restoring the last selected device at startup covers SSH devices only. Coder
devices are unavailable on Windows.

### Boat sandboxes (experimental)

Settings > **Cloud devices** > **Boat** holds a Boat account: the API URL
(`https://boat.dev/api/v1` for Boat's cloud), an optional organization wallet,
machine type, and environment, saved to `[boat]`, and an API key saved to the
credential store (`HERDR_BOAT_API_KEY` takes precedence). The key needs
`sandbox.create`, `sandbox.read`, `sandbox.update`, `sandbox.resume`, and `ssh`
actions. **Create sandbox** creates one with a generated name and, when the
**Install Herdr** switch is on (the default), installs Herdr if it is missing.
The device picker shows progress and a toast says when it is ready. The device
is saved as soon as Boat returns its ID, before waiting for readiness or
installing Herdr. If setup fails or is cancelled, **Finish setup** beside the
saved device retries that same sandbox, using the **Install Herdr** switch
above. It does not create a replacement. If saving the device itself fails,
the error includes its sandbox ID so it can still be found in Boat. Removing
a device leaves its sandbox in Boat.

**Auto-stop after** defaults to `never`: the sandbox keeps running and billing
until you stop it in Boat. To set a deadline, enter 1–2592000 seconds (30 days);
trial accounts require a deadline of at most 7200 seconds. This is a fixed
deadline, not an idle timeout: SSH activity does not extend it. The setting is
sent explicitly on creation and resume. To extend a running sandbox from now,
or disable its deadline, save the setting and click **Finish setup**. The config
key is `[boat] auto_stop = "never"` (or, for example, `"7200"`).

If creation's reply is lost, **Recover creation** retries the persisted
idempotency key and original request, including its original machine options,
even after restarting the app. It cannot create a replacement with a new key.
Recovery is available for 23 hours; afterwards the app refuses to replay the
request because Boat expires idempotency keys after 24 hours. Check the Boat
dashboard before using **Forget pending request…** and its confirmation: the
original sandbox may already exist and still be billed. Forgetting removes
only the local recovery record. Credential or API errors retain that record.

Each connection resumes an archived sandbox, authorizes a GUI-owned Ed25519
public key through Boat's API, and runs Herdr's remote bridge with OpenSSH. The
private key stays in the GUI state directory. Boat returns the current machine's
host key over the authenticated API connection; the app pins that exact key, so
a resume onto a different machine remains strict without trusting a first SSH
connection. Boat devices need `ssh` and `ssh-keygen` on PATH and are unavailable
on Windows. The API path is covered by tests against a scripted server, but has
not yet been exercised against a live Boat account.

### Daytona sandboxes (experimental)

Settings > **Cloud devices** > **Daytona** holds a Daytona account: the API URL
(`https://app.daytona.io/api` for Daytona's cloud), an optional organization,
region, and snapshot, saved to `[daytona]`, and an API key saved to the
credential store (`HERDR_DAYTONA_API_KEY` takes precedence). **Create sandbox**
creates one with a generated name and, when the **Install Herdr** switch is on
(the default), installs Herdr in it if it is missing; the device picker shows
its progress and a toast says when it is ready. **Daytona** in **Add Device…**
opens this tab. Removing a device leaves
its sandbox in Daytona.

Each connection starts a stopped sandbox, asks the API for a 10-minute SSH
token for it, and runs Herdr's remote bridge through Daytona's SSH gateway.
The token goes in a private temporary `ssh` config file that also replaces your
own, never on the command line. Daytona publishes no host key for its gateway,
so the first key seen is pinned in `daytona_known_hosts` in the GUI's state
directory and a changed key is refused. Daytona devices need `ssh` on the PATH
and are unavailable on Windows. This provider is a proof of concept: the API
path is covered by tests against a scripted server, but not yet exercised
against a live Daytona account.

### Railway free VMs (experimental)

Settings > **Cloud devices** > **Railway**, which **Railway** in **Add Device…**
also opens, sets up the temporary Linux VM that
`ssh railway.new` gives out without an account. The SSH key is its identity, so
pick an existing private key or generate one by name (an ed25519 key without a
passphrase, in `~/.ssh` when that exists, otherwise in Herdr's state directory;
an existing file is never replaced). **Connect and set up** creates the VM if the
key has none, installs Herdr in it when the **Install Herdr** switch is on, and
saves the device with the key's path (as its account). Every reconnect uses the
same key, never creates a VM or installs anything, and refuses a key file whose
fingerprint changed or whose readable private key disagrees with its `.pub`
file. Removing a device keeps its key files. Connections run
`ssh -F /dev/null` with options that offer only that key and pin `railway.new`'s
host key on first use (`railway_known_hosts`). The VM is temporary: Railway
limits its lifetime and how many a network can create. Needs
`ssh` and `ssh-keygen` on the PATH; unavailable on Windows.

### Setup scripts for new cloud devices

A new cloud device can install coding agents and run your own setup the first
time it connects. Settings > **Cloud devices** > **New device setup** has one
switch per agent (Claude Code, Codex, Gemini CLI, OpenCode), saved to
`[cloud] agents`, and previews your own lines from `[cloud] setup`, which
**Edit setup script in config file** opens:

```toml
[cloud]
agents = ["claude", "opencode"]
setup = """
git config --global pull.rebase true
"""

# Replaces either key for one provider; an empty list or script turns it off.
[cloud.coder]
agents = []
```

The script is the agents' installs, in that order and each skipped when the
tool is already on the machine, then your lines, run with `sh -e` so the first
failing command stops it. Claude Code and OpenCode use their official
installers, downloaded before they run so a failed download stops the script;
Codex and Gemini CLI need `npm` on the device and
are skipped with a note when it is missing (a system Node may also need root
for `npm install -g`). Open a new tab afterwards to pick up PATH changes.

Each provider's form has a **Run the setup script when it connects** switch,
shown when a script is configured: on for a new machine, and in Coder's form
off when attaching an existing workspace. Boat's **Finish setup** on a saved
sandbox does not run it again. When the device is ready the script
waits, for up to 30 minutes, until a window shows that device, then runs in a
new, focused `setup` tab of the device's first workspace, like a worktree's
setup script; nothing runs in the background. The script is yours, from this
computer's config, so no trust question is asked. Waiting scripts are kept in
memory only, at most eight at once; one that never ran, or a later run, is
started from **Run setup script** in the device's sidebar menu or with
`herdr-gpui cloud setup ID`. Reconnecting never runs it again by itself.

Each provider is a Cargo feature of `herdr-gpui`, on by default: `boat`, `coder`,
`daytona`, and `railway`. A build with `--no-default-features` has no Cloud
Devices section and no provider rows in the device picker; it still accepts
the `[boat]`, `[cloud]`, `[coder]`, and `[daytona]` tables, so one config file serves every
build, but never reads them.

Switching revokes the old host's focus before releasing its surface, then resizes
and activates the selected host. Input waits for the activation acknowledgement
and a coherent surface at the current viewport size. Handoffs time out after five
seconds and return to Local; returning to Local never waits on a remote release.
Servers without surface-switching support remain usable as single targets.

Default and named-session startup discovers Herdr on PATH or in standard
Homebrew, Cargo, or `~/.local/bin` locations (`herdr.exe` on Windows, where the
Homebrew paths are skipped), then waits up to 20 seconds to
connect without blocking the UI. If Herdr cannot be found, an installation modal
offers an **Install** button that opens [herdr.dev](https://herdr.dev/); it never
downloads or runs an installer. After installing, choose Terminal > Reconnect.
A daemon that refuses the handshake because it is too old, or newer than this
app supports, opens a notice naming the host, its reported version, and which
side to update. For an old daemon it shows `herdr update`, which **Copy Command**
puts on the clipboard; nothing runs it, since updating may stop that host's
sessions. For a newer daemon, **Check for Updates** opens the app updater. The
notice appears once per refused endpoint, retries continue with backoff so an
updated daemon reconnects on its own, and the device list reads "Herdr update
needed" or "app update needed" until a handshake succeeds.
**QA > Show herdr non-detected modal** previews the warning without restarting,
disconnecting, or changing daemon detection. Closing the GUI leaves the daemon
and its terminals running.

Reopening the GUI keeps each pane's scrollback, which the daemon holds. Herdr
replays scrollback after the daemon itself restarts only when its opt-in
`[experimental] pane_history = true` is set. **Settings > General > Session
restore** toggles that shared setting and asks the daemon to reload it. Herdr
then saves pane output to `session-history.json` next to `session.json`, so
treat that file like terminal history. The GUI never writes terminal output to
disk itself.

Each connected SSH host's daemon reads its own config, so the same card lists
one switch per host. A switch reads that host's
`${HERDR_CONFIG_PATH:-${XDG_CONFIG_HOME:-~/.config}/herdr/config.toml}`, as
the SSH login environment resolves it, over the noninteractive connection
policy. It writes only `[experimental] pane_history`, keeping comments and
other keys. The edit is renamed into place only if the file still matches
what was read; otherwise it reports a conflict and asks for a reload. It then
asks that host's daemons to reload. Symlinked configs and files over 64 KiB
are refused, and a file the GUI creates is private (`0600`).

## Configuration

GUI settings live in `$XDG_CONFIG_HOME/herdr/`, falling back to
`~/.config/herdr/` and, on Windows, to `%APPDATA%\herdr\`. An app opened from
Finder or a desktop launcher does not inherit a shell's `XDG_CONFIG_HOME`, so
when it has none and no daemon answers at `~/.config/herdr/` (a socket file
an exited daemon left behind does not count), it asks the login shell for its
value once at startup and uses it for these settings, the daemon's
`config.toml`, and the daemon's sockets. A daemon the app starts is given the
same root, so it listens where the app connects:

- `config-gpui.toml` contains managed defaults and documentation. Its first line
  warns **DO NOT EDIT -- WILL BE OVERWRITTEN**. Startup and GUI config reload
  replace it with the current release's defaults, exposing newly added settings.
- `config-gpui.local.toml` contains your persistent overrides. Edit this file;
  omitted keys inherit the managed defaults, nested tables merge key by key,
  and arrays replace rather than append. Theme-picker saves also go here.

Close older GPUI versions before upgrading: they still write theme changes to
the old managed path rather than the local override file.

The files are created automatically. Existing pre-managed configs are copied
verbatim into the local file before the original is replaced. This preserves
comments and all explicitly set values, including old defaults; remove a local
key to follow the current default again. If a different local file already
exists, migration stops without overwriting either file and asks you to merge
them. A sibling `config-gpui.lock` serializes application writes across windows
and processes. Invalid local settings keep the current in-memory configuration
on reload. Native overrides do not modify Herdr's shared config; the shared
settings controls described below explicitly edit that separate file.

See
[`config-gpui.example.toml`](config-gpui.example.toml) for a complete example.
Font sizes use logical pixels (finite 8..48), not typographic points. Saving
`config-gpui.local.toml` automatically reloads every open GUI window, usually
within half a second. Font family, font size, theme, and layout changes apply
together; invalid edits keep the last valid settings and show a load error.
Reload waits while a theme preview/save is active. The manual GUI config reload
action remains available; daemon config reload is separate.

Settings opens a separate, reusable native window with **Appearance, Fonts,
Indicators, Sound, Notifications, Integrations, Plugins, Code, and General** in
a sidebar. General lists every optional `[features]` flag with a switch and a
description of what it does.
The terminal stays usable while Settings is open. Cmd-W (or Ctrl-W) closes only
Settings; reopening activates the existing window instead of creating a duplicate.
Herdr UI's own settings remain editable if the originating session window closes.
Each row is tagged with the app that reads it: **Herdr** for settings in
Herdr's shared config, which its terminal client follows too, and **Herdr UI**
for settings only this app reads, kept in `config-gpui.local.toml`.

Appearance provides a large live preview and a searchable, virtualized grid of
theme cards, including every discovered Ghostty theme rather than a small
shortlist. Each card shows its own background, foreground, and palette swatches;
search stays above the grid. Cards adapt to the window width, and their palettes
load off-thread into a bounded cache.
Typing filters the entire catalog without changing the applied theme. Clicking a
result or navigating with the arrow keys applies it immediately to Settings,
open app windows, and Logs. There is no Apply button. Theme selection is an
in-memory draft: browsing does not write or reload the TOML configuration.
Uncached theme definition files load in the background; recent definitions are
cached for quick switching.

Closing Settings, through its native close button or Cmd-W/Ctrl-W, saves only the
final theme and sidebar layout selections. Quitting also flushes those selections. Closing waits for
accepted work without blocking the UI; a save failure keeps Settings open with
the draft and an error so it can be retried. Other controls keep their existing
automatic-save behavior without resetting the live theme or layout. Reload also
preserves these drafts. Theme, font, and layout writes from Settings are serialized.

**Ghostty** and **Herdr** are independent library filters, both enabled initially.
Turn either off to show only the other; search text is preserved, and changing a
filter does not change the live theme or pending draft. Ghostty includes native
built-ins and theme files. The built-ins bundle popular Ghostty themes from
iTerm2-Color-Schemes under their Ghostty names, so a platform without Ghostty
still lists them; when Ghostty is installed, its copy of a built-in, including
an older spelling such as `tokyonight` or `catppuccin-mocha`, is listed once,
as the built-in. A theme saved under such a spelling stays listed and loads
from its file. Every card identifies its source, including themes
with the same name in both libraries. Herdr selections edit the shared theme,
which changes the GUI only when it follows Herdr rather than a native override.
Follow Herdr and high contrast remain available independently.

Fonts uses compact rows for Terminal, Sidebar, Tabs, and Interface, with family
and size together and a shared specimen of the selected role below. Click a
family to open the searchable installed-font chooser; **Set all fonts...** changes
families together without changing their sizes. The catalog stays hidden until
requested. Size steppers coalesce repeated changes; click a size to type an
integer from 8 through 48. Enter or blur saves, Escape cancels.
Accepted saves survive closing Settings. Family and size edits preserve configured
fallbacks and unrelated settings in `config-gpui.local.toml`. General retains
browser-skill installation/removal and configuration paths.

Settings groups its pages: Appearance (Theme, Sidebar, Fonts, Status bar),
Behaviour (General, Tabs & panes, Code, Notifications), Connect (Integrations,
Plugins, Cloud devices) and Advanced (Configuration), with a search box above
them that finds any row. Every row shows whether it is shared with Herdr or
local to this app. General has **Show listening ports**, **Checkpoint agent
turns**, **Smooth scrolling**, the clipboard notification overrides, **Remove
worktrees of merged PRs**, the browser skill and **Install beta releases**.
Tabs & panes has the tab bar, **Confirm before closing** a tab, pane or
workspace, **Focus follows mouse** and session restore. Notifications has
delivery, **Finished notifications**, the in-app toast overrides (a 0-3600
second delay field), agent sounds with a 0-100% **Volume**, and the status
indicator style and icon set. An override dropdown's first entry follows
Herdr's value and removes only the local override. Sidebar has the layout
preset, selection marks, **Device footer**, **Show agents**, **Show search**,
**Show repository icons**, **Show device icons**, spacing sliders and host
colours. Theme ends with the window backdrop and terminal padding. Custom sound
paths and per-agent sound policies remain shared-file settings. Configuration
paths are diagnostic facts rather than editable preference values.

These native edits save automatically through the serial background save path,
preserving unrelated TOML keys/comments and pending theme/layout selections.
Controls are disabled while a save or reload is in progress. Shared settings
retain their platform restrictions; Windows can edit native overrides but not
shared Herdr settings. System delivery posts OS notifications (see
[System notifications](#system-notifications)); terminal delivery remains
unsupported by this GUI.

Integrations is sorted case-insensitively by name, with the original integration
ID breaking ties. Its search field filters names and IDs without contacting the
daemon. Install actions still use the original ID and selected source host;
filtering never changes the action destination.

Theme, indicator style, sound, and toast delivery are **shared with the local
Herdr TUI**. They read `HERDR_CONFIG_PATH`, otherwise
`$XDG_CONFIG_HOME/herdr/config.toml` or `~/.config/herdr/config.toml`. The GUI uses
the release `herdr` namespace even in debug builds; select a `herdr-dev` config
explicitly with `HERDR_CONFIG_PATH` when needed. These are local preferences,
not a remote daemon's configuration: a socket does not expose the daemon's config
path or effective settings. General displays the shared file and reload control.

This follows Herdr's own client/server split, which `herdr --remote` uses too.
Selecting a different device does not change which settings apply:

| Scope | Settings | Source |
| --- | --- | --- |
| GUI-wide | Theme and palette overrides, indicator style, sound, toast delivery and clipboard toast, sidebar agent rows (`state_text`), sidebar collapsing (`sidebar_collapsed_mode`, `sidebar_start_collapsed`), wheel and right-click input (`mouse_scroll_lines`, `right_click_passthrough_modifier`), and `[keys]` including the prefix | Local `config.toml`, under `config-gpui.toml` overrides |
| Per host | Worktree directory and custom commands, plus pane defaults, pane borders, gaps, scrollbars, and border labels, and integrations, which the daemon applies itself | That host's daemon, through its snapshot |

Herdr's pane chrome settings in `[ui]` — `pane_borders` (`auto`, `always`,
`off`), `pane_outer_borders`, `pane_gaps`, `pane_scrollbars`, and
`show_agent_labels_on_pane_borders` — are applied by the daemon, not this GUI:
the daemon lays out every pane and draws its borders, gaps, and labels into
the surface it sends, with the pane, content, and scrollbar rectangles this
client paints and hit-tests from. The GUI therefore shows whatever the
connected daemon's own `config.toml` chose (reloaded by the daemon's config
reload), not the local file, and draws a pixel scrollbar thumb only where the
daemon reserved a scrollbar column. Changing these locally has no effect on a
remote device's daemon. `mouse_scroll_lines` and
`right_click_passthrough_modifier` are client input settings, read from the
local `config.toml` like the TUI does.

Keybindings stay local on an SSH device, as with Herdr's default
`--remote-keybindings local`. **Use server keybindings** in a device's
right-click menu is the equivalent of `--remote-keybindings server`: that
device's published `[keys]` profile applies while it is selected (see
Supported below). The GUI never reads a remote host's `config.toml`.

Shared saves preserve comments and unknown keys, reject conflicting external
edits and unsafe paths, and run off the UI thread. A symlinked `config.toml`, or
a symlinked directory above it, is followed as Herdr follows it when the links
belong to you or root and sit in directories other users cannot write: saves
replace the file the link points at and keep the link, while a link to a missing
file is refused. Save success
is separate from the local daemon reload request, which is reported as queued,
not acknowledged. Opening Preferences, its Reload button, and the daemon's reload
signal reread the local file. Debounced disk polling also reloads saved changes,
deferring reloads during active settings saves and font/theme picker operations.
On Windows shared
settings are readable, but shared-file writes are unsupported; native settings
remain editable through the Windows-capable local override writer.

On Unix, group- or other-writable config files and user-owned directories are
refused. For example, a `664` config under a `775` directory
cannot load shared settings or Herdr theme previews. The error identifies the
offending path and its mode. Remove group/other write permission from that path
with `chmod go-w`, then click **Reload** in Settings. For the default paths,
`chmod go-w ~/.config/herdr ~/.config/herdr/config.toml` changes `775` to `755`
and `664` to `644` without changing the config contents or read permissions.
If another ancestor or a symlink's target is writable, correct the path named
by the next error too. The app does not change permissions automatically.

Existing native theme selections remain overrides. Choose **Follow Herdr** to
use the shared theme, all 18 upstream palettes, custom colors, and automatic
light/dark selection. Selecting a shared theme disables upstream auto-switching,
as in the TUI. Status indicators always use the shared status colors,
independent of a native terminal theme override, drawn with the set picked on
**Settings > Appearance > Indicators** and saved as `status_icons` in
`config-gpui.local.toml`. This app draws only its own sets and ignores Herdr's
`ui.status_indicators`, which the TUI keeps for itself; unset means **Dots**.
The page shows each set as a card previewing all five statuses with the
sidebar's own marks:

- **Dots**: Herdr's dots, with a ring and a turning arc while working.
- **Solid**: filled discs with a cut-out mark and a crescent for idle.
- **Outline**: the same marks in outline.
- **Tiles**: each mark on a tile tinted with its status color.
- **Wave**: after Superset, every status in one 3×3 grid of cells. Working is
  Superset's wave, and a gap runs round the frame while an agent is blocked.
- **Braille**: every status a braille cell. Working spins, and a blocked cell
  empties and fills dot by dot.
- **Orca**: after Orca, a stepping ring, a question bubble when blocked, and a
  ringed check when done.

Working always moves, and moving marks turn in step on one shared clock. They
hold still when the system asks to reduce motion. Drawn icons are about a line
tall and replace the dot wherever one shows, tabs and the collapsed sidebar
included. Herdr's own rows and Orca's cards center them across all of a row's
lines, and Superset badges its icon slot with them. The **Edge rail** switch at
the top of the page (`edge_rail`) draws a bar on the leading edge of each
working, done, or blocked row, in every layout and with any set. The retired
`status_icons = "rail"` reads as Outline with the edge rail on.
Terminal/default reset colors are projected to opaque native colors.

A saved SSH device can paint its terminal with a theme of its own, so the host
an agent runs on shows where you are looking rather than only in the sidebar.
**Terminal theme…** in the device's right-click menu lists the same themes as
the picker, previews the hovered one on the terminal while that device is
selected, and saves it in `config-gpui.local.toml` as
`[devices.<id>] theme = "NAME"`; **Same as app** removes it. Any `theme` value
works, `light:NAME,dark:NAME` included, and
`herdr-gpui config set devices.<id>.theme NAME` sets it from a script (`host
list` shows the IDs). Only the terminal area changes: the sidebar, titlebar,
and menus keep the app theme. Local and explicit sockets always use the app
theme. A device theme file that cannot be read leaves that device on the app
theme and is logged; one edited in place is read again when the config next
changes.

New tab and New Workspace follow the shared `ui.prompt_new_tab_name` (default
on) and `ui.prompt_new_workspace_name` (default off). When set, every button,
menu item, shortcut, and palette entry first asks for a name, proposing the next
tab number or the workspace's folder; Enter creates, Escape cancels, and an empty
or unchanged name lets the daemon choose it, as in the TUI. Teleport, worktree
creation, and browser tabs never prompt.

Sound uses the dedicated Rodio backend, shared global/per-agent settings and
custom local paths, with Herdr's bundled sounds as fallbacks. The Sound tab sets
this app's volume and offers an explicit QA preview. Shared Herdr toast delivery enables in-app notifications
and System delivery posts OS notifications; Terminal delivery is not executed by
this native client, which has no outer terminal. Per-field
`[notifications]` settings in the native local override file take precedence.
Semantic events are bounded, target-validated, and fenced by connection/boot.
Clipboard feedback has its own shared defaults and native overrides and does not
authorize remote clipboard writes.

Integrations are managed on the **selected daemon's host** through advertised
`integration.list`/`integration.install` methods. Installation is only triggered
by an explicit button click, modifies agent hook/plugin configuration on that
host, and refreshes the list afterward. No uninstall action is offered because
the upstream binary endpoint does not advertise it. Native GitHub sign-in remains
separate from agent integrations. When the daemon reports outdated integration
assets, **Integrations** in the Settings sidebar shows an **Update** badge before
the list is loaded.

The selected daemon's product announcement appears as a card at the top right
of the terminal area until its close button is clicked. Dismissing it calls
`product_announcement.dismiss`, so every client of that daemon stops showing it;
a daemon that rejects the request brings the card back, and one that does not
offer the method only hides it for the current connection. The sidebar menu's
**what's new** (or **update ready**, when the daemon offers a newer Herdr) opens
the daemon's release notes; closing them calls `release_notes.dismiss`. Both
texts are daemon data shown as text only: control characters are stripped,
`http`/`https` links open in the browser only when clicked, and a displayed
install command is never run.

The terminal face can also be resized for the current session from the View menu,
the in-app menu, the command palette, or `cmd-=` / `cmd--` / `cmd-0`. Adjustments
are clamped to the same 8..48 range, apply to the terminal only, and are never
written to disk, so a reload or a restart returns to the configured size. While
a browser tab is focused, the same commands zoom its page instead (see
[Browser Tabs](#browser-tabs)).

A tab asks before closing only while one of its agents is working or blocked
on a prompt; tabs whose agents are idle or done, or that have none, close at
once. Set top-level `confirm_close_tab = false` to never ask for tabs
(including their running processes), `confirm_close_pane = false` to close
panes without asking, `confirm_close_workspace = false` to close a workspace
from its menu without the dialog, and `show_agents = false` to hide the Agents
section and give Spaces the full sidebar height. All four default to `true`.
A workspace close still asks when its Git check finds uncommitted files or
unpushed commits, whatever `confirm_close_workspace` says.
Set `remove_merged_worktrees = true` (**Settings > General > Remove worktrees
of merged PRs**) to remove a linked worktree's checkout, and close its
workspace and terminals, once its GitHub pull request is merged. It is off by
default because it deletes a checkout without asking. It acts only on this
machine's daemon, only on linked worktrees, and only when the checkout has no
uncommitted or untracked files, sits exactly at the pull request's merged head
commit, and has no agent working or waiting on a prompt. A repository with an
archive script is left for the delete dialog. The focused workspace opens that
dialog instead of being removed. The branch is not deleted, and a checkout
the daemon finds dirty is never force removed.
The pane dialog's **Do not ask again** checkbox (click it or press Space) saves
`confirm_close_pane = false` to `config-gpui.local.toml` once the close is sent. Saved edits apply automatically. The
**Show agents** control in **Settings > Sidebar** saves
`show_agents` immediately, independently of the layout draft saved on close.
Set `show_agents_header = false` (**Settings > Sidebar > Show agents header**)
to hide the Agents title row and its sort toggle while keeping the list. The
order then follows Herdr's `ui.agent_panel_sort` (`priority` or `grouped`)
unless it was toggled earlier or a plugin sets the agent view, which keeps the
header shown while that view is active.

Agents list in their own section below Spaces by default. Set top-level
`agents_placement = "nested"`, or pick **Under workspaces** for **Agents
placement** in **Settings > Sidebar**, to list each agent under the workspace
it runs in instead; Spaces then takes the full sidebar height. The collapsed
rail keeps its own agents section either way.

A search field at the top of the sidebar finds devices, worktrees, and
branches by name; while it holds text, results grouped by kind replace the
spaces list. Up and Down move through them, Enter opens one, and Escape
clears the search. Set top-level `show_sidebar_search = false`, or turn off
**Show search** in **Settings > Sidebar**, to hide it.

In the Normal, Compact, and Comfortable layouts and in Superset, each space's
sidebar row starts with its repository owner's avatar, or the GitHub mark;
Orca and Minimal draw neither. Set top-level `show_repository_icons = false`,
or turn off **Show repository icons** in **Settings > Sidebar**, to leave
both out and give the name their room. The Superset layout
keeps its icon slot and draws its branch glyph there instead.

`[usage]` provides independent switches in `config-gpui.local.toml`:

```toml
[usage]
show = true     # Native bottom-bar usage
topbar = true   # Daemon toolbar text, including plugin percentages
inline = true   # Daemon sidebar rows, styles, and custom tokens
```

All three default to `true` and apply on config reload. Set `topbar`
to `false` to hide daemon text without hiding the Git/account controls. Set
`inline` to `false` to use native sidebar layouts.
These switches affect GPUI only; they do not change
the daemon or its plugins. Header text uses GPUI's standard truncation: character boundaries, with trailing
space or punctuation trimmed before the ellipsis.

The status bar shows the selected host's CPU and memory: a sparkline of recent
CPU use and a memory meter, each with its current share, and cores, load
averages, memory in gigabytes, free space on the volume holding the home
directory, and uptime in its tooltip. With more than one host, each
host row in the sidebar shows its own: right-aligned gauges after the name in
compact layouts, and the sparkline and meter on a second line otherwise. This
machine is read in process; each connected Linux or macOS remote host is read
every two seconds over its own SSH shell, kept open while the host is connected
(`/proc` on Linux; `vm_stat` and a one-second `iostat` on macOS; `df -Pk
"$HOME"` on both, with `/proc/uptime` on Linux and `kern.boottime` on macOS).
Other remote systems show it as unavailable. Set top-level `show_system_load = false`, or turn off **Show CPU
and memory** in Settings, to hide it and stop sampling while no
[Devices overview](#devices-overview) is open.

The coffee cup in the status bar keeps this machine's display on and stops it
from sleeping when idle, for as long as the cup is full; click it again to let
go. It is app-wide, so every window shows the same cup. macOS holds power
assertions (`pmset -g assertions` lists them), Windows sets the execution
state (`powercfg /requests`), and Linux asks the session's
`org.freedesktop.ScreenSaver` and logind (`systemd-inhibit --list`). Quitting
or a crash lets go too. Closing a laptop lid still sleeps the machine.

Workspaces that run a server show the TCP ports it listens on, as `:3000`
chips on a line under the workspace's sidebar row and, for the focused
workspace, in the status bar. Clicking one opens the page in a browser tab of
that workspace (`http://localhost:<port>` on this machine). A port belongs to
a workspace when its process inherited the `HERDR_WORKSPACE_ID` Herdr sets in
every pane, so system services and servers started outside Herdr never show,
and its `HERDR_SOCKET_PATH` names the same daemon, so two sessions on one
machine never claim each other's servers.
Every five seconds each host runs `ss -ltnp` (Linux) or `lsof -iTCP
-sTCP:LISTEN` (macOS) in its own shell, this machine's locally, a connected
remote host's over SSH, and a WSL distribution's through `wsl.exe` (see
[WSL distributions](#wsl-distributions)), listing only your own processes. A remote server
always opens at `http://127.0.0.1:<port>` through an SSH tunnel started on the
first click, never at the address SSH reaches the host by (a Tailscale
`HostName`, say): `ssh -N -L` from this machine's loopback, with the same
noninteractive options as the remote connection. The tunnel dials the remote's
`localhost` for a server listening on every address or on loopback, and the
server's own address for one bound to a single address. Its local port is the
server's own number when that is free (a privileged port moves up by 10000, as
**Forward port…** moves it), otherwise any free one. The page opens only once SSH
itself reports the forward bound, never because something answers on the
port. Tunnels do not share a `ControlMaster` connection, so they need
noninteractive authentication (keys or an agent). The tunnel stays up while
the port is listed, is reused by later clicks, and closes when the port
stops listening, the host disconnects, scanning is turned off, or the window
closes; a page reopened after a dropped tunnel keeps its address while that
local port is free. Windows clients
do not scan this machine. Set top-level `show_listening_ports = false`, or
turn off **Show listening ports** in Settings, to hide them and stop scanning.

Set top-level `show_network_quality = true` to show latency and packet loss
from this machine in the status bar, after CPU and memory: `NET 31ms 0%`.
A worker thread times one TCP connect to `1.1.1.1:443` every 15 seconds, in
process and without ICMP, about a megabyte a day, so it suits a metered link
such as a phone hotspot; no throughput test runs. Latency is the median of
the last minute's handshakes; loss counts connects that failed or timed out
over the last five minutes, by wall-clock time, so probes from before a sleep
do not count after waking. All windows share one worker. The tooltip shows the
last probe and the range.
It is off by default; turn it on here or with **Network latency and loss** under
Settings > Status bar. Turning it off stops the probes.

`[notifications]` in `config-gpui.local.toml` overrides shared toast preferences
for GUI-local in-app delivery, independently per key. `enabled = true` shows
in-app toasts even when shared delivery is `system`; `enabled = false` leaves
shared `system` delivery posting OS notifications:

```toml
[notifications]
enabled = false
delay_seconds = 1
position = "bottom-right"
finished = true
```

`finished` has no shared Herdr counterpart and defaults to `true`. Set it to
`false` to stop "finished" toasts and OS notifications while keeping "needs
attention" and custom notifications, for example when an agent hook posts its
own summary with `herdr notification show`. It does not change sounds, which
follow the sound settings, and QA previews still appear. Settings > Notifications
has the same switch, and `herdr-gpui config set notifications.finished false`
writes it.

Delivery defaults off, matching upstream. The delay accepts integer seconds from
0 through 3600; Custom notifications always bypass the delay. Corners are
`top-left`, `top-right`, `bottom-left`, and `bottom-right`; an explicit corner in
the notification overrides this default. Preferences shows these values read-only,
following the existing config-file settings pattern. Reload applies them without
restarting: pending deadlines use the new delay relative to original arrival,
disabling clears normal pending/queued/visible cards, and re-enabling does not
replay discarded notifications. Enabling establishes an arrival cutoff, so events
already waiting in a connection inbox from the disabled period are discarded too.
Failed reloads preserve current settings. QA
previews remain available regardless of delivery settings.

The sidebar button at the left of the titlebar hides or shows the sidebar.
On Linux and Windows it stays in the same position when the sidebar collapses
to a rail. The leftmost tab strip takes the button when the sidebar is hidden
or the rail cannot fit it beside macOS's traffic lights.
It stays available when the sidebar is hidden; the existing View menu command and shortcut still work.
In **Settings > Status bar**, toggle **Usage** to turn the bottom quota display on or off.
The choice is saved to `config-gpui.local.toml` and follows the existing `[usage] show` setting.

The app's own colored marks and labels (status dots and words, pull request
badges, diff counts, usage warnings, online dots, toasts) keep a minimum
contrast against the sidebar, menus, and selected rows. Each color keeps its
hue and moves only its lightness, and only as far as it has to, so a theme
that already reads well is drawn as authored: on dark themes the status dots
keep upstream's colors, while light themes such as Catppuccin Latte get darker
versions of the same hues. **Settings > Theme > High contrast**, or
top-level `contrast = "high"`, raises that minimum from 3:1 (the WCAG level for
graphics) to 4.5:1 (the level for text), lifts dim labels to it too, and makes
selected rows stand further off the surface. Terminal output is never
adjusted; programs keep the colors they asked for.

On Windows, top-level `accent = "system"` takes the app's own accent (the
current tab, selected rows, focus marks) from **Settings > Personalization >
Colors** instead of the theme. It is read whenever a theme loads, so a new
accent shows at the next theme or appearance change, or the next start. Other
platforms keep the theme's accent.

**Settings > Theme > Window** offers three backdrop tiles: **Solid** (the
default), **Translucent**, and **Picture**, each showing only its own controls.
Translucent starts at 85%, and its **Background opacity** stepper moves in 5%
steps between 10% and 100%; `[backdrop] opacity` in `config-gpui.local.toml`
takes any value from 0 to 1. The theme's background is painted once, at the
window's root, and the terminal grid skips the cells that would only repaint
it, so a translucent window draws fewer quads than an opaque one rather than
more. The sidebar, tab strip, title bar, and status bar keep their own colors
at the same opacity, which leaves them a little more solid than the terminal.
Colors a program chooses, popups, menus, dialogs, and the Settings window stay
opaque.

**Blur behind** (`blur = true`) asks the platform to blur the desktop behind a
translucent window, through GPUI's public window API: macOS and Windows blur;
on Linux only Wayland compositors with KDE's blur protocol do, and X11 needs a
compositing window manager for any translucency at all.

On Windows 11 (22H2 and later) the blur is a DWM system backdrop that also
covers the title bar: acrylic by default, which blurs the windows behind, or
Mica (the **Mica** switch, `material = "mica"`), which tints the window with
the wallpaper the way Windows' own apps do. Windows 10 keeps the older blur,
and other platforms ignore `material`.

The **Picture** tile opens a file chooser and puts an image behind the window
instead (`image`, an absolute path or one under `~/`), and starts it at 75%
opacity if the window was opaque, so the picture shows. The picture covers the
desktop, so the window itself stays opaque and blur does not apply. It is
decoded off the UI thread, scaled to at most 3840 pixels on its longest side,
and uploaded once; files over 64 MiB are refused. A picture that cannot be read
leaves the window translucent over the desktop and logs why; choosing one in
Settings reports the error there.

Choose the sidebar layout from **View > Layout**, which lists every layout,
checks the one in use, switches at once, and saves the choice to
`config-gpui.local.toml`. The same setting can be written by hand as a
top-level line there (before any table headers):

```toml
layout = "compact"
```

**Settings > Sidebar > Layout preset** lists all nine choices. Choosing a layout
applies it live to the app windows, like themes, without writing on each click.
Only the final choice is saved when Settings closes, **Save now** is pressed, or
the app quits.

Three densities of Herdr's own rows are available:

- `normal` (managed default): TUI-like spacing, with branch lines beneath root workspaces,
  single-line worktree children, and two-line agents. Modest horizontal and heading
  spacing keeps the sidebar readable without padding every row.
- `compact`: the tightest spacing, hiding all workspace branch lines.
- `comfortable`: the previous Normal layout, with roomier padding, branch lines
  on all workspace rows, and PR addition/deletion counts.

Add `-rounded` to any density (`normal-rounded`, `compact-rounded`,
`comfortable-rounded`) for inset rows with rounded corners, a bordered selection,
and title-case section headings. Rounded rows are a little taller, and worktree
children keep their indent without tree guides, which would break across the
gaps between rows.

Normal and Compact show PR numbers without change counts. Status indicators and
agent-name lines remain visible in every density, and flat ones keep tree guides;
font sizes and terminal spacing are unchanged. Saved edits apply automatically.

When the daemon's `[ui.sidebar.agents]` rows name the `state_text` token, the
GUI shows the same status word beside each agent, in the activity dot's color,
in every layout. An agent with its own `rows_by_agent` entry follows that entry
instead, as the terminal client does. Rows without the token keep the dot
alone, so an unconfigured pair of clients renders alike.
Status dots and words follow the contrast setting described above.

Three more layouts draw rows with a design of their own, each with fixed
spacing:

- `superset`: one line per row. An icon slot carries the pull request's state
  or the repository owner, with the activity status as a compact corner badge
  (dot, symbol, or drawn icon, following the indicator setting) that leaves the
  main icon visible; Tiles tint the whole slot instead. The PR's change counts sit on the right, and the focused row is filled
  with a stripe down its leading edge. Agents show where they run after their name.
- `orca`: inset cards with a status column, the name and a `primary` mark on a
  repository's own checkout, then a meta line with the host, the branch when it
  differs from the name, and the pull request. Agents are single compact lines.
- `minimal`: one line per row with only the status dot and the name, for narrow
  sidebars or long lists.

The `devices` layout lists agents by device instead of workspaces. Every
device gets a header, even when there is only one, with its connection dot
and how many of its agents are working (`2/5`). The device's agents sit under
the header in Herdr's normal rows, and the separate agents section is hidden.
A search field under the heading keeps only the devices whose name, address,
agents, or workspaces match every word typed. The
[Devices overview](#devices-overview) shows the same devices with their
activity and load.

New installs start with `comfortable-rounded`: the first launch writes it into
the new `config-gpui.local.toml`. Existing override files and migrated personal
configs are left alone, so current users keep the managed `normal` default.
Remove that line to follow the managed default.

In code, each layout maps to a `RowLayout` in `src/sidebar/layouts/` and the
spacing around it. Render hands the layout typed row data and a shared
per-frame `RowContext`, and marks each row with `Cell::selected`,
`Cell::highlighted`, and `Cell::lift`, which says which row a workspace drag
carries so each layout draws its own lifted card. Layouts are assembled from
the shared pieces in `layouts/parts.rs`: a `Line` gives fixed pieces (icons,
status, fold) their size, lets labels shrink to a share of the row, and hands
the rest to the name, so the whole `minimal` layout is under a hundred lines.

Agent names have small theme-tinted icons for every agent Herdr detects, selected
from the daemon's agent identity: brand marks for Pi, Claude Code, Codex (OpenAI),
Gemini, Cursor, Devin, Antigravity, Cline, Mastra Code, OpenCode, GitHub Copilot,
Kimi, Kiro, Amp, Grok, Hermes Agent, Kilo Code, Qoder CLI, Qwen Code, and Muse
Code (Meta's mark), and lettermarks for oh-my-pi, Droid, Letta, and Maki.
Unknown or missing identities use a generic terminal icon, regardless of custom
names.
Icons sit immediately before the name, including orphan agents whose name is
on the first line, and reserve space before long names are truncated.

`[terminal]` `padding_x` and `padding_y` keep blank space between a terminal's cells and the edges of its tab group:

```toml
[terminal]
padding_x = 8        # left and right
padding_y = [4, 8]   # top, then bottom
```

- One number pads both sides of the axis. A pair of numbers pads each side: left then right for `padding_x`, top then bottom for `padding_y`.
- Each side is a finite number of logical pixels from 0 to 64. The default is `0`, which keeps the cells against the edges.
- Every terminal takes the padding, in every tab group, whether the sidebar shows or not. A browser, VS Code, or review tab pads its own content and takes none.
- The terminal keeps the remaining size, so the daemon is resized to the columns and rows it actually has.
- The padding shows the theme background.
- **Settings > Theme > Terminal padding** sets each of the four sides with a slider, a typed value, or one-pixel steps. It writes one number when the two sides of an axis agree, and a pair when they differ.

`[layout] sidebar_gap` is deprecated. It padded only the left side of the terminal beside the sidebar. A file that still sets it keeps loading: while `padding_x` is unset, the gap becomes the left padding of every terminal, and the app reports the key as deprecated. Setting the horizontal padding in Settings writes `padding_x` and removes `sidebar_gap`.

Any space smaller than one character cell at the right or bottom edge takes the adjacent terminal cells' background colors, without stretching text or changing input coordinates.

The `[clipboard_toast]` table controls the `copied to clipboard` flash shown
after a terminal selection is copied. Like the keymap (see the daemon `[keys]`
under keyboard shortcuts), it starts from the daemon's own config:
`[ui.toast.clipboard]` in `config.toml` (resolved like
the sound settings below) answers it first, so setting it there covers both
clients, and each key here overrides that answer on its own.

```toml
[clipboard_toast]
enabled = true
position = "bottom-center"
```

Positions are `top-left`, `top-center`, `top-right`, `bottom-left`,
`bottom-center`, and `bottom-right`, measured against the terminal area rather
than the window. Both keys default to herdr's own defaults, shown at the bottom
center, and the example file leaves them commented out so an unedited GUI keeps
following the daemon config. Only these two keys are read from that table. The
file is never written, and an unreadable, oversized, malformed, or unrecognized
value leaves these defaults standing.

The `[bell]` table decides what a pane's terminal bell (BEL) does. Herdr has no
bell setting: it forwards each bell to its foreground client and leaves the
reaction to it, as the TUI hands BEL to the outer terminal.

```toml
[bell]
attention = true  # request attention (bounce the Dock) while the window is inactive
sound = false     # play the system alert sound
```

Only bells from the selected endpoint's own connection ring; parked editor-group
connections and other endpoints are dropped, as the TUI drops presentation
effects from inactive endpoints. A burst rings at most once per 500 ms. On
platforms where GPUI does not implement attention requests or the system bell,
those settings do nothing.

**QA > Ring Bell in 3 Seconds** (QA builds) previews both reactions after a
delay, so there is time to switch to another app and watch the Dock: it plays the
system alert and, if the window is then inactive, requests attention. It
bypasses `[bell]` and the rate limit, and needs no daemon.

The window title follows the daemon's `WindowTitle` message for the selected
endpoint: a title an agent set with `client.window_title.set`, or Herdr's own
rendering of `ui.window_title` (`{hostname}`, `{workspace}`, `{tab}`, `{pane}`,
`{terminal_title}`) for this window's view. Control characters are stripped and
the title is capped at 200 characters. Without one, after a disconnect, or once
the daemon restarts, the window keeps its own `Herdr — <workspace>` title.

The `src/config.rs` module exposes `Config::load()` and
`Config::path()` (managed defaults) and `Config::local_path()` (user overrides),
all returning the crate's typed `Result`. `Config::theme()` resolves
built-ins or Ghostty files into a `Theme` with packed 24-bit RGB colors and all
256 palette entries. Theme resolution is a separate fallible step from loading
and validating TOML. Font sections can override either family or size without
repeating the other field. `FontConfig::line_height()` returns `size * 20 / 14` unless
`[terminal] line_height` sets the multiple. `[terminal] weight` takes a CSS
weight name or 100 to 900; bold cells draw `bold_font_weight` of it, 300
heavier and at least bold. On macOS, font resolution reports a weight the
family has no face for; other platforms do not check.
The `[features]` table holds opt-in behaviors as `Features`, with every flag off
by default and unknown keys rejected like the other sections; Preferences lists
each flag and its state read-only, since only the config file turns one on.
First-frame config and theme loading is read-only: no config lock, migration,
writes, or fsync delays window creation. It reads local overrides (or the legacy
file before migration) so the first frame uses the configured layout, theme, and
font sizes. Config maintenance, font fallback discovery, and subsequent reloads
run on the GPUI background executor. External theme files still require disk I/O;
startup appearance timing is recorded at debug level. Additional
windows start from the last successfully loaded pair. Failed reloads retain
current settings. Theme selection cancels pending reload application so a delayed
load cannot overwrite the newer selection.

Production operations use the root `Error`/`Result` types (`src/error.rs`) with
`thiserror` variants for validation and source-preserving I/O/parser failures.
Catalog channels retain typed errors, and rename results share errors with `Arc`
across cloned UI snapshots. Strings are produced at presentation boundaries, not
as internal error transport. `anyhow` is reserved for framework boundaries and
test harnesses, not internal catch-all errors.
Updater workers and the restart helper use typed `UpdateError` variants, preserving
sources and recovery context while keeping remote diagnostics out of display text.
Active regression tests cover typed sources, redaction, and recovery failures;
the standalone updater harness includes these tests without GPUI dependencies.

## Notification Sounds

Sounds share the **local TUI configuration**, not `config-gpui.toml` or a remote
host's files: `HERDR_CONFIG_PATH` takes precedence, then
`$XDG_CONFIG_HOME/herdr/config.toml`, then `~/.config/herdr/config.toml`.
Debug GUI builds and `--dev` still use the production `herdr` sound settings.
The GUI only reads this file. Daemon `ReloadSoundConfig` messages reload it
asynchronously; invalid reloads retain the last valid settings.

With `[usage] inline = true`, the same file's `[ui.sidebar.agents]`
and `[ui.sidebar.spaces]` configure sidebar rows for every endpoint
shown: the same tokens (`state_icon`, `state_text`, `machine`, `workspace`,
`tab`, `pane`, `agent`, `terminal_title`, `terminal_title_stripped`, `branch`,
`git_status`, and `$name` for a value a plugin reported), the same `fg`, `bold`,
`dim` and `rules`, `rows_by_agent` and `row_gap`. A token with no value drops
out, an empty row drops out, and a row too narrow for its text loses tokens
from the left. If all configured rows disappear, agents keep only their status
icon and workspaces keep one blank selectable line. Saving the file and
**Reload GUI config** re-read it; an invalid section falls back to the
default rows, including whether the status word is shown. While a section still
matches the built-in rows, the selected sidebar layout paints it. A section
that differs replaces each row's text with its lines inside the selected
layout's own frame: Herdr's rows, Superset's icon slot and stripe, Orca's card,
or Minimal's compact line. A configured row shows its status only when its
first line leads with `state_icon`.
`rows_by_agent` keys match the daemon's agent IDs directly (for example, `claude`
or `codex`); the GUI does not maintain a separate registry of allowed IDs.

[Usage Tracker (`herdr_agents_tracker`)](https://github.com/VHemanth45/herdr_agents_tracker)
uses these fields for account usage percentages in the header and per-agent
context meters in the sidebar. The same rendering supports other plugins that
publish daemon status segments or custom tokens.

```toml
[ui.sidebar.agents]
rows = [
  ["state_icon", "workspace", "tab"],
  ["agent", { token = "$usage_ctx_warn", fg = "#f9e2af" }, { token = "$usage_ctx_hot", fg = "#f38ba8" }],
]
```

```toml
[ui.sound]
enabled = true
# path = "sounds/all.mp3"
# done_path = "sounds/done.mp3"
# request_path = "sounds/request.mp3"

[ui.sound.agents]
droid = "off"
# claude = "off"
# open_code = "on"

[ui.toast]
delay_seconds = 1
```

Sound is enabled by default; Droid alone defaults to off. Agent values are
`default`, `on`, or `off`; the global switch takes precedence. Per-sound paths
override `path`, and relative paths resolve beside the local TUI config.
`HERDR_DISABLE_SOUND` or `NEXTEST`, when present, disables playback entirely.

Volume is this app's own, since Herdr's `[ui.sound]` has none: `[sound] volume`
in `config-gpui.local.toml`, a percentage of each sound's own level from 0 to 100
(default 50), also under **Settings > Notifications > Volume**. It follows loudness rather than
raw amplitude, so 50 plays at a quarter of the amplitude, about 12 dB quieter;
0 plays nothing. It applies to every sound, previews and `sound play` included,
and not to the system alert `[bell]` plays, which follows the OS alert volume.

```toml
[sound]
volume = 30
```

**QA > Play Sound** explicitly tests Rodio playback with the built-in Done sound
on the same background worker. No daemon or active pane is needed. This manual
test bypasses notification mute (including `enabled = false`), agent filters,
custom sound paths, delay, and focus suppression. Volume and environment/test
suppression still apply, as do the bounded queue, one-second queue expiry, and playback
budget below. Closing the window cancels the test; endpoint disconnects do not.

Semantic notifications from all connected endpoints use the TUI's timing:
`delay_seconds` is 0..3600 (default 1), Custom is immediate, delayed attention
requires a Blocked agent, and Finished requires projected Done state. Completion
evidence may wait up to one second from receipt, rechecking every 50 ms. New
notifications replace pending ones for the same endpoint/pane. Only Finished is
suppressed for the selected endpoint's active tab while the native window is
focused (workspace focus is the fallback for events without a tab).
Legacy `Notify`, terminal BEL, and terminal escape sequences never play Herdr
sounds; BEL can play the system alert through `[bell]`.

Built-in Done and Request MP3s are the upstream Herdr sounds, attributed in
[SOUND-NOTICE.md](SOUND-NOTICE.md). Rodio 0.22 uses CPAL native output and
Symphonia MP3 decoding, with no external players or temporary audio files.
Custom sounds must be MP3 regular files of at most 16 MiB; unreadable, oversized,
or undecodable files fall back to the built-in sound. Other codecs are not enabled.
Embedded bytes and bounded custom-file reads are decoded in memory. Configuration,
file reads, device initialization, and playback waits stay off the UI thread.

Delivery and pending queues are bounded to 32 events per endpoint; the playback
worker queues at most eight jobs, dropping overflow and jobs waiting over one
second rather than playing stale bursts. Each job has a 15-second wall-clock
budget, checked between setup operations and every 25 ms during playback; sources
are also limited to 15 seconds. OS file/device setup calls cannot be interrupted.
The worker opens the current default device per job and releases it afterward.
Device errors stop the job; later notifications try the current default again,
without replaying failed audio. Cancellation and timeout never trigger fallback.
Disconnect,
reconnect, boot change, endpoint removal, and window closure cancel old sounds.
Multiple GUI/TUI clients each play their own sounds; there is no cross-client
audio deduplication. Native playback and device-switch/unplug behavior require manual QA.

## Title Bar

With Herdr's tab bar at the top (`ui.tab_bar_position = "top"`, the default),
the tab row is the title bar, as in Chrome or Conductor, and the window draws
no separate header. Strips along the top grow to the header's 34px. The
sidebar column starts with a row holding the traffic-light clearance, the
sidebar toggle, and Back and Forward; with the sidebar collapsed to its rail or hidden, the leftmost
group's strip leads with whatever clearance the column leaves and the toggle.
The rightmost group's strip ends with the header's status text, Git button,
account, and window controls. Every strip keeps at least 40px of empty room
after its tabs that moves the window, so the window stays movable however many
tabs a strip holds; tabs beyond the strip's width scroll as before. A bottom
tab bar, or a lone tab hidden by `hide_tab_bar_when_single_tab`, brings back
the full-width header described below, and so does a window with no strips.
The worktree banner moves to the window's foot in this layout so it never sits
under the traffic lights.

Back and Forward follow the sidebar toggle, in the header or in the expanded
sidebar's first row; a sidebar dragged too narrow for them, or collapsed,
hands them to the leftmost strip with the toggle. They walk the panes this connection has
focused, across tabs and workspaces, the way a browser walks its pages: going
somewhere new drops what was ahead, closed panes are stepped over, and the
trail (at most 100 panes) starts over when the daemon restarts. `back`
(Cmd-[) and `forward` (Cmd-]) do the same from the keyboard, and so do a
mouse's side buttons anywhere in the window. Each platform draws the pair its
own way: macOS joins two chevrons in one segmented bezel as Finder and Xcode
do, Windows uses Fluent's subtle arrow buttons as File Explorer does, and
Linux uses GNOME's flat rounded header bar buttons. A header narrower than
320px leaves them out so the account and window controls stay reachable, and
a strip squeezed by the sidebar, a panel, or a split gives up its tabs' room
first, then drops the pair whole before the trailing controls would be
pushed out.

macOS keeps `Some(TitlebarOptions)` and the native Herdr window title/traffic lights,
with transparent chrome and lights positioned at (9, 9) logical pixels. A full-width
34px header blends `theme.surface` roughly 10% toward white, subtly lifting dark
themes while keeping light themes light. It sits above the sidebar and tabs: 80px
of traffic-light clearance, an empty flexible center, and a 40px upper-right slot.
The slot centers a 16px circular user avatar with a 12px SVG in a 28px hover target, tinted from
the theme foreground. This profile control opens native GitHub sign-in and shows
the authenticated user's avatar when connected. It consumes clicks so
double-clicking it does not invoke the title-bar action.
The header and clearance remain in fullscreen so the body layout stays stable.
On Windows and Linux the main window shows the same header below the native
frame, with an 8px lead in place of the traffic-light clearance; Settings,
Logs, and the mockup board rely on the native frame alone.

Some Linux compositors, GNOME's Wayland session among them, draw no frame for
other applications, and GPUI falls back to client-side decorations there.
Every window then draws its own: pressing the header starts a compositor move
(a double-click maximizes or restores instead), a right-click opens the compositor's window menu when it offers one, and the
header ends in minimize, maximize/restore, and close buttons, each shown only
when the compositor supports it, with close always present. Secondary windows
show the header in this mode too. Close runs the window's own close path, so
Settings still finishes pending preferences first. A 10px transparent band
around the window carries a shadow and the resize handles; edges the
compositor tiles lose both, so a maximized window fills the screen. X11 and
compositors with server-side decorations keep their native frame.

Linked-worktree builds add a full-width, 22px amber banner below
the macOS header (above the body on Linux), with the compile-time branch or short
SHA and optional open PR number, clickable to open that PR on GitHub. The branch
truncates while the PR stays visible. It participates in the root flex
layout, so terminal painting,
hit testing, resize, and IME geometry continue to use the actual canvas bounds.
The banner does not query Git/GitHub or intercept keyboard focus. Main-checkout
builds have no banner. Headless tests cover banner presence/absence, long branch
labels, PR presence/absence, and body bounds at 360px, 640px, and 1200px widths.
Build identity and icon selection are described in the
[release notes](../../scripts/release/README.md#build-identity).

The reference is Zed's `crates/platform_title_bar/src/platform_title_bar.rs` and
window options in `crates/zed/src/zed.rs`, not a build dependency. On macOS every
window sets `app_owns_titlebar_drag`, so AppKit never moves the window from under a
tab or delays titlebar clicks; pressing the header, the sidebar's top row, or a
strip's empty room calls `Window::start_window_move`, and a double-click calls
`Window::titlebar_double_click()` to honor the OS preference. `is_movable` stays
unchanged, so the Window menu's tiling items stay enabled.

Headless tests check the actual root header/center/account-slot bounds at wide,
minimum, and narrow sizes, including mock fullscreen entry/exit, and that the
avatar and hit target stay centered. An SVG decoding test checks the embedded user
icon produces a nonempty mask. These do not verify AppKit behavior. Native QA remains
required for dragging across the header, traffic-light alignment and actions,
double-click preferences (zoom/minimize/do nothing), fullscreen transitions and
auto-hidden controls, theme changes, and modal/focus/IME behavior. Windows/Linux
native-frame appearance also remains unverified by these macOS tests. Headless
tests check the client-decorated frame (inset, handles kept out of the content,
tiled edges, and button order and support) with forced decorations. Native QA
on GNOME Wayland remains required for dragging, resizing, the window menu,
maximize/restore, and the shadow.

## Terminal Selection And Copy

Mouse-aware applications receive clicks, button releases, drags, and pointer
motion. Hold Shift to select locally instead; releasing a selection of terminal
text in a pane also offers the annotation pill described below. In applications
without mouse reporting, selection works without Shift. A forwarded drag stays
in the pane or popup where it started, including when the pointer moves outside it.
Many agent TUIs take the mouse without selecting text, so the first left drag an
application receives in a session shows **Hold Shift and drag to select text**
for a few seconds, where the `copied to clipboard` flash appears. It shows once
per session, never for a click, and not at all once you have pressed with Shift
over such a pane.

Right-click opens the GUI pane menu unless Herdr routes that pane's right-clicks
to the application (`herdr pane input --right-click pane`, or **Send
Right-Clicks to Pane** in the pane menu). The routing belongs to the daemon, so
the TUI and every other client follow the same setting. A routed pane passes a
plain right-click to a mouse-aware application; right-click with Shift, Control,
Option, or Command still opens the menu, where **Open This Menu on
Right-Click** switches the pane back. A routed pane whose application has mouse
reporting off opens the menu, since nothing would receive the click. A
mouse-aware popup has no pane menu and keeps its right-clicks.

Herdr's shared `[ui] right_click_passthrough_modifier` (default `""`, off) is
honored as in the TUI: a right-click held with exactly that combination goes
to a mouse-aware application in any pane, routed or not, and the application
receives it without that modifier for the whole press, drag, and release,
including over a link, since a right-click opens no link. Any other
combination still opens the menu. It accepts Herdr's values: `ctrl` or
`control`, `alt` or `option`, `cmd`, `command`, or `super` (the Command,
Windows, or Super key), joined with `+`; `off`, `none`, and `disabled` turn it
off, and anything with `shift` is refused. GPUI cannot report the `meta` or
`hyper` modifiers, so a combination naming either never matches here, and an
invalid value leaves it off without affecting other settings.

Drag across the terminal to select cells; releasing the button copies them and
shows the `copied to clipboard` flash described under
[Configuration](#configuration). Selection is client-local: it reads the surface
the client already has, and asks the daemon only for rows scrolled out of view.

To annotate terminal text, hold **Shift** while starting a selection in a pane.
Releasing the mouse shows a small, borderless **Annotate** pill beside the
selection. Copying still follows your normal settings, and the terminal keeps
keyboard focus. Click **Annotate** to open a compact, borderless note field
beside the selected text. Its **Message to agent…** field accepts comments,
questions, or requests.
Press **Enter** to send the note, **Shift+Enter** to queue it and select more text,
or **Escape** to dismiss the field; the popover also has Send, Queue, and close
buttons. Typing in the terminal or clicking elsewhere dismisses the pill.
The whole selection must be visible on screen.
This also works with copying on selection disabled or the highlight set to clear
after copying. For an existing selection, use **Annotate Selection for the Agent**
in the command palette (Cmd+Shift+A on macOS, Ctrl+Alt+Shift+A on Linux and
Windows).
Notes go to the agent in that pane, falling back to the first agent in its
workspace; with no agent, they are copied to the clipboard.

A program that copies with OSC 52 — many editors and agent CLIs do, especially
when they own the mouse — is honored too: the daemon forwards the bytes to this
client, which writes them to the same system clipboard and shows the same flash.
Only bounded UTF-8 text is written; malformed, non-text, or oversized writes are
dropped without replacing what was already on the clipboard.

A selection stays inside the pane it started in, and a drag that leaves the pane
or the window selects up to its edge rather than into its neighbor. A selection
inside a popup takes the popup's own cells, never the panes it covers. Because
each end anchors on the half of a cell the pointer sat in, a single character is
selectable, while a press that never crosses a midpoint selects nothing.

Copied rows are separated by newlines. Wide graphemes copy once, without spaces
from their continuation cells; actual selected spaces are preserved. A partial
wide grapheme copies only when its leading cell is selected.
Concealed cells copy as blanks so hidden content does not reach the
clipboard, and trailing blanks are dropped only from rows selected through to the
pane's right edge, where a terminal pads short lines. A copy is bounded, and one
too large to copy reports in the status bar instead.

The highlight stays after the release copies it, until the next click or
keystroke, a reconnect, detach, or endpoint switch. While it shows, Cmd-C and
**Edit > Copy** copy it again; Ctrl-C still reaches the pane, which interrupts
the program there and clears the highlight. Set `keep_selection_after_copy =
false` in `config-gpui.local.toml` to clear the highlight on release instead.

With Herdr's `[ui] copy_on_select = false`, shared with the TUI, the release
copies nothing: the highlight waits for Cmd-C, Ctrl-C, or **Edit > Copy**, and
any other key drops it, as in Herdr.

Cmd-V sends semantic paste. While a terminal has focus, the native **Edit** menu
enables **Paste**, and **Copy** while a selection is highlighted. In dialogs and
search fields, **Cut**, **Copy**, **Paste**, and **Select All** do the same as
Cmd-X, Cmd-C, Cmd-V, and Cmd-A.

The terminal is exposed to accessibility clients, such as VoiceOver and
selection tools that read the focused element's selected text (PopClip,
OpenClip, dictionary lookup). It reads as a text area holding the rows on
screen of the pane with the selection, or, without one, of the focused pane
or open popup. Each row reads as a copy would: concealed cells as blanks and
no trailing padding. A selection reaching rows scrolled out of view exposes
only its visible part, though the copy still includes all of it. Copy mode's
keyboard selection is not exposed.

## File Drops

Drop files from your file manager onto a terminal pane or popup to paste their
paths there, even if another pane is focused. Paths are pasted bare, as native
macOS terminals do, with a backslash before each space, quote, bracket, comma,
colon, or other character a shell would interpret (`/tmp/Shot\ 1.png`), and
before a final period, separated by spaces; the drop never presses Enter. The
escapes keep a dropped path one Cmd-clickable link. Local drops only paste paths; SSH drops
read and transfer the selected files. The exception is a live macOS screenshot
thumbnail, dragged before it saves: macOS lets only Herdr UI read its temporary
file, not the pane's processes, so its image goes through the
[image bridge](#images), as a pasted clipboard image does.
Drops are limited to 256 paths and 64 KiB of escaped text. Non-UTF-8 paths and
paths containing control characters are rejected rather than altered.

On an SSH endpoint, a single supported image is transferred as described below.
Other regular files and multiple-file drops are streamed using the SSH file-copy
path below. A WSL distribution is given its own paths for the files instead,
with nothing copied; see [WSL distributions](#wsl-distributions). POSIX shell
escaping is not intended for Windows command shells.

Files copied in the file manager (Cmd-C in Finder) and pasted into the focused
pane or popup with Cmd-V are handled the same way as a drop. Finder also puts
each file's name on the clipboard as text; the paste uses the full paths instead.

## SSH File Copies

Drop regular files onto an SSH pane or popup to copy them to that host. A
`Copying...` card shows the filename (or file count), transferred bytes, percentage,
progress bar, and Cancel button. Once all files are received and the SSH processes
exit successfully, their escaped remote paths are pasted into the original target.
No partial list is pasted on failure. Directories and special files are rejected;
symlinks to regular files are followed. A single recognized image uses the image
bridge below instead; multiple-file drops copy their originals unchanged.

One file-copy batch runs per window. Files are streamed in bounded 64 KiB chunks
with 64-bit byte counters, so a 4 GiB ISO does not require a 4 GiB allocation.
Progress counts bytes written to the SSH stream; `Finalizing copy...` waits for
remote byte-count verification and SSH completion. This is not a checksum or
durability guarantee. Files changing size during transfer are rejected.

Copies use a separate noninteractive SSH connection with the same host-key and
authentication policy as the terminal. Terminal input remains responsive, and
typing is not queued behind a multi-gigabyte copy: wait for completion before
submitting a command that needs its path. Switching hosts, losing the target,
reconnecting, or closing the window cancels the copy. Cancel only terminates the
copy process, never the Herdr daemon or its terminal connection.

Each file keeps its basename inside a unique private `herdr-upload.*` directory
under the remote `${TMPDIR:-/tmp}`. Existing files are never overwritten. Partial
and cancelled copies are removed where possible; cleanup failures display a
warning identifying the original host. Successfully pasted files remain until
you remove them or the remote OS cleans its temporary directory: unlike image
bridge files, they are not owned or deleted by Herdr on disconnect. Network loss
can prevent cleanup, and kernel-blocked local filesystem operations cannot be
forcibly interrupted. A copy stalls out after 30 seconds without progress.

## Worktree Scripts

A repository can commit scripts that prepare, start, and clean up its
worktrees, in `.herdr/worktree.toml` at the repository root:

```toml
[scripts]
# Runs when a new worktree is created from the New worktree dialog.
setup = """
cp "$HERDR_ROOT_PATH/.env" .env
ln -s "$HERDR_ROOT_PATH/node_modules" node_modules
"""
# Runs when you choose Run script from the workspace's menu.
run = "npm run dev"
# Runs when you delete the worktree checkout, before it is removed.
archive = "docker compose down"
```

Every key is optional, and an unknown key is an error rather than a script
that silently never runs. The file is read from the checkout the script is
for, so each branch can carry its own.

- **setup** runs after a worktree is created. Opening an existing checkout
  does not run it. Linked worktrees also offer **Run setup script** in their
  menu, to run it again.
- **run** starts from **Run script** in the menu of any Git checkout. A
  repository without one says so.
- **archive** runs when you confirm **Delete worktree checkout**. Its tab
  removes the checkout with the pane's own `herdr worktree remove` only if
  the script succeeds; if it fails, the tab stays open on its output and the
  checkout stays in place.

Each script runs in a new, focused tab labelled `setup`, `run`, or
`archive` in the checkout's workspace, never in the background, as
`sh -e`, so it stops at the first failing command. Its working directory is
the checkout. `HERDR_WORKTREE_PATH` names that checkout and
`HERDR_ROOT_PATH` the repository's main checkout. The script reaches the tab
as an environment variable, and the line typed into the tab's shell is
fixed, so the file's contents never pass through your interactive shell.
Scripts need `sh` on the daemon's host: a remote host's checkout is read and
run there, not on this machine. WSL distributions are not supported yet: Run
script says so, and creating or deleting a worktree there runs no script.

**Trust.** The file is repository content, so nothing in it runs until you
trust it. The first time a script would run, a dialog shows every script in
the file, every line of it. The dialog can open by itself, so it has no
keyboard shortcut and ignores clicks for its first moment: a keystroke or
click meant for the terminal cannot answer it. **Trust and run** remembers
that exact file for that repository on that host. Any change to the file, such as a pulled commit, asks again. The
same repository on another host is asked about separately. **Don't run**
runs nothing. For an archive script, **Remove without running** removes the
checkout as the delete dialog would have, and Escape keeps it. Trust is kept
by this client in `trusted-worktree-scripts.json` in its state directory;
delete that file to forget every grant.

The repository file, rather than this client's configuration, holds the
scripts, so a team shares them and they follow the branch, as Conductor's
`.conductor/settings.toml` and Superset's `.superset/config.json` do. Only
the trust decision is local.

## Worktree Notes

Right-click a Git checkout and choose Note... to keep a reminder on it, such as
what it is waiting on. A row with a note carries a note icon beside the
uncommitted-work icon, and the note's first line reads under the row, cut off
when it is too long; click that line to edit it. Card layouts (Superset, Orca)
show the line only. Saving the note empty removes it.

The Worktree Notes command opens the palette on its Notes filter, which lists every
open checkout with a note on any connected host, most recently edited first;
the whole palette also finds a workspace by its note's text. Edit Worktree
Note opens the focused checkout's note. Neither has a default shortcut;
`worktree_notes` and `edit_worktree_note` bind them under `[keybindings]`.

A note is one line of at most 500 characters. Notes stay in this client, in
`worktree-notes.json` in its state directory: they never reach the daemon or the
repository, and every window shows the same ones. Like teleport marks they
belong to a host, repository and branch, so a note survives a daemon restart
and follows its branch, and it does not travel with a teleported worktree.

## Moving Panes

Right-click a pane, or an agent's row in the sidebar, for **Move to New Tab**
(in the pane's own workspace), **Move to New Workspace**, or **Move to…**, which
lists the other tabs of every workspace, a new tab in each other workspace, and
then, for each other device that has the pane's repository open, a new worktree
and that device's spaces of the repository. Outside a Git repository, it lists
connected devices for an agent session move instead. The command palette's **Move Pane…**
(`move_pane`, unbound by default) opens the same list for the focused pane, with
the new tab and new workspace first. Moving into an existing tab splits that
tab's focused pane and puts the moved pane on its right. The terminal keeps
running, and the move asks Herdr to focus the pane where it lands.

Herdr does not offer `pane.move` to GUI clients, so the move runs `herdr
[--session NAME] pane move` on the endpoint's host, locally or over SSH, the way
Teleport does, on a background thread. The menu says **Moving pane…** until
Herdr answers and closes when the move succeeds. Herdr's refusals appear in the
menu: it will not move a pane out of or into a zoomed tab, and a missing `herdr`
CLI is named as such. A failure that arrives after the menu was closed appears
in the window's status instead. Moves are offered only on Linux and macOS
clients and only for the local daemon, a named session, or an SSH device, not a
custom socket endpoint or a development (`--dev`) session, whose panes the
`herdr` CLI cannot address by session name. A tab or workspace that closed after
the menu opened is refused before anything runs.

### To Another Device

**DEVICE › New Worktree** and **DEVICE › SPACE** copy the pane's work to that
device and move the pane there. The device must be connected and have a
workspace of the same repository open; the review then checks that the two share
a remote, and refuses a repository matching only by name. The review lists the
branch, the unpushed commits and changed files, and what the pane will run
there; Enter starts the move.

The move works like Teleport, for one pane: the checkout's commits and its
staged, unstaged and untracked changes go to a new worktree there, on the same
branch when that device can take it, otherwise on a new branch from the same
commit. A space takes them only if nothing there would be overwritten: its
checkout must have no uncommitted changes and be at the pane's commit, or behind
it on the same branch, in which case it fast-forwards first; an ignored file
there that a commit would replace also stops it. The pane then runs in a new
tab of that space. A supported agent's session moves with them and resumes;
another program starts again with the same arguments, and the pane, shell or
program, starts in the same subdirectory. The work is
copied, so the checkout here keeps its changes, and the other panes in its
workspace keep running. The pane here closes only once it runs on the other
device, and the window then follows it there.

Outside a Git repository, **DEVICE › Agent Session Only** moves a supported
agent's transferable session into a new workspace on that device. Its cwd maps
relative to home: `~/notes` here becomes `~/notes` there, and the home directory
itself maps to the destination's home. The destination directory must already
exist; the move never creates it. A directory outside home is refused. Only the
agent session is transferred, with its cwd rewritten, then resumed with its kept
flags: no directory files, worktree, Git bundle, or GitHub credentials move, and
no Git commands run. The same agent must be installed there; a session transfer
failure leaves the source pane open. Plain shells and agents without a
transferable session still show connected devices as muted rows; choosing one
explains why it cannot move. The review makes clear that no files are copied.

A working agent is still writing its session, so it moves only once it is idle
or waiting; this is checked again on Enter. A commit made here after the review
stops a repository move before anything lands, so it can be reviewed again.
A pane whose directory has left its repository space's checkout must change
back into it first. A session-only move rechecks the session, cwd, destination
directory, and whether the agent is working, on its host, on confirmation. The
pane here closes only once the agent is the running program there; if it does not
start within 20 seconds, or the launch fails, its new workspace closes again, and
an unsuccessful cleanup reports the workspace left behind. A refused
**Agent Session Only** row is checked again when chosen if the pane's agent has
changed since, for example after it reports its session. If a repository move fails
before the pane runs there, a new worktree is removed again, and if it cannot
be, the error says where it was left; in a space, the tab made for the pane
closes and the copied changes stay in its checkout. Closing the menu abandons a
review but not a move under way, whose result then appears as a flash. The same
hosts as Teleport can be reached, so neither device may be a custom socket, WSL
or development (`--dev`) endpoint.

## Teleport

Right-click a linked worktree and choose Teleport... to move it to another
connected host: its branch and commits, staged, unstaged and untracked changes,
tabs and splits, the programs running in them, and agent sessions. It is offered
on Linux and macOS clients when the worktree's host is the local session or a
saved SSH host or cloud device (Coder, Daytona, or Railway); custom socket and
WSL endpoints are not scripted. Neither is a
development (`--dev`) session: the `herdr` CLI names sessions of the release
installation only, so Teleport, fan-out, and dispatch are not offered there,
and an orchestrator action that drives that session through the CLI reports an
error instead of reaching the release session of the same name.

Cloud devices use the same saved account and machine identity as their sidebar
connection, in either direction. Coder and Daytona wait for the saved machine to
be ready; Railway uses its saved, verified SSH key. Teleport does not create a new
cloud device or install Herdr. Cloud authentication and command preparation run
only after a host is chosen, on the background worker.

1. **Choose a host.** Every other enabled supported host is listed at once, connected or
   not; nothing is probed until one is chosen. The review then finds where the
   worktree goes there: the checkout it once left (see below), else the
   repository if it is open, matched by normalized Git remote (`origin` first,
   then any remote) or, only when this repository has no remotes, by name.
   Otherwise Teleport sets it up: a checkout of the same repository at the same
   place under the home directory (`~/code/app`) is opened as a space; failing
   that, the repository is cloned there from `origin` without prompting, or,
   when that host cannot reach `origin`, copied from this machine with its
   branches and tags and `origin` restored. A different repository already at
   that place is left alone and the copy goes to `<place>-teleport`.
2. **Review.** The dialog lists the branch, unpushed commits, changed and
   untracked files, and what each pane becomes. The destination's branch must be
   absent or an ancestor of this one, and must not be checked out there.
3. **Teleport.** Closing the dialog does not stop a move in progress; its result
   arrives as a flash, and success switches to the new workspace on the
   destination, waiting for that host to connect and list it if need be.

Commits travel as a Git bundle holding only what the destination lacks. The
uncommitted work travels as two temporary commits, built through a temporary
index so the source checkout, index and branch are not modified. The branch is
set to the source's commit, fast-forwarding a branch already there but never
overwriting one that diverged. The destination's `herdr worktree create` checks
it out under the same name, then the staged index and working tree are restored exactly, binary files
included. Ignored files such as `.env` and build output stay behind, and so do
submodule contents.

When `origin` is on GitHub and the destination cannot reach it by itself (no
SSH key there, or an unknown host key), the review says so and the move lends
it this machine's `gh auth token`. The token travels over the script's stdin
and is stored in the repository's Git directory, `.git/herdr/github-token` (mode
600), so no working tree or commit ever holds it. A repository-local credential
helper answers `https://github.com` from that file, `git@github.com:` URLs are
rewritten to HTTPS for that repository, and `.envrc` gains an `export GH_TOKEN`
that reads the file, for `gh` under direnv; it is excluded locally, and a
repository that tracks its own `.envrc` is left untouched. Replace the file to
rotate the token. If `gh` is not signed in here, the review warns that pull
and push will not work there.

Tabs, split directions and ratios, and custom tab and workspace labels are
rebuilt. Each pane starts in the same directory relative to the checkout.
Running commands start again with their arguments, with paths under the
checkout moved to the new checkout. Environment variables, shell history,
background jobs and unsaved editor buffers do not carry over.

Agent sessions move in each agent's own format, so the full history resumes:

| Agent | Moved as | Resumed with |
| --- | --- | --- |
| Claude Code | transcript into the new cwd's `~/.claude/projects` directory | `claude --resume <id>` |
| Codex | rollout under `~/.codex/sessions` | `codex resume <id>` |
| opencode | `opencode export`, then `opencode import` | `opencode --session <id>` |
| pi, omp | session file into the new cwd's session directory | `pi --session <file>`, `omp --resume=<file>` |
| GitHub Copilot CLI | session directory under `~/.copilot/session-state` (`$COPILOT_HOME`) | `copilot --resume=<id>` |
| Letta Code | nothing: conversations stay on the Letta server, which the destination must be signed in to | `letta --conversation <id>` |

The checkout path is rewritten inside each moved session. Model and permission
flags from the original command line are kept for the agents above except Letta.
Initial prompts are dropped. An agent the destination lacks, or one with no
reported session, is asked first to write a handoff note to
`.herdr/teleport/handoff-N.md`. The note travels with the changes, even where
`.herdr` is ignored. The same agent, or else the first installed of Claude Code,
Codex, opencode and pi, then starts with that note. Anything nothing can continue
is listed as skipped.

Herdr can resume Devin, Droid, Kimi, Mastra Code, Hermes, Qoder, Qwen Code, Kilo,
Cursor Agent, Antigravity and Grok sessions too, but only on the host that
stores them. Teleport does not yet know where these agents store their
sessions, so a destination that has the agent runs its original command line again
without a handoff note.

Once the destination worktree, changes and tabs exist, the source workspace's
programs stop: its tabs are replaced by one idle `teleported` shell tab, and the
workspace and checkout stay. Herdr has no moved or disabled state, so this
client remembers the move (`teleported.json` in its state directory) and marks
the row with a teleport icon. Its menu offers Go to teleported copy and Clear
teleported mark instead of Teleport. The copy on the destination offers
Teleport back, which opens Teleport already aimed at the host the work came
from. Teleporting the work back picks that checkout as the destination: its current state, committed or not, is saved to
`refs/herdr-teleport/backup/...` first, then it takes the returning branch and
changes, its tabs are rebuilt, and the mark is cleared.
Everything runs as noninteractive scripts calling Git and the `herdr` CLI on
each host. Remote hosts use the terminal's SSH trust and authentication policy,
and the UI thread never blocks. The GUI connection's API does not expose layouts,
process details or agent sessions, which is why Teleport uses the CLI.

## Smart Dispatch

When more than one host is connected, the New worktree dialog (its branch form)
and the New workspace dialog for a Git checkout offer a host for the new
checkout. The three hosts with the most room are tiles, best first; every other
host, offline ones last, opens from the Other field below them. The host the
window shows stays chosen until you pick another, so pressing Enter still
creates where it always did.

Hosts rank by spare cores: cores less the 5 minute load average (or the CPU
share where there is none), from the same samples as the CPU and memory
display. While a picker is open every connected host is sampled, even with
that display off. Each agent working there takes half a core off, a host that
would have to clone the repository first takes one, memory above 90% halves
its room, and each of the repository's last eight new checkouts that went
there adds half a core, up to three. The ranking follows the samples for a few
seconds, then holds still so the tiles do not move under the pointer.

Choosing another host creates the checkout there through the same scripts as
Teleport: the repository is found by remote (or opened, or cloned, as Teleport
does), the base commit is resolved here and shipped as a Git bundle only when
that host lacks it, and that host's own daemon creates the worktree from it.
Closing the dialog does not stop it; the result arrives as a flash, and the
window switches to the new workspace. Once it is shown, a new worktree runs the
repository's setup script there, asking for trust as a local creation does.
Setups of several such worktrees wait their turn; one whose host is not shown
within two minutes is given up on, with a warning. A
new workspace opens the repository's main checkout on that host.

The fan-out dialog offers Spread agents across hosts. Each lane then goes to
the host with the most room left, a lane counting as a core, and its host chip
opens every host, best first, to move that lane. Each other host is set up once
before its lanes, and a lane whose agent is not installed there fails with a
reason. Lanes on another host show it next to their status, and Open, Keep and
the comparison work across hosts; a host that cannot be read marks only its
own lanes, keeping the changes read before.

Where new checkouts went is kept in `dispatch-history.json` in the state
directory, by repository name and host. Like Teleport, dispatch needs a Linux
or macOS client and the local session or a saved SSH host on both ends.

## Activity bar

An icon column at the window's left edge picks what the window shows. Every
section but Workspaces takes the window's body, keeping the title bar and the
status bar; zen mode keeps the terminals. **Settings > Sidebar > Show
activity bar** (or `[sidebar] activity_bar = false`) turns it off in every
window at once, and `herdr-gpui ui panel NAME` shows a section.

- **Workspaces** is the sidebar beside the terminals. Clicking it while shown
  folds the sidebar to its rail.
- **Overview** shows what needs you on every connected host: agents waiting on
  you and failed runs first, in a card of their own, then finished agents
  ready for review, working agents by host, and idle ones. Hovering a row
  offers **Snooze** (until a preset time), **Settle**, **Pin**, and **Open**;
  snoozed and settled rows fold away at the bottom and come back once their
  time comes or the agent works or asks again. These marks are this client's
  own. An agent whose pane closes or that exits, while its host stays up,
  waits under **Closed** at the bottom: **Reopen** starts it again in a new
  tab of its workspace, in the folder it ran in, continuing its last
  conversation there where the agent offers that (`claude --continue`,
  `codex resume --last`, `opencode --continue`), and **Forget** drops it.
  The window keeps closed agents while it is open. Reopen and workspace
  navigation require their original daemon session; after a restart or a
  session switch, stale workspace IDs are not reused. A shelf shows only when it
  holds something, and with nothing to list the page says what it is for. A
  column beside the list shows each host's CPU, memory, agents, and ports;
  each host folds, and the column folds to a strip of load dots. The icon
  counts what needs you.
- **Pull requests** and **Issues** list every repository the connected hosts'
  workspaces hold (see [Orchestrator](#orchestrator)). Refresh spins while a
  sync runs.
- **Usage** shows every host's plan limits, one card per signed-in agent. Only
  the selected host is read; another host shows what was read while it was
  selected.

When Overview or a section opens an agent's pane, the title bar's Back returns
to it before walking back through panes.

A section's icon shows its count in the accent color until the section is
opened, then in a muted gray until something it counts changes again: an
issue or pull request updated, an alert or report opened, or an agent coming
to need you. When each section was last open is remembered per daemon between
runs. Agent states are timed from when this client first saw them, so after a
restart an agent already waiting on you counts as new once.

## Orchestrator

The activity bar's **Pull requests** and **Issues** sections list, across the
window, every repository the workspaces of every connected local or SSH host
hold: their GitHub issues and pull requests or GitLab issues, their
[Beads](https://github.com/steveyegge/beads) issues when a main checkout has a
`.beads` folder, and the agent runs dispatched for them. Folders outside Git
are quietly skipped unless they have a `.beads` folder; **Refresh** checks
skipped folders again. Git and SSH discovery failures still show their errors.
Worktrees of one
repository count once, and each repository syncs on its own, so up to 32 are
listed. A row shows its project while several are listed. Under the search, a
row of pills picks backends (GitHub, GitLab, Beads) and projects, each with its
count; none picked lists all. The Issues row of a group's "…" menu or of a
workspace's popover, the Show Issues command, and `herdr-gpui orchestrator`
open the Issues section narrowed to that workspace's project. A repository
without a GitHub remote has no GitHub source; the header says "no GitHub
remote" when none of them has one.

- **Issues** lists open GitHub or GitLab issues and beads together, grouped by
  priority. A bead's children sit under it. The **Filters** button unfolds a
  column narrowing to items with a run, blocked ones, one source, or one
  label; folded, the filters in use show as chips that clear them. Five sorts
  are offered. Search matches every word you type across title, id, author,
  labels, state, and body, case-insensitively unless a word has a capital, and
  keeps a match's parents in view. **Start agent** on a row opens the dispatch
  dialog.
- **Pull requests** is a table grouped by what each waits on: a review asked
  of you, your own to fix (changes requested or failing checks), those ready to
  merge, the rest, and recently closed or merged ones. Each row shows its
  branch, author, review state, its checks as colored pills, diff size, its
  run, and age. Review state, review requests, and checks are read from GitHub
  after every sync and kept in memory only, never in the shared database.

Runs have no list of their own: the [Overview](#activity-bar) shows the agents and
failed runs that need you, and an item's **Agent** page shows its runs. Their
status comes from Herdr's live view of each connected host.

A click selects a row; Enter or a double-click opens its page in a pane beside
the list, which keeps only the columns that fit. The pane's left edge drags to
resize or close it, and × closes it. An issue's page shows its
description as plain text with Markdown structure, its runs, and its details. A
pull request's page also has its conversation, read as on GitHub: authors'
avatars, Markdown comments, each review holding its inline threads with
resolved ones folded, and a summary of verdicts on top; you can comment there.
It also shows the checks, review decision, and merge state. Merge offers the methods the
repository allows and names the head commit shown, so GitHub refuses it if the
branch moved.

**Dispatch agent** opens a dialog: a prompt, an agent installed on the
selected destination host, an optional model (passed as `--model`), a branch
(`agent/<number>-<slug>-<hash>` as agent-launcher names it, editable), and
extra instructions. The new worktree is
created from the main checkout's `HEAD` in the workspace's repository, and the
agent starts in it with the prompt. With more than one host connected, the
dialog also ranks the hosts as [Smart Dispatch](#smart-dispatch) does; picking
another one sets the repository up there the same way and starts the agent
there, and its run card then acts on that host. A pull
request's **Review with agent** does the same on an `agent/review-…` branch with a
read-only prompt: the agent verifies the pull request, reads its diff, and
reports findings without writing to GitHub. A run's card sends it messages, stops
it, or removes its worktree; a bead's details can delete it with `bd`. Each of
those asks first. Leaving the section leaves an already-started dispatch
running; it does not remove the new worktree.

The hash at the end of a branch name is the item's own, so the Orchestrator
finds work without its database, such as on another machine: a Herdr workspace
on an item's branch shows as that item's run, working or stopped by whether an
agent is in it, and so does an `agent/…` branch in the repository that no
workspace shows. These runs open their workspace but have no controls.

Issues, pull requests with their reviews and checks, and runs are kept in a
per-repository database in Herdr UI's state directory, so a restart lists them
at once and syncs after. The first time a repository opens, agent-launcher's
database for it is copied in, with its runs. The tab uses agent-launcher's
prompt profiles from `~/.config/agent-launcher/agents/<name>/prompt.md` (the
built-in prompt when none is chosen). GitHub is read as the account used
for pull requests on that host. A GitLab remote, on gitlab.com or a
self-managed host, lists its open issues (not yet its merge requests), read
with `GITLAB_TOKEN` or `GITLAB_ACCESS_TOKEN` when `GITLAB_HOST` names that host
(gitlab.com by default), else with the GitLab CLI's sign-in (`glab auth
login`), and sent only over HTTPS; public projects need neither, and an
`http://` remote is read without one. GitHub Enterprise is listed as not
supported yet. The window button in the tab's header moves the view into
a window of its own.

## Images

On a selected SSH endpoint, drop one PNG, JPEG, GIF, WebP, or BMP image onto a pane
or popup to send it through Herdr's existing image bridge. Clipboard images use
Cmd-V (normal paste, with text taking precedence) or Ctrl-V (Herdr TUI's default
image-paste shortcut). Ctrl-V retains its normal terminal meaning when the
clipboard has no image. A pasted absolute image-file path is also recognized,
including the backslash-escaped paths used by terminal file drops.

Local endpoints on macOS, Linux, and Windows use the same bridge for Cmd-V
clipboard images (Ctrl-Shift-V or Shift-Insert on Windows), because a GUI text
paste cannot carry image data. Local drops and pasted
paths stay ordinary path pastes, except a live screenshot thumbnail (see
[File Drops](#file-drops)), and local Ctrl-V is sent to the terminal
unchanged so agents can read the shared clipboard themselves.

The daemon writes a temporary file and pastes its path into the
target terminal. OpenCode or another agent can recognize that path as an image;
the GUI never presses Enter or claims that the agent accepted an attachment.
Files are connection-owned and Herdr removes them when the client disconnects.

Images are limited to 16 MiB, with one queued/sending image per connection and
at most four clipboard preparations per window. File reads, remote clipboard
acquisition, and encoding run in the background. A FIFO reservation keeps
subsequent typing and Enter behind the paste. Images within 16 MiB pass through
unchanged. Larger static images are recompressed, then downscaled if necessary,
to fit that same daemon limit. Transparency and EXIF orientation are preserved;
the original file is never modified. A notification reports that the smaller
copy was queued, not that an agent accepted it.

Automatic resizing accepts at most 128 MiB of encoded source data and a bounded
64-megapixel / 256-MiB decoded raster. Oversized GIF, WebP, and APNG images are
rejected instead of silently losing animation. Invalid, too-large-to-process,
and unsupported images show an `Image discarded` notification with the reason;
no fallback local path is pasted for resize failures. Unreadable, empty, or
nonregular image-file candidates retain the TUI's original path-paste fallback.
Clipboard images published only as TIFF (for example by Preview) are converted
to lossless PNG in the background, and use the same resize path when that PNG
is too large. Dropped TIFF, HEIC, and SVG files are not image-bridge formats but
can be dropped as ordinary files using SSH file copy.

Switching endpoints, reconnecting, or invalidating the target cancels pending
work. Cancelling an image already partially written closes that client connection
to avoid corrupting framing; it does not stop the daemon. Slow/stalled transfers
have a 60-second deadline. There is no upload acknowledgement or progress API.

macOS acquires clipboard data in a short-lived copy of the application, on that
process's main thread before any windows or daemon connections are opened. The
background preparation task accepts only a complete, bounded response from a
successful reader, with a three-second deadline. A native pasteboard crash or
stall fails that paste without closing the GUI; the reader is killed and reaped
on timeout. Clipboard data travels only over a private socket, never a temporary
file or a log. The existing format priority, revision check, and paste ordering
are preserved. AppKit may still materialize data inside the reader before its
size can be checked. Linux uses `wl-paste` (Wayland) or
`xclip` (X11) for explicit Ctrl-V image acquisition, with bounded output and a
three-second acquisition deadline. Ordinary Linux paste retains GPUI's native
clipboard reader and needs no helper; that existing synchronous API can still
materialize image data on the UI thread. Install the matching utility for
background image paste. Regular-file reads use bounded
chunks and a three-second deadline between reads, but an OS-blocked network/FUSE
filesystem operation cannot be forcibly interrupted. Such a read stays isolated
from the UI and holds its bounded preparation slot until it returns.

Windows reads the clipboard on a thread of its own, for every paste: copied
files (`CF_HDROP`), then text, then the `PNG`, `JFIF`, and `GIF` formats other
programs register, a device-independent bitmap (`CF_DIB`, sent as BMP), and
`CF_TIFF`. Each format's size is checked before any of it is copied, and the
whole read has a three-second deadline. A program that renders its clipboard
data on request and never answers stalls only that thread: the paste reports a
timeout, and no second reader starts until the stuck one returns. WSL
distributions are the remote endpoints there, so dropped images, pasted image
paths, and Ctrl-V images reach a distribution's panes through the same bridge.
A pasted Windows path keeps its backslashes as separators; only matching quotes
around it are removed, as Explorer's "Copy as path" adds them.
Files and images dropped or pasted on an SSH host's pane are not uploaded from
a Windows client yet.
Native clipboard/drop and real SSH behavior require explicit desktop/host
verification in addition to the mock-peer and headless tests.

### Native Clipboard Regression Test (macOS)

With an active desktop, Xcode command-line tools (`/usr/bin/python3`), and an
explicitly selected installed daemon:

```sh
just test-gui /opt/homebrew/bin/herdr
# Only the daemon-backed native test:
HERDR_TEST_BINARY=/opt/homebrew/bin/herdr cargo test --locked -p herdr-gpui \
  --features integration-test --test live_gui native_gui_live \
  -- --ignored --nocapture --test-threads=1
```

The parent process preserves every OS clipboard item/type as opaque in-memory
data, then restores it after the GUI exits, including failure/timeout cleanup.
Do not copy new content while this test owns the clipboard. No clipboard backup
is logged or written to disk; only synthetic fixtures reach the test GUI.

The test launches a private daemon and the real application (not GPUI's headless
test platform). It publishes UTF-8 plain text, Unicode, multiline text, PNG-only,
and TIFF-only pasteboards, and sends an AppKit Cmd-V key equivalent to the exact
fixture window. A raw-mode process in the daemon's terminal checks the exact
bracketed-paste bytes followed immediately by typing and Enter. Image checks
verify the pasted path, unchanged PNG bytes, and lossless TIFF-to-PNG pixels.
Local image-file paths remain literal; remote paths must be staged by the daemon.

The same matrix exercises the real SSH connection worker and remote paste policy
through a sandbox-only `ssh` substitute that relays to the isolated daemon socket.
It never invokes OpenSSH, contacts a network host, or discovers a personal daemon.
This covers the process/wire/remote-routing path, not SSH authentication or a
different remote operating system. AppKit injection verifies native key routing,
not hardware keyboard delivery or global shortcut interception. Headless clipboard
tests remain useful for cancellation and ordering, but do not exercise NSPasteboard.

## Terminal Links

Cmd-click (Ctrl-click elsewhere) a link in a pane to open it. A click without
the modifier opens nothing: it selects, focuses, or goes to the application.

Each visual element has one meaning:

- **A dashed underline** marks text that may be a link and is not verified.
  An explicit terminal hyperlink (OSC 8) says it is a link, so it has one at
  rest.
- **A solid underline** marks text that is verified as a link.
- **A hand cursor** marks text that a click acts on.

A pointer that only passes over text shows and checks nothing. Holding the
modifier over text says that you mean to use it as a link, and only then is
the text verified:

- **An address** is verified when it is well formed. Plain text is read as one
  for `http://`, `https://`, `mailto:`, and `vscode:`; any other scheme there
  is plain text.
- **A file path** is verified when the file or folder exists, which the app
  checks in the background. Any printed word may be one, such as
  `src/main.rs:12:5`, `./build/out`, `~/notes.md`, `README.md`, or a
  `file://` address. Relative paths resolve against the pane's working
  directory. A backslash-escaped path, as a file drop pastes, names the
  unescaped file.

Verified text gets the solid underline and the hand cursor, and the modifier
click opens it. In a mouse-aware application, the modifier click on a link
opens it instead of going to the application; hold Shift to select text there.
Links inside a popup target that popup, and menus block activation. Dragging
does not activate a link: a drag across a link copies it as text, and the
click that opens it is the one that never left the half-cell it pressed in.

An explicit hyperlink shows the hand cursor whenever the modifier is held, and
its dashes turn solid once its target is verified:

- **A web, `mailto:`, or `vscode:` address** opens: web addresses in your
  default browser, the others in the application registered for them.
- **An address with any other scheme** first shows the whole address and asks
  whether to open it, because terminal output is untrusted and such an address
  starts another application.
- **A file**, named by a `file://` address or by a target without a scheme
  such as `/tmp/a`, `~/notes.md`, or `./src/main.rs`, opens when it exists.
  When it does not, the link keeps its dashes and a click says that the file
  was not found.
- **A `file://` address on another host**, or a target that is no valid
  address, keeps its dashes; a click says why nothing opens.

A plain URL is read from its row. One that reaches the pane's right edge may
wrap onto the next row; a daemon that offers `pane.link.resolve` reads it
whole, and otherwise it needs explicit terminal hyperlink metadata. Paths are
not links in panes on SSH hosts, whose files live on that host.

A file opens in your terminal editor at the printed line, in a new pane split
to the right of the pane that printed it (see [Code Navigation](#code-navigation)).
Folders, images, videos, and PDFs open with the system's default application
instead, as does every file with `open_files_in = "system"`. Holding Alt
(Option on macOS) as well opens a file in the other one. There, terminal output
is untrusted, so a click never launches anything: only a plain folder or a
non-executable document (text, source, Markdown, JSON, images, PDF, and
similar) opens itself, judged by where symlinks lead. An application, script,
or unknown file type opens the folder that holds it, and a network path opens
nothing.

Set `open_links_in = "browser-tab"` to open web links in a [browser tab](#browser-tabs)
instead. Holding Alt (Option on macOS) as well opens a link in the other target.

With a daemon that offers it, a link click, a file path or `file://` hyperlink
included, first goes to the daemon so a plugin's `[[link_handlers]]` entry can
claim it; see [Herdr Plugins](#herdr-plugins).

## Herdr Plugins

Herdr runs plugins; this client never reads plugin manifests, runs plugin
commands, or touches the plugin registry. It works with the parts of a plugin
the daemon exposes to clients:

- **Actions.** Bind a plugin action in Herdr's own config with
  `[[keys.command]] type = "plugin_action"`. It then runs from its key, as a
  direct shortcut or a prefix chord, from the command palette, and is listed in
  the shortcut reference. Herdr does not offer clients a list of plugin actions
  nobody bound, so an unbound action cannot be run from here.
- **Selected text.** A plugin action run while text is highlighted in the focused
  pane, by a mouse selection kept after release or a copy-mode mark, is given that
  text as `selected_text` in `HERDR_PLUGIN_CONTEXT_JSON`. The client sends the
  selection's cells and Herdr reads the text from its own terminal, so the plugin
  gets exactly what was highlighted on the host that runs the pane, without going
  through the clipboard. As in Herdr's terminal client, only plugin actions receive
  it, only from the focused pane, and never from a popup. The palette takes the
  selection when it opens. Herdr reads the highlighted cells as they are in the
  frame on screen when the action runs, and refuses the action, showing the
  error, if the pane changed after that frame was drawn; run it again. With
  `copy_on_select` on and
  `keep_selection_after_copy = false`, a mouse selection is cleared on release, so
  there is none to send.
- **Link handlers.** A link click that a handler claims opens nothing here. If the
  handler fails, its error is shown and nothing opens in its place. A click that
  nothing claims opens here as described in [Terminal Links](#terminal-links).
  Herdr matches handlers against the URL it reads at the click (a terminal
  hyperlink's target, or a web address in the text), so a plain printed path is
  never claimed. A link-modifier click on any terminal hyperlink is offered to
  the daemon, including one this client never opens itself, such as an SSH
  host's own `file://` link, so a handler on that host can still claim it.
- **Popups and panes.** Plugin popups, splits, and tabs are ordinary daemon
  terminals: keys, paste, input methods, and the mouse go to a popup while it is
  open, and focus is whatever the daemon reports, including after a plugin's
  process exits and its popup closes.
- **Metadata.** Tab-bar status segments, custom sidebar tokens and row rules, and
  agent views from `agent.view.set` are shown as the daemon reports them; see
  [`[usage]`](#configuration) for the switches that hide them.
- **Settings > Plugins.** Lists every custom `$name` value that plugins and hooks
  report on a connected host (`pane.report_metadata` for agents,
  `workspace.report_metadata` for workspaces), with an example and the hosts
  reporting it, plus agent status labels. Herdr draws a value only where its
  `[ui.sidebar]` rows name it, so each has a switch that adds the token as its
  own row to `rows` in the shared Herdr config, or removes it there; the
  terminal client shows the same rows. Showing a token when `rows` is unset
  writes out Herdr's default rows first, styled occurrences count as shown, and
  per-agent `rows_by_agent` overrides are never edited. Tokens already in `rows`
  stay listed while nothing reports them, so they can be hidden. A preview
  draws an example workspace and agent with the configured rows, including
  token styles and the rules that restyle or hide a value. While the rows are
  still Herdr's defaults, this app draws its own native rows; the section says
  the first switch moves the sidebar to the configured rows, in your row
  style. With `[usage] inline = false` the section says the switches only
  change the terminal client and offers a switch that turns `inline` back on.
  The switches are read-only on Windows, where shared settings are. Herdr offers clients no
  way to list, enable, or disable plugins or read their logs, and the snapshot
  does not say which plugin reported a value.
- **Clipboard.** A plugin terminal that copies with OSC 52 reaches this machine's
  clipboard, local or remote, through the same bounded path as any pane. A plugin
  that runs a clipboard command such as `pbcopy` on a remote host writes that
  host's clipboard, not this one.

## Code Navigation

Herdr UI is not an editor, but it can get you to the right line quickly,
next to the agent that is changing the code.

- **Open in your editor.** Cmd-click (Ctrl-click elsewhere) a printed path
  such as `src/main.rs:42` to open it at that line in your terminal editor, in
  a new pane split to the right of the pane that printed it. In a
  [review](#reviewing-an-agents-changes), `e` opens the file at the top of the
  diff the same way. Quitting the editor closes its pane. When the editor is
  Neovim, later files for the same tab open in that Neovim instead of a new
  split, as [herdr-nvim](https://github.com/ChmaraX/herdr-nvim)'s sidebar
  does.
- **Go to Symbol** (`cmd-shift-o`) lists the functions, types, modules, and
  constants of the focused pane's checkout; **Go to File** (`cmd-o`) lists its
  files, and takes `file:line` to open at a line. Tab switches between the two.
  Before you type, both start with what the agent is changing: the checkout's
  uncommitted files, newest first, with their `+added −removed` line counts,
  or the definitions in them.
  Enter opens the result in your editor beside the focused pane;
  Cmd-Enter (Ctrl-Enter elsewhere) opens it in a code tab instead.
- **Code tabs** show a file read-only with syntax colouring and an outline of
  its definitions, beside the workspace's terminals like any other tab. Click
  an outline entry to jump to it, or a line to mark it. `j`/`k` and the arrows
  scroll, Space and Page Up/Down page, `g`/`G` go to the top and bottom, `o`
  shows or hides the outline, and `e` opens the marked line, or the top line in
  view, in your editor.

The editor is `$VISUAL`, then `$EDITOR`, then `vi`, as the new pane's shell
sees them, started as `EDITOR +LINE FILE`. Set `editor_command` to choose
another, with `{file}` and `{line}` placeholders where it needs them, such as
`editor_command = "hx {file}:{line}"`. Placeholders may sit inside double
quotes, as in `subl "{file}:{line}"`; either way the path stays one word. The command is typed into the pane's
shell, so the file's path must not contain a quote, backslash, or control
character; such a file opens in the system's application instead.

A Neovim editor is started with `--listen` on a socket in this app's private
state folder (`herdr/gpui/nvim` under `$XDG_STATE_HOME` or `~/.local/state`,
created with mode 700), and
reused with `nvim --server … --remote-expr`, which opens the file with `:drop`,
so `nvim` must be on the app's `PATH`. If that Neovim has quit, the file opens
in a new split; if it is busy, such as on a swap-file prompt, its pane comes
forward to be answered. An `editor_command`
naming `nvim` listens too; one with `{file}` or `{line}` runs exactly as
written and is never reused.
On Windows, where a pane's shell has no `sh` to start the editor with, printed
paths open in the system's application and Go to Symbol and Go to File open
code tabs.

The symbol index is built in the background from the files Git lists (tracked
files and untracked ones it does not ignore), and kept for the next search;
an index older than 30 seconds is read again while the last one stays usable.
Symbols come from each line's leading keywords, as `ctags` reads them, for
Rust, Swift, Ruby, Python, Go, JavaScript and TypeScript, Kotlin, Java, C#,
Elixir, Zig, and the types and macros of C-family files. This is a heuristic, not a language server:
a definition a macro generates, or one split oddly across lines, is missed, and
C and C++ functions are not listed. Only local checkouts are indexed. Limits
keep a huge repository responsive: at most 100,000 files are listed, files over
1 MiB or that are not UTF-8 text are skipped, and at most 300,000 symbols are
kept. Code tabs read files up to 1 MiB and colour their first 20,000 lines.

## Devices Overview

The activity card at the top of the device picker (the "All Devices" button
at the foot of the sidebar), View > Devices Overview, or the command
palette's "Devices Overview" opens a tab listing every device the window
connects to. The card itself shows every device's agent activity for the
last two hours and how many agents are working and blocked now. Like
any tab, it can share a group with terminals, take a group of its own, or fill
a window.

- **Agent activity:** working agents, with blocked ones stacked above them, for
  each of the last 120 minutes, with how many are working now and the peak.
  "View all devices" adds one lane per device on the same time axis. Darker
  cells mean more agents were working, red cells mean one was waiting for
  input, and blank cells mean the device was not connected.
- **Totals:** working, blocked, and idle or done agents across every device,
  and how many devices are online.
- **Devices:** one row per device with a dot per agent, how many are working
  and blocked, CPU, memory, free space on the volume holding the home
  directory, and uptime. Clicking a row opens it on that device's agents,
  working first, with each agent's workspace and state; clicking an agent
  shows its pane.

The search field narrows the lanes and the table to devices whose name,
address, agents, or workspaces match every word typed. Load columns wrap in
narrow tabs. **Add Device** opens SSH setup, with WSL beside it on Windows; it is
unavailable in explicit-socket and development-catalog windows.

The window keeps the activity history from the snapshots it already receives,
so it starts empty each time the window opens; nothing is saved or asked of
the daemon. Each minute holds the most agents seen working and waiting at
once, and minutes the machine slept stay empty. A wall-clock jump forward of
at least five seconds beyond monotonic elapsed time is treated like sleep;
backward adjustments do not rewind or pause the history. CPU, memory, disk, and uptime
come from the same sampling as the status bar's load, and every enabled host
is sampled while an overview tab is open, even with the status bar's load
hidden. A cloud machine has no host to sample, so its load columns stay
empty.

## Editor Groups

The split button at the right end of the tab strip (or **Split Editor**,
`Cmd-\`) splits the view into side-by-side groups, as an editor does: keep an
agent's terminal in one group and its page in another. Split as often as you
like; each split opens a group to the right of the one it came from, without
tabs. Splitting opens nothing new and moves no tab.

- A tab belongs to one group, and only that group's strip lists it. A new
  group says how to open a tab in it; a tab opened in a group is that
  group's own. A tab opened elsewhere, by an agent, the CLI, or another
  client, goes to the group in use.
- Drag a tab onto another group to move it there: the group it is carried
  over is outlined, and dropping the tab shows it in that group. The group it
  left goes to the tab beside it, and closes when that was its last tab.
- Groups show different Herdr tabs live side by side: each group showing a
  terminal has its own client connection to the daemon, which keeps a focused
  tab per client. The group in use holds the window's connection and gets the
  keyboard; the others only display their tab, report themselves unfocused so
  notifications, sounds, and the title follow the group in use, and focus
  their own tab again when an agent or the CLI moves every client. Pressing a
  group swaps its connection in. When the daemon moves the window's focus to
  a tab of another group, that group becomes the group in use.
- The group in use has the keyboard, and its chosen tab carries the accent.
  **+** opens a terminal tab in that group. The group's **…**, at the end
  of its strip, offers the other kinds of tab: **New Browser Tab**
  (`cmd-shift-b`), **Review Changes** when the Git chip tracks a checkout,
  and the workspace's listening ports, each opening its page. `cmd-t` and
  `cmd-shift-b` open in the group in use, where Close Tab acts too. Close
  Tab in a group without tabs closes the group.
- Drag a divider to resize the groups beside it. Groups are the window's own,
  per workspace. A group's own connection closes with the group, or when the
  window leaves the workspace or host; returning reconnects it.
- Each workspace's groups, their tabs and widths, and the group in use are
  saved in `$XDG_STATE_HOME/herdr/gpui/editor-groups.json` and come back
  after a restart; the group in use returns to its own tab. Closing a
  workspace in Herdr forgets its groups, and a damaged file starts every
  workspace unsplit. With several windows on one workspace, the last change
  is what is saved.
- Closing a tab closes it for good, split or not: a tab's close button and
  its right-click **Close Tab** close a page at once and a Herdr tab in
  Herdr, through its confirmation. Split, **…** offers **Close Tab**, which
  does the same for the tab the group shows; **Close Others**, which closes
  the group's other tabs one after another, each as its own close button
  would, and stops at the first close you decline; and **Close Group**,
  which closes no tab: the group's tabs go to the group that takes its room.
  A group whose last tab closes, closes too.
- A layout saved by an earlier version, where every group listed every tab
  except the ones closed in it, still restores: each tab goes to a group
  that had kept it.
- Right-click a tab for **New Tab** (opening in that group), **Rename**, and
  **Close Tab**, as in Herdr's own tab menu. A zoomed tab, where one pane
  fills the tab, shows a corner mark after its title, and the pane menu
  offers **Zoom** or **Unzoom** to match.
- **…** also offers **Split Right**.
- A split's new group opens from the right, sliding in at its own width
  while the group it came from gives up the room; a closed group folds away
  to the right as its neighbour takes the room back.
- A tab that opens grows into its strip, and one that closes or moves to
  another group shrinks out where it stood. The tabs a strip already has when
  the window first draws it appear at once.

## Browser Tabs

A browser tab shows a web page in place of the terminal, next to the
workspace's Herdr tabs. Herdr panes are always terminals, so browser tabs
belong to this app alone: the daemon, the TUI, and other clients never see
them. Every window lists the same tabs for a workspace, and each window has its
own page for each one. Tabs are saved in
`$XDG_STATE_HOME/herdr/gpui/browser-tabs.json` (default `~/.local/state/`) and
come back after a restart; closing a workspace in Herdr removes its tabs.

- Open one from a group's **…** menu, with **New Browser Tab** (`cmd-shift-b`)
  in the command palette, from a clicked link
  (see [Terminal Links](#terminal-links)), or from an agent (below). A new
  tab starts blank with its address field focused, and lists the
  workspace's listening ports: clicking one loads its page in that tab.
- A tab shows its page's favicon, drawn from the page's `<link rel="icon">`
  or a site's `/favicon.ico`, and a globe until the page has one. An icon
  from another origin shows only when that origin allows the page to read
  it (CORS); otherwise the tab keeps its globe.
- The toolbar has back, forward, reload, the address field, and a button that
  opens the page in the system browser. The address field accepts bare hosts:
  `localhost:3000` becomes `http://localhost:3000/`.
- While a browser tab is focused, Increase and Decrease Font Size
  (`cmd-=` / `cmd--`) zoom its page a step, from 50% to 300%, and Reset Font
  Size (`cmd-0`) returns it to 100%. The tab is the one whose page has the
  keyboard, or else the one the group in use shows. On macOS the keys bound
  to those commands zoom a page that has the keyboard too, as Safari's do,
  so the page cannot take them for itself. Each tab keeps its own zoom, saved with it, and every window's
  page for the tab follows it. A zoomed page's toolbar shows its zoom;
  clicking it returns to 100%. An agent zooms a tab with
  `herdr-gpui browser zoom`.
- Close Tab and Close Pane close the browser tab being shown, without asking.
  Clicking a Herdr tab, or switching Herdr tabs in the workspace, shows its
  terminal again. Next Tab, Previous Tab, and the tab numbers count browser
  tabs too, after the workspace's Herdr tabs. Move Tab Left and Right move
  the browser tab being shown among the browser tabs, wrapping at either
  end, as dragging it does; a Herdr tab moves among the Herdr tabs.
- Only `http` and `https` pages open, plus local files an agent shows (below).
  Pages cannot navigate to other schemes
  (so `file:` and applications' custom URL schemes stay closed), downloads are
  refused, and a page's new windows open as new browser tabs. The only
  messages a page can send the app are annotation picks, which fill a draft
  note and nothing else, and its icon, which shows on its tab; the app runs
  only its own scripts in pages. On
  Windows, a frame inside a page that links to an application's URL scheme
  gets WebView2's own confirmation prompt rather than being refused outright.
- Pages are native web views (WebKit on macOS, WebView2 on Windows). On
  macOS a page lies beneath the window's own drawing. Menus, popovers,
  dialogs, tooltips, toasts, and the file-transfer card show over it, and
  the page stays live around them. The grip between two groups keeps its
  full width over a page. While a menu or dialog is open, a file dropped on
  the window never reaches a page. A file dropped on a toast or the
  file-transfer card still reaches the page beneath.
- On Windows a page is layered above the window, so it steps aside for a
  menu, popover, or status bar tooltip that would fall over it, and for a
  dialog, which dims the whole window. Its place is empty until the menu
  closes. A page also steps aside while a toast or the file-transfer card
  falls over it, as when a page or VS Code fills the right-hand group or the
  whole editor area, so they stay visible and clickable. A page that none of
  these reach keeps showing.
- Linux has no embedded pages yet: browser tab requests open the system
  browser, and local files and annotations are unavailable.

### Annotating A Page

**Annotate** in a browser tab's toolbar lets you pin notes to a page and send
them to the agent that opened it, so it can change the page.

- Hover to outline an element and click to pick it, or select text. For
  anything else, draw a region: turn on **Region** in the notes panel and
  drag, or Shift-drag at any time. Links do not follow while annotating.
  **Note on page** writes a note about the whole page. Escape drops the
  current pick; a second Escape stops annotating.
- On macOS each pick also takes a screenshot of that part of the page, as it
  looks on screen and without the annotation overlay, shown in the note. It
  comes from WebKit's own snapshot, the one place the app calls WebKit
  directly (`browser/snapshot.rs`). Screenshots are saved when you send, as
  private files in `$XDG_STATE_HOME/herdr/gpui/annotations/`, removed after a
  week, and the prompt names each file. Windows notes have no screenshots.
- Notes are written in the panel beside the page, not in the page, so the
  page never sees them. Queued notes are numbered on the page. Up to 20 wait
  per tab. The panel slides open and closed; a new note grows into it, and
  its number pops onto the page unless the system asks for reduced motion.
- **Send to agent** turns them into one prompt: the page, then for each note
  the element's selector path, text, and a short HTML snippet (for a region:
  its position, the container and elements it covers, and their text), its
  screenshot, and your note.
  Page text is cleaned of control characters and marked as quoted data.
  **Copy** puts the same prompt on the clipboard instead.
- When the agent takes the notes, its tab replaces the page and its pane
  takes the focus, so you see it pick them up. Notes held for a busy agent
  do this once they are pasted; an agent in another workspace is brought up
  once the window arrives there. Notes from a mockup window bring the Herdr
  UI window forward the same way. Notes that are kept for `browser
  feedback` or copied leave the page where it is.
- Sent notes stay listed, dimmed and marked sent, so a note can be changed
  and sent again. Click a note's text to edit it in the composer; saving it
  marks it unsent. Send then delivers only the notes not sent yet (**Send 2
  new**), or every note again once all were sent (**Resend all**), and Copy
  copies the same ones. **Clear sent** removes the sent notes, ending the
  round. Another Send waits while screenshots are saving, delivery is pending,
  or the previous batch is still waiting to be collected through `browser feedback`.
  While a batch waits for a busy agent or for `browser feedback`, the panel
  says so and offers **Withdraw** in place of Send: it takes the batch back
  before the agent has it and marks its notes unsent, so you can edit them and
  send again. A batch already typed into the pane cannot be withdrawn, and
  nothing joins an agent's running turn: Herdr offers this client no way to
  prompt an agent mid-run, so busy agents get notes once they are idle.
- The prompt goes to the agent one way only. An agent waiting in
  `browser feedback --wait` receives it there. Otherwise it is typed into the
  agent's pane and submitted once Herdr reports the agent idle (or still
  working after two minutes). It is never typed into a pane where Herdr sees no
  running agent, since Enter there would run it in a shell, nor into an agent
  that is asking you a question; those notes wait for `browser feedback`, as
  do notes for a pane this window does not show. Notes written in a session
  that is no longer shown are copied, never handed to another session's
  agent.
- Feedback is scoped to the caller's daemon socket and pane together, so two
  local sessions with the same pane ID cannot take each other's notes. Use the
  matching CLI build: older pane-only feedback requests are rejected rather
  than guessed. The mockup receiver still watches its file fallback when an
  older running app rejects scoped feedback.
- Tabs you open yourself have no agent to send to; **Copy** is offered
  instead.
- A local page can also queue a result for its agent with
  `herdr.send(summary, data)`, for example the mockup variant you picked.
  It appears in the notes panel as **Page result**, with a hint to add a note,
  and goes to the agent only when you press Send, as "The page sent a result"
  followed by the summary and the data as JSON. A newer result replaces an
  unsent one and keeps the note you added to it. The bridge is injected only into local pages, only while they
  show the preview scheme. Any frame can still post to the app, so it checks
  the frame that sent each result and drops it unless that frame shows a
  local file: a web page embedded in the local page, or one it is navigating
  to, cannot send.
  The summary is one cleaned line of at most 300 characters; data over 8 KB
  is refused rather than cut. `herdr.send` returns false for a result the
  app would refuse, so the page can say so; whether the app kept one it was
  handed shows in the window, as the note or a warning.

### Reviewing An Agent's Changes

**Review Changes** in a group's **…** menu opens a review tab on the focused
local checkout's changes, with untracked text files as wholly added,
and lets you send review notes to the agent that made them, like inline
comments on a pull request.

- The review is a tab like a browser tab: it sits in the group in use, can be
  moved to a group of its own beside the terminal, take the whole window, or
  close, and it is restored with the other tabs. Opening the review again
  brings back the checkout's tab and reads its changes afresh. Its notes go
  to the pane of the agent that was focused when it opened.
- The icons at either end of its header show or hide the list of changed
  files and the notes. A review narrower than 1,000 px starts with both
  hidden so the diff has the room; once toggled, the choice holds. Writing a
  note always shows the notes, and the notes icon counts the unsent ones.
  Shown in a narrow group, each panel keeps to 30% of the review.

- **Uncommitted** shows what is not committed yet (`git diff HEAD`).
  **Branch** shows everything the branch's pull request will hold: the
  working tree against where the branch left its base, so its commits and
  uncommitted work together. The base is the pull request's base branch when
  GitHub reported one, else `origin/HEAD`, then `main` or `master`, whichever
  exists locally; nothing is fetched. The choice is remembered.
- Click the base after "against" in a branch review to compare with another
  branch, such as an older release branch or the feature branch this one was
  cut from. The list holds the checkout's local and remote-tracking branches,
  those this branch left most recently first; **Automatic** goes back to the
  base above. The pick holds while the review tab is open, and a picked
  branch that no longer exists falls back to the automatic base.
- The two icons in the header draw the diff **unified**, removed lines above
  the ones that replaced them, or **side by side**, the old version on the
  left and the new on the right, each run of removed lines paired with the
  added lines that follow it. Either side's line can take a note, notes and
  their numbers carry over when switching, and the choice is remembered.
  Side by side, drag the line between the halves to give either more room
  (each keeps at least a fifth); a double-click on it evens them again.
- A list of the changed files sits left of the diff, as on a pull request:
  a tree of folders, a folder holding only one other sharing its line
  (`src/review/`), each file with how it changed (A, M, D, R), its added
  and removed lines, how many notes are queued on it, and a check once
  viewed. Click a folder to fold it. Type in the list's filter to keep the
  files whose path holds the text, and **Hide viewed** to leave viewed files
  out. Clicking a file, or picking it with the arrows and pressing Enter,
  brings it to the top of the diff, in either layout; the file at the top of
  the diff is marked and kept in view as the diff scrolls. The list resizes
  by its right edge and its width is remembered.
- Long lines wrap, as on GitHub, so nothing is cut off at the edge.
- Each file's header folds it (the chevron, or `x`) and marks it **Viewed**,
  which folds it too. Deleted files, lockfiles and files `.gitattributes`
  marks `linguist-generated` start folded. A viewed file that changes again
  is no longer viewed. Renames show as `old → new`.
- A hunk header shows how many unchanged lines it leaves out above it;
  click that to show them, up to 200 at a time, read from the working tree.
- Removed lines and the added lines that replaced them have the words that
  changed marked, when the two lines are an edit of each other.
- The whitespace icon leaves changes in whitespace alone out
  (`git diff --ignore-all-space`).
- Find in the changes with the search icon, `/`, or Cmd-F (Ctrl-F) while
  the review is in use; Enter and Shift-Enter, or `n` and `N` in the diff,
  step through the matches, and a match in a folded file opens it. A query
  with no capitals ignores case.
- With the diff in use, `j`/`k` and the arrows scroll a line, Space and Page
  Up/Down a page, `]`/`[` move to the next or previous hunk and `.`/`,` to
  the next or previous file, `x` folds the file at the top and `v` marks it
  viewed.
- The review reads its changes again when the checkout's change counts
  move, keeping the line it was on, unless a note is being written.
- A scrollbar along the diff's right edge shows how much of it is in view
  and where; drag its thumb to move through a long change.
- Changes of any size stay reviewable. The files are listed first
  (`git diff --numstat` and `--name-status`), so the list and every file's
  header show at once; their lines are then read a batch of files at a time
  in the background, at most 64 files or 20,000 changed lines per Git call,
  and files scrolled into view are read first. Past 500,000 changed lines,
  a file is read once it scrolls into view. A file with more than 20,000
  changed lines, or larger than 4 MiB on disk, is listed as "Large change
  not shown" with a **Load diff** button that reads it whole. Binary files
  are listed and never read. Each file keeps its text in one buffer, so a
  change of hundreds of thousands of lines stays compact, and the diff only
  lays out the rows in view.
- The notes panel resizes by dragging its left edge, with the sidebar's
  cursor and hover tint, and a double-click on the edge restores its
  default. Settings' section list resizes the same way by its right edge.
  The review and an annotated page share the width, which is remembered
  with the sidebar's.

- Click a line's gutter (its numbers and sign), added, removed or unchanged,
  or a file name, write what should change, and press Enter or **Add note**.
  Escape drops the note being written. Noted lines carry the note's number,
  and unsent notes stay while the tab is open.
- The code itself selects like an editor's: drag across it, double-click a
  word, triple-click a line, Shift-click to extend, and Cmd-A (Ctrl-A
  elsewhere) for the whole file at the top. Cmd-C or **Edit > Copy** copies
  the code alone, never line numbers, `+`/`-` signs, or hunk headers; side by
  side, a selection keeps to the side it started on, and Select All takes the
  side last clicked in that file, else the new side (the old side of a deleted
  file). A folded file has no code to select. Escape clears it. Tabs copy as the four spaces they are drawn as.
- The copy icon on a file's header copies its path, and the one on a hunk
  header copies the hunk as a patch reads, signs and header included.
- Notes go to the agent in the focused pane, or else to the first agent Herdr
  reports in the focused workspace; the header names it. **Send to agent**
  turns them into one prompt: the checkout, then for each note the
  `path:line`, the quoted line, and your note. Line numbers are the working
  tree's in both views; a removed line names the revision it is numbered in
  (`HEAD`, or the base and commit), so notes from either view stay exact and
  stay queued when you switch. It reaches the agent the same one way as page notes above,
  including `browser feedback`. Without an agent, **Copy** is offered.
  As with page notes, sent notes stay listed until **Clear sent**: click one
  to edit it, and Send delivers only new or edited notes, or all of them
  again once every one was sent. Another Send waits until the previous batch
  has been delivered or collected through `browser feedback`, or until you
  **Withdraw** it. The notes icon
  counts the unsent ones.
- Git runs in the background, never on the UI thread, with explicit `a/`/`b/`
  prefixes and no external diff tools or text conversion. Diff text is
  cleaned of control characters and bounded (16,384 characters a line,
  250,000 lines a file, 5,000 untracked files; links and binaries are not
  read). Only the local daemon's checkouts can be reviewed.

### Local Pages

`herdr-gpui browser open ./mockup.html` shows a local file. The app serves it,
with the other files in its folder, through a private `herdr-preview:` scheme,
since pages cannot open `file:` addresses:

- Only that folder is served, never a file a link leads out of it, and never a
  hidden file or folder. A folder holding your home directory is refused.
- Web pages cannot load the folder's files: requests carrying another origin
  are refused.
- Files are read off the UI thread and are never cached, so
  `herdr-gpui browser reload` shows an edited file. Showing the same file
  again from the same pane reuses its tab.

### Letting Agents Open Pages

Agents running in the app's panes can open a page for you:

```sh
herdr-gpui browser open http://localhost:3000   # macOS: /Applications/Herdr.app/Contents/MacOS/Herdr
herdr-gpui browser open design/mockup.html      # in a split beside the agent's terminal
herdr-gpui browser open --no-focus https://example.com/docs
herdr-gpui browser open --tab design/mockup.html # a tab, like a web address
herdr-gpui browser reload                       # reload the pages this pane opened
herdr-gpui browser feedback --wait 600          # wait for the notes you send it
herdr-gpui browser --help
```

The tab joins the caller's own workspace, which Herdr names in the pane's
`HERDR_WORKSPACE_ID`, and remembers the caller's `HERDR_PANE_ID` so notes on
the page go back to it. The command talks to the running app over a socket at
`$XDG_STATE_HOME/herdr/gpui/control.sock`, readable only by you, and exits with
3 when the app is not running. On Windows it is a named pipe only your user
can open ([details](../../docs/cli.md#windows)). Only processes on this
machine can reach it.

Agents on a saved SSH host open pages another way, since that host cannot
reach this machine. The app writes a small `herdr-gpui` shim there, at
`~/.local/state/herdr/gpui/bin/herdr-gpui`; it needs only `sh`, `cut`, and
`sed`. The shim leaves the request on the caller's Herdr workspace as metadata
tokens (`herdr workspace report-metadata`, named `herdr-gpui-*`), which expire
after 30 seconds. A window connected to that host sees them in its snapshot
and opens the page in that workspace, once per request:

```sh
~/.local/state/herdr/gpui/bin/herdr-gpui browser open localhost:3000
~/.local/state/herdr/gpui/bin/herdr-gpui browser open --no-focus https://example.com/docs
~/.local/state/herdr/gpui/bin/herdr-gpui browser reload
```

- An address on the host's own loopback (`localhost`, `127.0.0.1`, `[::1]`,
  `0.0.0.0`) is forwarded first, as `herdr-gpui ports open` does, and the tab
  shows the forwarded `127.0.0.1` port with the same path and query.
- Only `browser open` and `browser reload` work from a host; every other
  command exits 8. Files on the host are not shown: serve their folder (for
  example `python3 -m http.server 8000`) and open its port. Addresses are
  limited to 960 characters.
- Notes you send on such a page are typed into the agent's pane once it is
  idle, as for a local agent; `browser feedback` cannot reach the app from
  the host.
- Anything on the host that can run `herdr` can set the same tokens, so a
  request can do no more than open an http or https page in the workspace
  that carries it, or reload the tabs its pane opened. A bad address is
  refused with a notice in the window.
- Every Herdr UI connected to the host acts on the request, and a request
  made while no window is connected is dropped when its tokens expire. Each
  request names its own tokens, so requests made together from several panes
  all open; a workspace holds at most 32 tokens, so a burst of long addresses
  can be refused until earlier ones expire.

A local file opened from a pane goes beside that pane's terminal, since it is
usually a mockup or explainer the agent wrote for you to read while you talk
to it; a web address takes a tab of the group in use. `--beside` and `--tab`
choose instead. Beside means an editor group next to the group showing the
caller's terminal: the group the page already belongs to, the group beside
the terminal when the window is split, or a new group split off to its
right. The terminal's group stays in use and keeps the keyboard, so you can
keep typing to the agent. A workspace the window is not showing, or a
terminal the user has switched away from, gets an ordinary tab instead, and
the command says so: splitting beside another terminal would leave the
agent hidden and the keyboard with someone else.

#### Explainer Kit

Local pages can load a small kit the app serves at `/.herdr/kit.css` and
`/.herdr/kit.js`, for interactive explainers and UI mockups: segmented
controls, a row of steps with their status, the selected step's detail, an
outcome, placeholders, and mockup variants with **Pick** buttons that call
`herdr.send`. `/.herdr/theme.css` carries the window's theme colors, so the
page matches the app and follows a theme change on its next load. Dotted
paths are never read from the page's folder, so these cannot hide a file of
the page's own. Opened elsewhere, the kit falls back to its own light and
dark colors. `herdr-gpui browser example` prints a complete page built on it.
The kit inserts every text as text, never as HTML.

Agents only use what they know about, so the app offers, once, to install a
`herdr-gpui-browser` skill that teaches them these commands. It goes into
`~/.claude/skills/` and `~/.agents/skills/`, for whichever of `~/.claude` and
`~/.agents` exists, and names this app's executable. Claude Code reads
`~/.claude/skills/`; Codex reads `~/.agents/skills/`; opencode reads both, as
well as its own `~/.config/opencode/skills/`. After you agree, each
start refreshes it off the UI thread, so it follows the app when it moves or
updates. Only files carrying the app's managed-skill marker are rewritten or
removed; a skill of the same name you wrote yourself is left alone.

- The offer appears in the first window after it connects. **Not now**, or
  closing it, is remembered in `$XDG_STATE_HOME/herdr/gpui/agent-skill.json`
  and it is not asked again.
- Preferences > Agents installs or removes it at any time; so does the
  palette's **Install Browser Skill for Agents**.
- Only release builds offer or refresh the skill. A development build never
  points your agents at itself unless you install from it explicitly.
- Once you agree, the app also puts the shim and a copy of the skill naming
  it on each saved SSH host it connects to, once per run, over the same
  noninteractive SSH. The copy goes only into the host's existing `~/.claude`
  or `~/.agents`. As here, a shim or skill file without the app's marker is
  left alone. A host without a POSIX shell is skipped. Removing the skill
  takes the shim and those copies off each host again, the next time a
  window connects to it.

`herdr-gpui browser skill` prints the same text, for example to install it
elsewhere:

```sh
mkdir -p ~/.claude/skills/herdr-gpui-browser
herdr-gpui browser skill > ~/.claude/skills/herdr-gpui-browser/SKILL.md
```

## VS Code

VS Code opens as a tab of a space's editor groups, like a browser tab, for
reading code and reviewing diffs while agents work in the terminals: split
the group to see it beside a terminal, or show it alone. It is a second frontend beside Herdr's: the
page comes from VS Code's own web server, `code serve-web`, and the app adds
nothing to what that server serves.

**Settings > Code** chooses where that server comes from.

**Start VS Code automatically**, the default once VS Code is found, has the
app run the server itself. It looks for VS Code's `code` command on your
login shell's `PATH`, then, on macOS, in
`/Applications/Visual Studio Code.app`, and runs the `code-tunnel` program
beside it, which is what `code serve-web` runs:

```sh
code-tunnel serve-web --host 127.0.0.1 --port <port> \
  --connection-token-file <state>/vscode-token --accept-server-license-terms
```

- The server is Microsoft software under the
  [VS Code Server License Terms](https://aka.ms/vscode-server-license). The
  app never accepts them for you: the VS Code tab and the settings page show them
  with **Accept and start**, and only after that does the app pass
  `--accept-server-license-terms`. The acceptance is saved as
  `license_accepted = true` under `[code]`.
- It starts the first time a VS Code tab needs a page, never before, and says "Starting VS Code… (the first run downloads it)"
  until VS Code answers: the first start downloads the server build.
- The port is picked once and saved as `port` under `[code]`, since VS Code
  keeps its settings in the storage of the page's address. If another
  program holds that port, the VS Code tab and the settings page say so; quit it,
  or remove `port` to have a new one picked (VS Code then starts without the
  settings it kept). The app sends nothing to a port it does not hold, since
  the token must reach only its own server. It asks that server for its
  version alone, never with the token, once its own `serve-web` says it
  listens there, and counts the answer only if `serve-web` still runs after
  it; a page, which must carry the token, is made only while it runs.
- The connection token is made once and kept in `vscode-token` in the app's
  state folder (`~/.local/state/herdr/gpui` unless `XDG_STATE_HOME` is set),
  readable only by you on macOS; on Windows it has your profile folder's
  permissions. It is passed by that file's path,
  never shown, and never logged; messages name only the host and port.
- The app owns that one process. It stops it when it quits: it asks the
  server and what it started to stop, and kills them if they have not after
  a moment. On macOS the server runs in a process group of its own, made
  for it, since `serve-web` leaves its server running when only it
  is stopped; Windows ends the server's process tree with `taskkill /T`. If
  the server stops or fails, the VS Code tab shows **Cannot reach the VS Code
  server** with the reason, and the app starts it again, waiting 1 second,
  then twice as long each time, up to a minute.
- Spaces on SSH hosts still see this computer's VS Code: their folders are on
  another machine, so their pages open an empty window. Running the server on
  the host would need it started there and its port forwarded, which the app
  does not do.

**Use an address** keeps a server you run yourself, as before:

```sh
code serve-web --host 127.0.0.1 --port 8000 --accept-server-license-terms
```

It prints the address to open, with a `?tkn=` connection token. Paste that
address in **Settings > Code**, which saves it in `config-gpui.local.toml`:

```toml
[code]
mode = "address"
url = "http://127.0.0.1:8000/?tkn=..."
```

A config with a `url` but no `mode`, as written before there was a choice,
keeps using its address. Keep the port fixed: VS Code keeps its settings in
the page's storage, which belongs to one address.

- **Test connection** on that page asks the server whether it answers. A
  server that does shows its build's commit; one that does not says why: the
  connection was refused, the token was refused, or the address is not a
  VS Code server. Messages name only the host and port, never the token.
- **VS Code** in an editor group's **…** menu, beside the VS Code icon,
  opens the space's VS Code tab in that group. The row appears once a
  server is set, or VS Code is found to start, in builds that can show
  pages. **Open VS Code** in the Terminal menu or the command palette opens
  it in the group in use, as does a key you bind to `open_code` under
  `[keybindings]`. It has no default key.
- Each space has one VS Code tab, listed with the other tabs of the group it
  belongs to. Opening it again moves that same tab to the group it is opened
  from, and a group left without tabs closes, so
  one group can show VS Code beside another showing a terminal, or a lone
  group can show it alone. It splits, resizes, and is covered by dialogs as
  any page is, and its page keeps running, keeping its state, while no group
  shows it; closing the space in Herdr closes it. It still waits for its
  server, and no agent opens pages in it.
- The page opens on the address above, then goes wherever it navigates, and
  comes back there after a restart. A space's page first opens the folder
  the space started in, as its first tab's first terminal reports it, when
  the space is on this computer; a space on an SSH host opens an empty
  window, since its folder is on another machine than the server. **Open
  Folder** in VS Code picks another; the page then remembers it.
- The app asks the server whether it answers before it opens a page, since a
  page that cannot load stays blank. Linux builds show no pages, so they
  never start VS Code either. While the server does not answer, the tab says
  so and why, and asks again every 5 seconds. Setting a new address moves
  each VS Code tab to the new server, where it stays in its group with its
  folder. A new token for the same server reopens each page where it was,
  with the new token, and so does a restart.
- VS Code fills its tab, without a browser tab's address bar, back,
  forward, reload, or **Annotate**: it is an editor, not a page to browse.
  Its page is otherwise a [browser tab](#browser-tabs)'s, so the same rules
  apply: `http` and `https` only, native web views, and no pages on Linux.
- The keyboard goes to whatever has focus. While a page has it, this one or a
  browser tab's, the page's own shortcuts win, as a terminal program's do
  while its pane has focus. On macOS, Cut, Copy, and Paste act on the page,
  and the keys macOS keeps for the app (quit, hide, minimize, and cycling
  windows) still reach the app. So do the keys bound to Next Tab, Previous
  Tab, the tab numbers, Move Tab Left and Right, and moving to another
  workspace or agent, as a web browser keeps its tab keys from its pages,
  and the keys bound to the font size commands, which zoom the page. Click a
  terminal to take the keyboard back.

## px0

**Open px0** in the Terminal menu or command palette, or **px0** in an editor
group's **…** menu, opens [px0](https://px0.ai/) on the local workspace's
starting directory. Each workspace has one px0 tab; opening it again moves it
to the group in use. Split it beside terminals to browse, edit, and review code.
The tab shows px0's own web UI without the browser toolbar, and keeps its page
alive when another tab is selected. `open_px0` is available in keybindings with
no default shortcut.

Install a px0 build with the review workbench (`/api/wb/ping` and `?open=`).
Herdr UI discovers a running workbench from px0's `workbench.lock`, or looks
for `px0` (`px0.exe` on Windows) on the login shell's PATH and starts it when
a tab first needs it. It passes `-no-open -no-update -no-color -host 127.0.0.1
-port 0 -base-path /` with no target, then reads the actual assigned address.
It never installs or updates px0. An owned workbench keeps its registry and
workspace sessions under `<Herdr UI state>/px0-state/px0`; px0's normal settings
still apply. A discovered workbench keeps its existing state.

**Settings > Code > px0** accepts the address of a workbench you run yourself;
leave it empty for automatic discovery/start and press **Apply px0 address**.
The same setting can be written in `config-gpui.local.toml`:

```toml
[px0]
url = "http://127.0.0.1:7777/"
```

Only loopback HTTP addresses are accepted, including a base path when needed.
The app checks that the address answers as a workbench before loading a page.
Startup failures appear in the tab with **Retry**. Quitting stops the workbench
the app started and its children; attaching to an existing workbench never gives
Herdr UI ownership of it. Closing a tab leaves the workbench available for other
tabs. px0 manages its own per-checkout children and idle cleanup.

Agents can use `herdr-gpui px0 open --json` to open or focus their workspace's
tab. Success reports the tab ID and workspace; it means the tab opened, while
server startup continues in the tab. The existing `--window` targeting applies.
This integration currently supports local workspaces on macOS and Windows;
remote workspaces and Linux embedded pages report unsupported. px0's own agent
harnesses are separate from Herdr's running terminal agents.

## System Notifications

With shared `[ui.toast] delivery = "system"`, daemon notifications (agent
finished and needs attention, plus custom and update notices) go to the OS
notification center instead of in-app toasts. They use the same delay, Done and
Blocked evidence, and connection/boot fencing as toasts. As in the Herdr TUI, a
notification for the active tab of the selected host is skipped only while the
window is focused; background tabs, other hosts, and unfocused windows always
post. Notifications are silent: sounds still follow the Sound settings and
per-agent overrides. Text is the same bounded, control-stripped plain text as a
toast. When the window has more than one host, the body's first line names the
host. A newer event for the same host, daemon boot, and pane replaces the older
notification where the platform allows. When several windows are attached to the
same host, an event is posted once.

Clicking a notification brings the app and its window forward and opens the
target pane, tab, or workspace through the same validation as a toast click. It
only focuses the window when that connection has since reconnected, the daemon
restarted, the target is gone, or a menu page is open. Clicks are handled only
while the GUI runs: a click cannot reopen a closed window.

- **macOS** uses `UNUserNotificationCenter`, which works only from an app bundle
  with a bundle identifier: the release `Herdr.app` (`so.pen.herdr-gpui`) or the
  `just run`/`just run-debug` bundles (`so.pen.herdr-gpui.dev`), which the
  recipe ad-hoc signs so the signature carries that identifier. A bare
  `target/*/herdr-gpui` binary logs that notifications are disabled and posts
  nothing. The first notification asks for permission, and macOS shows that
  prompt instead of the notification itself. macOS remembers the
  answer per bundle identifier, so development bundles ask separately. If
  permission is denied, notifications are dropped; allow them again in
  **System Settings > Notifications > Herdr**. Banners also appear while Herdr is
  frontmost, for events outside the active tab.
- **Linux** uses the freedesktop notification service on the session D-Bus;
  without a notification daemon nothing appears. Clicking requires a server that
  supports the default action, and raising the window depends on the
  compositor's focus policy. Notifications are neither replaced nor retracted, so
  each event shows separately, and each keeps a small waiting thread until the
  server closes it.
- **Windows** uses WinRT toasts under the AppUserModelID `so.pen.herdr-gpui`,
  which the app sets at startup and registers, with its display name, under
  `HKCU\Software\Classes\AppUserModelId`. Once the installer has registered
  `herdr-gpui://` links, a toast opens through one: clicking the banner, or the
  toast later in the notification center, starts the launcher with
  `herdr-gpui://notification?tag=…`; `herdr-gpui.exe` hands the click to the
  running app, and its window comes forward. A click after the app quit only starts it.
  Without that registration (a build run from `target`), toasts answer clicks
  only while the app runs and still holds them, as GPUI posts them.

## macOS Dock Badge

The Dock icon shows the number of agents reporting `Done` (finished) or `Blocked`
(waiting for input), and clears at zero. It covers all connected hosts and
main windows, including minimized windows, without counting the same agent twice.
Sidebar visibility, muted sounds, dismissed toasts, and the notification
delivery setting (including off) do not affect the badge.
Existing attention is counted from each host's first snapshot at startup; no
new completion or notification is needed. The first positive focus report waits
until the terminal surface is ready so loading cannot acknowledge unseen work.

The badge follows daemon status, just like the sidebar: foregrounding the app
does not clear it locally. Finished agents clear according to the daemon's
acknowledgement behavior: two completions become `1` after the daemon marks one
seen. The daemon may acknowledge all panes in a viewed tab together. Blocked
agents remain counted until their status changes, even after you view them.
Disconnecting a host or closing a window removes its contribution, while other
windows can keep the badge visible. Quitting the GUI stops monitoring; this is
not a background notification service. Windows shows the same count as an
overlay on the taskbar button (see [Windows shell](#windows-shell)); Linux shows
no badge.

macOS draws the badge only when **Badges** is allowed in **System Settings >
Notifications > Herdr**. The first time the count rises above zero, Herdr asks
for notification permission including badges, alongside the alerts and sounds
that system delivery uses, so either feature can ask first. macOS prompts only
once per bundle identifier. If Herdr was already allowed notifications without
badges, turn **Badges** on there; if the switch is missing, remove Herdr from
that list and relaunch it to be asked again. A bare `target/*/herdr-gpui`
binary cannot ask for permission.

To preview it without waiting for an agent, choose **QA > Enable badge** in the
macOS menu bar (on Windows, in the window's menu). The preview shows at least `2` and stays on until you choose **QA > Disable badge preview**
or quit. Disabling the preview restores daemon-driven behavior, so real agent
attention can keep the badge visible. This QA setting is not saved.

## Logs

The client's own logs are written to
`$XDG_STATE_HOME/herdr/gpui/logs/herdr-gpui.jsonl` (falling back to
`$HOME/.local/state`, or `%LOCALAPPDATA%` on Windows when `HOME` is unset), one
JSON record per line, readable only by you on Unix. Windows installations with
`HOME` set retain their existing state directory after upgrading. If both
`HOME` and `LOCALAPPDATA` are missing, Windows uses
`%USERPROFILE%\AppData\Local`. Empty variables are treated as unset. Past
16 MiB the file is rotated to `herdr-gpui.1.jsonl`, replacing the previous one,
so at most two files are kept. Logs are not held in memory: **Window > Logs**
reads the newest 5,000 records of the file while it is open, including earlier
runs, and filters, copies, or exports them. Nothing is uploaded. Logging never
waits on the disk; lines that cannot be queued or written are counted as dropped
in the window's status bar. If none of these state-directory variables is
available, nothing is saved and the window says so.

### Crash reports

If the app panics, it writes `crash-YYYYMMDD-HHMMSS.mmm-PID.txt` (the time in UTC) beside the log
before it exits. The report has the version, platform, thread, panic message,
source location, and a backtrace. The app also writes a `Herdr UI panicked`
error record to the log, naming the report, and waits up to two seconds for the
log writer to save it. Release builds ship without debug symbols, so their
backtraces are mostly addresses; the message and location are exact. On Windows
it also writes a minidump, `crash-….dmp`, for a panic and for a crash Rust never
sees, such as an access violation or a stack overflow (an unhandled-exception
filter; `src/crash/windows.rs`). Such a crash gets a short `.txt` report with the
exception code and address, and a `Herdr UI crashed` record. A dump holds every
thread's stack and registers but not the heap. A thread started at launch writes
it, because the crashing thread may have no stack left. Windows Error Reporting
still runs afterwards. Only the five newest reports and three newest dumps are
kept. On macOS and Linux a signal such as `SIGSEGV` leaves no report here. Crash
files never leave the machine. `herdr-gpui diagnostics bundle PATH` includes the
three newest reports, redacted like the log records, and lists dumps by name and
size without including them: they hold the app's memory, so attach one only
when asked.

Local Windows connections are supported through the daemon's named pipe; pass
the binary client socket path to `--socket`, not the JSON API socket. No debug
build or `RUST_LOG` setting is needed: client debug records are captured in
release builds too. For a connection failure, inspect `operation`, `kind`, and
`raw_os_error` in the log: `pipe_name` is name conversion, `pipe_connect` is the
pipe library's open/wait operation, `pipe_read` is a read that failed after
connection, and `pipe_cancel` is a cancelled operation the kernel did not
finish within five seconds of the connection closing. Config failures distinguish `config_lock_open` from
`config_lock_acquire`. These operation records omit paths and retain native
error codes (for example, `5` for Windows access denied). A successful plain
pipe client alone does not establish why the GUI was denied; include these
records when reporting the failure.

## Keyboard Shortcuts

Open **Herdr > Keyboard Shortcuts** to search every application command,
including commands with no binding. Click **Edit**, enter a shortcut such as
`ctrl-shift-t`, and choose **Save** (or press Enter). Separate alternatives with
spaces, for example `ctrl-shift-t alt-t`. **Unbind** removes all shortcuts for
that command; **Reset** removes its GUI override and restores platform defaults
and inherited daemon bindings. Escape cancels the draft. Invalid shortcuts,
duplicate configured bindings, and save failures are shown beside the editor.

Edits are saved in `config-gpui.local.toml` under `[keybindings]` and reload live
in menus, the command palette, and open windows. The same configuration can be
edited by hand:

```toml
[keybindings]
new_tab = "ctrl-shift-t"
split_right = ["ctrl-shift-d", "alt-d"]
close_tab = [] # unbind; remove this line to restore inherited defaults
```

Saving a command replaces its whole shortcut list, including inherited daemon
prefix chords. Assigning a default shortcut to another command moves it there;
two explicitly configured commands cannot share a shortcut. Daemon/plugin
shortcuts remain visible in the reference and are configured in Herdr's own
`[keys]` / `[[keys.command]]` tables. Clipboard shortcuts are fixed Edit actions.

Defaults are selected independently for Linux, macOS, and Windows. macOS uses
Cmd. Linux uses **Ctrl+Shift** for terminal-facing application actions, keeping
Super available to the desktop and plain Ctrl keys available to terminal
programs. This follows terminal conventions alongside the
[GNOME keyboard guidelines](https://developer.gnome.org/hig/reference/keyboard.html),
whose plain Ctrl editing shortcuts also work in the app's text fields.

Windows starts from the Linux profile, which matches Windows Terminal's
Ctrl+Shift conventions: no default uses the Win key, which Windows keeps for
itself, and plain Ctrl keys still reach terminal programs. Reset font size is
Ctrl+0 there, as in Windows Terminal, because Windows can take Ctrl+Shift+0 for
switching input languages. Alt+F4 closes a window through Windows itself.

| Action | Linux and Windows default |
| --- | --- |
| New tab / New window | Ctrl+Shift+T / Ctrl+Alt+N |
| New workspace / New worktree | Ctrl+Alt+Shift+N / Ctrl+Shift+N |
| Copy / Paste | Ctrl+Shift+C / Ctrl+Shift+V |
| Find / Command palette | Ctrl+Shift+F / Ctrl+Shift+P |
| Resume palette | Ctrl+Alt+Shift+R |
| Split right / Split down | Ctrl+Shift+D / Ctrl+Alt+Shift+D |
| Close pane / Close tab | Ctrl+Shift+W / Ctrl+Alt+Shift+W |
| Next / Previous tab | Ctrl+PageDown / Ctrl+PageUp |
| Toggle sidebar / New browser tab | Ctrl+Shift+B / Ctrl+Alt+Shift+B |
| Settings / Keyboard Shortcuts | Ctrl+Shift+, / Ctrl+Shift+/ |
| Larger / Smaller text | Ctrl+Shift+= / Ctrl+Shift+- |
| Reset text size | Ctrl+Shift+0 (Linux) / Ctrl+0 (Windows) |
| Quit | Ctrl+Shift+Q |
| Full screen | F11 |

The in-app reference lists all platform defaults and current overrides. The
Cmd examples elsewhere in this document describe the macOS profile.
Ctrl+Shift+U is left unbound for
[GNOME/IBus Unicode character entry](https://help.gnome.org/users/gnome-help/stable/tips-specialchars.html.en#ctrlshiftu).

## Supported

- Workspace/worktree sidebar with main-checkout parents, indented linked
  workspaces, local collapse arrows (grouped like the TUI: every non-linked
  checkout stays a parent, and a repository groups only while one of its
  linked worktrees is open), branch details, and daemon-driven
  filled/hollow activity indicators taken from the daemon's own status, so the
  GUI and the terminal client always show the same dot. Each worktree row also
  carries its cached pull request number and diff counts. Set
  `change_counts = "local"` under `[sidebar]` (or Settings > Sidebar > Change
  counts) to show the checkout's uncommitted `+added/-deleted` lines against
  HEAD instead; only one set is ever shown. They sit inside the yellow pencil
  chip that marks uncommitted work, so they never read as the pull request's;
  a change with no lines to count (an untracked or binary file) keeps the bare
  pencil. Local counts cover checkouts on this
  machine only, and every one but the focused checkout is counted on the slow,
  rate-limited schedule of the uncommitted-work probe (one checkout at a time,
  each at most every 30 seconds while the window is active). A branch that has
  drifted from its upstream shows the daemon's counts as in the terminal
  client: a green `↑` for commits to push and a red `↓` for commits to pull.
  They follow the branch on two-line rows (`main ↓18`) and sit at the row's
  end on one-line rows, Compact included. Minimal rows leave them off. They
  appear only while the daemon's `[ui.sidebar.spaces]` rows name `git_status`,
  as its defaults do, because the daemon computes them only then.
- With several hosts listed, the Spaces list groups workspaces under a row per
  host, and the row of the host whose workspaces are at the top of the
  scrolled list stays pinned there, so you can always see which machine they
  are on, until the next host's row pushes it out. The pinned copy works like
  the row itself: click to select the host, use the arrow to collapse it, or
  right-click for its menu.
- Agents panel header ends with its sort, `grouped` or `priority`, which a
  click flips; an active agent view names itself there instead. Client-local
  and persisted beside the sidebar width, as in the terminal client. With
  several hosts listed, `grouped` lists each host's agents in turn, while
  `priority` orders them all together as the terminal client does: a
  disconnected host's after connected ones, then attention, then the most
  recent change on any host. While any host shows an agent view, each host
  keeps its own order.
- Resizable sidebar with width persisted per local daemon socket, shared across
  host groups. Drag the divider between Spaces and Agents up or down to resize
  their sections; double-click it to restore an even split. The split is saved
  across launches and retained while Agents is hidden.
  Local workspace titles show repository owner avatars; remote
  workspaces use the GitHub fallback mark without resolving remote paths locally.
  `show_repository_icons = false` leaves both out.
  Profile and owner avatars share a bounded public-image disk cache with 24-hour
  stale-while-refresh behavior; see [avatar caching](../../README.md#native-github-sign-in)
  for limits, location, and the startup authentication requirement. Neither cache
  reads nor downloads block rendering; sign-out discards profile refresh results.
- In-app sidebar menu for settings, keybinds, config reload, update information,
  and detach/reconnect. The standalone Settings window combines shared Herdr settings,
  daemon agent integrations, editable native fonts, and general configuration.
  A searchable installed-font picker can set all four families
  together or each independently (including Platform default), while sizes have
  −/+ controls and editable whole-number fields (8–48; Enter or leave to save,
  Escape to cancel). Size changes appear immediately; repeated clicks stay enabled
  while a background writer coalesces the latest size for each font. Save failures
  appear in the Settings footer and restore the previous size. Both families
  and sizes save to the local GUI overrides file and reload in all windows.
- A searchable theme picker previews the available names from built-ins and
  Herdr/Ghostty theme folders. Selecting a theme applies and saves it while
  preserving other GUI config settings and comments.
- Right-click spaces for Rename, Close (Close group on a repository's only
  non-linked parent while linked worktrees sharing its `worktree.key` are open;
  a parent beside another parent closes alone), and New worktree /
  Open worktree... on non-linked Git parents, including spaces with a known
  Git branch but no worktree metadata yet.
  Linked spaces offer New worktree too, while their main checkout is open: the
  daemon creates it through the main checkout, but the new branch starts from
  the linked space's branch rather than the main checkout's `HEAD`.
  Right-click also selects the space, switching the terminal and sidebar highlight
  when the daemon confirms the selection. A compact header repeats the target name
  and Git branch. Right-clicking another visible space while this menu is open
  selects it and switches the menu in one click.
  With `features.sidebar_hover_menu` enabled (**Settings > General > Open
  space menus on hover**), resting the pointer on a
  space of the selected connection opens the same menu, and moving the pointer
  anywhere but into that menu closes it again; the flag is off by default, so
  spaces normally open their menu only on right-click, and a menu opened by
  right-click stays until it is dismissed. Close requires
  confirmation and terminates terminals, not checkout files or branches. Before
  enabling Close, the dialog checks every affected local checkout for uncommitted
  files (including staged, untracked, and submodule changes) and commits absent
  from all local remote-tracking refs. It does not fetch. If either is present,
  type `close` to consent explicitly. Unverifiable status, including remote
  endpoints and missing Git metadata, also requires this consent. Cancel keeps
  the workspaces open. New
  worktree proposes the branch name the daemon would generate, previews the
  checkout path derived from it, rejects invalid Git branch names before submission,
  reports failures in the active dialog tab rather than the connection status, and selects
  and reveals the created checkout once the daemon reports it. Rename and branch
  dialogs support Unicode/IME, grapheme
  editing, Shift-arrow selection, Home/End, and Cmd-A/C/X/V. Escape/outside click
  cancels; dialog input never reaches terminals or native creation actions.
  Context menus and dialogs anchor to the pointer and clamp to the viewport.
  Rename trims surrounding whitespace and rejects blank labels inline.
  The PR and issues tabs need a GitHub account signed in to Herdr itself; until
  then they are faded, and hovering one says to sign in from the account
  button. A `gh` or browser login is not picked up by itself, and a development build
  (`just run`) keeps its sign-in apart from the installed app's Keychain entry.
  The PR tab supports fork pull requests: it fetches GitHub's PR head ref from
  the listed repository's remote and creates a local `pr/<number>` branch.
  Existing local branches are preserved; repository trust is not granted. In a
  fork checkout whose `upstream` remote is on GitHub.com, the tabs list the
  upstream repository: its own branches are fetched from `upstream`, pull
  requests from the fork behind `origin` check out their real branch from
  `origin`, and other forks use the PR head ref fetched from `upstream`.
- Open worktree... asynchronously lists the clicked parent's existing checkouts
  through `worktree.list`, including already-open and detached checkouts but
  excluding bare/prunable entries. Use Up/Down and Enter, the Open button, or
  click a row. A centered modal with the standard dimmed backdrop focuses a native
  search field, like Color Scheme. Typing immediately filters branch, label, and
  daemon path case-insensitively, including Unicode text. Selection resets to the
  first match; no matches is distinct from an empty repository listing. The
  bounded, scrollable list retains original paths for opening, and IME composition
  cannot accidentally select or dismiss it. There is no manual-path entry.
  Rows fill the list width with status labels aligned at the right edge. The
  top-right ESC control dismisses the modal; Cancel and Open remain in the footer.
  A bottom status area appears only for errors or an in-flight open, not idle hints.
  It accepts at most 512 returned entries with 8 KiB per
  string field, rejecting malformed, duplicate-path, or oversized lists rather
  than presenting a partial list. Empty results and failures are shown inline;
  dismiss and reopen to refresh. Both list and open use the clicked
  `workspace_id` and `trust_repository: false`; open sends the exact returned
  `path` and `focus: true`, never a locally resolved path or guessed branch.
  Unadvertised methods are rejected by the client worker. Correlated responses
  are fenced by endpoint selection/generation, boot, and parent workspace
  identity. While an open is pending, a branch-only parent may acquire the exact
  non-linked repository identity returned by its list response; this expected
  daemon update does not discard the correlated open result. Other identity
  changes still invalidate the picker. Success selects and reveals the daemon-returned workspace using the
  same focus flow as creation. Escape/outside click dismisses even while waiting;
  this does not cancel queued daemon work, but late replies cannot reopen the
  picker or steal this client's focus. All Git/filesystem work stays in Herdr.
- Signed-in workspace menus include a compact, divided PR summary. The number/title
  is the last selectable menu action: click it or use arrows and Enter to open the
   validated URL. Cache-only menu opening shows prefetched results immediately,
   or loading for an initial miss; no separate Open/Refresh controls or O/R shortcuts.
   One background Git/native HTTPS GraphQL worker refreshes the selected device's
   eligible workspace metadata every 90 seconds. A successful “no PR” result for
   the focused workspace (or the workspace whose menu is open) is eligible for
   another lookup after 15 seconds, so newly created PRs appear sooner. It uses
   a 128-entry LRU cache, 128 queued jobs, and
   alternating open/focused priority and round-robin scheduling. Failed refreshes
   retain successful data. Ordinary failures back off five minutes; auth/rate-limit
   errors pause the account for an hour by default, honoring numeric retry/reset
   hints within five minutes to 24 hours. Auth/endpoint generations fence late results.
   Discovery uses the daemon repository key and exact branch to resolve a unique
   Git worktree, followed by common-directory/current-branch checks and an explicit
   GitHub repository/head query. It never occupies the deletion dialog response slot.
   PR heads use the branch's configured upstream remote owner/repository and merge
   branch, so renamed local branches can identify fork PRs. Without an upstream,
   lookup uses the local branch name and requires the origin owner as before.
   Unsupported upstreams fail closed rather than matching an unrelated fork.
   A checkout with a GitHub.com `upstream` remote is treated as a fork: lookup
   queries the upstream repository, an untracked branch's head defaults to the
   fork behind `origin`, and **Create pull request** pushes to `origin` and
   opens the pull request against upstream's default branch with an
   `owner:branch` head.
  On macOS and Linux, all socket modes (including explicit/inherited sockets)
  require a same-user kernel peer (`getpeereid` on macOS, `SO_PEERCRED` on
  Linux) at the standard configured session socket. The socket must be owned by
  your user and not group/world-writable; its parent must be owned by your user
  and not world-writable. Group-writable directories such as `775`, commonly
  created by a umask of `002`, are accepted. Executable upgrades/removal do not
  invalidate this local endpoint trust. Sockets elsewhere remain blocked; a
  same-user proxy deliberately replacing the trusted socket is not detectable.
  Reconnect rechecks the endpoint. A refused endpoint hides local Git actions and
  reviews and logs `Daemon endpoint not trusted as local` with the failed check.
  A local peer or permissions failure also produces an in-app toast once the
  terminal connects, explaining why Git/PR details are unavailable and how to
  restore them. It appears once per connection even when daemon notifications
  are muted; permission failures include a quoted command on its own line for
  the affected socket or directory and a reminder to reconnect the GUI. Intentionally
  non-local sockets do not produce this toast.
  `DirectoryPermissions` means the session directory's ownership could not be
  verified or it is world-writable; check ownership, remove world-write access
  with `chmod o-w` on it, and reconnect.
  On a saved SSH device, the checkout lives on that host, so local Git cannot
  verify it. The worker instead reads the repository's `remote.upstream.url` or
  `remote.origin.url` over the same noninteractive SSH options as the bridge
  (`BatchMode=yes`, strict host keys, no master connection), keeping stdout
  bounded and discarding stderr. Each resolved origin repository is reused for
  ten minutes. Upstream configuration is read over SSH on each lookup using the
  daemon-reported local branch. Sidebar PR badges show only on the selected
  device's rows, because the cache holds that device's lookups and the same path
  and branch may exist on another host.
- Each saved SSH device can have its own GitHub account, for hosts whose
  repositories another account owns. Select the device, open the GitHub panel,
  and choose **Use another account** to run the same device sign-in for that
  device only. It is stored with the main account's mechanism (the app's Keychain,
  Secret Service, or Credential Manager entry under a per-device account name, or its own private
  `github-credentials-<device-id>` file) and renewed the same way. Pull requests on
  that device then use it; a device without one uses the main account. Signing
  out in that panel removes only the device's credential. `GH_TOKEN` /
  `GITHUB_TOKEN` apply only to the main account. Removing a device keeps its
  saved credential until you sign out of it, so re-adding the device finds it. See
  [PR lookup scope and limits](../../README.md) for authentication and remote limits.
- Additional GitHub accounts read the repositories another account owns, such as
  an Enterprise Managed User's beside a personal account. Once signed in, the
  GitHub panel offers **Add account (A)**; a strip then switches between the
  device's account and each additional one (Left/Right). Each signs in with the
  browser, or with **Use a token (T)** for a personal access token, which GitHub
  checks before it is saved: an Enterprise Managed User cannot install Herdr's
  GitHub App on its own repositories, so a token is how one reads them. A
  repository is read with the first additional account whose login, organizations
  (read when it signs in, if its token may list them), or configured owners
  include the repository's owner, and with the device's account otherwise. This
  applies to sidebar PR badges, the New worktree pull request and issue lists, PR
  checks, comments and merges, creating a pull request, and the orchestrator.
  Accounts are listed in `[github] accounts` with an `id` naming their credential
  entry (`github.com/account/<id>`, or `github-credentials-account-<id>`) and
  optional `owners`; **Remove account (D)** deletes the credential, then the entry.
   Herdr does not give endpoint clients a workspace's checkout path, so every
    daemon version uses this worktree-registry path. No Git or HTTP requests run from menu-open or render paths.
  Opening the top-right Git/PR dropdown also queues a fresh lookup for the focused
  local branch, keeping cached details visible while the background worker runs.
  The dropdown shows draft/ready-for-review status, review decisions, merge
  conflicts or blockers, and passed/failed/pending/skipped check counts. Repeated
  opens share an in-flight lookup; account authentication/rate-limit pauses still
  apply.
  PR numbers in the sidebar and titlebar share readiness colors: green for a clean
  merge, red for conflicts, failing checks, or requested changes, yellow for pending
  checks/reviews or an unknown merge status, and orange for a blocked/behind branch
  without a more specific check/review status. Drafts remain gray, merged PRs
  purple, and closed PRs red.
- The header, or the rightmost tab strip when tabs form the title bar, shows the
  selected daemon's status segments, including usage percentages supplied by
  plugins, to the left of the Git and account controls.
  Long text truncates to keep those controls reachable; status clears on disconnect.
- The top-right titlebar profile control starts native GitHub device sign-in on
  a signed-out click, shows the authenticated user's avatar, and offers Sign out
  on right-click. Signed-out workspace menus have no GitHub section or requests.
  The signed-out GitHub icon and connected avatar share a 20px size and subtle
  hover glow; authentication errors appear in the account panel, not a red border.
  It uses Herdr GPUI's public client ID `Iv23liurUcwxPjrdIFYT`, overridden by
  `[github].oauth_client_id`, then `HERDR_GITHUB_OAUTH_CLIENT_ID`. No client secret
  or private key is needed or shipped.
  A GitHub App sign-in reads only the repositories the App is installed on,
  with the permissions each owner approved. After the profile loads, and at
  every five-minute session check, the app compares the App's requested
  permissions (`GET /apps/<slug>`) with each installation the account can see
  (`GET /user/installations`). If no installation exists, or one still has
  fewer permissions than the App now requests, the avatar shows a red dot, a
  toast says so once per window, and the account panel links each owner's
  approval page (or the install page, for Herdr GPUI's own App). With the
  panel open, the check runs again within 30 seconds. Tokens from the GitHub
  CLI, `GH_TOKEN`, an OAuth App, or a personal access token are not checked.
  The compact native macOS titlebar design
  is integrated from main commit `3909f21`, without unrelated tab changes.
  Signed macOS release builds keep tokens in this app's Keychain entry; unsigned
  development and worktree builds use the private file store instead, so a new
  code identity per rebuild cannot trigger a Keychain prompt on every launch.
  Linux builds keep tokens in the desktop keyring through the freedesktop Secret
  Service (GNOME Keyring, KWallet, KeePassXC), under service
  `dev.herdr.gpui.github`; the desktop may ask to unlock it. With no Secret
  Service running, restore finds nothing saved and saving a sign-in says so.
  Windows builds keep tokens in Windows Credential Manager as generic
  credentials named `dev.herdr.gpui.github/github.com` (and
  `dev.herdr.gpui.github/github.com/host/<device-id>` per device), persisted for
  your Windows account on this computer only, never roamed; see
  [Windows](#windows).
  `GH_TOKEN` / `GITHUB_TOKEN` override any of them. Access tokens and retained device/user codes use redacted, zeroizing
  `secrecy` types; HTTP headers are sensitive and application-owned raw OAuth
  buffers are wiped. The user code is intentionally exposed for rendering.
  Library/OS/rendering copies are not guaranteed to be erased. Linux supports an
  explicit `allow_plaintext_credentials = true` opt-in with a prominent warning,
  separate private credential file and atomic no-follow Unix writes, which
  replaces the Secret Service for desktops without one; a token saved in one store
  is not moved to the other, so changing the opt-in means signing in again; macOS
  development builds use that same store, enabled by default and warned about in
  the profile panel. Signed macOS release builds still use Keychain. Windows has
  no private file store, so the opt-in does not select the file there and
  Credential Manager stays in use. Sign-out suppresses environment tokens for this app session and
  fences late profile/avatar/PR results. Plaintext policy reloads re-evaluate the
  active credential, without reactivating an explicitly signed-out session.
  Disabling plaintext stops its session use but keeps the file; explicit sign-out
  still removes the saved file regardless of opt-in, or reports a safe error.
   Device-flow refresh tokens are saved alongside access tokens in the same
   store, including their access-token expiry when GitHub supplies it. Connected
   sessions are checked in the background every five minutes and renewed within
   ten minutes of expiry, without restarting the app. Older saved pairs without
   expiry metadata renew when GitHub rejects the access token. The rotated pair
   is saved before loading the profile again. Temporary network and keyring
   failures keep an active session visible and retry on the next check. They do
   not delete credentials; environment tokens are never renewed or replaced by
   saved credentials. Successful sign-in closes the GitHub panel and returns
   focus to the terminal. Reopen the account panel to use the red Sign out action.
   Older versions saved only access tokens, so an expired legacy token needs
   one more sign-in to obtain a refresh token. Revoked or expired refresh tokens
   also require sign-in. See
  [setup, cancellation, scopes, and sign-out](../../README.md#native-github-sign-in).
- When the GitHub CLI is signed in to github.com, the main account's GitHub
  panel offers **Use GitHub CLI authentication** beside the device sign-in. The
  panel asks `gh auth token --hostname github.com` on a background thread, at
  most every 30 seconds while it is open and signed out, and drops the answer.
  Choosing it reads the token again, verifies it with GitHub, and saves only a
  marker in the credential store, never the token. Every load and five-minute session check
  asks `gh` again, so `gh auth logout`, a revoked token, or an uninstalled `gh`
  signs the app out at the next check with an error naming the fix, and a token
  `gh` refreshed is picked up. The panel reads "Connected with GitHub CLI", warns
  when the token's scopes leave out `repo` (`gh auth refresh --scopes repo`),
  and **Stop using GitHub CLI** removes the marker without signing `gh` out.
  `gh` runs under the PR lookup process policy: login-shell `PATH`, a 10-second
  deadline, capped output, no prompt, and `GH_TOKEN` / `GITHUB_TOKEN` removed,
  which still take precedence over it in the app. Saved SSH devices do not offer
  it: this computer's CLI is the main account's, and a device without its own
  sign-in already uses that.
- Workspace actions retain the clicked ID and boot, revalidate before queueing,
  and reject changed close-group membership. Reconnect clears dialogs. Queue
  errors remain in the dialog; daemon errors appear in the connection status bar.
  Queue acceptance dismisses the dialog, not an optimistic state mutation.
- Linked spaces offer Delete worktree checkout with a daemon-resolved path and a
  single confirmation, matching the Herdr TUI. Unlike other workspace dialogs,
   deletion stays open until the correlated daemon result arrives, with submission
   disabled while waiting. Dirty/untracked refusals show the error and **Force remove**
   in the same dialog; force requires a new confirmation. Dismissing while waiting
   does not cancel the request, and its reply never reopens the dialog. All Git/filesystem work
  stays in Herdr. Unpushed commits are not checked by this API. See
  [WORKTREE-DELETION.md](WORKTREE-DELETION.md) for safety limits and sources.
- Worktree creation sends the clicked `workspace_id`, optional `branch`,
  `base: "HEAD"`, `focus: true`, and `trust_repository: false`. Blank branches use
  daemon policy. The daemon's deferred endpoint navigation focuses the result;
  no follow-up focus request or local Git subprocess is used.
- Title-only tabs, without an added tab number. Externally created workspaces
  arrive through pushed snapshots without manual refresh.
- Right-click any tab without focusing it to open Rename.
  Actions retain the clicked tab/workspace and reject stale connections or targets.
  Rename selects the current label in a native IME-aware field, with inline errors;
  Close uses the existing cancel-by-default confirmation when an agent in the
  tab is working or blocked, unless `confirm_close_tab = false`. Escape or an outside left/right click dismisses
  the menu without sending terminal input.
- Click workspace, tab, agent, or a visible split pane to focus through the API.
- Right-click a visible pane, including an inactive split, for Rename, Split
  Right, Split Down, Toggle Zoom, right-click routing, and Close without first
  focusing it, and, on another pane of the focused tab, Swap with Focused
  Pane, which moves the focused pane to the clicked one's place and keeps it
  focused. Actions
  retain the clicked pane/tab/workspace and daemon boot, and reject stale
  membership or a changed connection. Rename uses an IME-aware native field,
  trims surrounding whitespace, and clears the custom label when blank. It
  waits for the matching daemon response and reports failures inline. Close
  always asks for confirmation with Cancel selected. Popups and stale retained
  terminal frames block pane context actions. Escape or an outside left/right
  click dismisses the menu without forwarding input to the terminal.
  The pane menu also moves the pane; see [Moving Panes](#moving-panes).
- Native File/Edit/Terminal menus and creation buttons: **+ New Workspace** in the
  sidebar and a persistent 18px SVG **+** in a 44px-wide button beside the horizontally
  scrolling tab strip. Each tab has a 16px SVG close cross in a 24px hit target;
  it uses the same configurable confirmation without focusing an inactive tab.
  Both icons use the current theme's foreground tint.
- Cmd-T creates and focuses a tab; Cmd-Shift-N creates and focuses
  a workspace. Cmd-N opens the New worktree dialog for the focused workspace
  (for a linked worktree, through its repository's main checkout, starting from
  the linked worktree's branch). The dialog opens on
  its Branch field with the proposal selected, so typing replaces it; Tab
  back to the form returns there. The Name field is optional: left empty,
  the daemon picks the workspace name; anything
  typed is sent as the new workspace's label. The proposed branch follows the
  name, so "Fix login" proposes `worktree/fix-login`, until the Branch field
  is edited; a name with no ASCII letters or digits keeps the random proposal.
  As with a typed branch, one that already exists is checked out as it is, at
  its own commit, such as a branch left behind by a removed worktree of the
  same name, and one still checked out elsewhere is refused with Git's
  message. When the dialog cannot open,
  because the workspace is not a Git repository, the main checkout is not open,
  a linked worktree has a detached `HEAD`, nothing is focused, or the window is disconnected, a two-second flash in the
  clipboard toast's position says why.
  Cmd-D splits the focused pane vertically (new pane on the right);
  Cmd-Shift-D splits horizontally (new pane below). Cmd-Shift-] / Cmd-Shift-[
  cycles next/previous tab within the current workspace, wrapping at the ends.
  These shortcuts are native actions, not bytes sent to a terminal.
- Cmd-Alt-N runs **Open Notification Target**, also available in Terminal and the
  command palette. It uses the visible card's safe click path; stale, targetless,
  queued, or menu-hidden cards do not navigate or change endpoint selection.
- Cmd-1 through Cmd-9 focuses the tab at that position in the current
  workspace. Cmd-Alt-Left/Right/Up/Down focuses a pane in that direction;
  Cmd-Alt-] / Cmd-Alt-[ cycles next/previous pane within the current tab.
  Cmd-Shift-Enter toggles focused pane zoom. Cmd-K clears the focused pane's
  screen and scrollback through the daemon's `pane.clear`, without sending input
  to the running program; daemons that do not advertise it (Herdr 0.9.1 and
  older) leave it out of the palette and report why instead.
- Cmd-F (`find`, also in Terminal and the command palette) opens a find bar
  over the focused pane that searches its whole scrollback through the
  daemon's `pane.copy_search`. Matches are tinted, the current one more
  strongly, and shown as "3 of 17". Enter or Up moves to the next older match
  and Shift-Enter or Down to the next newer one (Cmd-G and Cmd-Shift-G work
  too, also once the terminal has the keyboard back; with no bar open, Cmd-G
  opens it), scrolling the pane to it. In a review, Cmd-G and Cmd-Shift-G
  step through its matches.
  Escape closes the bar. Lowercase queries ignore case; any uppercase letter
  makes the search case-sensitive, as Herdr's copy mode does. Text typed in
  the bar, IME composition included, never reaches the terminal. Daemons that
  do not advertise the method report why instead of opening the bar.
- Cmd-Shift-C (`copy_mode`) puts the focused pane in keyboard copy mode, as
  Herdr's `prefix+[` does in the TUI: a block cursor starts at the terminal
  cursor and walks the whole scrollback, and nothing typed reaches the
  program. `h` `j` `k` `l` and the arrows step, `0`/Home goes to the line
  start, `g`/`G` to the top of history or the last row, and Ctrl-U/D/B/F or
  Page Up/Down page. The text-aware motions `w` `b` `e` `W` `B` `E` `$` `^`
  `{` `}` come from the daemon's `pane.copy_motion`, so words and paragraphs
  mean what they mean in Herdr. `/` searches toward newer output and `?`
  toward older, through the daemon's `pane.copy_search`: type the query in the
  prompt that opens over the pane's corner and press Enter, or Escape to close
  the prompt and stay in copy mode. The query is literal; lowercase ignores
  case and any uppercase letter makes it case-sensitive, as in Herdr. The
  cursor moves to the match, matches are tinted, and the corner badge shows
  the query and "2 of 5". `n` repeats the search the way it was entered and
  `N` the opposite way. `v` or Space marks by cell and `V` by line, from
  wherever the cursor is, a match included. `y` or Enter copies the selection,
  or with nothing marked the current match, through `pane.selection.read`.
  If the pane has changed since the search, the search runs again first, so
  the copied match is one the pane still shows; if output changes again
  before it is read, the copy fails with a message rather than copy other
  text. Escape clears a selection and the search, or leaves when there is
  neither, as does `q`. While a motion or search is still waiting for the
  daemon, Escape drops it and the keys typed after it, and `q` leaves at
  once. Leaving scrolls the pane back to where it was.
- Cmd-Shift-A (`annotate_selection`, also in the command palette) writes a
  note on the text selected in a pane for an agent to act on, the way
  Annotate does for a page. A field opens over the pane: Enter adds the note
  and sends every queued one, Shift-Enter only queues it (up to 20), and
  Escape drops it. Enter on an empty field sends the queue, including when
  all 20 slots are full. Notes go to the agent running in that pane, else to the
  first agent in its workspace, as one prompt per agent: where the text was
  (workspace and tab), the text fenced and marked as quoted data with its
  control characters removed, and your note. They are delivered as page
  notes are (see [Annotating A Page](#annotating-a-page)), and copied when no
  agent is found. Queued notes remember their original device and daemon;
  if that daemon is no longer selected when you send or while delivery waits
  for a busy agent, those notes are copied instead. The whole selection must
  be on screen. Clipboard fallbacks from one Send are combined, including
  batches that fail later, so one agent's notes cannot replace another's.
- A mouse selection dragged past a pane's top or bottom edge scrolls the
  pane, and the selection stays with its text as the pane moves. A selection
  that reaches rows off the screen is copied through `pane.selection.read`;
  one that fits the screen is still copied from the painted cells.
- **Open Scrollback in Editor** (`edit_scrollback`, in the pane menu,
  Terminal menu, and palette) asks Herdr to open the pane's history in the
  configured editor, through `pane.edit_scrollback`. It is offered only by
  daemons that advertise the method.
- Cmd-W closes the focused pane and Cmd-Shift-W closes the focused tab only after
  a confirmation dialog (a tab asks only while an agent in it is working or
  blocked, and never with `confirm_close_tab = false`; a pane never asks with
  `confirm_close_pane = false`). **Cancel is selected by default**: Enter alone cancels;
  Tab then Enter selects and confirms Close. In the pane dialog, Space toggles
  **Do not ask again**. Closing can terminate running
  processes, unlike quitting the GUI, which only detaches.
- Double-tap Shift or press Cmd-Shift-P to open the unified **Command Palette**:
  workspaces on connected hosts, agents and terminal panes, native GUI actions,
  configured daemon commands, and local project folders. Cmd-P opens the same
  palette on **Navigation**. Choose **All**, **Navigation**, **Commands**, or
  **Projects**, or cycle filters with Ctrl-Tab / Ctrl-Shift-Tab (or Alt-] /
  Alt-[) without clearing the search. Search is fzf's: every space-separated
  term must match, and `'exact`, `^prefix`, `suffix$`, and `!negate` apply per
  term; host, workspace, path, status, and command ID are searchable context.
  Rows chosen often and recently rise among the matches for a typed query, and
  the Projects filter lists the most frecent projects first before anything is
  typed; that history stays on this machine in `palette-frecency.json`.
  **Resume Palette** (Cmd-Shift-R, Ctrl+Alt+Shift+R on Linux and Windows) reopens the
  palette with its last query, filters, and selected row. Navigation
  (Go To) also filters agents by the status the daemon reports, like Herdr's
  navigator: click **All agents**, **Blocked**, **Working**, **Idle**, or
  **Done**, or press Alt with Herdr's letters (`alt-b`, `alt-w`, `alt-i`,
  `alt-d`, `alt-a` for all) from any filter, which switches to Navigation.
  Plain letters stay search text, the search still narrows within a status,
  and while a status is chosen each workspace (and tab) is listed only as the
  heading of its matching agents, ranked with the best of them. Each new
  palette starts on **All agents**. Terminals without an agent appear only
  under **All agents**, and the other filters ignore the status. A keystroke
  already bound by `[keybindings]`, the daemon's `[keys]`, or
  `navigate_workspace_up`/`_down` stays with that binding. Up / Down, Tab /
  Shift-Tab, Ctrl-N / Ctrl-P, and Ctrl-J / Ctrl-K select, Enter or a click
  activates, and Escape or an outside click dismisses without sending terminal
  input. Choosing a destination on another host switches to it. When the window
  has room, Navigation shows a preview beside its list (Alt-P toggles it): the
  selected pane's screen when its tab is the one its host is streaming, painted
  from cells this client already holds; else, for a tab shown earlier, its last
  frame from before you moved on, labelled "Last seen … ago"; or else its
  directory, status, and place. Remembered frames are bounded by memory, 4 MiB
  per host and 8 MiB in all (roughly 15–30 tabs at common window sizes), and
  forgotten when their daemon restarts. The preview never asks the daemon for anything.
  Double-Shift requires two short completed taps within 400 ms; shifted typing,
  mouse interaction, held Shift, other modifiers, composition, and modal dialogs
  do not trigger it. It is window-local, not a system-wide hotkey. Native browser
  content may handle modifier events itself; use the native menu when needed.
- Configure project discovery in `config-gpui.local.toml`:

  ```toml
  [palette]
  double_shift = true # false disables only this gesture
  project_roots = ["~/Code", "$HOME/Projects"]
  ```

  The palette lists immediate non-hidden folders, not just Git repositories.
  A leading `~`, `$VAR`, and `${VAR}` expand without shell execution. Unset
  variables, missing roots, or discovery limits show diagnostics while other
  sources stay usable. Scans run in the background, visit at most 8192 directory
  entries, and list at most 2048 projects across up to 16 roots. Roots may be
  symlinks, but child symlinks are skipped; duplicate paths are listed once.
  Selecting a folder focuses a workspace whose first surviving pane's launch directory
  exactly matches it, or creates one with that directory and its basename label.
  Foreground process directories and later splits do not claim a project while
  that first pane remains. The snapshot does not identify an original root pane;
  after it is closed, the first surviving pane supplies this directory.
  Local folders always open on the local connection, even while viewing SSH;
  an unavailable local connection produces an error, never a remote creation.
  The palette waits for the daemon's creation response before following the new
  workspace and does not automatically trust repositories. Dismissing a queued
  creation does not undo it. The GUI owns these settings independently of
  `herdr-utils`; use the same root paths in both configs if desired.
- Every native shortcut can be rebound in `config-gpui.local.toml` under
  `[keybindings]`, keyed by command name (`new_tab`, `new_workspace`,
  `split_right`, `focus_tab_1`, `quit`, ...). A value is one keystroke or a list;
  an empty string or list unbinds the command. A keystroke assigned there moves
  away from its default command, keystrokes need a cmd, ctrl, alt, or fn
  modifier (or a function key F1–F24), and unknown names, unparseable keys, or one key on two configured
  commands reject the config. Saved changes rebind the keymap and menu bar live.
  On macOS, Cmd-H, Cmd-Option-H, and Cmd-M are Hide, Hide Others, and
  Minimize, which are menu items rather than commands. A command bound to one
  of those keystrokes here or in the daemon's `[keys]` runs instead, and the
  menu item stays without a shortcut. A `[[keys.command]]` shortcut cannot
  take them.
- `[pane_keys]` maps a keystroke to the key the focused pane receives instead,
  like Ghostty's `text:` binds. On macOS, Cmd-Left, Cmd-Right, and
  Cmd-Backspace send Ctrl-A, Ctrl-E, and Ctrl-U by default, so zsh and agent
  prompts jump to the line's ends or delete back to its start as in every
  other Mac terminal, and Option-Left and Option-Right send Esc-b and Esc-f
  to move by word. On every platform Shift-Enter sends Alt-Enter (Esc CR)
  by default: Herdr sends a program that never enabled the kitty keyboard
  protocol or modifyOtherKeys a bare CR for Shift-Enter, which submits the
  line, while prompt_toolkit apps and zsh read Esc CR as a newline.
  `"shift-enter" = ""` sends Shift-Enter unchanged again. A value is a key
  a terminal can receive (`ctrl-a`, `home`, `alt-b`, `shift-enter`), or `text:` followed by up to 1024 bytes of
  text, and an empty string removes a default. Text reaches the pane byte for
  byte, so a newline in it is a line feed, not the Enter key. On a layout
  that keeps symbols on Option, `option_as_alt = "left"` or `"right"` leaves
  one Option key typing them with no entries here; `text:` is for
  `option_as_alt = true`, where both send Alt: `"alt-7" = "text:|"` on a
  Norwegian keyboard. Name the key as the layout has it without Option, so
  Option-Shift-7 there is `"alt-/"`. A pane key takes its keystroke from a
  default or daemon command, so `"cmd-k" = "ctrl-l"` replaces Clear; listing
  it under `[keybindings]` as well rejects the config, as do a bare character,
  an unknown key, a target with cmd, or an empty `text:`. The find field and
  dialogs answer Cmd-Left, Cmd-Right, Cmd-Backspace, and Cmd-Delete
  themselves.
- The daemon's own `[keys]` table in `config.toml` (resolved like
  `[ui.toast.clipboard]` above) applies in the GUI too, Herdr's defaults
  included, so a TUI habit such as `prefix+v` or `alt+1..9` works in both
  clients. The prefix (`ctrl+b` unless `prefix` says otherwise) arms the
  window, shown by a keycap in the status bar; the next keystroke runs its
  chord or, when nothing is bound to it, is dropped, as in the TUI. Typing the
  prefix twice sends it to the terminal, and Escape cancels. Chords work from
  a menu's or dialog's text field too: the chord closes it and runs. A prefix
  that types text, such as `@` or `-`, works the same way everywhere, so a
  text field gets it by typing it twice. Every
  daemon action has a GUI command except the `navigate_pane_*` keys (Go To
  has no pane cursor). `open_worktree` and `remove_worktree` act on the
  focused workspace, as in the TUI: the first opens its Open worktree...
  dialog (a linked checkout's through its repository's main checkout), the
  second its Delete worktree checkout confirmation, only for a linked
  worktree. When neither applies (a main checkout, a workspace outside Git,
  nothing focused, or no connection) nothing opens and a flash says why.
  Herdr leaves both unbound, and so do the GUI's `open_worktree` and
  `remove_worktree` commands, which `[keybindings]` and the palette offer
  too. `workspace_picker` and
  `goto` open Go To, where `navigate_workspace_up`/`_down` move the selection
  when the key cannot be typed into its search. `split_vertical` and
  `split_horizontal` are Split Right and Split Down. As in the TUI,
  `previous_workspace`/`next_workspace` and `previous_agent`/`next_agent` step
  through the sidebar's rows on every listed host and wrap around,
  `switch_workspace` counts the selected host's rows, `focus_agent` counts the
  agent panel's, `last_pane` returns to the pane focused before this one,
  `move_tab_previous`/`_next` wrap a tab at either end to the other, and
  `[keys.indexed]` combos bind the digits 1-9. On macOS, the prefix, its
  chords, and custom command shortcuts recognize a shifted key such as
  Shift+1 by the active keyboard layout, so
  `switch_workspace = "prefix+shift+1..9"` works on any layout that types
  the digits unshifted. A chord or custom command bound to the symbol the
  key types, such as `prefix+?`, takes precedence. A shortcut without the
  prefix in `[keybindings]` or the daemon's `[keys]` does not get this and
  needs the symbol itself, such as `ctrl+!`; such a shortcut, or a
  `[pane_keys]` entry on the symbol, keeps its key even when a custom
  command is bound to the shifted key. `resize_mode` (`prefix+r`)
  makes h, j, k, l or the arrows resize the focused pane until Escape, Enter,
  or the shortcut again; the status bar shows it. Renames and
  `close_workspace` open the same dialogs as their menus, and `reload_config`
  reloads both the daemon's config and this one. The daemon's
  `[[keys.command]]` shortcuts, prefix chords or direct keys, run their
  command through `command.invoke` on the focused pane, and the palette lists
  them with these keys; like Herdr, a keystroke one of its actions already
  holds keeps that action, as does one listed in `[keybindings]`. A GUI
  catalog default is no Herdr action, so it yields: a custom command on
  `ctrl+shift+t` runs instead of New Tab's Linux default, without unbinding
  `new_tab`. Daemon keys add to the catalog
  defaults and take a keystroke from its default command. A command named in
  `[keybindings]` keeps exactly the keystrokes listed there, daemon chords
  included, and a keystroke listed there outranks the daemon's, even the
  prefix. Herdr validates its own file, so a daemon entry the GUI cannot
  express (a `hyper` modifier, a direct key without cmd, ctrl, alt, or fn) is
  skipped rather than rejected. Saving either file rebinds live.
- Saved SSH devices use these local keybindings too, as `herdr --remote` does
  by default. **Use server keybindings** in a device's right-click menu opts
  that device into the `[keys]` profile its server publishes, like
  `herdr --remote-keybindings server`: its prefix, actions, and indexed keys
  replace the local daemon `[keys]` while that device is selected, and moving
  to another host switches back. The GUI's own `[keybindings]` still apply on
  top. The choice is saved per device in `config-gpui.local.toml` as
  `[devices.<id>] keybindings = "server"`. A server that publishes no profile,
  or one that cannot be read, leaves the device on local keys, and the menu
  says why. Only keybindings follow the server; themes, sidebar, and toasts
  stay local, and no remote config file is read.
- Cmd-B toggles sidebar visibility locally without changing daemon state.
  Cmd-, opens Settings; Cmd-/ opens the grouped native shortcut reference.
  Native shortcut labels and keycaps come from the shared `controls::COMMANDS`
  catalog, overridden by the config's `[keybindings]` table, with Cmd-V semantic
  paste shown separately. **Plugin & custom commands** lists the connected
  daemon's `[[keys.command]]` entries, including plugin actions, with their
  effective prefix and key combinations. A binding that cannot run says why:
  a Herdr or GUI shortcut (Cmd-V paste and the Edit menu included) holds it,
  an earlier command holds it, it lacks a modifier, no usable prefix exists,
  it is past the eight-shortcut limit, or this client cannot type it (such as
  Herdr's `hyper`). A command named like a GUI action or an earlier command
  is numbered, such as "Audit (2)". Commands without bindings show **No shortcut assigned**,
  and none run or appear while disconnected. Search filters by
  action, section, or key combination. Preferences, keybinds, theme/palette pickers, and
  close confirmations use themed centered modals and configured UI fonts;
  modal input does not reach the terminal.
- Ordinary creation shortcuts omit `cwd`, labels, environment overrides, and split ratio: the
  daemon applies its existing defaults and directory policy. Workspace creation
  supplies the currently focused source workspace when available; tabs and splits
  target the current workspace/pane explicitly. An empty session can create a
  workspace without guessing a local path. Nothing is created while disconnected.
- Mouse-wheel/trackpad scrolling targets the pane under the pointer (inside its
  content, not borders). Fractional pixel motion accumulates into terminal
  lines, and horizontal motion into columns, with bounded per-event work (at
  most 128 lines per event). Each discrete wheel notch scrolls Herdr's shared
  `[ui] mouse_scroll_lines` (default 3; zero or an unreadable value keeps 3),
  applied live when the shared config reloads, whether the pane slides through
  scrollback or moves by whole rows. As in Herdr's TUI, the count moves only
  scrollback: an application reading the wheel gets one wheel report per
  event from the daemon. GPUI reports notches already
  scaled by the platform: three lines on Linux and FreeBSD, the system wheel
  setting on Windows (assumed to be its default of three, so a changed Windows
  setting scales the result proportionally), and about one, with
  acceleration, on macOS. Precise trackpad motion keeps following the pointer
  one cell height per line and ignores the setting, as does horizontal motion.
  Horizontal scrolling reaches only a pane or popup whose application reports
  the mouse, since Herdr has nothing else to do with it; sideways drift during a
  mostly vertical swipe is ignored. Shift+wheel arrives already turned
  horizontal by the platform (macOS, X11, Wayland), so it is sent as horizontal
  motion with Shift held, not swapped back. Popups capture wheel input only
  within their displayed bounds; input never falls through to a covered pane.
- The daemon shows whole rows, at most one surface per render interval (16 ms),
  so wheel scrollback in a pane draws where the OS's own deltas, momentum
  included, put the content. Each delta slides in over 48 ms. The wheel asks
  the daemon for each row as the motion enters it, and the rows are drawn from
  the presented surface and the earlier ones that showed them. When the
  gesture stops part-way into a row, the pane rests there; clicks, selection,
  and links target the cells where they are drawn. Applications reading the
  wheel, a dragged scrollbar thumb, a frame placing images, an open popup,
  keyboard scrolling, and any change to the pane's rows besides the scroll move
  the content to the daemon's whole row. Only the moving pane repaints; other
  panes replay their cached paint. Set top-level `smooth_scroll = false`, or
  turn off **Smooth scrolling** in Settings, to scroll by whole rows.
- Direct semantic cell canvas: named ANSI colors, indexed 256-color palette,
  RGB, reset foreground/background, reverse, dim, hidden, bold, italic,
  underline, strikeout, wide-cell skip handling, and cursor shapes.
- Server popup text surfaces centered above the main surface, with popup input
  routing while one is active.
- Native committed text through `EntityInputHandler`, including Unicode and
  composition. In-progress marked text is drawn inline at the input cursor,
  and the input method's caret or converted clause anchors its candidate
  window. Moving to another pane or running a command drops a composition in
  progress, in the input method too, so it never carries into the new pane.
- Enter, Tab/BackTab, Escape, Backspace, arrows, navigation/editing keys,
  F1-F24, Control characters and modifiers on special keys. Option-printable
  input follows the macOS keyboard layout, including dead keys. With the
  default `option_as_alt = "auto"` on the U.S. and ABC layouts, the left
  Option sends Alt shortcuts and the right Option types accents and dead keys
  (right Option-E, then E, types é); `"left"` and `"right"` pick a side on
  any layout. Option-Left and Option-Right move a word back and forward
  (Esc-b and Esc-f), Cmd-Left and Cmd-Right to the line's start and end.
  Windows reports AltGr as Ctrl+Alt. When AltGr makes a key type a
  character, as AltGr-W types `?` on a Brazilian ABNT2 layout, the pane gets
  that character; a Ctrl+Alt key that types nothing still reaches it as a
  chord.
- Ctrl-Cmd-F toggles full screen and Cmd-` brings the app's next window
  forward, as in other Mac apps, from the Settings and Log windows too. The
  cycle visits every window in a stable order, not front to back.
- Pointer selection of terminal cells, copied to the clipboard on release with
  a configurable flash. Selections stay within one pane or the popup above it,
  anchor on half cells, and never reach the daemon.
- Cmd-V sends semantic Paste; Cmd-Q or window close detaches without killing
  the daemon or its terminals. Window activation is reported to the daemon.
- Resize uses the actual terminal canvas bounds and measured configured font cell width,
  excluding the native sidebar, tabs and status bar.
- Semantic daemon notifications appear as nonmodal, host-labeled in-app toasts,
  when enabled, including from background endpoints, without changing focus or
  selection. A window-wide scheduler orders arrivals across coalesced host inboxes,
  keeping one visible card, at most eight queued cards, and at most eight delayed
  pending events. Each connection's ingress mailbox is separately bounded at eight.
  Presentation-queue overflow drops the oldest waiting entries, not the visible
  card. Ingress overflow conservatively retires all older cards for that endpoint
  before delivering the surviving batch: a lost event may have invalidated a pane's
  prior notification. This uses one loss flag, not an unbounded invalidation ledger.
  A new event for
  the same endpoint and pane replaces any pending, queued, or visible predecessor;
  targetless events are not coalesced. Titles/bodies are inert plain text, stripped
  of controls and bidi overrides and capped at 160/512 input characters.
  Lifetimes begin at promotion: NeedsAttention 8 seconds, Finished 5,
  UpdateInstalled 3, and Custom 5. The close button dismisses independently.
  Disconnect, detach, replacement, and boot changes clear that endpoint's cards.
  Finished always requires projected Done evidence, even at zero delay and never
  without a pane. Working or missing evidence may wait until one second after
  arrival, with 50ms rechecks on the UI poll loop; other states reject immediately.
  Delayed NeedsAttention requires Blocked evidence. Grace is measured from arrival,
  not added to the configured delay. The active endpoint's focused tab (or workspace
  when no tab is specified) suppresses normal in-app delivery, regardless of outer
  window focus; an identically focused background endpoint is not suppressed.
  Requested corners are honored at wide sizes; below 720px all cards use bottom
  right. Windows under 180px in either dimension hide cards. Menus and undersized
  windows pause visible expiry and prevent queued promotion so cards receive a
  visible lifetime after the obstruction closes.
  Long text is clipped to keep cards bounded. Toast presentation and previews add
  no extra sounds; semantic notification audio follows the independent
  [sound policy](#notification-sounds). No OS notifications or terminal escapes
  are performed. Clicking a targeted toast activates its
  originating endpoint and focuses its pane, tab, or workspace through the API.
  Targets are checked against the current snapshot, original boot, and connection;
  deleted or reparented targets fail closed, without falling back to another space.
  A target arriving before its snapshot can initialize during the first second,
  within the same connection and known boot; after that it remains inert. Inferred
  parents are frozen on initialization, so later snapshots cannot retarget a click.
  Navigation waits for endpoint activation and dismisses only after the focus
  request is queued (not daemon acknowledgement). An accepted click pauses that
  card's expiry while navigation is pending; dismissal, same-pane replacement,
  ingress loss, removed membership, and connection/boot changes still invalidate it.
  Targetless Custom toasts stay
  inert, and the close button only dismisses.
  A busy connection inbox defers validation without blocking the UI. A contended
  source inbox also defers focus release; the destination cannot activate before
  the source release is acknowledged or its transport has drained. Returning to
  Local remains an escape hatch: an unsent remote release retires that transport
  without waiting. Pending
  toast navigation keeps terminal input fenced until validation completes.

Socket I/O belongs to `herdr-client`'s worker. A separate event thread drains all
ordered events into a bounded latest-state cache. The UI samples changed state
at most once per 16 ms without blocking. A metadata-only snapshot is staged
while the displayed snapshot and surface stay coherent, so input keeps flowing;
a boot, focus, pane-layout, zoom, keybinding, or command change invalidates the
surface, and input waits for a coherent one. Notices, sounds, and agent
checkpoints judge agent status from the staged metadata, so a status change
that arrives before its surface still counts. Reconnect replaces the cache, so
late events from an old connection cannot affect the UI.

### Scrolling Semantics

Upstream `PaneScrollParams` is exactly `{ "pane_id": string,
"offset_from_bottom": u64 }`, an absolute scrollback position, not a wheel delta.
Like the upstream TUI's normal wheel handling, this GUI instead sends semantic
`ClientPaneInputEvent::Mouse` (`ScrollUp`/`ScrollDown`, pane-relative position,
modifiers, and line count). The daemon's `apply_scroll` chooses host scrollback,
alternate-screen behavior, or application mouse reporting using the current
terminal mode. `ScrollLeft`/`ScrollRight` carry a column count but are only
ever encoded as one wheel report for a mouse-reporting application, so the GUI
sends them only to such a target. This avoids racing absolute `pane.scroll` offsets against incoming
frames and avoids duplicating terminal-mode policy in the GUI. Scrolling does not
change keyboard focus to the hovered pane. The existing client advertises no pixel
mouse capability, so the daemon uses the supplied cell-coordinate fallback.

Reference sources (read-only): Herdr's `src/api/schema/{workspaces,tabs,panes}.rs`,
`src/app/api/workspaces.rs`, `src/client/shell/mouse.rs`, and
`src/server/pane_input.rs`; Arbor's `crates/arbor-gui/src/app_bootstrap.rs` for
GPUI native action/menu/keybinding patterns.

## Deliberate Limitations

- macOS defaults to Menlo and the system font; Windows defaults to Cascadia Mono
  (Consolas when missing) and the system font; Linux defaults to DejaVu Sans Mono
  and DejaVu Sans, or, when DejaVu is not installed, the first installed common
  monospace and sans family (JetBrains Mono, Noto, Liberation, Ubuntu, any other
  `Mono` family). No bundled Nerd Font.
  Private-use icons may be missing. Fonts and palettes are configured locally,
  not synchronized from the host terminal's theme.
- No draggable scrollback UI, split dragging,
  image rendering, or animated blinking.
- No server-owned keybindings, session picker, saved-host editing, or daemon
  stop/upgrade management.
- IME uses a minimal transient buffer, not a local editable terminal document;
  the composition is drawn inline at the cursor with one underline, not a
  thicker one under the clause being converted. Key releases are
  reported only while Herdr says the focused pane asks for every key (kitty
  report-all), and only for keys sent as key events: typed text still arrives
  as text rather than escape codes, so IME and dead keys keep working.
  Physical-key metadata is not reported.
- Popups have a basic centered text presentation, without native title/border
  chrome. Only semantic notifications get in-app toasts; legacy notification
  commands and server clipboard writes are not executed. There is no notification
  history.
- Rendering is a simple two-pass cell painter, not an optimized damaged-row
  renderer. Large/high-frequency surfaces can consume significant CPU.

## Windows

CI checks formatting, lints every target and feature,
and runs workspace tests with default and all features on `windows-2025` (x86_64)
and `windows-11-arm` (ARM64), including headless UI and CLI tests. Its
`windows-native` job then runs the native tests in both runners' desktop
sessions: `just test-sidebar`'s fixtures, and on x86_64 `just test-gui` against
a pinned Herdr `herdr.exe` (0.9.5) in the shared test sandbox, with PowerShell
as the pane shell. Herdr publishes no ARM64 Windows build; emulated on
`windows-11-arm`, its panes echo typed text but never act on Enter, so CI skips
`native_gui_live` there. Both runners then run the release-mode performance
budget, whose hover and scroll arrive as Win32 messages sent to the fixture's own
window. The paint probes accept the sub-pixel offsets layout snapping gives at
fractional display scales such as 150%. The release workflow builds and
CLI-tests the optimized executable natively for each architecture, publishes
`Herdr-VERSION-x86_64-pc-windows-msvc.zip` and
`Herdr-VERSION-aarch64-pc-windows-msvc.zip`, and installs and uninstalls both
signed installers. Each zip also holds
`herdr-gpui-launcher.exe` (`crates/herdr-gpui-launcher`), a windowed program
that starts the console-subsystem `herdr-gpui.exe` from its own directory with
`CREATE_NO_WINDOW` and exits. The GUI then shows no console window, and console
programs it starts (git, gh, `wsl.exe`) share its hidden console instead of
each opening one. `herdr-gpui.exe` stays a console program so the command line
keeps its output, exit codes, and a shell that waits for it. When the GUI
cannot be started, the launcher shows the error in a message box. When the
launcher sits beside it, every Herdr window carries the AppUserModelID
`so.pen.herdr-gpui` and relaunch properties naming the launcher
(`src/taskbar.rs`), so pinning a running window to the taskbar pins
`herdr-gpui-launcher.exe` as **Herdr UI**, and the pin and the running window
share one taskbar button. The properties are cleared as each window closes. Local
connections use the named pipe the Windows
daemon binds, derived from the same socket path string upstream uses, so
discovery and framing are the same code as on Unix. The pipe, and a WSL
bridge's standard streams, are driven with overlapped I/O, so receive and send
timeouts hold as they do on a socket, and clipboard and dropped images upload
as on macOS and Linux. Configuration and state
follow upstream's Windows layout: `%APPDATA%\herdr` and `%LOCALAPPDATA%\herdr`,
still overridden by `XDG_CONFIG_HOME` / `XDG_STATE_HOME` when they are set.

`herdr-gpui COMMAND` and the managed agent skill reach the running app over
`\\.\pipe\herdr-gpui-control-SID`, named for the Windows user. The app creates
it owned by that user with a DACL granting only that user's SID, as the first
instance, and with remote clients rejected; the command line opens it at
identification level and checks its owner before sending anything, since any
user could create a pipe of that name first. `docs/cli.md` has the details.
Reading the user's SID and the pipe's owner takes two `unsafe` Win32 calls
(`GetTokenInformation`, `GetSecurityInfo`), which neither the standard library
nor `interprocess` wraps.

### SSH hosts

Saved SSH hosts work as they do on macOS and Linux, through OpenSSH for
Windows: the copy Windows installs as an optional feature,
`%SystemRoot%\System32\OpenSSH\ssh.exe`, or without it the first `ssh.exe` in
an absolute `PATH` directory. The system copy comes first because `PATH` often
leads with Git's MSYS `ssh`, which reads another home's config and known hosts.
`ssh-keygen.exe` is found the same way. No console window opens for either.
The `ssh` child's standard streams are two private pipes rather than a socket
pair, driven with the same overlapped read and write deadlines as the daemon's
named pipe. **Add Device** lists **SSH host** first, then **WSL**. It checks the
host, asks before trusting an unknown host key (appended to the first user
known hosts file `ssh -G` names, normally `%USERPROFILE%\.ssh\known_hosts`),
and saves a host that already runs a compatible Herdr through `herdr machine
add`. Sessions on the host can be listed, created, and deleted.

Where this differs from macOS and Linux:

- **No terminal setup.** A host that needs a password or passphrase prompt,
  your key installed, or Herdr installed, updated, or restarted is set up in a
  local workspace on macOS and Linux. On Windows the dialog says which of
  these it needs and asks you to run `herdr machine add TARGET` in a terminal,
  then add the device again.
- **No connection sharing.** Windows OpenSSH has no `ControlMaster`, and with
  any `ControlPath` set it fails every connection ("getsockname failed: Not a
  socket"). The bridge passes `ControlPath=none`, so each connection
  authenticates on its own: keep a key in the OpenSSH Authentication Agent
  service (`ssh-agent`), since a master you authenticated interactively cannot
  carry these connections.
- **`ProxyJump` and `ProxyCommand`** come from your SSH config and run as
  Windows OpenSSH runs them. A jump host that cannot be reached is reported as
  an unreachable host.
- **Forward port…** is not offered in a device's menu: each of those forwards
  holds its listener in a private control master. A listening port found on
  the host still opens, through its own `ssh -N -L` child.
- **Uploads.** Files and images dropped or pasted on an SSH host's pane are
  not uploaded yet.
- **Steps that also run here.** Agent checkpoints, teleport, fan-out, process
  lists, and pane moves also run this machine's `/bin/sh` and stay
  unavailable from Windows.

CPU and memory, plan usage, listening ports, and the agent skill's install run
on the host's `/bin/sh` through `ssh`, as they do from macOS and Linux.

### WSL distributions

A Herdr daemon inside a WSL distribution is reachable as a device. Choose
**Add Device…** in the device picker: on Windows it lists the installed
distributions (`wsl.exe --list --quiet`, without Docker Desktop's own) and,
for the one you pick, checks Herdr inside it and saves it once a compatible
copy answers. A stopped server is started on the first connection, as for an
SSH host. Install Herdr inside the distribution first; this app never installs
or upgrades it there. Each connection runs Herdr's `remote-client-bridge`
through `wsl.exe --exec`, so terminals, the sidebar, and the session list work
as they do for an SSH host. Right-click a distribution's sidebar header to
remove it, which only forgets it here.

Saved distributions are kept in `gpui-wsl.json` beside upstream's endpoint
catalog (`%LOCALAPPDATA%\herdr\client`), never in the catalog itself, whose
strict schema cannot name one. The device you last chose, a distribution
included, is selected again at startup.

CPU and memory are sampled as for an SSH host running Linux: one `/bin/sh`
per distribution, started with `wsl.exe --distribution NAME --exec /bin/sh -s`
and kept open between samples, which reads `/proc`. The status bar and the
distribution's sidebar row show them. A distribution shares the machine's
CPUs, so its CPU figure is what its processes use, and its memory is what
WSL has given that distribution.

Plan usage is read through its own `wsl.exe` shell in the same way, as on an
SSH host: the distribution's own agent sign-ins, read with its `python3` or
`jq` and sent with its `curl`, so they never leave it.

Listening ports are scanned with the distribution's own `ss` (or `lsof`)
through another `wsl.exe` shell. WSL forwards a distribution's `localhost`
ports to Windows, so a port opens at `http://localhost:<port>` on this machine
with no tunnel.

Agent checkpoints run Git inside the distribution, one `wsl.exe --exec
/bin/sh -c SCRIPT` per step under the same output and idle limits as on an SSH
host, so its checkouts are snapshotted and restored as an SSH host's are.
Stopping a step kills its `wsl.exe`, which ends the shell it started.

Files dropped onto a distribution's pane, or copied in Explorer and pasted
there, paste as the distribution's own paths for them, as its `wslpath -u`
maps them: `C:\Users\me\a.txt` becomes `/mnt/c/Users/me/a.txt` (or wherever
`wsl.conf` mounts drives), and a file from inside the distribution
(`\\wsl.localhost\NAME\home\me\a.txt`) becomes `/home/me/a.txt`. Nothing is
copied: a distribution already reads this machine's drives, so a file of any
size is ready at once, an image is pasted as its path rather than through the
image bridge, and nothing is left in a temporary directory. The paths travel
on `wsl.exe`'s standard input, at most 256 per drop. Keys typed while they
are mapped are held and sent after the paths, so Enter cannot run a command
ahead of them, and one drop maps at a time. The paths are not pasted if the
pane is covered by a popup or the window moves to another device meanwhile;
paths that arrive while the terminal waits for its next frame wait with the
held keys and are pasted once it arrives.
A file the distribution
cannot reach, such as another distribution's or one on a network share, is
named in the error and nothing is pasted.

Ports need no forwarding: a distribution's device menu offers none, and
`herdr-gpui ports forward` refuses with that reason, since WSL already forwards
its `localhost` ports to Windows. Teleport is not offered. It runs only on
Linux and macOS clients, which cannot reach a distribution, and a Windows
client cannot script its own checkouts, so the only move left would be between
two distributions on the same machine.

### Window chrome on Windows

Every window keeps the title bar Windows draws, so its buttons, double-click,
drag-to-snap, and the window menu are the system's own, and hovering the
maximize button opens the Snap Layouts flyout. The title bar is light or dark
with the window's theme rather than with the system's app mode, so a dark
theme under light Windows no longer sits under a white bar. A translucent
window's blur is DWM acrylic or Mica on Windows 11 (see
**Settings > Theme > Window** under
[Configuration](#configuration)), and `accent = "system"` takes the app's
accent from Windows.

### Screen readers on Windows

GPUI reports the window to UI Automation through AccessKit, which is what
Narrator and NVDA read. Checked against the UI Automation tree of a running
window, the main window exposes:

- the terminal as a document named **Terminal**, whose text pattern holds the
  rows on screen and the selection (see `terminal/accessibility.rs`), so
  Narrator's reading commands and selection tools work on it;
- the in-window menu bar as a menu bar whose headings expand and collapse, and
  an open menu as a menu: each item by its name and shortcut, a checked item
  as a checked menu item, an unavailable one as unavailable, and the
  highlighted one reported as focused as the arrows move;
- the title bar's **Sidebar** (expanded or collapsed), **Back**, **Forward**
  (unavailable without history), and **GitHub account** buttons;
- the tab strip as a tab list named **Tabs**: each tab by its label, with the
  current one selected, its **Close tab** button, **New tab**, **Split**, and
  **Tab actions**;
- the activity bar as a tab list of its sections, each named with its count,
  and **Settings**;
- the sidebar as a navigation group: its search field, the **Workspaces** (or
  **Devices**) list and the **Agents** list, whose rows are list items named
  by what they show (a workspace's label and status, an agent's name, state,
  and place, a device's name and connection), selected when focused, and
  expandable when they fold others, which Expand and Collapse do; **New
  workspace**, **Menu**, the agents' sort, the device picker, and **Sessions**;
- the status bar as a status bar: the connection message, the model, usage,
  CPU and memory, and network readings by what they say, and its buttons
  (**Keep the display awake** as a switch);
- every popup as a menu (context menus) or a dialog named by its title, with
  its menu rows, its text field, and the workspace dialogs' **Cancel** and
  submit buttons.

The other windows are named the same way: Settings as a navigation list of
sections (a tab list, the open page a tab panel named for it), each setting a
group named by its title with its description as help text, and its switches,
choices, buttons, and fields by name and state; Logs as its search field,
buttons, a list of entries, the level menu, and its status line; Orchestrator
as its buttons, filters (switches), and its rows by identifier, title, and
state.

Each button can be invoked from the screen reader. With a Windows contrast
theme on (Settings > Accessibility > Contrast themes), the app's own marks and
labels use the **High contrast** level whatever `contrast` says; terminal
colors and the theme itself are unchanged, since GPUI does not paint with the
system's contrast colors.

#### The keyboard in the chrome

Tab belongs to the terminal, so the chrome is reached a region at a time, as
in other Windows apps. **F6** (Focus Next Region) moves the keyboard from the
terminal to the title bar, the sidebar (from its activity bar), and the status
bar, then back to the terminal; **Shift+F6** (Focus Previous Region) goes the
other way. Inside a region, Tab and Shift+Tab move between its controls and
wrap, the arrows also move in the sidebar, Enter or Space uses the control, and
Escape returns to the terminal. The focused control has an accent ring.
**F10** opens the menu bar. Both region commands can be rebound in Keyboard
Shortcuts (Cmd+F6 and Cmd+Shift+F6 on macOS). A region's controls are tab
stops only while it holds the keyboard, so clicking a chrome button never pulls
typing away from the terminal.

`tests/accessibility_windows.rs` reads a running window's tree and checks
these names and where F6 and Tab move focus:

```bash
HERDR_TEST_BINARY=C:/path/to/herdr.exe cargo test -p herdr-gpui --test accessibility_windows -- --ignored --nocapture
```

Still missing, and not faked with labels that do nothing:

- inside the larger dialogs and pickers (Add Device, the command palette's
  results, themes, checkpoints, fan-out, teleport, pull requests), most rows
  and buttons other than the shared field have no role yet, so a screen reader
  hears the dialog's name and its field but not every choice in it;
- the Orchestrator's detail pane, conversation, and dispatch dialogs, and the
  Settings controls built from arbitrary elements (font and color pickers)
  are not named yet;
- Settings, Logs, and Orchestrator have no region keys: there Tab follows
  GPUI's own order only where a control registers a tab stop;
- when the app starts in the background (from a script rather than the Start
  menu), AccessKit can miss the first activation and report the window rather
  than the terminal as focused until the window is activated again. This is
  in GPUI's Windows adapter setup, not in this app.

### Saved sign-ins and the avatar cache

GitHub sign-ins are kept in Windows Credential Manager, which plays the part
of the Keychain and the Secret Service. Each account is one generic credential,
`dev.herdr.gpui.github/github.com` for the main account,
`dev.herdr.gpui.github/github.com/host/<device-id>` for a device's own, and
`dev.herdr.gpui.github/github.com/account/<id>` for an additional account, holding
exactly what the other keyrings hold: the device-flow token pair with its
expiry, or only the GitHub CLI marker, never the CLI's token. Entries are
persisted for your Windows account on this computer only
(`CRED_PERSIST_LOCAL_MACHINE`), so they survive restarts and sign-outs of
Windows but never roam to other computers in a domain, and other Windows users
cannot read them. They are listed under **Windows Credentials** in Control
Panel's Credential Manager; signing out in the app removes them. All windows
share one credential through the same in-process lock as elsewhere, so a
single-use refresh token is rotated once. Boat, Coder, and Daytona sign-ins use
the same store under their own service names. A secret larger than Credential
Manager's 2,560-byte limit is refused with an error rather than truncated. There
is no private-file store here, so `allow_plaintext_credentials` changes nothing,
and `GH_TOKEN` / `GITHUB_TOKEN` still take precedence for the main account. The
`Cred*` calls are the only `unsafe` code involved (`github/credential_manager.rs`):
neither the standard library nor the workspace's dependencies wrap them, and the
maintained wrapper crates would add a second `windows-sys` version.

Profile and owner avatars are cached on disk in
`%LOCALAPPDATA%\herdr\gpui\cache\avatars-v1`, with the same 128 slots, 1 MB
entry limit, and 24-hour stale-while-refresh behavior as on Unix.
`XDG_CACHE_HOME` is not honoured for it. A non-blocking `File::try_lock` on the
directory's `lock` file keeps one writer at a time, and a contended cache is a
miss rather than a wait. Entries are opened without following reparse points,
and a symbolic link or junction, whether an entry or the directory itself, is
refused. There is no ownership or mode check, unlike Unix: the directory is
always inside your profile, whose inherited access list lets only you, SYSTEM,
and Administrators create, replace, or link files there, and anyone else who
could would already be able to replace the app itself.


In-app updates download the release's signed per-user installer, check it
against `windows-manifest.json` and its Authenticode publisher, and run it
silently; Setup restarts the app (see
[App Updates](../../docs/updating.md#windows-installers)). Only the copy in
`%LOCALAPPDATA%\Programs\Herdr UI` updates itself. The app registers with Restart
Manager, never after a crash or hang; the `herdr-gpui.exe --restart-manager`
Windows starts hands off to the launcher (`src/restart_manager.rs`). A crash
leaves a minidump and a report beside the log; see [crash reports](#crash-reports).

#### Launch at login

**Settings > General > Launch at login**, off by default, writes
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Herdr UI`, naming the
`herdr-gpui-launcher.exe` beside the running app, so it opens without a console
window when you sign in; turning it off removes the value. The registry is the
setting, so nothing goes in the config file, and the switch shows what Windows
will do: it is read when Settings opens and each time it comes to the front. A copy with no launcher beside it
cannot turn it on and says so. Task Manager's **Startup apps** and **Settings >
Apps > Startup** can switch the entry off without removing it (a mark under
`Explorer\StartupApproved\Run`). The app never overrides that: the row then
says where to turn it back on. The installer's uninstaller removes both values;
an `install.ps1` copy has no uninstaller, so turn the setting off before
deleting its folder. macOS and Linux have no launch-at-login setting yet
(`src/launch_at_login.rs`).

The app runs once per user. A plain second launch, such as the Start menu
entry or the launcher with no link, sends `app focus` over the control pipe,
so the running app's window comes forward and the new process exits. The
launcher and the new process pass on the shell's foreground right, so the
window comes forward instead of flashing in the taskbar. A launch with
`--socket` or `--session` still starts a separate app. So does a plain launch
when the running app cannot answer, for example one that hangs.

Starting a local `herdr server` looks for `herdr.exe` and uses
`CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW` in place of `process_group(0)`, so
the daemon survives the GUI and no console window appears.

Console children (git, ssh, `wsl.exe`, a shell) must not open a window of
their own either: every child is built with `herdr_client::new_command`, which
sets `CREATE_NO_WINDOW` on Windows, and clippy's `disallowed_methods`
(`clippy.toml`) rejects `Command::new` elsewhere outside tests and the build
script. Started by the launcher, they would share its hidden console anyway;
run as `herdr-gpui.exe` from Explorer or another app, they still open none.

The build script embeds the Herdr icon as icon resource 1, which GPUI loads for
its windows, and version information that Explorer and Task Manager show as
**Herdr UI** with the release version, or `dev` for a local build. See
[the icon README](../../assets/icons/README.md#windows).

Both executables also carry one application manifest,
[`windows.manifest`](windows.manifest): per-monitor DPI awareness, long paths
(once Windows allows them), the UTF-8 code page for ANSI APIs, Windows 10 and
11 as supported systems, and no elevation. It replaces GPUI's own manifest,
which GPUI's platform crate always asks for and an executable cannot hold
beside a second one, so the workspace patches GPUI's `embed-resource` build
dependency with the no-op in `crates/embed-resource`.

Each release also has a per-user Inno Setup installer,
`Herdr-VERSION-TARGET-setup.exe` (`scripts/release/herdr-gpui.iss`), which
installs the zip's files where `site/install.ps1` does, adds the directory to
the user `PATH`, points the Start menu shortcut at the launcher with the app's
AppUserModelID, and adds an uninstaller under **Settings > Apps**. See
[`scripts/release/README.md`](../../scripts/release/README.md) for building,
signing, and smoke-testing it.

Cross type-check it from a Mac or Linux machine with `just lint-windows`, which
targets `x86_64-pc-windows-gnu` because those hosts cannot supply the MSVC C
toolchain; it needs mingw's `x86_64-w64-mingw32-windres` on `PATH` to compile
the icon resource. CI lints the MSVC target on a Windows runner. `just lint-linux` is
the matching check for Linux, in the Ubuntu 24.04 container CI uses, because a
`cfg` gate that is wrong only on Linux is invisible from both macOS and the
Windows cross-check.

### Windows shell

- **Taskbar badge.** While agents are `Done` or `Blocked`, the taskbar button
  carries a red overlay with their number, `9+` past nine: the count the macOS
  Dock badge shows, from the same deduplicated source. It is drawn at the
  system's small-icon size, set on every window (windows group into one
  button), and cleared at zero. Screen readers hear "N agents need attention".
- **Taskbar progress.** A window's button fills in while it copies dropped
  files to an SSH host, teleports a worktree, moves a pane to another host, or
  downloads and installs an app update. Copies and downloads show their real
  fraction; steps without one show the moving bar. Checks the app runs on its
  own, and dialogs waiting for you, show nothing.
- **Jump list.** Right-click the taskbar button, or the Start menu entry, for
  the Git checkouts of the workspaces you last focused on this computer, under
  **Recent workspaces**, and a **Show DEVICE** task for each device. Each entry
  is a `herdr-gpui://` link (below) opened through the launcher, so it reaches
  the running app, or starts it. Windows shows recent workspaces only while **Settings > Personalization >
  Start > Show recently opened items in Start, Jump Lists, and File Explorer**
  is on; with it off, only the devices are listed. An entry you remove from the
  list is forgotten. The recent checkouts are kept in `jump-list.json` in the
  state directory.
- **`herdr-gpui://` links.** The installer and `site/install.ps1` register the
  scheme for the current user (`HKCU\Software\Classes\herdr-gpui`) and remove
  it on uninstall.
  `herdr-gpui://open?path=C:\src\repo` shows a local Git checkout or worktree:
  the workspace already open there, or, after you agree in the window, a new
  one (the repository's scripts are not trusted). `herdr-gpui://device?id=ID`
  switches to a device by the ID `host list` prints. A link is untrusted input:
  only those shapes are accepted, paths must be absolute drive paths (never
  `\\server\share`, which would send your credentials to that server, nor
  `..`), and a link never runs a command. Windows starts
  `herdr-gpui-launcher.exe` with the link as its only argument, so no console
  window opens; `herdr-gpui.exe` hands the link to the running app over the
  control pipe, or starts the app and follows it once connected. See
  [the command line](../../docs/cli.md#links).

## Build And Test

```sh
cargo check -p herdr-gpui
cargo test -p herdr-gpui
cargo clippy -p herdr-gpui --all-targets -- -D warnings
cargo fmt -p herdr-gpui -- --check
```

Requires the normal macOS Rust/Xcode development environment or the
[Linux build dependencies](../../README.md#linux-builds). Registry GPUI's default
X11/Wayland backends are retained. Linux uses Vulkan; macOS GPUI's
`runtime_shaders` feature compiles native Metal shaders at app launch, avoiding
the separate downloadable build-time Metal compiler. Integrated Linux ARM64
compilation, Clippy, default/all-feature tests, and release CLI checks were
verified in Ubuntu 24.04, not native desktop rendering or input. Linux uses
[Ctrl+Shift application shortcuts](#keyboard-shortcuts); explicitly configured
`cmd` bindings still mean Super. Linux shows an **in-window menu
bar** using the same Herdr, File, Edit, View, Terminal, and Window definitions as
macOS (plus QA in `qa-menu` builds). It remains visible with the sidebar hidden
and collapses to **Menu** in narrow windows. In client-decorated windows, the
minimize, maximize, and close buttons share its right edge. Drag the empty
menu-bar background to move the window; it also supports the title bar's
double-click and right-click behavior. Press **F10** to open a menu; arrows
navigate, Enter selects, and Escape or F10 dismisses. Submenus open in the same
panel; their back header returns to the parent. The menu shows current layout
checkmarks and configured shortcuts, and restores the previous input focus on
dismissal. Desktop-global menus are not exported. Tests cover wire colors,
cell modifiers, viewport bounds, semantic key selection, revision coherence,
creation request parameters, workspace-local tab cycling, wheel accumulation,
pane-relative hit testing, and popup routing.
Workspace-menu regressions check clicked-target schemas, stale boot/group
rejection, Unicode composition, and headless right-click/input routing.
`just test-sidebar` additionally exercises the native dialogs at narrow and wide
sizes, but does not validate OS IME candidate-window delivery or live daemon
worktree creation/close.
They do not replace an interactive smoke test against a live daemon.

On an active Linux desktop, exercise the menu bar in an isolated, daemon-free
native window with:

```sh
cargo test --locked -p herdr-gpui --features integration-test --test live_gui native_application_menus -- --ignored --nocapture --test-threads=1
```

This checks native painting and geometry at wide and narrow sizes, action
dispatch, focus restoration, input isolation, and shortcut editing, unbinding,
resetting, and live reload using GPUI-injected events;
it does not verify compositor-delivered keyboard input or OS IME behavior.

Selection regressions cover unflagged CJK continuation cells, real spaces,
partial wide characters, emoji/combining text, popup/pane boundaries, and headless
mouse-to-clipboard routing. On macOS, `just test-gui /absolute/path/to/herdr`
also prints CJK text through the isolated daemon, drags forward/backward using
exact-window native mouse events, checks the OS clipboard, and pastes through
Cmd-V to verify the UTF-8 bytes returned by the shell. This opt-in test requires
an active desktop; normal CI compiles it but does not run the native scenario.

Notification policy tests use explicit times for evidence grace, delay changes,
cross-host arrival order, queue bounds, replacement, promotion lifetimes, and
hidden-card expiry. Mock-peer navigation tests cover clicks and the native command,
including inbox contention, handoffs, stale targets, and input fences. To draw all
four offline previews in an isolated native window at narrow/wide sizes:

```sh
cargo test --locked -p herdr-gpui --features integration-test --test live_gui native_notifications -- --ignored --nocapture
```

This native check verifies disabled/delayed policy bypass and inert offline
commands, not live-daemon navigation or pixel-level notification glyph clipping.

`just test-sidebar` runs isolated, daemon-free native fixtures on the active
desktop. On macOS it checks exact-window clicks with a decoy key window, host
selection/disabled hosts, scoped collapse, duplicate-ID navigation routing,
composition preservation, menu isolation, and long-label native glyph clipping.
Scroll independence uses scroll handles and native draws, not trackpad events.
Native paint-probe failures report the label and geometry/glyph mismatch in the
captured `gui.log` output and fail the test with a nonzero exit instead of panicking
inside the native paint callback. The first failure survives subsequent redraws.
An intentionally wrong-width native fixture verifies exit code 1, useful diagnostics,
and absence of an abort signal. Sidebar and notification drivers exit explicitly
so AppKit termination cannot turn a failure into exit code 0.

GPUI 0.3.6's Wayland window retains its platform input handler during deferred
native cleanup. `ElementInputHandler` holds a strong entity reference, which can
therefore outlive GPUI's shutdown leak check and cause an `Exited with leaked
handles` panic in leak-detection builds. This was reproduced both before and
after [PR #177](https://github.com/penso/herdr-gpui/pull/177#issuecomment-5998314856).

On Linux, the client stores a weak reference in each terminal, dialog, and search
input handler. GPUI picks Wayland or X11 at runtime, so this applies to both
backends, although only Wayland shutdown has been observed to leak. The leak
check exists only in test and `leak-detection` builds; release builds never
panic, but they use the same handler so tests exercise what ships. The UI owner keeps the view alive; each input callback upgrades
the weak reference for its duration and delegates to GPUI's existing handler.
After the view is released, callbacks return an empty result or do nothing.
This addresses the shutdown ownership problem within the client while keeping
the pinned dependencies and leak detection intact. It avoids carrying a vendored
platform implementation while an upstream teardown fix is unavailable. Windows
and macOS keep GPUI's original handlers. Revisit the workaround when the pinned
GPUI release drops its Wayland input handler before checking for leaked entities.

With `WAYLAND_DISPLAY` pointing to the active compositor, exercise normal
shutdown with terminal, dialog, and search focus:

```sh
cargo test --locked -p herdr-gpui --all-features --test live_gui native_input_shutdown_ -- --ignored --nocapture --test-threads=1
```

This daemon-free fixture leaves each handler installed and quits normally with
leak detection enabled. Headless tests also cover Unicode composition, selection,
paste, and callbacks after the view is released; they do not verify a desktop
IME's candidate UI.

See the root README for the full verification scope and remaining limitations.
