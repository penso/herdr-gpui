# Plan: Luau mods

People and their agents extend Herdr UI without a Rust change: a mod is a
small sandboxed Luau script that adds a status bar item, a sidebar section,
palette commands, a tab, or a whole app with its own icon in the activity
bar, drawn by the app in the user's theme and reloaded live when its files
change. Read `AGENTS.md` first; its rules apply
throughout.

The idea comes from herdr-app (`~/code/herdr-app`), where most of the window is
JavaScript mods. This plan keeps the idea and changes the engine and the
contract: Luau rather than JavaScript, a UI described as data rather than
GPUI calls, and capabilities a person approves rather than unrestricted
access.

## Decisions

| Question | Decision |
| --- | --- |
| Language | Luau through `mlua`, already in the workspace for automations. No JavaScript engine, no gpui-kit. |
| VM | The automation VM's setup (`herdr-automation/src/vm.rs`) moves to a shared `herdr-luau` crate. Each user picks its own policy for what globals are removed. |
| UI contract | A mod returns a tree of plain tables from a closed set of elements, checked into a typed `Node`. The app builds the GPUI elements. A mod never calls GPUI. |
| Events | Elm-style: `init`, `update(state, msg)`, `view(state, app)`. Buttons carry a catalog `command` or a `msg`, never a Lua function, so the tree is plain data that can leave the mod's thread. |
| Effects | Commands in the `herdr-commands` catalog, the same ones the palette and `herdr-gpui run` use. A destructive command still opens its own confirmation. |
| Threads | One worker thread per mod. The window renders the last good tree from a mailbox. Mod code never runs on the UI thread or during rendering. |
| Placement | Fixed places only: activity bar app, status bar item, sidebar section, and palette commands first; tab and toast next; sidebar row cells later. Mods add to the app; they do not replace built-in parts. |
| Activity bar apps | A mod may add one icon to the activity bar. Selecting it gives the mod the window's body, as Overview or Usage has, keeping the bar, title bar, and status bar. The icon can show a count and a busy pulse, as built-in sections do. |
| State | Read-only, built from the same data the window shows. Once the `herdr-backend` traits land, it is built from them, so one mod works on Herdr 1 and herdr-core. |
| Capabilities | A mod that only reads state and runs catalog commands needs no approval. A mod that declares `exec`, `network` or `read` is approved in the app's dialog, pinned to the hash of its manifest and sources, as automations are. *(Proposed; confirm.)* |
| Folder | `<config root>/herdr/gpui-mods/<name>/`, beside `config-gpui.toml`, so it cannot collide with folders Herdr itself may add under `herdr/`. *(Proposed; confirm.)* |
| Default UI | Stays Rust. Example mods live under `crates/herdr-gpui/mods/` for people to copy; the app installs none, so no item appears uninvited. |

## Why not herdr-app's model

| | herdr-app | This plan |
| --- | --- | --- |
| Engine | QuickJS-JIT through gpui-kit's `gpui-shell`, with patched vendored copies | Luau, already a dependency |
| UI API | GPUI's builders, nearly its whole surface | About 15 elements and theme tokens |
| Effects | `exec`, file read and write, `fetch`; no sandbox | Catalog commands; anything else declared and approved |
| Budgets | None | Memory, interrupt, node-count, and rate limits per mod |
| Runs on | The UI thread's executor | Its own worker thread |
| Scope | Most of the window | Fixed places alongside a Rust UI |

herdr-app's core is about 20k lines of Rust, so its window can be mostly mods.
Herdr UI's is far larger and carries paint budgets, so mods extend it rather
than replace it. A narrow element set is also a smaller contract to keep
compatible across releases.

## Architecture

```
mods folder ──notify──▶ loader ──▶ worker thread (one per mod, herdr-luau VM)
                                      │ ▲
                         Node tree ───┘ └── state snapshot, msgs, timer ticks
                             ▼
                       mailbox (last good)
                             ▼
              HerdrWindow: status bar · sidebar · palette · tab
                             │
                  click ─────┴──▶ msg → worker   or   command → catalog
```

### `crates/herdr-luau` (new, no GPUI)

