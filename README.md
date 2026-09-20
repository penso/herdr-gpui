# Herdr GPUI

[![CI](https://github.com/penso/herdr-gpui/actions/workflows/ci.yml/badge.svg)](https://github.com/penso/herdr-gpui/actions/workflows/ci.yml)

A native Rust/GPUI interface to an existing local Herdr daemon. Workspaces and
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
keybind help, daemon config reload, available-update information, and safe
detach/reconnect. Escape or clicking outside dismisses it; menu typing never
reaches the terminal. Update commands are displayed, not executed automatically.

This is an initial working macOS client, not complete TUI feature parity.
Herdr owns terminal processes and session state; closing this app only detaches.
The Herdr checkout does not need to be modified or linked into this build.

## Run

Install Rust/rustup and the macOS Xcode command-line tools. The repository pins
Rust 1.96.1 and GPUI 0.2.2. Start Herdr normally, then:

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
The app never installs, starts, stops, or upgrades your personal daemon. A failed
connection appears in the compact single-row status bar with a red dot (green
when connected). Use Terminal > Reconnect after starting the daemon; there is no
permanent reconnect button.

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
`Info.plist`, and its native `.icns` icon. It connects to your existing daemon;
it does not bundle, install, start, or stop a daemon. This is a local unsigned,
unnotarized bundle, not a distribution/signing pipeline. Its version metadata lives
in `assets/macos/Info.plist` and should be updated for releases.

The original charcoal/blue connected-H artwork and provenance are in
[`assets/icons`](assets/icons/README.md). `just icons` regenerates the checked-in
PNG and ICNS from the SVG with macOS Swift/CoreGraphics and `iconutil`.

## Controls

| Control | Action |
| --- | --- |
| Sidebar workspace/agent | Focus its workspace or pane |
| Top tab / terminal pane | Focus the tab or pane |
| Cmd-N | New workspace using daemon directory policy |
| Cmd-T | New tab |
| Cmd-D / Cmd-Shift-D | Split right / below |
| Cmd-Shift-] / Cmd-Shift-[ | Next / previous tab |
| Wheel / trackpad | Scroll the hovered terminal through Herdr |
| Cmd-V | Semantic paste |
| Cmd-Q | Quit the GUI, leaving terminals running |

The native File and Terminal menus expose the creation and navigation actions.
Terminal keyboard input and committed Unicode text go directly to Herdr's
semantic input protocol.

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
fixture data and never connects to your daemon. It is not a screenshot/pixel
comparison test.

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

## Next Milestones

- Selection/copy, hyperlink interaction, richer mouse support, and inline IME.
- Rename/close dialogs and full worktree/agent management.
- Resizable sidebar, editable settings and bundled fonts.
- Automatic reconnect, optimized terminal painting and graphics support.
- Signed macOS app packaging, then SSH endpoints.

Current rendering uses Menlo and a fixed ANSI palette. Images and terminal
notifications/clipboard writes are deliberately not executed. See
[`crates/herdr-gpui/README.md`](crates/herdr-gpui/README.md) for the detailed scope.

## License

Apache-2.0. See [the license](crates/herdr-protocol/LICENSE-APACHE) and
[upstream protocol attribution](crates/herdr-protocol/NOTICE.md).
