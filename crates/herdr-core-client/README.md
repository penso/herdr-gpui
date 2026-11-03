# herdr-core client

A client of [herdr-core](https://github.com/herdrdev/herdr-core) (`hc`,
Herdr 2), the terminal runtime that keeps programs running and streams their
raw output to viewers. It speaks protocol generation 1: JSON-RPC on
channel 0 and framed byte streams on the others (`schema/README.md` in
herdr-core). No GPUI or async runtime.

- `Endpoint::resolve` finds a session's socket or pipe the way `hc` does.
- `Core::connect` joins a running core and never starts one; `start` runs
  `herdr-core server start` when the app decides to.
- `Core::attach` and `Core::subscribe` return streams whose reads give
  credit back as bytes are taken, so a slow view never blocks other calls.

- `CoreBackend` implements `herdr_backend::Backend`: it emulates attached
  terminals with `herdr-terminal`, so the app gets the same cells and sends
  the same semantic input as with Herdr 1.

Every call blocks: use it from a worker thread, never from the UI thread.
