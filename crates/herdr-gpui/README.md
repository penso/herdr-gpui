# Herdr Native Shell

A GPUI 0.3.6 (`gpui-pre`) client for a Local daemon and saved SSH hosts, with macOS
support, experimental Linux x86_64/ARM64 builds, and an experimental Windows build with
headless CI coverage. See [Windows](#windows) for what is unavailable there.
It starts an installed local `herdr server` when absent; explicit socket and
development targets remain attach-only. It does not link or install Herdr, stop
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
field types during deserialization; `toml_edit` preserves comments on theme saves.

Solid light/heavy box-drawing characters and block elements (including fractional
blocks and quadrants) are drawn on the terminal cell grid, with device-pixel-aligned
edges. Borders and block-art logos remain joined across rows and columns regardless
of font line spacing. Dashed, double, rounded and diagonal lines, shading characters,
and graphemes with combining marks continue to use font rendering.

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
without input replay. Detach pauses retries for that endpoint until Reconnect.
The status dot pulses amber during local daemon startup and is red when disconnected.
Healthy connections leave the status bar quiet; connection indicators live in the
device picker. Startup, disconnection, and operation errors remain in the status bar.

Normal launches restore the main windows left open at quit, including each
window's size and screen position, using the current launch's connection options.
Closing an individual window removes it from the saved set; closing the final
main window retains its geometry for next time. Additional windows cascade from
the last open main window. Fullscreen windows restore to their normal rectangle.
Each window reopens on the display it was on; if that display is disconnected,
it opens on the primary display, resized and moved to fit. Logs windows
are not restored. Geometry is stored in `window-state.json` under
`$XDG_STATE_HOME/herdr/gpui`, or `~/.local/state/herdr/gpui` by default. Native
test modes skip this state. Up to 64 main windows are restored.

The Rust GitHub updater verifies signed archive manifests and presents a shared
GPUI panel through **app updates** in the sidebar menu or **Herdr > Check for
Updates...**. Background offers change the status version label without taking
focus. Download and **Install and Restart** are separate approvals; closing the
panel does not cancel work. Explicit **Cancel** requests cancellation.
The panel shows archive download progress and an animated bar for work without a
known percentage. Homebrew-managed installs use `brew upgrade --cask herdr-gpui`;
if the installed version is behind the offer, `brew update` refreshes metadata
before one retry. Homebrew upgrades cannot be cancelled mid-install.
The existing UI timer polls the updater mailbox; workers own blocking work, and
the restart helper is dispatched before CLI parsing or GPUI startup.

Only normal launches with a calendar release version `YYYYMMDD.COUNTER` (tag
`vYYYYMMDD.COUNTER`) and embedded public signing key start the service. Test
fixtures use a disabled, worker-free updater. Update targets are macOS app
bundles and user-owned Linux executables under `HOME` on x86_64/aarch64
GNU systems, not arbitrary packages or a claim of full Linux app support.
**QA > Show app update available** and the sidebar's **preview app update** use
independent synthetic version `9999.0.0`: Download becomes Ready and Install and
Restart only dismisses the panel. No preview action reaches the updater service or quits.
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
saved-device management is unavailable on Windows or for disabled devices.
Remote hosts need an installed Herdr supporting the session commands and
noninteractive SSH authentication. The GUI does not install or update it.

**Add Device…** accepts an SSH target, label, and optional remote session (default:
`default`). Herdr's `machine add` saves a new profile every time and takes no
lock, so the dialog keeps a host from being saved twice itself. A host counts as
already saved when a profile with the same session reaches the same user, host
name, and port as `ssh -G` resolves them, so an SSH alias, `user@address`, and
`ssh://` spellings of one machine all match. The host is claimed for this app
before anything else, so a second add from any window is refused while the
first runs. The catalog file is read again right before `machine add` runs and
right after: if another client saved the same host in between, the profile added
second is removed, so exactly one remains. A terminal setup keeps its claim for
15 minutes, because the GUI cannot see when its `machine add` finishes; another
client adding the host during that window can still create a duplicate.

Right-click a saved SSH device's header in the Spaces list to **Rename** it or
choose **Remove device…** to forget it. Renaming runs `herdr machine rename`;
an empty name falls back to the SSH target, as when adding. Removal runs the installed `herdr machine remove`, which
edits only the local catalog: the host's own Herdr keeps running. When the
device has its own GitHub sign-in, the confirmation also offers to delete it,
since its account panel goes away with the device. Local has no such menu.

The label is optional: an empty one names the device after its SSH target as
typed, such as `user@host` or an address. Once the device is saved, the dialog
closes by itself.

**Add device** first checks the host over non-interactive SSH, using
the same executable search and compatibility rules as the connection bridge, and
never installs or starts anything while checking:

- Herdr running, or installed but stopped: the installed `herdr machine add`
  runs without a terminal and saves the device. It starts a stopped server
  itself. Any approval it would need fails instead of waiting for input.
- Herdr missing: the dialog asks "Herdr was not detected on the host. Should we
  install it?" An outdated Herdr asks to update it instead.
- SSH needs a prompt (unknown host key, password, passphrase), the check failed,
  or saving without a terminal failed: the dialog offers to continue in a
  terminal.

Accepting creates a new workspace on this device's Herdr and types
`herdr machine add` into its shell. Herdr handles SSH prompts, approval to
install/update remote software, server startup, and saving the machine only after
successful setup; complete any prompts in that workspace. The saved device
appears automatically on the next catalog refresh. No credentials are collected
by the GUI. Explicit-socket and development-catalog windows do not offer setup,
and saved SSH devices remain unsupported on Windows.

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
**QA > Show herdr non-detected modal** previews the warning without restarting,
disconnecting, or changing daemon detection. Closing the GUI leaves the daemon
and its terminals running.

## Configuration

GUI settings live in `$XDG_CONFIG_HOME/herdr/`, falling back to
`~/.config/herdr/` and, on Windows, to `%APPDATA%\herdr\`:

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
on reload. The daemon's config is never modified.

See
[`config-gpui.example.toml`](config-gpui.example.toml) for a complete example.
Font sizes use logical pixels (finite 8..48), not typographic points. Saving
`config-gpui.local.toml` automatically reloads every open GUI window, usually
within half a second. Font family, font size, theme, and layout changes apply
together; invalid edits keep the last valid settings and show a load error.
Reload waits while a theme preview/save is active. The manual GUI config reload
action remains available; daemon config reload is separate.

The terminal face can also be resized for the current session from the View menu,
the in-app menu, the command palette, or `cmd-=` / `cmd--` / `cmd-0`. Adjustments
are clamped to the same 8..48 range, apply to the terminal only, and are never
written to disk, so a reload or a restart returns to the configured size.

Set top-level `confirm_close_tab = false` to close tabs without confirmation
(including their running processes), and `show_agents = false` to hide the Agents
section and give Spaces the full sidebar height. Both default to `true`. Pane
closures still ask for confirmation. Saved edits apply automatically.

`[notifications]` controls GUI-local in-app delivery, independently of the daemon:

```toml
[notifications]
enabled = false
delay_seconds = 1
position = "bottom-right"
```

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

Choose the sidebar layout from **View > Layout**, which lists every layout,
checks the one in use, switches at once, and saves the choice to
`config-gpui.local.toml`. The same setting can be written by hand as a
top-level line there (before any table headers):

```toml
layout = "compact"
```

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

Three more layouts draw rows with a design of their own, each with fixed
spacing:

- `superset`: one line per row. An icon slot carries the pull request's state
  or the repository owner, with the activity status as a dot on its corner; the
  PR's change counts sit on the right, and the focused row is filled with a
  stripe down its leading edge. Agents show where they run after their name.
- `orca`: inset cards with a status column, the name and a `primary` mark on a
  repository's own checkout, then a meta line with the host, the branch when it
  differs from the name, and the pull request. Agents are single compact lines.
- `minimal`: one line per row with only the status dot and the name, for narrow
  sidebars or long lists.

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

Agent names have small theme-tinted icons for OpenCode, Claude Code, Codex
(OpenAI), Gemini, Cursor, and GitHub Copilot, selected from the daemon's agent identity. Other
or missing identities use a generic terminal icon, regardless of custom names.
Icons sit immediately before the name, including orphan agents whose name is
on the first line, and reserve space before long names are truncated.

To customize spacing too, use a `[layout]` table **instead of** the top-level
string, replacing it (including the one a new install writes): TOML rejects a
file with both. Existing spacing-only tables remain supported and use normal mode:

```toml
[layout]
mode = "compact"
sidebar_gap = 8
```

`sidebar_gap` (finite 0..64 logical pixels,
default `8`) is blank space between the sidebar and the terminal beside it, so
the first column does not sit against the divider; `0` restores the flush edge.
The terminal keeps the remaining width, so the daemon is resized to the columns
it actually has, and the gap is ignored while the sidebar is hidden.

The `[clipboard_toast]` table controls the `copied to clipboard` flash shown
after a terminal selection is copied. It is the one GUI setting that starts from
the daemon's own config: `[ui.toast.clipboard]` in `config.toml` (resolved like
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
following the daemon config. Only these two keys are read from that file, it is
never written, and an unreadable, oversized, malformed, or unrecognized value
leaves the defaults standing.

The `src/config.rs` module exposes `Config::load()` and
`Config::path()` (managed defaults) and `Config::local_path()` (user overrides),
all returning the crate's typed `Result`. `Config::theme()` resolves
built-ins or Ghostty files into a `Theme` with packed 24-bit RGB colors and all
256 palette entries. Theme resolution is a separate fallible step from loading
and validating TOML. Font sections can override either family or size without
repeating the other field. `FontConfig::line_height()` returns `size * 20 / 14`.
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

**QA > Play Sound** explicitly tests Rodio playback with the built-in Done sound
on the same background worker. No daemon or active pane is needed. This manual
test bypasses notification mute (including `enabled = false`), agent filters,
custom sound paths, delay, and focus suppression. Environment/test suppression
still applies, as do the bounded queue, one-second queue expiry, and playback
budget below. Closing the window cancels the test; endpoint disconnects do not.

Semantic notifications from all connected endpoints use the TUI's timing:
`delay_seconds` is 0..3600 (default 1), Custom is immediate, delayed attention
requires a Blocked agent, and Finished requires projected Done state. Completion
evidence may wait up to one second from receipt, rechecking every 50 ms. New
notifications replace pending ones for the same endpoint/pane. Only Finished is
suppressed for the selected endpoint's active tab while the native window is
focused (workspace focus is the fallback for events without a tab).
Legacy `Notify`, terminal BEL, and terminal escape sequences never play audio.

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
Windows/Linux keep the existing native frame and do not render this header.

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
window options in `crates/zed/src/zed.rs`, not a build dependency. Double-click calls
`Window::titlebar_double_click()` to honor the OS preference. We leave `is_movable`
unchanged and rely on native AppKit dragging, with no custom drag handlers or platform
patches. That choice predates GPUI 0.3.6: 0.2.2 had no macOS `start_window_move`
implementation and ignored `WindowControlArea::Drag`, while 0.3.6 implements
`start_window_move`.

Headless tests check the actual root header/center/account-slot bounds at wide,
minimum, and narrow sizes, including mock fullscreen entry/exit, and that the
avatar and hit target stay centered. An SVG decoding test checks the embedded user
icon produces a nonempty mask. These do not verify AppKit behavior. Native QA remains
required for dragging across the header, traffic-light alignment and actions,
double-click preferences (zoom/minimize/do nothing), fullscreen transitions and
auto-hidden controls, theme changes, and modal/focus/IME behavior. Windows/Linux
native-frame appearance also remains unverified by these macOS tests.

## Terminal Selection And Copy

Mouse-aware applications receive clicks, button releases, drags, and pointer
motion. Hold Shift to select/copy locally instead, or Shift-right-click for the
GUI pane menu. In applications without mouse reporting, selection and the pane
menu work without Shift. A forwarded drag stays in the pane or popup where it
started, including when the pointer moves outside it.

Drag across the terminal to select cells; releasing the button copies them, drops
the highlight, and shows the `copied to clipboard` flash described under
[Configuration](#configuration). Selection is client-local: it reads the surface
the client already has, sends nothing to the daemon, and asks it for nothing.

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

The highlight is cleared by the release that copies it, and by a reconnect,
detach, or endpoint switch. Cmd-V still sends semantic paste; there is no copy
keystroke, because the release has already copied and nothing stays selected.
For the same reason, the native **Edit** menu enables only **Paste** while a
terminal has focus. In dialogs and search fields, **Cut**, **Copy**, **Paste**,
and **Select All** do the same as Cmd-X, Cmd-C, Cmd-V, and Cmd-A.

## File Drops

Drop files from your file manager onto a terminal pane or popup to paste their
paths there, even if another pane is focused. Paths are quoted as POSIX shell
words, separated by spaces; the drop never presses Enter. Local drops only paste
paths; SSH drops read and transfer the selected files.
Drops are limited to 256 paths and 64 KiB of quoted text. Non-UTF-8 paths and
paths containing control characters are rejected rather than altered.

On an SSH endpoint, a single supported image is transferred as described below.
Other regular files and multiple-file drops are streamed using the SSH file-copy
path below. POSIX quoting is not intended for Windows command shells.

## SSH File Copies

Drop regular files onto an SSH pane or popup to copy them to that host. A
`Copying...` card shows the filename (or file count), transferred bytes, percentage,
progress bar, and Cancel button. Once all files are received and the SSH processes
exit successfully, their quoted remote paths are pasted into the original target.
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

## Images

On a selected SSH endpoint, drop one PNG, JPEG, GIF, WebP, or BMP image onto a pane
or popup to send it through Herdr's existing image bridge. Clipboard images use
Cmd-V (normal paste, with text taking precedence) or Ctrl-V (Herdr TUI's default
image-paste shortcut). Ctrl-V retains its normal terminal meaning when the
clipboard has no image. A pasted absolute image-file path is also recognized,
including the quoted/backslash-escaped paths used by terminal file drops.

Local endpoints on macOS and Linux use the same bridge for Cmd-V clipboard
images, because a GUI text paste cannot carry image data. Local drops and pasted
paths stay ordinary path pastes, and local Ctrl-V is sent to the terminal
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

macOS uses the native pasteboard on a background executor; AppKit may materialize
its data before the client can check its size. Linux uses `wl-paste` (Wayland) or
`xclip` (X11) for explicit Ctrl-V image acquisition, with bounded output and a
three-second acquisition deadline. Ordinary Linux paste retains GPUI's native
clipboard reader and needs no helper; that existing synchronous API can still
materialize image data on the UI thread. Install the matching utility for
background image paste. Regular-file reads use bounded
chunks and a three-second deadline between reads, but an OS-blocked network/FUSE
filesystem operation cannot be forcibly interrupted. Such a read stays isolated
from the UI and holds its bounded preparation slot until it returns.

Windows SSH and image uploads, including local clipboard images, remain
unsupported. Native clipboard/drop
and real SSH behavior require explicit desktop/host verification in addition to
the mock-peer and headless tests.

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

Click an explicit terminal hyperlink or a visible `http://` / `https://` URL to
open it in your default browser. A hand cursor indicates a clickable destination.
In a mouse-aware application, hold Shift while clicking to open a link locally
instead of sending the click to the application.
Only HTTP and HTTPS destinations are opened. Links inside a popup target that
popup, and menus block activation. Dragging does not activate a link: a drag
across a link copies it as text, and the click that opens it is the one that
never left the half-cell it pressed in.

Plain URL detection is limited to one row within one pane; links that wrap or
reach the right edge need explicit terminal hyperlink metadata. Other URI schemes
and local file paths are not activated.

## macOS Dock Badge

The Dock icon shows the number of agents reporting `Done` (finished) or `Blocked`
(waiting for input), and clears at zero. It covers all connected hosts and
main windows, including minimized windows, without counting the same agent twice.
Sidebar visibility, muted sounds, and dismissed toasts do not affect the badge.
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
not a background notification service. Linux and Windows do not show this badge.

To preview it without waiting for an agent, choose **QA > Enable badge** in the
macOS menu bar. The preview shows at least `2` and stays on until you choose **QA > Disable badge preview**
or quit. Disabling the preview restores daemon-driven behavior, so real agent
attention can keep the badge visible. This QA setting is not saved.

## Logs

The client's own logs are written to
`$XDG_STATE_HOME/herdr/gpui/logs/herdr-gpui.jsonl` (falling back to
`~/.local/state`), one JSON record per line, readable only by you on Unix. Past
16 MiB the file is rotated to `herdr-gpui.1.jsonl`, replacing the previous one,
so at most two files are kept. Logs are not held in memory: **Window > Logs**
reads the newest 5,000 records of the file while it is open, including earlier
runs, and filters, copies, or exports them. Nothing is uploaded. Logging never
waits on the disk; lines that cannot be queued or written are counted as dropped
in the window's status bar. Without `XDG_STATE_HOME` or `HOME` (as on a default
Windows setup) nothing is saved and the window says so.