The parts of `herdr-automation/src/vm.rs` that are not specific to
automations:

- `Limits` (interrupts, memory) and the interrupt `Meter`.
- `open(limits, policy)`: the safe standard libraries, the memory limit, the
  interrupt budget, and the removed globals.
- `seal`: freezes the libraries and the host's globals with `sandbox(true)`.
- `Policy`: the globals to remove. Automations keep their current strict list
  (no `pcall`, `print`, clock, or randomness, so a plan holds). Mods keep
  `pcall`, and get `herdr.log` and `herdr.now` from the host instead of
  `print` and `os`.
- Its own `Error` and `Result`.

`herdr-automation` depends on it, and its behavior and tests stay unchanged.
This is the first step and lands on its own.

### `crates/herdr-mods` (new, no GPUI)

- **`manifest`:** `mod.toml`, parsed with `serde`:
  ```toml
  name = "pr-badge"
  api = 1              # the mod API it was written for
  places = ["status_bar", "sidebar"]
  commands = [{ id = "refresh", title = "Refresh PR badge" }]

  [activity]           # optional: an icon in the activity bar and a full app
  title = "Deploys"
  icon = "icon.svg"    # monochrome, tinted by the theme

  [needs]              # optional; any entry requires approval
  exec = ["gh"]
  network = ["api.github.com"]
  read = ["~/notes"]
  ```
- **`source`:** reads `main.luau` and its `require`d siblings inside the
  mod's folder only, with size limits, and computes the hash an approval is
  pinned to (manifest plus every source file, in a fixed order).
- **`node`:** the typed `Node` enum and `TryFrom<mlua::Value>`. Elements:
  `row`, `column`, `text`, `icon`, `badge`, `button`, `list`, `progress`,
  `spacer`, `divider`, `link`, `toggle`, `meter`, `tooltip`, and later
  `terminal`. Apps also get `scroll`, `split` (resizable list and detail),
  `table`, `tabs`, `input`, `select`, `markdown`, `code` (read-only,
  highlighted), `empty` (an empty state), and `dialog`. Styles are tokens (`tone = "attention"`, `size = "small"`),
  never colors or pixel values. The checker enforces a maximum depth, node
  count, and string length, and refuses unknown keys, so a typo is reported
  instead of ignored.
- **`state`:** the read-only snapshot a mod sees: hosts, workspaces, tabs,
  agents with their status, the focused tab, usage, and the branch's pull
  request. Each field is bounded. A mod reads only what it subscribes to
  (`watch = { "agents", "usage" }` in `herdr.mod`), so it re-runs only when that part
  changes.
- **`runtime`:** loads a mod into a sealed VM and runs `init`, `update`, and
  `view` with fresh per-call interrupt budgets. It also enforces the rate
  limits: at most N views a second, and timers no faster than one second.
  Returns `Node` or a typed error with the Luau line.
- **`grant`:** what a mod is allowed to do, from its manifest and the stored
  approval. `exec` and `network` calls check the grant before running, and
  run on the worker, never on the UI thread.
- Its own `Error` and `Result`. The app's root `Error` gets one
  `Mods(#[from] herdr_mods::Error)` variant.

Only `herdr-luau`, `mlua`, `serde`, `toml`, `thiserror`, and `herdr-commands`
(for command IDs) are needed. There is no Tokio.

### `crates/herdr-gpui/src/mods/` (new module)

- **`Mods`:** one per window. It owns a scanner thread that lists the mods
  folder, one worker per mod, and their mailboxes. It offers the window's
  state at most every 250 ms when it changed, and takes results on the next
  tick. Workers are never joined on the UI thread.
- **`worker`:** each mod's thread checks its files' stamps (names, lengths,
  modification times) every second rather than through a `notify` watcher,
  which avoids a new dependency for at most 32 small folders. A save reloads
  the mod; a broken save keeps the last good tree and records the error. A
  catalog command from `update` runs only when a click or palette command
  sent the message; from a timer it is ignored and logged.
- **`render`:** `Node` to GPUI elements with the app's theme, fonts, and
  `crate::motion`. It does no Lua work and allocates nothing beyond what the
  tree needs.
