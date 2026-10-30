# Mods

A mod adds to Herdr UI without changing its code: a status bar item, a
sidebar section, a section of the panel on the window's right, palette
commands, or an app with its own icon in the activity bar. It is a folder of sandboxed Luau that
describes what to draw as plain tables; the app draws it in your theme.
Saving a file reloads the mod. The design and what comes next are in
[the plan](mods-plan.md).

## A mod's folder

```
<config>/herdr/gpui-mods/<name>/
  mod.toml     name, places, palette commands
  main.luau    the mod
  *.luau       more modules, loaded with require("./<file>")
```

`<config>` is `~/.config` on Linux and macOS (or `$XDG_CONFIG_HOME`) and
`%APPDATA%` on Windows. The folder's name must match `name`. Only `mod.toml`,
`*.luau`, and `*.svg` at the folder's top count; other files are ignored, and
subfolders and symbolic links are never read. A mod has at most 64 such
files, 256 KiB each and 1 MiB in all.

`crates/herdr-gpui/mods/agents` (a status bar item),
`crates/herdr-gpui/mods/workspace` (a mod panel section), and
`crates/herdr-gpui/mods/dashboard` (an activity bar app) are complete
examples: copy one into the mods folder to try it.

### `mod.toml`

```toml
name = "agents"          # lowercase letters, digits, and hyphens
api = 1                  # the mod API it was written for
places = ["status_bar"]  # "status_bar", "sidebar", "panel"
commands = [{ id = "names", title = "Show agent names in the status bar" }]
```

Each command appears in the palette as `agents: Show agent names...`, and
choosing it sends the mod the message `command:<id>`.

### A mod panel section

```toml
places = ["panel"]

[panel]
title = "Todo"    # the section's header; the mod's name when unset
order = 400       # sections are ordered by this, then by name
```

The mod panel is a column on the window's right, beside the terminals, as
opencode's sidebar sits beside a session. Each mod placed there gets a
section under its title; clicking the header folds it. A view that returns
`nil` has no section, so a mod can show itself only when it has something to
say. The panel shows while any mod has a section; Toggle Mod Panel in the
palette (`toggle_mod_panel`, bindable to a key) or `herdr-gpui mod panel
hide` hides it, and its left edge resizes it. Watch `focus` to describe the
focused workspace: its `name`, `path`, and IDs to match against `agents` and
`workspaces`.

### An activity bar app

```toml
[activity]
title = "Dashboard"
icon = "icon.svg"   # an SVG in the mod's folder
```

A mod with an `[activity]` table gets an icon after the activity bar's own
sections; picking it shows the mod's `view` across the window's body, under
its title, as Overview or Usage show theirs. Draw the icon with
`currentColor` (as a 24×24 stroke icon) and the bar tints it like its own.
`herdr-gpui mod open <name>` shows it from a script. Picking another section
leaves it, as does an app command the mod runs or a link it opens in a
browser tab, so what they open shows; the mod keeps running meanwhile. Many
app icons scroll in a short window, with Settings kept under them.

## `main.luau`

```lua
local ui = require("@herdr/ui")

return herdr.mod({
  watch = { "agents" },                 -- what the view reads
  init = function() return { names = false } end,
  update = function(state, msg)         -- clicks, palette commands, timers
    if msg == "toggle" then state.names = not state.names end
    return state                        -- nil keeps the state
  end,
  view = function(state, app)           -- what to draw
    return ui.row({ gap = "small",
      ui.badge({ text = tostring(#app.agents.blocked), tone = "attention" }),
      ui.button({ label = "Names", msg = "toggle" }),
    })
  end,
})
```

- `init` runs once and returns the mod's state, a table by default.
- `update(state, msg)` runs for each message and returns the new state.
  It may also return a list of effects as a second value:
  - `herdr.run("<command>")` runs a command from the app's catalog, as the
    palette would (`herdr-gpui commands` lists them). Only a click or a
    palette command may run one; from a timer it is ignored and logged.
  - `herdr.timer(seconds, msg)` sends `msg` once, after 1 to 86400 seconds.
