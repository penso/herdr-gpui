# Terminal backends

What Herdr UI needs from whatever runs its terminals, as two traits:

- `Backend`: list, watch, attach, start, and end terminals on one host.
- `Terminal`: one attached terminal: semantic input, size, focus, and the
  next surface updates.

Two generations implement them:

| Generation | Crate | Terminal content |
| --- | --- | --- |
| Herdr 1 | `herdr-client` | The daemon emulates and sends cells |
| Herdr 2 (herdr-core) | `herdr-core-client` | Raw bytes, emulated in the client by `herdr-terminal` |

Either way the app receives `herdr_protocol` cells (`FrameData`,
`PaneSurfacePatchRow`) and sends `ClientPaneInputEvent`, so painting, hit
testing, and input routing do not depend on the generation.

Every method may block on I/O: call them from a worker thread, never from
the UI thread. This crate has no GPUI, runtime, or transport of its own.