- **`places`:** the status bar item (`window/status_bar.rs`), the sidebar
  section, and palette entries for each manifest command. Each place has a
  size budget shared by every mod there: status bar items fit in one area
  that shrinks before the app's own controls, and sidebar sections share one
  scrolling area of at most 40% of the sidebar, so the built-in lists keep
  the rest.
- **`approve`:** the approval dialog, listing the mod's `needs`, the files
  and hash, and the diff from the last approved version when there is one.
  Approvals are stored per host like automation approvals, with the same
  refusal to approve from a pane running an agent.
- **`error.rs`:** the module's `Error`.
- Mods appear in Settings with an on/off switch, their errors, and their
  approval state.

### Mod shape

```lua
--!strict
local ui = require("@herdr/ui")

return herdr.mod {
  watch = { "agents" },

  init = function()
    return { expanded = false }
  end,

  update = function(state, msg)
    if msg == "toggle" then
      state.expanded = not state.expanded
    end
    return state
  end,

  view = function(state, app)
    local blocked = app.agents.blocked
    return ui.row { gap = "small",
      ui.badge { text = tostring(#blocked), tone = "attention" },
      ui.button { label = "Review", command = "open_review" },
      ui.button { label = "More", msg = "toggle" },
    }
  end,
}
```

`view` gets a fresh state snapshot each time. `update` may return effects
(`herdr.run(command, args)`, `herdr.timer(seconds, msg)`, and with a grant,
`herdr.exec(...)` or `herdr.fetch(...)`, whose results come back as messages),
so every effect leaves the VM as data and returns as a message.

## Activity bar apps

A mod with an `[activity]` table becomes a full app: an icon in the activity
bar and the window's body when selected, beside Overview, Pull requests,
Usage, and the other built-in sections.

- **Section identity.** `ActivityPanel` in `herdr-control-protocol` is a closed
  enum of built-in sections and stays one. The window's current section
  becomes `Section::Builtin(ActivityPanel)` or `Section::Mod(ModId)`, so
  built-in code keeps matching on `ActivityPanel`. The control protocol gets
  a `Mod { name }` form for showing a section, which older clients never
  send.
- **Order and visibility.** Mod icons sit after the built-in ones, in a
  `[sidebar] activity_mods` order, and each can be hidden from Settings like
  built-in sections. A disabled, failing, or unapproved mod keeps its icon,
  greyed out, with its error or "Needs approval" in place of the app, so the
  icon does not move under the pointer.
- **Icon.** An SVG from the mod's folder, read on the worker thread. Its size
  is limited, it must not reference files or URLs, and it is drawn
  monochrome in the theme's icon color. A broken icon falls back to a
  generic one. The icon's `count` and `busy` come from the mod's `badge`
  function, which runs like `view` but only reads state.
- **Lifetime.** The app's VM keeps running while another section is shown, so
  its timers and state continue and returning finds it as it was. `view`
  runs only while the app is shown; `badge` runs while it is not.
- **Navigation.** An app keeps its own route in its state (`state.page =
  "detail"`), and `herdr.route(msg)` maps the window's Back and Forward to
  messages. `split`, `tabs`, and `list` keep selection and scroll position in
  the app, not in Lua, so scrolling never runs mod code.
- **Text input.** `input` and `select` are drawn natively, with IME,
  selection, and the usual keys. The app keeps the text and sends the mod
  `on_change` (debounced) and `on_submit` messages, so typing never waits for
  Lua.
- **Keys.** An app may declare keys in its manifest (`keys = [{ key = "r",
  msg = "refresh" }]`), active only while it is shown and focused. They never
  override the app's global shortcuts.
- **Storage.** `herdr.store` is a small key-value table per mod, saved under
  the app's state folder with a size limit, so an app keeps settings and
  caches across restarts without file access.
- **Commands.** `herdr-gpui activity show --mod NAME` opens an app, and its
  manifest `commands` appear in the palette as `Deploys: Refresh`.
- **Budgets.** An app's tree may be larger than a status bar item's (for
  example 5,000 nodes against 200), and `list` and `table` rows beyond what is
  visible are kept as data and drawn lazily by the renderer.

## For agents

