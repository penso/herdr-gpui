# Sessions

What Sheprd keeps about a session that no terminal backend does, in one place
every client shares: the app draws it and the remote control daemon reads it.

- `panes`: the tree of terminals one tab splits into, and its geometry.
- `SavedTabs`: the app's saved tabs, read by anything that is not the app.

herdr-core keeps terminals only, so on Herdr 2 this is the session's layout.
`docs/session-model.md` says where the rest of the model is going. This crate
has no GPUI, runtime, or I/O.
