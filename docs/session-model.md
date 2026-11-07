# Session model

Sheprd supports backends that disagree about what a session is. Herdr 1 owns
workspaces, tabs, panes, and agents. herdr-core owns terminals only. The goal
is one model the app reads whichever backend runs, so a feature stops depending
on Herdr 1's snapshot and a new backend costs an adapter rather than a rewrite.

## Where it stands

| Layer | Herdr 1 | herdr-core |
| --- | --- | --- |
| Terminals (`sheprd-backend`) | Not implemented; `sheprd-client` talks to the daemon | `CoreBackend` |
| Tab and pane layout | The daemon's snapshot | `sheprd_session::panes`, saved with the tab |
| Workspaces | The daemon's snapshot | One per window (`core_terminals::WORKSPACE`), only with no Herdr 1 installed |
| Agents | The daemon's snapshot | `TerminalInfo::agent`, per terminal |
| Commands | 43 `sheprd_client::Method` variants | Handled per feature in `core_terminals` |

`sheprd-session` holds what is already shared: the pane tree and the reader of
the app's saved tabs, which the remote control daemon used to parse as untyped
JSON. A test in the app reads what the app saves through that reader, so the two
cannot drift apart.

The app still reads `ClientShell*` types directly in about 67 files. That is the
work left.

## Moving the app onto it

Move one reader at a time, in separate pull requests, so parallel work conflicts
as little as possible:

1. Add `Session` to `sheprd-session`: workspaces, tabs, and agents as domain
   types, with `From<&ClientShellSnapshot>` in the app rather than a new
   dependency here. It is a model, not a mirror of the wire types: only what
   features read, and nothing the daemon's protocol alone needs.
2. Keep it next to `LiveState::snapshot`, built once when a snapshot lands, so
   render paths stay thin and revision fencing stays where it is.
3. Move readers that only list things first: the overview, then the sidebar's
   workspace rows, the tab strip, and the palette.
4. Move commands last. Each becomes an intent the backend carries out or
   refuses, so a backend without the capability says so instead of failing late.
5. Build a `Session` from herdr-core: several workspaces, saved by the app. This
   replaces `core_terminals::WORKSPACE` and is what the default flip in the
   Herdr 2 rollout waits on.

## Decisions

- Layout stays in the app's saved tabs rather than a core document. The app's
  core connection is local and the remote control daemon reads the same file,
  so a document would have no reader the file does not already serve. Revisit
  when a second client on another machine needs a core's layout.
- Sheprd's saved tabs win on the phone. A `herdr-app` document lists what they
  do not hold, so a terminal in both stays in Sheprd's tab and its split.
- No daemon of our own. Add one only for something that must run while the app
  is closed and that neither `doc.*` nor `sheprd-automate` can hold.