- **Types:** the app writes `herdr.d.luau` (the API's Luau types) and a short
  `README.md` into the mods folder on start, so `luau-analyze` and an agent's
  editor can check a mod. These are generated, not hand-kept.
- **Commands** (`herdr-control-protocol`, `control/server`, `control/client`,
  `--help`, tests, `docs/cli.md`), all with `--json` and `--window`:
  - `herdr-gpui mod check DIR`: load and view the mod once against a fixture
    state, headless; nonzero exit with the Luau error and line.
  - `herdr-gpui mod list`: mods, places, enabled, approval state, last error.
  - `herdr-gpui mod reload NAME`
  - `herdr-gpui mod logs NAME`: the bounded `herdr.log` output.
  - `herdr-gpui mod approve NAME`: opens the app's approval dialog; it never
    approves by itself.
- **Skill:** a managed `herdr-mods` skill (`skills/herdr-mods/SKILL.md`),
  installed beside the browser and automations skills, on when to write a mod
  and how to check it. (Built with phase 3.)

## Phases

Each phase is its own pull request, with `just ci` passing.

1. **`herdr-luau`.** Move the VM setup out of `herdr-automation` behind a
   policy. Automations behave the same and their tests pass unchanged. New
   tests: each policy's removed globals, the memory limit, and the interrupt
   budget.
2. **`herdr-mods` core, no window.** Manifest, sources and hash, `Node` and
   its checker, state snapshot types, and the runtime. Tests: malformed and
   oversized trees, unknown keys, budget overruns, a `view` that errors,
   rate limits, `require` outside the folder, and hash changes on any file.
3. **First places.** The `mods/` module: host, workers, watcher, renderer,
   the status bar item, the sidebar section, and palette commands. `mod
   check`, `list`, `reload`, and `logs`. One example mod. (Built; Settings
   switches moved to phase 4, `[mods] disabled` covers them until then.) Headless GPUI tests for rendering and reload, and a worker
   test that a stuck mod cannot block the window. Native QA in light and dark
   themes and a narrow window.
4. **Mod panel.** (Built.) A `panel` place: sections in a resizable panel
   on the window's right beside the terminals, ordered by `[panel] order`,
   foldable, hidden when a view returns nil; Toggle Mod Panel and `mod
   panel`; `focus` carries the focused workspace's name and folder.
5. **Activity bar apps.** (Started: mod icons in the bar, tinted from the
   mod's SVG; the app body; `mod open`. The rest below remains.) `Section`, mod icons in the bar with count and
   busy, the app body, the app elements (`scroll`, `split`, `table`, `tabs`,
   `input`, `select`, `markdown`, `code`, `empty`, `dialog`), routes, keys,
   `herdr.store`, and `activity show --mod`. Tests: section switching and
   restore with a missing mod, an unapproved or failing mod's greyed icon,
   input that keeps the text while the worker is busy, lazy rows, and that a
   hidden app runs `badge` but not `view`. Native QA of focus, IME, and
   narrow windows. A shipped example app.
6. **Capabilities.** `needs`, the approval dialog, `mod approve`, `exec` and
   `fetch` through the grant, with hash pinning. Tests: an edited mod loses
   its approval, an ungranted call is refused with a typed error, and
   approving from an agent pane is refused.
7. **More places and agents.** The mod tab kind, toasts, `herdr.d.luau`
   generation, and the skill section.
8. **Later.** Sidebar row cells through `RowLayout`, the `terminal` element
   on herdr-core's observer terminals, and state from the `herdr-backend`
   traits.

## Open questions

- **Approval for pure-UI mods.** This plan skips approval for mods with no
  `needs`, which keeps live reload smooth while an agent edits one. The
  stricter alternative pins every mod.
- **Folder.** `herdr/gpui-mods/` is proposed. Herdr's own folder is the
  other option, if Herdr is not expected to use the name.
- **Replacing built-in parts.** Out of scope here. Revisit once mods exist in
  the fixed places, for example a mod that replaces the status bar's layout.
- **Remote hosts.** Mods run on this computer. A mod's `exec` with
  `on = host` would go over the app's existing SSH connection; that is left
  until phase 4 shows how grants should name hosts.
