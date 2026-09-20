# Herdr Native Shell

A macOS GPUI 0.2.2 client for a Local daemon and saved SSH hosts.
It starts an installed local `herdr server` when absent; explicit socket and
development targets remain attach-only. It does not link or install Herdr, stop
daemons, spawn a local PTY, or emulate a terminal. Herdr's remote bridge may start
the named remote session. SSH requires an installed POSIX Herdr, noninteractive authentication,
and an already trusted host key.
Runtime dependencies include GPUI, `herdr-client`, `serde_json` for API parameters,
`ureq` for background GitHub owner avatar downloads, `serde`/`toml` for GUI configuration,
and `unicode-segmentation` for grapheme-aware composer editing.

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
The status dot pulses amber during local daemon startup, is green when connected,
and red otherwise.

Spaces lists Local first, then saved hosts in the upstream catalog's order.
Enabled hosts connect in the background with inactive terminal surfaces; disabled
hosts remain visible. Host and repository collapse state is endpoint-scoped, and
Agents aggregates all connected endpoints with host labels. The catalog is read
through `herdr-client` every two seconds; changes to targets, sessions, enablement,
and ordering are reflected without restarting. Catalog errors preserve the last
valid list. The GUI never edits saved hosts or installs remote software.
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
in the status bar and do not undo the UI choice. Host editing remains in
`herdr machine`.

Switching revokes the old host's focus before releasing its surface, then resizes
and activates the selected host. Input waits for the activation acknowledgement
and a coherent surface at the current viewport size. Handoffs time out after five
seconds and return to Local; returning to Local never waits on a remote release.
Servers without surface-switching support remain usable as single targets.

