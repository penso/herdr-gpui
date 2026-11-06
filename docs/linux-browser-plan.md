# Plan: browser tabs on Linux

Hand this to an agent working on a Linux desktop. It describes where Linux
stands today, the target, and a phased path, each phase shippable and
verifiable on its own. Read `AGENTS.md` first; its rules apply throughout.

## Where Linux stands

- `crates/sheprd-gpui/Cargo.toml` pulls `wry` only for macOS and Windows:
  "wry has no working GPUI child-view backend on Linux yet".
  `browser/page.rs` lays a native view out where its group draws.
- `browser::EMBEDDED` is `false` on Linux. `control::open_browser` then opens
  web addresses in the system browser and refuses local files with
  `ErrorCode::Unsupported`.
- About 160 `cfg(any(target_os = "macos", windows))` gates across 26 files
  leave out the parts that need a page: `browser/native.rs` (web views),
  `browser/preview.rs` (the `herdr-preview` scheme serving local files and
  the kit), `browser/annotate_view*` (notes panel and delivery), page
  presentation in `browser/view/present.rs`, reload in
  `browser/view/requests.rs`, and the VS Code tab's page in `browser/code.rs`.
  `grep -rn 'target_os = "macos", windows' crates/sheprd-gpui/src` lists them.
- macOS only: popups adopted as tabs (`browser/popup.rs`), WebKit snapshots
  for note screenshots (`browser/snapshot.rs`), page keys
  (`browser/page_keys.rs`), and pages beneath the window's drawing
  (`browser/stacking.rs`). Windows does without these; Linux can too.
- What already works on Linux and must keep working: tab strips, editor
  groups and splits, the control socket, `browser feedback`, notes sent from
  mockups, and the agent skill.
- `browser open` never places a tab on Linux: `open_browser` returns before
  any window is asked when `EMBEDDED` is false. Placement beside the
  caller's terminal (`--beside`, and the default for local files) therefore
  does not work yet, and comes with phase 2.

## Target

On a Linux desktop, `sheprd browser open` shows web addresses and local
files inside the window, beside terminals, with annotations, `herdr.send`,
the explainer kit, and reload, matching Windows' feature set. Where the
display server cannot embed a page, the app says so plainly and falls back
to the system browser, never to something that looks embedded but is not.

## Constraints that shape the design

- GPUI on Linux draws its own X11 or Wayland window; it does not use GTK.
- wry on Linux is WebKitGTK. `WebViewBuilder::build_as_child` works only on
  X11 (it reparents a GTK window into the X11 parent) and needs GTK
  initialized on the thread that runs it, with GTK events pumped. Wayland has
  no child-surface embedding through wry.
- Every page is untrusted. Keep the existing rules: navigation limited by
  `native::navigable`, no downloads, local files served only from the page's
  own folder with dot files refused, `preview::from_the_web` blocking web
  origins, the bridge only on local pages, and nothing reaching an agent
  until the user presses Send.
- No blocking I/O on the UI thread; bounded queues; no new runtime.

## Phase 1: local pages in the system browser (X11 and Wayland)

Smallest change that makes local files, the kit, and `herdr.send` work on
every Linux desktop, while pages still open outside the window.

1. Serve local pages over loopback HTTP from the app: bind
   `127.0.0.1:0` on a background thread, reuse `location::requested_file`,
   `preview::read`, and `kit::asset` for the bytes, and route each page by an
   unguessable per-page token in the path (`/p/<token>/...`) that maps to its
   folder. Refuse requests whose `Host` is not the bound address (DNS
   rebinding) and keep `from_the_web` semantics for `Origin`/`Referer`.
2. `control::open_browser` on Linux: for `Location::Local`, open
   `http://127.0.0.1:<port>/p/<token>/<entry>` in the system browser and
   answer `OpenedIn::SystemBrowser`. Tokens live in memory only and expire
   when the app quits.
3. `herdr.send` without `window.ipc`: serve the bridge as `/.herdr/bridge.js`
   for these pages and have it `POST /p/<token>/.herdr/send` with the same
   JSON. The app parses it with `annotate::Report::parse`, accepts only
   `Report::Sent`, and keeps it for the agent as page notes. With no
   in-window panel to confirm in, require a confirmation step the page cannot
   fake: a GPUI toast or dialog in the window ("The page sent a result:
   <summary>. Send to agent?"). Do not deliver straight from the HTTP request.
4. Tests: unit-test token routing, `Host` checks, path traversal, dot files,
   and the size limits, then the end-to-end flow with a loopback client in a
   test. Update the skill's "When it does not work" section for Linux.

## Phase 2: embedded pages on X11 and XWayland

1. Add a Linux dependency on `wry` with its GTK feature set, and give
   `browser/page.rs`'s `Page` a Linux branch with the same
   `show`/`hide`/bounds behavior. Keep the versions in root
   `[workspace.dependencies]`.
2. Initialize GTK once on the main thread at startup when the window is X11
   (`gtk::init()`), and pump it from the GPUI foreground loop with a bounded
   `while gtk::events_pending() { gtk::main_iteration_do(false) }` per tick.
   Measure the tick cost in release (`just run`); it must not starve terminal
   rendering.
3. Get the X11 window handle from GPUI's `HasWindowHandle` and use
   `build_as_child`, as `native::Pages::ensure` does on macOS and Windows.
   Position and size the child where `browser/page.rs`'s `PageElement` lays
   it out, and keep it in step on resize, split drags, scrolling sidebars,
   and HiDPI scale changes.
4. Widen the cfg gates from `any(macos, windows)` to include
   `all(target_os = "linux", feature-or-runtime X11)`. Prefer a runtime check
   (`EMBEDDED` becomes a function of the window's backend) so one binary
   serves X11 and Wayland, and route a Wayland window to phase 1.
5. Bring over, in order: page creation and presentation, placement beside
   the caller's terminal (local files by default, `--beside`, `--tab`),
   `herdr-preview`
   custom protocol (WebKitGTK supports custom URI schemes), the bridge as an
   initialization script, title and load events, reload, annotation picker
   and notes panel, and then note screenshots with
   `webkit_web_view_get_snapshot` if it is practical.
6. Overlays: an X11 child window sits above GPUI, so menus and toasts must
   hide the pages they reach, as on Windows (`present_browser`'s cover
   logic, which only Windows builds include today). Hidden pages show a
   blank, as on Windows.

## Phase 3: Wayland

Investigate before committing, and report findings rather than guessing:
WPE WebKit or WebKitGTK offscreen rendering into a GPUI image (input
forwarding and IME are the hard parts), or a CEF off-screen renderer. Until
one works, Wayland keeps phase 1 and says so in the UI and the skill.

## Verification for every phase

- `just format`, `just ci`, `just lint-linux`, and `just lint-windows`; a
  Linux-only gate is proven only on Linux.
- Default and all-feature builds, `--locked`.
- Native QA on both an X11 session and a Wayland session, recording which
  one each check ran on: open a URL and a local file, `--beside`, reload,
  annotate with an element, a text selection, and a region, Send to an agent
  in a pane, `herdr.send` from `sheprd browser example` and from a
  variants page, a theme switch picked up by `/.herdr/theme.css`, menus and
  toasts over a page, splitting and closing groups with pages, HiDPI, and
  quitting with pages open.
- Never describe headless tests as native verification. Screenshots of the
  real window go in the PR, light and dark, plus a narrow window.