- `view(state, app)` returns the tree to draw, or `nil` to draw nothing. It runs when a watched topic
  changes or `update` ran, at most ten times a second.
- `herdr.log(...)` writes a line that `herdr-gpui mod logs <name>` shows, and
  `herdr.now()` gives the time in Unix seconds.

A message is any plain value: nil, a boolean, a number, a string, or a table
of those. Functions cannot be messages, so a tree stays plain data.

### What a view reads

`app` holds only the topics in `watch`, read-only:

| Topic | Value |
| --- | --- |
| `hosts` | List of `{ id, name, connected }` |
| `workspaces` | List of `{ id, host, name, focused, tabs = { { id, name, focused } } }` |
| `agents` | `{ all, idle, working, blocked, done }`, each a list of `{ id, host, workspace, name, status }` |
| `focus` | `{ host, workspace, tab, name, path }`: the selected host's focused workspace, its name and folder |
| `usage`, `pull_request` | Not filled yet |

### Elements

Every element is `ui.<name>({ ... })`; children go in the list part of the
table. A bare string in a container is text, and `ui.text("x")` is too.
Unknown keys are refused, so a typo is reported where it is.

| Element | Keys |
| --- | --- |
| `row`, `column` | `gap` (`none`, `small`, `normal`, `large`), `align` (`stretch`, the default, `start`, `center`, `end`), children |
| `list` | children, divided by lines |
| `text` | `text`, `tone`, `size` (`small`, `normal`, `large`), `weight` (`normal`, `bold`), `truncate` |
| `badge` | `text`, `tone` |
| `button` | `label` and/or `name` (an app icon); `command` (a catalog command) or `msg`; `tone`, `disabled` |
| `icon` | `name` (an app icon, such as `chevron-right`), `tone` |
| `toggle` | `label`, `on`, `msg` |
| `progress` | `value` from 0 to 1, or none while unknown; `tone` |
| `meter` | `value` from 0 to 1, `label`, `tone` |
| `link` | `label`, `url` (`https://` only); opens in a browser tab |
| `tooltip` | `text`, one child |
| `spacer`, `divider` | none |

Tones are theme roles: `normal`, `muted`, `accent`, `success`, `warning`,
`danger`, and `attention` (the color of an agent waiting on you). Icons from
a mod's own `.svg` files (`file = "x.svg"`) are accepted but not drawn yet.

## Limits

Mods share their places with the app: every mod's status bar items fit in
one area of at most 360 px (280 px each) that gives way before the app's own
controls, and every mod's sidebar section shares one area of at most 40% of
the sidebar's height, which scrolls.

Mods run in a sandbox: no files, network, processes, `os`, `io`, `print`,
`load`, or `require` beyond the mod's own modules. Each call (loading,
`init`, `update`, `view`) gets 2 million instruction checks and 16 MiB of
memory; a view returns at most 200 elements, nested at most 16 deep.
A mod past a limit stops that call with an error and keeps running.

## Checking and debugging

```sh
herdr-gpui mod check ~/.config/herdr/gpui-mods/agents   # load and draw once
herdr-gpui mod                                          # what runs, any error
herdr-gpui mod logs agents
herdr-gpui mod reload agents
herdr-gpui mod open dashboard                           # show an app
```

A broken save keeps the last working version drawn and shows the error in
`herdr-gpui mod`. Turn mods off with `[mods] enabled = false`, or one with
`[mods] disabled = ["agents"]`, in `config-gpui.toml`.

## For agents

Herdr UI installs a `herdr-mods` skill beside its browser and automations
skills (once you agree), so agents in your panes know how to write a mod,
check it, and read its errors and logs. Its source is
[`skills/herdr-mods/SKILL.md`](../skills/herdr-mods/SKILL.md).