Default and named-session startup discovers Herdr on PATH or in standard
Homebrew, Cargo, or `~/.local/bin` locations, then waits up to 20 seconds to
connect without blocking the UI. If Herdr cannot be found, an installation modal
offers an **Install** button that opens [herdr.dev](https://herdr.dev/); it never
downloads or runs an installer. After installing, choose Terminal > Reconnect.
**QA > Show herdr non-detected modal** previews the warning without restarting,
disconnecting, or changing daemon detection. Closing the GUI leaves the daemon
and its terminals running.

## Configuration

See [GUI configuration](../../README.md#gui-configuration) for the config path,
font defaults, theme lookup order, and reload behavior, and
[`config-gpui.example.toml`](config-gpui.example.toml) for a complete example.
Font sizes use logical pixels (finite 8..48), not typographic points. Restart the
GUI or invoke GUI config reload after edits; daemon config reload is separate.

The standalone `src/config.rs` module exposes `Config::load()` and
`Config::path()`, both returning errors as strings. `Config::theme()` resolves
built-ins or Ghostty files into a `Theme` with packed 24-bit RGB colors and all
256 palette entries. Theme resolution is a separate fallible step from loading
and validating TOML. Font sections can override either family or size without
repeating the other field. `FontConfig::line_height()` returns `size * 20 / 14`.
Config and theme I/O is synchronous; GUI callers should schedule it accordingly.

## Supported

- Workspace/worktree sidebar with main-checkout parents, indented linked
  workspaces, local collapse arrows, branch details, and daemon-driven
  filled/hollow activity indicators with client-local unseen-completion tracking.
- Resizable sidebar with width persisted per local daemon socket, shared across
  host groups. Local workspace titles show repository owner avatars; remote
  workspaces use the GitHub fallback mark without resolving remote paths locally.
- In-app sidebar menu for settings information, keybinds, config reload, update
  information, and detach/reconnect. Styled Preferences include Appearance,
  Fonts, Configuration, and Connection sections, with theme selection and GUI
  config reload; font values remain read-only and are edited in the config file.
- A searchable theme picker previews the available names from built-ins and
  Herdr/Ghostty theme folders. Selecting a theme applies and saves it while
  preserving other GUI config settings and comments.
- Title-only tabs, without an added tab number. Externally created workspaces
  arrive through pushed snapshots without manual refresh.
- Click workspace, tab, agent, or a visible split pane to focus through the API.
- Right-click a tab to select Terminal (default) or Agent presentation locally.
  Agent mode keeps the terminal surface and adds a multiline composer for the
  focused pane. Enter inserts a newline; Cmd-Enter/Send queues Paste + Enter in
  one input batch. Modes and pane-specific drafts survive same-boot reconnects
  in memory only; closing the GUI does not persist drafts or replay input.
- Native File/Terminal menus and creation buttons: **+ New Workspace** in the
  sidebar and a persistent **+** beside the horizontally scrolling tab strip.
- Cmd-N creates and focuses a workspace; Cmd-T creates and focuses a tab.
  Cmd-D splits the focused pane vertically (new pane on the right);
  Cmd-Shift-D splits horizontally (new pane below). Cmd-Shift-] / Cmd-Shift-[
  cycles next/previous tab within the current workspace, wrapping at the ends.
  These shortcuts are native actions, not bytes sent to a terminal.
- Cmd-1 through Cmd-9 focuses the corresponding numbered tab in the current
  workspace. Cmd-Alt-Left/Right/Up/Down focuses a pane in that direction;
  Cmd-Alt-] / Cmd-Alt-[ cycles next/previous pane within the current tab.
  Cmd-Shift-Enter toggles focused pane zoom.
- Cmd-W closes the focused pane and Cmd-Shift-W closes the focused tab only after
  a confirmation dialog. **Cancel is selected by default**: Enter alone cancels;
  Tab then Enter selects and confirms Close. Closing can terminate running
  processes, unlike quitting the GUI, which only detaches.
- Cmd-Shift-P opens the command palette with native actions and configured daemon
  command entries, including native Themes and Reconnect actions without dedicated
  shortcuts. Cmd-P opens the workspace picker instead.
- Cmd-B toggles sidebar visibility locally without changing daemon state.
  Cmd-, opens Settings; Cmd-/ opens the grouped native shortcut reference.
  Native shortcut labels and keycaps come from the shared `controls::COMMANDS`
  catalog, with Cmd-V semantic paste shown separately. Search filters by action,
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
- Cmd-V sends semantic Paste; Cmd-Q or window close detaches without killing
  the daemon or its terminals. Window activation is reported to the daemon.
- Resize uses the actual terminal canvas bounds and measured configured font cell width,
  excluding the native sidebar, tabs, status bar, and optional agent composer.

Socket I/O belongs to `herdr-client`'s worker. A separate event thread drains all
ordered events into a bounded latest-state cache. The UI samples changed state
at most once per 16 ms without blocking. Snapshots invalidate surfaces with a
different boot/projection revision; input waits for a coherent surface. Reconnect
replaces the cache, so late events from an old connection cannot affect the UI.

### Composer Safety

Composer drafts are keyed by endpoint, daemon boot, tab and pane identity, never by label.
Send revalidates the captured recipient against the authoritative inbox and
refuses active popups, navigation in progress, stale surfaces, or disconnection.
Queue failure preserves the draft. Queue success clears it and displays **queued,
not confirmed**; it is not an acknowledgement from the agent application.

The native editor and terminal have separate input handlers. Registration-time
session guards reject stale native callbacks after target/mode/focus changes;
submission events also carry an editor revision. A same-pane snapshot/surface gap
blocks sending but does not redirect typing or cancel local IME composition.
Navigation failures are correlated to the latest tracked GUI request so a rejected
request does not permanently suspend the composer.

Herdr still owns the terminal process and conversation, so its TUI can continue
the same session. The local composer cannot see or replace an agent application's
existing prompt buffer. Use an empty prompt and direct terminal mode for approvals
and menus. Agent mode also works on ordinary panes; sending to a shell submits
shell input. No agent-specific chat protocol or automatic prompt detection is used.

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

- macOS first; defaults to system Menlo and system font fallback, no bundled Nerd Font.
  Private-use icons may be missing. Fonts and palettes are configured locally,
  not synchronized from the host terminal's theme.
- No draggable scrollback UI, text selection/copy, mouse button/motion reporting, split dragging,
  hyperlink activation, image rendering, or animated blinking.
- No rename dialogs, workspace close/delete actions, horizontal wheel handling,
  server-owned keybindings, session picker, saved-host editing, or daemon
  stop/upgrade management.
- IME uses a minimal transient buffer, not a local editable terminal document;
  composition appears in the status bar rather than inline. Key releases and
  physical-key/extended keyboard protocol metadata are not reported.
- The optional composer supports local selection, clipboard operations, Unicode
  grapheme editing and inline IME. Its 16 KiB documents scroll without soft wrap;
  undo/redo and persistent drafts/history are not implemented. Terminal creation
  shortcuts are suppressed while it is focused; buttons remain available.
- Popups have a basic centered text presentation, without native title/border
  chrome. Server notifications/clipboard writes are not executed.
- Rendering is a simple two-pass cell painter, not an optimized damaged-row
  renderer. Large/high-frequency surfaces can consume significant CPU.

## Build And Test

```sh
cargo check -p herdr-gpui
cargo test -p herdr-gpui
cargo clippy -p herdr-gpui --all-targets -- -D warnings
cargo fmt -p herdr-gpui -- --check

# Native agent menu/editor fixtures at two sizes, no daemon or agent process.
just test-agent
```

Requires the normal macOS Rust/Xcode development environment. GPUI's
`runtime_shaders` feature compiles native Metal shaders at app launch, avoiding
the separate downloadable build-time Metal compiler. Tests cover wire colors,
cell modifiers, viewport bounds, semantic key selection, revision coherence,
creation request parameters, workspace-local tab cycling, wheel accumulation,
pane-relative hit testing, and popup routing.
They do not replace an interactive smoke test against a live daemon.
Agent-view tests additionally cover inactive-tab menus, pane-specific drafts,
boot/reconnect fencing, stale native callbacks, transient surface gaps, popup
blocking and atomic input batching with a mock peer. The native fixture uses
AppKit right-click delivery and GPUI keyboard dispatch; IME calls use the explicit
`EntityInputHandler` fallback, not an OS input-method automation driver. Clipboard
shortcuts have headless coverage; native clipboard checks are opt-in via
`HERDR_AGENT_TEST_CLIPBOARD` and restore GPUI-readable clipboard contents. The
default native test leaves the system clipboard untouched.
Actual pi/OpenCode/Claude/Codex workflows and switching between live TUI/GUI clients
remain manual QA; no personal daemon is touched by these fixtures.

`just test-sidebar` runs isolated, daemon-free native fixtures on the active
desktop. On macOS it checks exact-window clicks with a decoy key window, host
selection/disabled hosts, scoped collapse, duplicate-ID navigation routing,
composition preservation, menu isolation, and long-label native glyph clipping.
Scroll independence uses scroll handles and native draws, not trackpad events.
See the root README for the full verification scope and remaining limitations.
