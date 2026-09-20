# Herdr GPUI

[![CI](https://github.com/penso/herdr-gpui/actions/workflows/ci.yml/badge.svg)](https://github.com/penso/herdr-gpui/actions/workflows/ci.yml)

A native Rust/GPUI interface to local and saved SSH Herdr hosts. Workspaces and
worktrees are on the left, agents below them, and the active workspace's tabs
across the top. The center paints the daemon's terminal cells, including split
panes, without running another terminal emulator or wrapping the TUI.

The compact sidebar uses single-line labels, muted branches, status dots, and
independently scrolling spaces and agents sections, following Herdr's TUI.
Linked workspaces are nested beneath the main checkout using Herdr's repository
group metadata, not branch-name guesses. Parent arrows collapse/expand children
locally without closing sessions or changing the selected pane; a child without
an open parent stays visible at the top level. Tabs show titles without added numbers.

Space and agent indicators follow Herdr's activity semantics: filled yellow for
working, red for blocked, teal for an unseen completion, hollow green for idle,
and muted for unknown. Like the TUI, the GUI tracks completion acknowledgement
per client using state-change sequences and coherent surfaces. A new connection
starts with a seen baseline, so its dots can differ from a long-running TUI's
unread history. Activity is never guessed from terminal output.

The sidebar's `menu` opens an in-app popover with read-only settings information,
keybind help, GUI and daemon config reload, available-update information, and safe
detach/reconnect. Escape or clicking outside dismisses it; menu typing never
reaches the terminal. Update commands are displayed, not executed automatically.

This is an initial working macOS client, not complete TUI feature parity.
Herdr owns terminal processes and session state; closing this app only detaches.
The Herdr checkout does not need to be modified or linked into this build.

### Agent View Prototype

Right-click a tab name and choose **Agent** to add a native multiline composer
below its existing terminal output. **Terminal** restores the normal layout.
The choice is per tab, defaults to Terminal, and does not start or replace an agent.
Right-clicking an inactive tab does not focus it in Herdr.

- The composer names its recipient. For split tabs it targets the focused pane,
  with separate drafts for each pane; the existing split terminal view remains.
- Enter inserts a newline. **Cmd-Enter** or **Send** queues one semantic Paste +
  Enter batch through Herdr. Nothing is sent while editing the local draft.
- Popups and pending navigation suspend composer input. Stale/disconnected state
  blocks Send. A temporary surface update gap keeps typing in the composer.
- Click the output for direct terminal interaction, or use **Terminal mode** for
  approval prompts, menus, and other agent-specific controls. Terminal creation
  shortcuts are paused while editing; explicit UI buttons still work.
- Modes and drafts are isolated by host and live only in this GUI process.
  Same-boot reconnects preserve them; closing the GUI loses unsent drafts, not the daemon session. Deleted panes
  and changed daemon boots discard their local drafts. Nothing is auto-replayed.

**This is terminal input, not a chat API.** The agent's own prompt remains visible;
the native composer is supplemental. Start with an empty agent prompt:
Send does not replace text already typed in Herdr's TUI, detect approvals inside
an agent's terminal, or confirm that the agent accepted the input. On a shell,
Send submits shell input. Specific agents' multiline/paste behavior still needs
manual verification. The prototype editor has a 16 KiB limit, scrolls rather than
soft-wrapping long lines, and does not yet provide undo/redo or persistent history.

`just test-agent` runs daemon-free native fixtures. Headless tests also verify
recipient/IME isolation and a real socket worker sending to a mock Herdr peer;
these do not certify interoperability with individual agent products.

## Run

Install Rust/rustup and the macOS Xcode command-line tools. The repository pins
Rust 1.96.1 and GPUI 0.2.2. Install Herdr, then:

```sh
cargo run --locked --release -p herdr-gpui
# Or, with just installed:
just run
just run --session my-project
just run --socket /absolute/path/to/herdr-client.sock
```

`just run` uses the optimized release build for interactive performance. Use
`just run-debug` when debugging; unoptimized GPUI scene construction is notably
slower with a dense terminal on screen.

The explicit socket must be the binary **client** socket, not `herdr.sock`.
The app starts `herdr server` if the default or named-session local daemon is
absent, then waits up to 20 seconds without blocking the UI. A pulsing status
indicator and "Starting Herdr server..." message remain visible during startup.
Herdr must already be installed; discovery checks PATH and standard Homebrew,
Cargo, and `~/.local/bin` locations. If missing, an installation modal's **Install**
button opens [herdr.dev](https://herdr.dev/) without downloading or running an
installer. **QA > Show herdr non-detected modal** previews the warning without
disconnecting or changing detection. Explicit `--socket` and `--dev` targets
remain attach-only. The app never installs, stops, or upgrades your daemons.
Failed connections retry automatically; Terminal > Reconnect retries the selected
host immediately. Closing the app leaves daemon sessions running.

Use **Report issue** on the right of the status bar to open this repository's
GitHub issue forms. Redact secrets and private terminal content before submitting.

### Saved Hosts

Normal launches read Herdr's existing saved-host catalog at
`$XDG_STATE_HOME/herdr/client/endpoints.json` (default
`~/.local/state/herdr/client/endpoints.json`). `--dev` selects `herdr-dev` instead.
Manage this list with Herdr's `herdr machine` commands; GPUI reloads changes while
running. An explicit `--socket` launch stays isolated and does not load saved hosts.
Normal launches restore the choice in the adjacent `endpoint-selection.json`
once the saved host's snapshot is ready. Explicit host choices persist there;
other clients' later choices do not move this window's focus. Startup connection
delays and automatic fallback never overwrite the preference. Disabled/removed
choices fall back to Local (or a valid legacy catalog choice at startup).
`--socket` never reads or writes saved selection.

Spaces lists Local first, then collapsible saved-host groups in catalog order.
Enabled hosts connect in the background so their workspaces and agents stay
current. Selecting a remote workspace activates its terminal; input is held until
the destination surface is ready. Only the selected host receives terminal input.

SSH connects directly to each remote host, not through the local daemon. It uses
noninteractive SSH authentication and existing trusted host keys. Remote Herdr
must already be installed on a supported POSIX host; its `remote-client-bridge`
may start the named remote session. GPUI does not install remote software or
prompt for passwords/host trust. Configure and verify access with Herdr first.
Closing GPUI detaches all connections without stopping remote sessions.

New workspaces created through Herdr appear automatically while connected.
Revisioned snapshots are pushed by the daemon and applied by the GUI without a
manual refresh. The native integration test checks creation by a separate client,
including preservation of the GUI's current selection and connection. Observed
latency is tens of milliseconds locally, not an instant-delivery guarantee.

### macOS App Bundle

`cargo run` and `just run` use an embedded original Herdr Dock icon, with no runtime
asset paths or image-generation processes. To create a local Finder-launchable app:

```sh
just bundle
open target/release/Herdr.app
```

The bundle is named **Herdr** and contains only the release GUI executable,
`Info.plist`, and its native `.icns` icon. It starts an installed daemon if needed;
it does not bundle, install, or stop a daemon. This is a local unsigned,
unnotarized bundle, not a distribution/signing pipeline. Its version metadata lives
in `assets/macos/Info.plist` and should be updated for releases.

The original charcoal/blue connected-H artwork and provenance are in
[`assets/icons`](assets/icons/README.md). `just icons` regenerates the checked-in
PNG and ICNS from the SVG with macOS Swift/CoreGraphics and `iconutil`.

## GUI Configuration

Click **? Keybinds** at the bottom right of the status bar, or press `Cmd-/`,
to open the native shortcut reference. Press Escape, click outside the modal,
or use its close button to return to the terminal. Terminal input is blocked
while the modal is open. Its search field filters by action, section, or key
combination (for example `pane zoom` or `Cmd+Shift+P`).

`Cmd-,` opens Preferences with Appearance, Fonts, Configuration, and Connection
sections. The theme picker and GUI config reload are available directly from
Preferences; font values remain read-only and are edited in the config file.

Click **Theme** beside Keybinds to browse built-in themes and theme files discovered
in the Herdr and Ghostty theme folders. Type to filter names (case-insensitive),
use Up/Down to navigate, then press Enter or click a result to apply and save it.
The current theme is marked in the list. Escape or clicking outside cancels without
changing the theme. Saving updates only `theme` in the GUI config, preserving its
comments and other settings; load/save errors leave the current appearance intact.

GUI settings live in `$XDG_CONFIG_HOME/herdr/config-gpui.toml`, falling back to
`~/.config/herdr/config-gpui.toml`. The GUI creates a commented default file if it
is absent, without overwriting an existing file. These settings are independent
of the daemon configuration and apply equally when connecting with `--dev`.

See the complete [example config](crates/herdr-gpui/config-gpui.example.toml).
Omitted settings keep their defaults, including individual fields inside a font
section. Unknown keys, empty font families, and invalid sizes are errors.

```toml
theme = "Nord"

[terminal]
family = "Menlo"
size = 14
```

| Section | Default Font Family | Default Size |
| --- | --- | --- |
| `sidebar` | `Menlo` | 12 |
| `tabs` | `.SystemUIFont` | 14 |
| `terminal` | `Menlo` | 14 |
| `ui` | `.SystemUIFont` | 12 |

Sizes are **logical pixels**, not points or physical display pixels. Fractional
sizes are supported; values must be finite and between 8 and 48 inclusive. Line
height scales as `size * 20 / 14`. Fonts must be installed locally; none are bundled.
Restart the GUI or use its **GUI config reload** action after editing. Reloading
daemon config is separate and does not apply these appearance settings. There is
no automatic file watcher.

### Themes

Built-in names are case-sensitive: `Default`, `Nord`, `Dracula`,
`Catppuccin Mocha`, and `Catppuccin Latte`. `Default` preserves the original
terminal background, foreground, cursor, ANSI/256-color palette and sidebar
surface/active/muted colors. Other themes derive chrome colors by blending the
background and foreground. Built-ins have small hardcoded palettes, not bundled
third-party assets; entries 16 through 255 retain the conventional color cube and
grayscale ramp.

`theme` also accepts an absolute path, a `~/` path, or a Ghostty theme filename.
Built-in names take priority; use an explicit path to select a file with the same
name. Named files are searched in this order:

1. `$XDG_CONFIG_HOME/herdr/themes` (or `~/.config/herdr/themes`).
2. `$XDG_CONFIG_HOME/ghostty/themes` (or `~/.config/ghostty/themes`).
3. `$GHOSTTY_RESOURCES_DIR/themes`, when set.
4. `/Applications/Ghostty.app/Contents/Resources/ghostty/themes`.
5. `$XDG_DATA_HOME/ghostty/themes` (or `~/.local/share/ghostty/themes`).
6. `ghostty/themes` under each `$XDG_DATA_DIRS` entry (defaults to
   `/usr/local/share` and `/usr/share`).

Ghostty files support `background`, `foreground`, `cursor-color`, and
`palette = INDEX=COLOR` for indices 0 through 255. Colors must be exactly six hex
digits, optionally prefixed with `#`. Blank lines and full-line `#` comments are
allowed; inline comments and named colors are not supported for color values.
Malformed supported colors report the file and line number. Other settings are
ignored, including includes and commands: theme loading does not execute them.
Repeated colors use the last value. Unspecified colors retain defaults, except
that an omitted cursor color follows the theme foreground.

## Controls

| Control | Action |
| --- | --- |
| Sidebar workspace/agent | Focus its workspace or pane |
| Top tab / terminal pane | Focus the tab or pane |
| Cmd-N | New workspace using daemon directory policy |
| Cmd-T | New tab |
| Cmd-D / Cmd-Shift-D | Split right / below |
| Cmd-Shift-] / Cmd-Shift-[ | Next / previous tab |
| Cmd-1 through Cmd-9 | Focus the corresponding numbered tab in the current workspace |
| Cmd-Alt-Left/Right/Up/Down | Focus a pane in that direction |
| Cmd-Alt-] / Cmd-Alt-[ | Next / previous pane in the current tab |
| Cmd-Shift-Enter | Toggle focused pane zoom |
| Cmd-W / Cmd-Shift-W | Confirm closing the focused pane / tab |
| Cmd-P | Workspace picker |
| Cmd-Shift-P | Command palette: native actions and configured daemon entries |
| Cmd-B | Toggle the sidebar locally |
| Cmd-, | Settings |
| Cmd-/ | Native shortcut reference |
| Wheel / trackpad | Scroll the hovered terminal through Herdr |
| Cmd-V | Semantic paste |
| Cmd-Q | Quit the GUI, leaving terminals running |

The native File and Terminal menus expose the creation and navigation actions.
Terminal keyboard input and committed Unicode text go directly to Herdr's
semantic input protocol.

The command palette includes native actions (including unbound Themes and
Reconnect) and configured daemon command entries. Cmd-P opens the workspace
picker, not the command palette. Cmd-B only changes this client's sidebar
visibility; it does not change daemon state.

Closing a pane or tab requires confirmation because it can terminate running
processes. **Cancel is selected by default**: Enter alone cancels; press Tab then
Enter to select and confirm Close. Quitting the GUI remains a detach operation,
not a pane/tab close.

## Structure

- `crates/herdr-protocol`: locally maintained generation-1 wire types, fixtures,
  framing and surface validation. See its `NOTICE.md` for upstream provenance.
- `crates/herdr-client`: bounded background socket transport, capability
  negotiation, ordered commands, boot fencing, and full text-surface assembly.
- `crates/herdr-gpui`: native shell, live state, input routing and GPUI canvas.

Following Arbor's approach, dependency versions live at workspace level and
blocking I/O stays off the GPUI thread. This build uses registry GPUI, not Zed's
editor crates or a dependency on a sibling checkout.

## Verification

```sh
just ci
just build-release
just test-build
# Optional, requires an installed Herdr executable:
just test-live /opt/homebrew/bin/herdr
# Native GUI integration; opens a temporary window on the active desktop:
just test-gui /opt/homebrew/bin/herdr
# Native sidebar text/glyph regression, no daemon required:
just test-sidebar
# Native hover/scroll benchmark with a 30 ms p95 CPU scene budget:
just test-perf
```

The live test creates a private temporary HOME/config/socket environment,
starts its own daemon, tests terminal input/output, creation/navigation, resize,
and detach/reconnect, and cleans up only that daemon. It is ignored by default.
It has been exercised against Herdr 0.9.1. Normal tests use fixtures/mock peers
and require no running daemon.

The native GUI test dispatches real GPUI keybindings and text input, checks the
state consumed by the window, and fails on status-bar errors. It covers tab and
workspace creation/navigation, both split directions, shell output, native window
resize, and reconnect with fresh input. It verifies that the isolated daemon
survives GUI exit. The `integration-test` feature enables this harness only for
test builds; it is not enabled by `just run`.

This tests the native app and GPUI event routing, but does not replace screenshot
comparison or OS-level keyboard/IME delivery testing.

### Continuous Integration

GitHub Actions checks formatting, denies Clippy warnings, and runs the tests with
both default and all features. The tests include real executable CLI checks for
help, malformed arguments, conflicting options, and test-mode gating, with a
timeout to catch startup hangs. These checks do not open windows.

A headless GPUI layout regression also renders the actual sidebar with 40
workspaces and short/long agent labels. It checks shaped text, not just container
widths: short names must remain intact, long names must retain a readable prefix
and ellipsis, and the agents section must stay visible. This catches premature
text truncation that protocol and action-dispatch tests cannot detect.

`just test-sidebar` complements that mock-platform test with the actual macOS
font renderer and full application layout. It checks native glyphs and clipping
over 12 draws at four window sizes. This catches truncated font runs that the
mock text system does not model. It requires an active desktop, but uses only
fixture data and never connects to your daemon. Its process uses the shared
cleared-environment sandbox; the fixture catalog and synthetic hosts use unused
explicit socket targets, with polling stopped and no saved-state reads or writes.
On macOS, exact-view AppKit clicks verify host selection and return, disabled
selection, collapse without navigation or composition loss, agents remaining
visible, duplicate workspace/pane ID routing, and endpoint-scoped repository
collapse. A separate key window guards against accidentally targeting global
focus. Native glyph probes check long host/agent labels at 480px and 360px window
widths, plus wider/narrower sidebar preferences and restoration after truncation.
Host and agent glyphs also run with 16px/20px sidebar fonts and Nord, then restore
the default theme and 12px font.
Independent list offsets are checked through GPUI scroll handles and native
draws, not physical wheel/trackpad delivery. Routing checks stop at the queued
navigation target; they do not claim daemon acknowledgement or SSH coverage.
Menu keyboard isolation and outside dismissal remain covered. This is not a
screenshot/pixel comparison or OS-level IME test.

On macOS this also verifies that the running application's native Dock image is
valid and 1024x1024; a normal unit test checks the embedded PNG header/dimensions.

### Performance

`just test-perf` opens a daemon-free native fixture with a dense 160x50 terminal,
40 workspaces, and 40 agents. It dispatches real window-local mouse/scroll events
and measures cold frames, warm hover, both sidebar lists' scrolling, and terminal
updates. It checks forced redraws and retained scenes, verifies zero terminal
paint calls during warm sidebar interactions, and checks native glyph layouts,
popup removal, resize invalidation, and activity acknowledgement on retained draws.

Before the upstream blank-cell decoration fix was merged, retained scenes and
verified ASCII-run batching on the development M4 Max reduced
release hover p95 from 12.67 ms to 5.76 ms and single-cell updates from 12.14 ms to
7.09 ms in five interleaved comparisons against the previous per-cell algorithm.
Full-screen alternating-color output remained essentially unchanged. Run
`just compare-perf` for relative improvement/regression gates, or `just test-perf 50`
for a different absolute sidebar budget. These measure CPU scene construction,
not GPU completion or pointer-to-screen latency. Native tests remain opt-in.

See [PERFORMANCE.md](crates/herdr-gpui/PERFORMANCE.md) for the before/after results,
reference mode, workload, deterministic checks, and remaining limitations.

Separate Apple Silicon and Intel macOS jobs build optimized executables and run
the CLI tests against those release binaries. CI validates builds but does not
publish distributable binaries; packaging, dependency notices, signing, and
notarization are a separate release milestone.
GPUI compiles its Metal shaders at runtime, so CI does not need the separate
build-time Metal compiler download.

The live protocol and desktop GUI tests are deliberately ignored in hosted CI:
they require an explicitly selected Herdr binary, and the GUI test also needs an
active desktop. Run `just test-live` and `just test-gui` locally as shown above.

## Sidebar Width

Workspace titles show the GitHub organization or owner avatar, resolved from
the local repository's `origin` remote. Git lookups and avatar downloads run
in the background, with results shared per owner for the app session. The
GitHub mark is used while loading or when an avatar is unavailable. No GitHub
token is needed; avatar requests go to `avatars.githubusercontent.com`.
Saved-host workspaces use the GitHub fallback mark: their remote paths are never
looked up on the local filesystem.

Drag the sidebar's right edge to resize it; double-click the divider to restore
the default width. The terminal resizes automatically. Width is remembered per
local daemon socket in `$XDG_STATE_HOME/herdr/gpui/local-<socket-hash>.json`, defaulting
to `~/.local/state/herdr/gpui/`. These logical-pixel preferences are separate
from the TUI's column-based settings. Narrow windows temporarily limit the
displayed width without replacing your saved preference.
Saves run in the background and continue after window close while the app remains
alive. App exit does not wait for pending writes, so the latest change may be lost.
The width applies to all host groups in the window; narrow sidebars hide host
status text to leave room for labels.

## Next Milestones

- Selection/copy, hyperlink interaction, richer mouse support, and inline IME.
- Rename dialogs, workspace deletion, and full worktree/agent management.
- Editable settings and bundled fonts.
- Optimized terminal painting and graphics support.
- Signed macOS app packaging and broader remote-platform support.

Current rendering defaults to Menlo with configurable fonts and themes. Images and terminal
notifications/clipboard writes are deliberately not executed. See
[`crates/herdr-gpui/README.md`](crates/herdr-gpui/README.md) for the detailed scope.

## Releases

Tagged builds (`YYYYMMDD.NN`) publish a universal `Herdr.app` bundle, signed
with a Developer ID certificate and notarized by Apple, alongside per-architecture
executables and a CycloneDX SBOM. Every asset ships SHA256/SHA512 checksums, a
Sigstore keyless signature, and GitHub build provenance; detached GPG
signatures from the maintainer's key are added shortly after publication.

```sh
just verify-release --version 20260920.01 --checksums
```

See [SECURITY.md](SECURITY.md) for what each of those claims actually proves.

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE), plus the
[upstream protocol attribution](crates/herdr-protocol/NOTICE.md) for the
vendored parts of `herdr-protocol`.