## Supported

- Workspace/worktree sidebar with main-checkout parents, indented linked
  workspaces, local collapse arrows, branch details, and daemon-driven
  filled/hollow activity indicators taken from the daemon's own status, so the
  GUI and the terminal client always show the same dot. Each worktree row also
  carries its cached pull request number and diff counts.
- Agents panel header ends with its sort, `grouped` or `priority`, which a
  click flips; an active agent view names itself there instead. Client-local
  and persisted beside the sidebar width, as in the terminal client.
- Resizable sidebar with width persisted per local daemon socket, shared across
  host groups. Drag the divider between Spaces and Agents up or down to resize
  their sections; double-click it to restore an even split. The split is saved
  across launches and retained while Agents is hidden.
  Local workspace titles show repository owner avatars; remote
  workspaces use the GitHub fallback mark without resolving remote paths locally.
  Profile and owner avatars share a bounded public-image disk cache with 24-hour
  stale-while-refresh behavior; see [avatar caching](../../README.md#native-github-sign-in)
  for limits, location, and the startup authentication requirement. Neither cache
  reads nor downloads block rendering; sign-out discards profile refresh results.
- In-app sidebar menu for settings information, keybinds, config reload, update
  information, and detach/reconnect. Styled Preferences include Appearance,
  Fonts, Configuration, and Connection sections, with theme selection and GUI
  config reload; font values remain read-only and are edited in the config file.
- A searchable theme picker previews the available names from built-ins and
  Herdr/Ghostty theme folders. Selecting a theme applies and saves it while
  preserving other GUI config settings and comments.
- Right-click spaces for Rename, Close (Close group on non-linked parents with
  multiple spaces sharing `worktree.key`), and New worktree / Open worktree... on non-linked Git
  parents, including spaces with a known Git branch but no worktree metadata yet.
  Right-click also selects the space, switching the terminal and sidebar highlight
  when the daemon confirms the selection. A compact header repeats the target name
  and Git branch. Right-clicking another visible space while this menu is open
  selects it and switches the menu in one click.
  With `features.sidebar_hover_menu` enabled, resting the pointer on a
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
  reports the daemon's own failures in the dialog rather than the connection status, and selects
  and reveals the created checkout once the daemon reports it. Rename and branch
  dialogs support Unicode/IME, grapheme
  editing, Shift-arrow selection, Home/End, and Cmd-A/C/X/V. Escape/outside click
  cancels; dialog input never reaches terminals or native creation actions.
  Context menus and dialogs anchor to the pointer and clamp to the viewport.
  Rename trims surrounding whitespace and rejects blank labels inline.
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
   eligible workspace metadata every 90 seconds, with a 128-entry LRU cache, 128 queued jobs, and
   alternating open/focused priority and round-robin scheduling. Failed refreshes
   retain successful data. Ordinary failures back off five minutes; auth/rate-limit
   errors pause the account for an hour by default, honoring numeric retry/reset
   hints within five minutes to 24 hours. Auth/endpoint generations fence late results.
   Discovery uses the daemon repository key and exact branch to resolve a unique
   Git worktree, followed by common-directory/current-branch checks and an explicit
   GitHub repository/head query. It never occupies the deletion dialog response slot.
  On macOS, all socket modes (including explicit/inherited sockets) require a
  same-user kernel peer at the standard configured session socket, with owned,
  non-group/world-writable socket and parent. Executable upgrades/removal do not
  invalidate this local endpoint trust. Sockets elsewhere remain blocked; a
  same-user proxy deliberately replacing the trusted socket is not detectable.
  Reconnect rechecks the endpoint.
  On a saved SSH device, the checkout lives on that host, so local Git cannot
  verify it. The worker instead reads the repository's `remote.origin.url` over
  the same noninteractive SSH options as the bridge (`BatchMode=yes`, strict host
  keys, no master connection), keeping stdout bounded and discarding stderr.
  Each resolved repository is reused for ten minutes, so refreshes do not dial the
  host each time. The daemon-reported branch is trusted as-is. Sidebar PR badges
  show only on the selected device's rows, because the cache holds that device's
  lookups and the same path and branch may exist on another host.
- Each saved SSH device can have its own GitHub account, for hosts whose
  repositories another account owns. Select the device, open the GitHub panel,
  and choose **Use another account** to run the same device sign-in for that
  device only. It is stored with the main account's mechanism (the app's Keychain
  service under a per-device account name, or its own private
  `github-credentials-<device-id>` file) and renewed the same way. Pull requests on
  that device then use it; a device without one uses the main account. Signing
  out in that panel removes only the device's credential. `GH_TOKEN` /
  `GITHUB_TOKEN` apply only to the main account. Removing a device keeps its
  saved credential until you sign out of it, so re-adding the device finds it. See
  [PR lookup scope and limits](../../README.md) for authentication and remote limits.
   The same worktree-registry path supports both current and older daemons without
    `workspace.get`. No Git or HTTP requests run from menu-open or render paths.
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
- The top-right titlebar profile control starts native GitHub device sign-in on
  a signed-out click, shows the authenticated user's avatar, and offers Sign out
  on right-click. Signed-out workspace menus have no GitHub section or requests.
  The signed-out GitHub icon and connected avatar share a 20px size and subtle
  hover glow; authentication errors appear in the account panel, not a red border.
  It uses Herdr GPUI's public client ID `Iv23liurUcwxPjrdIFYT`, overridden by
  `[github].oauth_client_id`, then `HERDR_GITHUB_OAUTH_CLIENT_ID`. No client secret
  or private key is needed or shipped. The compact native macOS titlebar design
  is integrated from main commit `3909f21`, without unrelated tab changes.
  Signed macOS release builds keep tokens in this app's Keychain entry; unsigned
  development and worktree builds use the private file store instead, so a new
  code identity per rebuild cannot trigger a Keychain prompt on every launch.
  `GH_TOKEN` / `GITHUB_TOKEN` override either. Access tokens and retained device/user codes use redacted, zeroizing
  `secrecy` types; HTTP headers are sensitive and application-owned raw OAuth
  buffers are wiped. The user code is intentionally exposed for rendering.
  Library/OS/rendering copies are not guaranteed to be erased. Linux supports an
  explicit `allow_plaintext_credentials = true` opt-in with a prominent warning,
  separate private credential file and atomic no-follow Unix writes; macOS
  development builds use that same store, enabled by default and warned about in
  the profile panel. Signed macOS release builds still use Keychain. Windows has
  neither store: the opt-in does not select the file there, saving a token
  reports `CredentialUnsupported`, and sign-in says to use `GH_TOKEN` /
  `GITHUB_TOKEN`. Sign-out suppresses environment tokens for this app session and
  fences late profile/avatar/PR results. Plaintext policy reloads re-evaluate the
  active credential, without reactivating an explicitly signed-out session.
  Disabling plaintext stops its session use but keeps the file; explicit sign-out
  still removes the saved file regardless of opt-in, or reports a safe error.
   Device-flow refresh tokens are saved alongside access tokens in the same
   store, including their access-token expiry when GitHub supplies it. Connected
   sessions are checked in the background every five minutes and renewed within
   ten minutes of expiry, without restarting the app. Older saved pairs without
   expiry metadata renew when GitHub rejects the access token. The rotated pair
   is saved before loading the profile again. Temporary network and Keychain
   failures keep an active session visible and retry on the next check. They do
   not delete credentials; environment tokens are never renewed or replaced by
   saved credentials. Successful sign-in closes the GitHub panel and returns
   focus to the terminal. Reopen the account panel to use the red Sign out action.
   Older versions saved only access tokens, so an expired legacy token needs
   one more sign-in to obtain a refresh token. Revoked or expired refresh tokens
   also require sign-in. No CLI authentication is used. See
  [setup, cancellation, scopes, and sign-out](../../README.md#native-github-sign-in).
- Workspace actions retain the clicked ID and boot, revalidate before queueing,
  and reject changed close-group membership. Reconnect clears dialogs. Queue
  errors remain in the dialog; daemon errors appear in the connection status bar.
  Queue acceptance dismisses the dialog, not an optimistic state mutation.
- Linked spaces offer Delete worktree checkout with a daemon-resolved path and a
  single confirmation, matching the Herdr TUI. Unlike other workspace dialogs,
  deletion stays open until the correlated daemon result arrives. Dirty/untracked
  refusals and errors are shown inline; force requires a new confirmation. All Git/filesystem work
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
  Close uses the existing cancel-by-default confirmation unless
  `confirm_close_tab = false`. Escape or an outside left/right click dismisses
  the menu without sending terminal input.
- Click workspace, tab, agent, or a visible split pane to focus through the API.
- Right-click a visible pane, including an inactive split, for Rename, Split
  Right, Split Down, Toggle Zoom, and Close without first focusing it. Actions
  retain the clicked pane/tab/workspace and daemon boot, and reject stale
  membership or a changed connection. Rename uses an IME-aware native field,
  trims surrounding whitespace, and clears the custom label when blank. It
  waits for the matching daemon response and reports failures inline. Close
  always asks for confirmation with Cancel selected. Popups and stale retained
  terminal frames block pane context actions. Escape or an outside left/right
  click dismisses the menu without forwarding input to the terminal.
- Native File/Edit/Terminal menus and creation buttons: **+ New Workspace** in the
  sidebar and a persistent 18px SVG **+** in a 44px-wide button beside the horizontally
  scrolling tab strip. Each tab has a 16px SVG close cross in a 24px hit target;
  it uses the same configurable confirmation without focusing an inactive tab.
  Both icons use the current theme's foreground tint.
- Cmd-T creates and focuses a tab; Cmd-Shift-N creates and focuses
  a workspace. Cmd-N opens the New worktree dialog for the focused workspace
  (for a linked worktree, its repository's main checkout). The dialog opens on
  its Name field: left empty, the daemon picks the workspace name; anything
  typed is sent as the new workspace's label. When there is none,
  because the workspace is not a Git repository, the main checkout is not open,
  nothing is focused, or the window is disconnected, a two-second flash in the
  clipboard toast's position says why.
  Cmd-D splits the focused pane vertically (new pane on the right);
  Cmd-Shift-D splits horizontally (new pane below). Cmd-Shift-] / Cmd-Shift-[
  cycles next/previous tab within the current workspace, wrapping at the ends.
  These shortcuts are native actions, not bytes sent to a terminal.
- Cmd-Alt-N runs **Open Notification Target**, also available in Terminal and the
  command palette. It uses the visible card's safe click path; stale, targetless,
  queued, or menu-hidden cards do not navigate or change endpoint selection.
- Cmd-1 through Cmd-9 focuses the corresponding numbered tab in the current
  workspace. Cmd-Alt-Left/Right/Up/Down focuses a pane in that direction;
  Cmd-Alt-] / Cmd-Alt-[ cycles next/previous pane within the current tab.
  Cmd-Shift-Enter toggles focused pane zoom. Cmd-K clears the focused pane's
  screen and scrollback through the daemon's `pane.clear`, without sending input
  to the running program; daemons that do not advertise it (Herdr 0.9.1 and
  older) leave it out of the palette and report why instead.
- Cmd-W closes the focused pane and Cmd-Shift-W closes the focused tab only after
  a confirmation dialog (tab confirmation can be disabled with
  `confirm_close_tab = false`). **Cancel is selected by default**: Enter alone cancels;
  Tab then Enter selects and confirms Close. Closing can terminate running
  processes, unlike quitting the GUI, which only detaches.
- Cmd-Shift-P opens the command palette with native actions and configured daemon
  command entries, including native Themes and Reconnect actions without dedicated
  shortcuts. Cmd-P opens **Go To** instead: every workspace on every connected
  host, each followed by one row per agent or terminal pane with its status,
  tab, and directory. Choosing a row on another host switches to it first.
- Every native shortcut can be rebound in `config-gpui.local.toml` under
  `[keybindings]`, keyed by command name (`new_tab`, `new_workspace`,
  `split_right`, `focus_tab_1`, `quit`, ...). A value is one keystroke or a list;
  an empty string or list unbinds the command. A keystroke assigned there moves
  away from its default command, keystrokes need a cmd, ctrl, alt, or fn
  modifier, and unknown names, unparseable keys, or one key on two configured
  commands reject the config. Saved changes rebind the keymap and menu bar live.
- Cmd-B toggles sidebar visibility locally without changing daemon state.
  Cmd-, opens Settings; Cmd-/ opens the grouped native shortcut reference.
  Native shortcut labels and keycaps come from the shared `controls::COMMANDS`
  catalog, overridden by the config's `[keybindings]` table, with Cmd-V semantic
  paste shown separately. Search filters by action,
  section, or key combination. Preferences, keybinds, theme/palette pickers, and
  close confirmations use themed centered modals and configured UI fonts;
  modal input does not reach the terminal.
- Creation omits `cwd`, labels, environment overrides, and split ratio: the
  daemon applies its existing defaults and directory policy. Workspace creation
  supplies the currently focused source workspace when available; tabs and splits
  target the current workspace/pane explicitly. An empty session can create a
  workspace without guessing a local path. Nothing is created while disconnected.
- Vertical mouse-wheel/trackpad scrolling targets the pane under the pointer
  (inside its content, not borders). Fractional pixel motion accumulates into
  terminal lines, with bounded per-event work. Popups capture wheel input only
  within their displayed bounds; input never falls through to a covered pane.
- Direct semantic cell canvas: named ANSI colors, indexed 256-color palette,
  RGB, reset foreground/background, reverse, dim, hidden, bold, italic,
  underline, strikeout, wide-cell skip handling, and cursor shapes.
- Server popup text surfaces centered above the main surface, with popup input
  routing while one is active.
- Native committed text through `EntityInputHandler`, including Unicode and
  composition. In-progress marked text is shown in the status bar.
- Enter, Tab/BackTab, Escape, Backspace, arrows, navigation/editing keys,
  F1-F24, Control characters and modifiers on special keys. Option-printable
  input follows the macOS keyboard layout, including dead keys.
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
at most once per 16 ms without blocking. Snapshots invalidate surfaces with a
different boot/projection revision; input waits for a coherent surface. Reconnect
replaces the cache, so late events from an old connection cannot affect the UI.

### Scrolling Semantics

Upstream `PaneScrollParams` is exactly `{ "pane_id": string,
"offset_from_bottom": u64 }`, an absolute scrollback position, not a wheel delta.
Like the upstream TUI's normal wheel handling, this GUI instead sends semantic
`ClientPaneInputEvent::Mouse` (`ScrollUp`/`ScrollDown`, pane-relative position,
modifiers, and line count). The daemon's `apply_scroll` chooses host scrollback,
alternate-screen behavior, or application mouse reporting using the current
terminal mode. This avoids racing absolute `pane.scroll` offsets against incoming
frames and avoids duplicating terminal-mode policy in the GUI. Scrolling does not
change keyboard focus to the hovered pane. The existing client advertises no pixel
mouse capability, so the daemon uses the supplied cell-coordinate fallback.

Reference sources (read-only): Herdr's `src/api/schema/{workspaces,tabs,panes}.rs`,
`src/app/api/workspaces.rs`, `src/client/shell/mouse.rs`, and
`src/server/pane_input.rs`; Arbor's `crates/arbor-gui/src/app_bootstrap.rs` for
GPUI native action/menu/keybinding patterns.

## Deliberate Limitations

- macOS defaults to Menlo and the system font; Linux defaults to DejaVu Sans Mono
  and DejaVu Sans. No bundled Nerd Font.
  Private-use icons may be missing. Fonts and palettes are configured locally,
  not synchronized from the host terminal's theme.
- No draggable scrollback UI, split dragging,
  image rendering, or animated blinking.
- No horizontal wheel handling,
  server-owned keybindings, session picker, saved-host editing, or daemon
  stop/upgrade management.
- IME uses a minimal transient buffer, not a local editable terminal document;
  composition appears in the status bar rather than inline. Key releases and
  physical-key/extended keyboard protocol metadata are not reported.
- Popups have a basic centered text presentation, without native title/border
  chrome. Only semantic notifications get in-app toasts; legacy notification
  commands and server clipboard writes are not executed. There is no notification
  history.
- Rendering is a simple two-pass cell painter, not an optimized damaged-row
  renderer. Large/high-frequency surfaces can consume significant CPU.

## Windows

Windows is experimental, not a supported platform. CI checks formatting, lints
every target and feature, and runs workspace tests with default and all features
on `windows-2025` (x86_64) and `windows-11-arm` (ARM64), including headless UI
and CLI tests. The release workflow builds and CLI-tests the optimized
executable natively for each architecture and publishes
`Herdr-VERSION-x86_64-pc-windows-msvc.zip` and
`Herdr-VERSION-aarch64-pc-windows-msvc.zip`; native window, rendering, input, and
live-daemon behavior remain unproven. Local connections use the named pipe the Windows
daemon binds, derived from the same socket path string upstream uses, so
discovery and framing are the same code as on Unix. Receive deadlines are
emulated with `PeekNamedPipe`, the one `unsafe` call in the workspace, because a
named pipe has no receive timeout; send timeouts cannot be enforced at all
there. Configuration and state
follow upstream's Windows layout: `%APPDATA%\herdr` and `%LOCALAPPDATA%\herdr`,
still overridden by `XDG_CONFIG_HOME` / `XDG_STATE_HOME` when they are set.

These features are unavailable on Windows and say so rather than failing quietly:

- **Saved SSH hosts.** The bridge gives the `ssh` child a socket pair as its
  standard streams, which requires `OwnedFd`. Connecting to an SSH endpoint
  reports `SSH endpoints are not supported on this platform`.
- **In-app updates.** `release::target()` has no Windows updater asset (the
  release zip is for manual download only), so the updater
  stays disabled and reports that no standalone updater exists for this platform.
  Homebrew delegation is macOS-only regardless.
- **Saved GitHub credentials.** Neither the Keychain nor the private `0600` file
  exists here, so `GH_TOKEN` / `GITHUB_TOKEN` are the only sources of a token.
- **The avatar disk cache.** It depends on `openat`, `flock`, and POSIX
  ownership and mode checks, so avatars stay in memory for the process lifetime.

Starting a local `herdr server` looks for `herdr.exe` and uses
`CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW` in place of `process_group(0)`, so
the daemon survives the GUI and no console window appears.

Cross type-check it from a Mac or Linux machine with `just lint-windows`, which
targets `x86_64-pc-windows-gnu` because those hosts cannot supply the MSVC C
toolchain; CI lints the MSVC target on a Windows runner. `just lint-linux` is
the matching check for Linux, in the Ubuntu 24.04 container CI uses, because a
`cfg` gate that is wrong only on Linux is invisible from both macOS and the
Windows cross-check.

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
verified in Ubuntu 24.04, not native desktop rendering or input. Linux Cmd bindings mean
Super and can conflict with desktop shortcuts; global macOS menus are not
available. Tests cover wire colors,
cell modifiers, viewport bounds, semantic key selection, revision coherence,
creation request parameters, workspace-local tab cycling, wheel accumulation,
pane-relative hit testing, and popup routing.
Workspace-menu regressions check clicked-target schemas, stale boot/group
rejection, Unicode composition, and headless right-click/input routing.
`just test-sidebar` additionally exercises the native dialogs at narrow and wide
sizes, but does not validate OS IME candidate-window delivery or live daemon
worktree creation/close.
They do not replace an interactive smoke test against a live daemon.

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
See the root README for the full verification scope and remaining limitations.
