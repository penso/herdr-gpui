---
name: herdr-mods
description: "Write Herdr UI mods: small sandboxed Luau extensions that add a status bar item, a sidebar section, a section of the right-hand mod panel, palette commands, or an app with its own activity bar icon to Herdr UI without changing its code, reloaded live when saved. Use it inside Herdr (HERDR_ENV=1) when the user asks to extend, customize, or add to Herdr UI's own interface: show a count, a list, a meter, or a button in the status bar, the sidebar, or a panel beside the terminals (like opencode's sidebar), add a palette command, add an activity bar app such as a dashboard of agents or workspaces, or write a mod, plugin, extension, or widget for Herdr UI. It covers the folder layout, the element tree, checking a mod with herdr-gpui mod check, and reading its errors and logs."
---
<!-- herdr-gpui-managed-skill v1: Herdr UI keeps this file up to date. Edit a copy under another name to keep your changes. -->

# Herdr UI mods

A mod is a folder of sandboxed Luau that Herdr UI runs and draws. Its `view`
returns a tree of plain tables (`ui.row`, `ui.badge`, `ui.button`, ...) that
the app draws in the user's theme. Mods cannot touch files, the network, or
processes; they read the app's state, keep their own, and run the app's own
commands when the user clicks. Saving a file reloads the mod.

## How to make one

1. Ask only what you cannot infer: what it shows, where (`status_bar`,
   `sidebar`, `panel` beside the terminals, or its own app in the activity
   bar), and what a click does.
2. Create `~/.config/herdr/gpui-mods/NAME/` (`$XDG_CONFIG_HOME/herdr/gpui-mods`
   when that is set; `%APPDATA%\herdr\gpui-mods` on Windows) with `mod.toml`
   and `main.luau`. The folder name must equal `name`.
3. Run `herdr-gpui mod check DIR` and fix every error it names, until it says
   the mod is valid. It runs the mod once without the app.
4. Run `herdr-gpui mod` to see that the app loaded it and drew it, with no
   error. For an app, `herdr-gpui mod open NAME` shows it. Edits reload within a second; `herdr-gpui mod reload NAME` forces
   it. `herdr-gpui mod logs NAME` shows what `herdr.log` wrote.
5. Tell the user where it appears. A mod is turned off with
   `herdr-gpui config set mods.disabled '["NAME"]'`.

## A mod

`mod.toml`:

```toml
name = "agents-waiting"   # lowercase letters, digits, hyphens
api = 1
places = ["status_bar"]   # "status_bar", "sidebar"
commands = [{ id = "names", title = "Show waiting agents' names" }]
```

`main.luau`:

```lua
local ui = require("@herdr/ui")

return herdr.mod({
  watch = { "agents" },
  init = function() return { names = false } end,
  update = function(state, msg)
    if msg == "toggle" or msg == "command:names" then
      state.names = not state.names
    end
    return state
  end,
  view = function(state, app)
    local blocked = app.agents.blocked
    local row = ui.row({ gap = "small", align = "center" })
    table.insert(row, ui.badge({ text = #blocked .. " waiting", tone = "attention" }))
    if state.names then
      local names = {}
      for _, agent in blocked do table.insert(names, agent.name) end
      table.insert(row, ui.text({ text = table.concat(names, ", "), truncate = true }))
    end
    table.insert(row, ui.button({ label = "Names", msg = "toggle" }))
    return row
  end,
})
```

A mod panel section uses `places = ["panel"]` and an optional `[panel]`
table with `title` and `order` (sections are ordered by it, then by name).
Its view may return `nil` to show no section; watch `focus` to describe the
focused workspace. `herdr-gpui mod panel show` shows the panel.

An activity bar app adds this to `mod.toml`, with `icon.svg` beside it (a
24x24 stroke icon drawn with `currentColor`, which the bar tints); its
`view` fills the window's body under the title:

```toml
[activity]
title = "Dashboard"
icon = "icon.svg"
```

## Reference

- `init()` returns the state (a table by default). `update(state, msg)` gets
  each message and returns the new state (nil keeps it), and optionally a
  list of effects: `herdr.run("COMMAND")` runs an app command, as the
  palette would (`herdr-gpui commands` lists them; it runs only after a
  click or palette pick, never from a timer), and `herdr.timer(seconds, msg)`
  sends `msg` once after 1 to 86400 seconds.
- `view(state, app)` returns the tree, or `nil` to draw nothing. It runs when a watched topic changes
  or `update` ran.
- A palette command declared in `mod.toml` sends `command:ID`.
- A message is nil, a boolean, a number, a string, or a table of those;
  never a function.
- `herdr.log(...)` writes to `herdr-gpui mod logs NAME`; `herdr.now()` is
  Unix seconds.
- `require("./parts")` loads `parts.luau` from the same folder.

`app` holds only the topics in `watch`, read-only:

| Topic | Value |
| --- | --- |
| `hosts` | `{ id, name, connected }` list |
| `workspaces` | `{ id, host, name, focused, tabs = { { id, name, focused } } }` list |
| `agents` | `{ all, idle, working, blocked, done }`, each a list of `{ id, host, workspace, name, status }` |
| `focus` | `{ host, workspace, tab, name, path }` of the focused workspace |

| Element | Keys |
| --- | --- |
| `row`, `column` | `gap` (`none`, `small`, `normal`, `large`), `align` (`stretch`, `start`, `center`, `end`), children in the list part |
| `list` | children, divided by lines |
| `text` | `text`, `tone`, `size` (`small`, `normal`, `large`), `weight` (`normal`, `bold`), `truncate` |
| `badge` | `text`, `tone` |
| `button` | `label` and/or `name` (an app icon); `command` (an app command) or `msg`; `tone`, `disabled` |
| `icon` | `name` (an app icon such as `terminal`, `chevron-right`), `tone` |
| `toggle` | `label`, `on`, `msg` |
| `progress`, `meter` | `value` from 0 to 1 (`progress` may omit it), `label` (meter), `tone` |
| `link` | `label`, `url` (`https://` only) |
| `tooltip` | `text`, one child |
| `spacer`, `divider` | none |

Tones: `normal`, `muted`, `accent`, `success`, `warning`, `danger`,
`attention` (the color of an agent waiting on the user). A bare string in a
container is text, and `ui.text("x")` works too.

## What mods cannot do

- No `os`, `io`, `print`, `load`, files, network, processes, or `require`
  beyond `@herdr/ui` and the mod's own `.luau` files.
- Unknown elements and keys are errors, reported with their place in the
  tree, such as `view: column[2].button: unknown key` followed by the key.
- Each call gets 2 million instruction checks and 16 MiB; a view has at most
  200 elements, nested 16 deep. A mod past a limit fails that call and keeps
  its last drawing.
- A broken save keeps the last working version drawn; `herdr-gpui mod`
  shows the error. Check the mod again after every change.
