# Terminal backends

What Sheprd needs from whatever runs its terminals, as two traits:

- `Backend`: list, watch, attach, start, and end terminals on one host.
- `Terminal`: one attached terminal: semantic input, size, focus, and the
  next surface updates.

`sheprd-core-client` implements them for Herdr 2 (herdr-core), whose raw bytes
`sheprd-terminal` emulates in the client. Herdr 1 does not implement them yet:
`sheprd-client` speaks to its daemon directly, and the app reads that daemon's
workspaces, tabs, and panes from its snapshot. `Generation` names both so a
Herdr 1 implementation can join without changing the traits.

Whichever runs a terminal, the app receives `sheprd_protocol` cells (`FrameData`,
`PaneSurfacePatchRow`) and sends `ClientPaneInputEvent`, so painting, hit
testing, and input routing do not depend on the generation.

Every method may block on I/O: call them from a worker thread, never from
the UI thread. This crate has no GPUI, runtime, or transport of its own.
