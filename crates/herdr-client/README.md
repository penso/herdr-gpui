# Native Client API

Local, SSH, and WSL client for Herdr's stable generation 1 endpoint. Local
connections and SSH endpoints work on Unix and on Windows, and WSL
distributions are Windows-only. This crate
does not link Herdr, GPUI, ratatui, crossterm, Tokio, or a PTY implementation.
The workspace centralizes the pinned GPUI dependency (`gpui-pre` 0.3.6) for the GUI member.

```rust,no_run
use herdr_client::{connect, ClientEvent, ConnectOptions, ConnectTarget};
use herdr_client::protocol::ClientPaneInputEvent;

let client = connect(ConnectTarget::Local, ConnectOptions::default())?;
let handle = client.handle.clone();
// Run this receiver loop on a GUI background task, NOT the UI thread.
while let Ok(event) = client.events.recv() {
    match event {
        ClientEvent::Snapshot(snapshot) => {
            if let Some(pane) = &snapshot.focused_pane_id {
                handle.send_input(&snapshot.boot_id, pane,
                    vec![ClientPaneInputEvent::TextCommit("hello".into())])?;
            }
        }
        ClientEvent::Surface(surface) => { /* publish to GPUI via cx.update */ }
        ClientEvent::Disconnected { reason, .. } => break,
        _ => {}
    }
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Public Interface

```text
connect(ConnectTarget, ConnectOptions) -> Result<Client>
connect_with_surface_active(ConnectTarget, ConnectOptions, bool) -> Result<Client>
load_saved_hosts(development: bool) -> Result<Vec<SavedHost>>
store_saved_host(development: bool, host: &SavedHost) -> Result<SavedHost>
load_saved_host_selection(development: bool) -> Result<(Vec<SavedHost>, Option<String>)>
store_saved_host_selection(development: bool, selected: Option<&str>) -> Result<()>
SavedHost { pub id: String, pub label: String, pub target: String, pub session: String, pub enabled: bool }
Client { pub handle: ClientHandle, pub events: Receiver<ClientEvent> }
ConnectTarget::Local
ConnectTarget::Session { name: String, development: bool }
ConnectTarget::Socket(PathBuf)
ConnectTarget::Ssh { target: String, session: String }
ConnectTarget::socket_path(&self) -> Result<PathBuf>
Stream  // std::os::unix::net::UnixStream, or an overlapped named pipe on Windows
session_socket(config_dir: &Path, name: &str) -> Result<PathBuf>
list_local_sessions(development: bool) -> Result<Vec<LocalSession>>
list_remote_sessions(target: &str) -> Result<Vec<RemoteSession>>
delete_local_session(executable: &Path, name: &str) -> Result<()>
delete_remote_session(target: &str, name: &str) -> Result<()>
ConnectOptions { surface_size: ClientSurfaceSize, cell_width_px: u32, cell_height_px: u32 }
HostTheme { pub foreground: ClientHostColor, pub background: ClientHostColor, pub palette: [ClientHostColor; 256], pub appearance: ClientHostAppearance }
```

`store_saved_host` atomically appends a validated profile without connecting to
or provisioning the remote host. It preserves existing profiles and selection,
enforces the catalog's count and byte limits, and returns the existing profile
for an exact target/session duplicate. Callers check connectivity and resolve
SSH aliases before saving. Calls are serialized within this process; Herdr's
CLI does not participate in a shared catalog lock. Like the catalog loaders,
this is blocking worker-only I/O.

Session listing and deletion are blocking worker-only functions. Deletion
requires explicit user confirmation: it delegates stopping the named session
and deleting its saved state to the installed CLI. `default` is refused before
spawning. The client never removes session directories itself. It validates
names, bounds local stopping to 20 seconds and deletion to 15 seconds (45 seconds
for the complete SSH operation), discards remote diagnostics, and kills/reaps
only its CLI or SSH child on failure. A
timeout is an uncertain result: refresh rather than replay. Creating a session is the connection/startup layer's
responsibility, not an interactive `session attach` command.

`ClientHandle` is cloneable. Its exact methods are:

```text
send_input(&self, boot_id: &str, pane_id: &str, events: impl IntoIterator<Item = ClientPaneInputEvent>) -> Result<()>
send_popup_input(&self, boot_id: &str, terminal_id: &str, events: impl IntoIterator<Item = ClientPaneInputEvent>) -> Result<()>
reserve_clipboard_image(&self, boot_id: &str, target: ClientClipboardImageTarget) -> Result<ClipboardImageUpload>
reserve_clipboard_input(&self, boot_id: &str, target: ClientClipboardImageTarget) -> Result<ClipboardImageUpload>
resize(&self, boot_id: &str, options: ConnectOptions) -> Result<()>
set_host_theme(&self, boot_id: &str, theme: &HostTheme) -> Result<()>
set_focus(&self, boot_id: &str, focused: bool) -> Result<()>
set_surface_active(&self, boot_id: &str, active: bool) -> Result<String>
request(&self, boot_id: &str, method: &str, params: serde_json::Value) -> Result<String>
focus_pane(&self, boot_id: &str, pane_id: &str) -> Result<String>
focus_tab(&self, boot_id: &str, tab_id: &str) -> Result<String>
focus_workspace(&self, boot_id: &str, workspace_id: &str) -> Result<String>
disconnect(&self)
is_disconnected(&self) -> bool
```

`Result<T>` is `std::result::Result<T, Error>`. `SendError` re-exports `Error`
for existing callers naming the command error type. Queue failures are `Full`
and `Disconnected`; invalid commands expose `MissingBootId`, `EmptySurface`,
`GeometryLimit`, or `Protocol` with its original codec/validation source.
Sending never waits for
channel capacity; success means queued, not server acknowledgement. Request
methods return a unique ID. The worker rejects stale boot IDs, requests before
the first snapshot and unadvertised methods via
`CommandRejected`. Navigation uses the real `pane.focus`, `tab.focus`, and
`workspace.focus` API methods, not synthetic terminal keys.

`set_host_theme` reports the client's terminal colors and light/dark appearance
as `ClientShellHostTheme` updates on the ordered command path. Herdr answers OSC
10/11/4 color queries from pane applications with them and picks its per-mode
theme overrides from the appearance. Each connection starts unknown to the
daemon: the first call queues the appearance, both default colors, and the full
palette; later calls queue only what changed, and an unchanged theme queues
nothing. A reconnect is a new connection and reports everything again. Like
other commands it needs the snapshot boot ID, and a failed call leaves nothing
recorded so the next one starts over in full.

`remote_config_value` reads a single Git configuration key on a saved SSH host
with the same bounded, noninteractive process policy as `remote_origin_url`.
Both are blocking helpers for background workers; absent or empty keys return `None`.

All fallible client APIs return the crate-root `Error`, derived with `thiserror`.
I/O, protocol, and JSON failures retain their concrete sources; callers can match
variants or inspect `std::error::Error::source()` rather than parsing messages.
`Error::kind()` preserves transport retry/cancellation categories. Historical
partial-frame, handshake, and request deadlines remain `InvalidData`; health and
SSH discovery deadlines are `TimedOut`, cancellation is `Interrupted`.
Errors are not `Clone`, `PartialEq`, or `Eq`; match their typed variants instead.

`connect_with_connector` also returns `Result<Client>`. Its injected connector
still returns `io::Result<Stream>` because it is an actual I/O interface.
`Stream` is the crate's local endpoint type, re-exported at the root. It is
`std::os::unix::net::UnixStream` on Unix. On Windows it is a named pipe,
because that is what the Windows daemon binds: upstream maps the same socket
path string into the NPFS namespace, so discovery is unchanged. `interprocess`
opens the pipe, overlapped, and the stream takes its handle. A named pipe has
neither `SO_RCVTIMEO` nor `SO_SNDTIMEO`, so each direction keeps one overlapped
operation and waits for it only until the caller's deadline:

- A read starts into a 64 KiB buffer the stream owns. At the deadline it reports
  `TimedOut` and stays in flight, so whatever arrives meanwhile is what the next
  read returns: no byte is lost and no partial frame restarts.
- A write copies up to 64 KiB into an operation it starts and reports those
  bytes written, as a socket's send buffer would. The next write, or a flush,
  first waits for that operation, again only until its own deadline. One that
  passes leaves the operation in flight with its bytes, so a call that timed
  out wrote nothing, and the caller resumes at the same offset. A completion
  that moved fewer bytes than were reported fails the stream as `WriteZero`.
- Nothing is cancelled except by dropping the stream, which ends the
  connection: `CancelIoEx`, then a bounded wait for the kernel to finish with
  the buffer, on the worker thread. A peer that never reads cannot hold the
  worker or the drop.

These calls (`transport/overlapped.rs`) are the stream's only `unsafe`, because
`interprocess` and the standard library block in `WriteFile` and the asynchronous
wrappers need a runtime or a completion-port reactor. The crate also exports
`pipe_available`, one `PeekNamedPipe` call, for the app's control pipe, which
reads a synchronous pipe and has no other bounded way to. An SSH or WSL bridge's
stream is the same type over two private pipes the crate creates for the child's
stdin and stdout, overlapped on this end, with the same deadlines; the standard library's
`Stdio::piped` ends are not documented to be overlapped.
If adapting a typed error to that callback, use
`io::Error::new(error.kind(), error)`, not `error.to_string()`, to retain sources.

Catalog and selection failures carry `Error::Storage { operation, path, source }`.
`StorageOperation` distinguishes filesystem, encoding, decoding, and validation
steps; `Replace { destination }` retains both rename paths. The boxed source keeps
the typed failure and its original I/O/serde cause, and `kind()` delegates through
the context wrapper. Paths are diagnostic fields, not included in Display.
`CatalogSchema` and `SelectionSchema` intentionally redact JSON details from
Display while retaining the serde source, which may contain private field names.
Only final `ClientEvent::Disconnected` reasons are strings: disconnect text removes
controls and is capped at 1024 characters. `CommandRejected.reason` remains a typed
`Error` (`CommandBoot`, `UnsupportedMethod`, or `UnsupportedSurfaceInterest`) with
its original optional request ID. Convert it to display text only at the UI
boundary. Raw errors and source chains are diagnostic data, not automatically
safe UI text.

## Remote Clipboard Images

`reserve_clipboard_image` reserves a position in the normal FIFO immediately,
before asynchronous clipboard or file reads. It validates a nonempty boot ID,
connection cancellation, and a target ID of 1 through 1024 bytes (`Pane` or
`Popup`; `DirectTerminal` has no ID). The worker still checks the boot against
the ready snapshot. This is the existing Herdr TUI `ClipboardImage` wire message,
not a new API method, and does not acquire the API request lease.

For unknown native clipboard content, use `reserve_clipboard_input` instead. It
has the same validation, FIFO slot, preparation deadline, and cancellation API,
but does not claim the image lease until `complete(extension, data)`, immediately
before encoding. Text/key completion via `complete_input` never claims that lease,
so text can be reserved and published while an image is active without losing its
position before subsequent input. A competing image completion returns
`ClipboardImageBusy` and skips its slot; it does not retry or replay.
Both APIs use the same 64-entry command queue (plus one worker-held slot) and
allocate no payload buffer until completion. Callers must separately bound their
background acquisition tasks and data (the GUI limits preparation to four tasks).

```text
ClipboardImageUpload::complete(self, extension: &str, data: Vec<u8>) -> Result<()>
ClipboardImageUpload::complete_input(self, event: ClientPaneInputEvent) -> Result<()>
ClipboardImageUpload::cancellation_handle(&self) -> ClipboardImageCancellation
ClipboardImageUpload::is_cancelled(&self) -> bool
ClipboardImageCancellation::cancel(&self)
ClipboardImageCancellation::is_finished(&self) -> bool
```

The upload is an owned, sendable RAII permit, not cloneable. Dropping it skips
the slot. Only one image lease can be held per connection: the eager image API
claims it at reservation, the input API only on image completion. Another eager
image reservation or deferred image completion returns `Error::ClipboardImageBusy`. The atomic
lease remains held until both preparation and the worker slot are gone, so
repeated drop/cancel cannot accumulate image slots. The ordinary bounded command
queue still returns `Full`. A cancellation handle is cloneable and does not
retain that lease or keep the connection alive.
Retain it after publication until `is_finished()` returns true: only destruction
of the worker slot (sent, rejected, cancelled, or connection teardown) sets this
flag. Calling `cancel`, dropping the preparing permit, and successfully publishing
do not themselves mark it finished. Finished is not daemon acknowledgement.

GUI handoff: reserve on the UI thread at the paste/drop event, capture the
connection epoch, boot, and target, and perform bounded file/clipboard reads on
a background executor. Back on the UI thread, validate that captured identity
and target are still current. If valid, move the permit and bytes to a background
executor for `complete`; **encoding must not run on the UI thread**. Retain a
cancellation handle so a subsequent focus/host/epoch change can cancel encoding,
queued publication, or transmission. Check `is_cancelled` between bounded file
reads when practical. The client does not inspect GUI focus or read local files.
Completion checks cancellation before and after encoding; success only means
published to the worker, never daemon acknowledgement. As with ordinary input,
cancellation cannot retract bytes already fully transmitted to the daemon.

Completion accepts nonempty opaque bytes up to 16 MiB and case-insensitive
`png`, `jpg`/`jpeg`, `gif`, `webp`, or `bmp`; JPEG becomes `jpg`. Like the TUI,
this is extension/size validation, not image decoding or signature validation.
Invalid input returns `Error::Protocol` with `ClipboardImageSize`,
`ClipboardImageExtension`, or `ClipboardImageTarget`. Explicit upload
cancellation returns `ClipboardImageCancelled`; connection cancellation returns
`Disconnected`. Late completion after a worker rejection fails rather than
replaying on another connection.

If a candidate image path is missing, unreadable, oversized, or not a regular
file, call `complete_input(ClientPaneInputEvent::Paste(original_text))` instead of
dropping the permit and sending new input. If background native clipboard
acquisition finds no image for Ctrl+V, pass the **original captured semantic Key
event** instead. This preserves all key fields without synthesizing terminal bytes.
It encodes exactly one event for the captured pane/popup using the ordinary
2 MiB cap, in the same reserved slot, so
later input (including Enter) cannot overtake the fallback. It shares completion's
background-only encoding, GUI identity validation, and cancellation rules.
`DirectTerminal` returns `ClipboardImageInputTarget`: upstream's legacy `Input`
carries raw terminal bytes, and this client cannot infer the direct terminal's
keyboard or bracketed-paste mode. There is no paste-only completion wrapper.

Preparation has a 60-second deadline from reservation, including encoding.
`is_cancelled` reflects expiration, and late completion returns
`ClipboardImageCancelled`. An expired, still-empty slot is cancelled and skipped
by the worker, with `CommandRejected { reason: ClipboardImagePreparationTimeout,
request_id: None }`; ordinary input then resumes without replay. A retained
expired eager image permit still holds the single-image memory lease until dropped, but no
longer blocks ordinary commands. Already published data is not subject to the
preparation deadline; the write deadline starts separately on the first attempt.

While preparation is pending, the worker polls the slot without blocking, keeps
reading and probing health, and holds later commands in FIFO order. Image writes
use at most 64 KiB per attempt with a 10 ms socket write timeout, retaining exact
offsets over short writes/timeouts and yielding to reads after each attempt.
Each read turn drains one progressing frame for at most 128 reads (1 MiB) or
10 ms, stopping immediately on a read without progress. This avoids retrying a
blocked write before every 8 KiB of a large inbound frame, without starving
outbound progress or cancellation when a peer sends slowly or stops mid-frame.
An inbound partial frame fences state-dependent outbound commands. On Unix,
explicit input to the latest focused pane can proceed after a complete, bounded
raw surface identity header matches the authoritative snapshot and a validated
retained surface. This exception is limited to 4 KiB encoded commands, preserves
FIFO, and excludes popups, API requests, images, unknown headers, and changed
boot/projection identities. Already-readable body bytes are drained first;
strict full-body validation still controls surface publication and disconnection.
Such input is written in bounded steps, so a peer still sending its frame cannot
stall the worker: once attempted, it waits as the one held command for room in
the socket (60-second deadline), reads continue, and once on the wire it
finishes before any other bytes, including health probes.
Windows retains the full inbound barrier. An already-started outbound frame
continues in bounded chunks to avoid a full-duplex stall. No command
or health frame can interleave with a partial image. Health probes are deferred
during partial writes; the image has an absolute 60-second write deadline.
Dropping a pending permit or cancelling before any bytes are written skips the
slot. Cancellation after a partial write, a write deadline, or a boot change
closes the connection instead of leaving broken framing or resuming stale data.

Windows follows the same rules over its overlapped pipe, whose send timeout
behaves as the socket's does: an image write waits at most 10 ms per attempt,
and a timed-out attempt has written nothing.

`ClientEvent` variants:

```text
Connected(EndpointServerWelcome)
Snapshot(Arc<ClientShellSnapshot>)
Surface(Arc<PaneSurfaceFrame>)
SurfaceImages(Arc<SurfaceImages>)
Response { request_id: String, response: serde_json::Value }
CommandRejected { request_id: Option<String>, reason: Error }
Message(ServerMessage)
VersionMismatch(VersionMismatch)
Disconnected { reason: String }
```

`VersionMismatch` precedes the `Disconnected` of a connection refused because one
side must be updated: `DaemonOutdated` for a local daemon that closes after the
hello without any welcome (as releases before `MIN_HERDR_VERSION`, 0.9.0, do
when they cannot decode it), one that answers with a pre-endpoint welcome,
reports an older generation, or lacks a required
capability, or, over SSH or WSL, has no installed Herdr able to serve this client;
`ClientOutdated` for a newer generation. Each carries the daemon's reported
version, stripped of controls and capped at 64 characters, when it gave one.
`Error::version_mismatch` applies the same classification to an error. Other
failures, including an SSH host with no Herdr at all, are not mismatches.

The receiver is `crossbeam_channel::Receiver`, re-exported as `Receiver`.
Responses preserve either the endpoint's `{id,result}` or `{id,error}` object;
an API error is not a transport error. Optional unknown named controls are
ignored. Clipboard/title/notification events are data only: the client does
not execute escape sequences, read graphics file paths, or mutate the clipboard.

## Transport Rules

- One dedicated thread owns connect, handshake, ordered writes, and reads.
- 64 channel-queued commands plus one worker-held command, 8 ordered events,
  and one in-flight API request. A waiting request holds later commands in FIFO
  order until the preceding response is complete. The one exception is a
  `client_shell.surface.set` queued directly behind `pane.focus`, `tab.focus`, or
  `workspace.focus`: Herdr applies that focus synchronously and handles surface
  activation before its busy check, so the activation follows unacknowledged.
  Its response is delivered after the focus response. If a daemon refuses it as
  `endpoint_busy`, it is resent once after focus completes and the connection
  stops pipelining; focus is never resent. No capability promises this
  ordering, so only stable Herdr 0.9.0 through 0.9.5, whose sources were
  checked, pipeline it; other versions keep the serialized order. Events
  use backpressure rather than losing snapshots, input, patches, or responses.
- 2 MiB ordinary outbound payload cap; clipboard images alone allow 16 MiB of
  data plus 2048 bytes of bounded envelope overhead. 32 MiB inbound cap (semantic
  surfaces may include images), 8 MiB aggregate response assembly cap, strict
  full-payload decoding.
- 10-second handshake/initial-snapshot and partial-frame deadlines, 60-second
  request deadline (matching the upstream command lane);
  1-second socket write timeout; 10 ms read/cancellation polling. Partial reads
  survive polling timeouts without discarding any prefix or payload bytes.
- Geometry matches server bounds: nonzero dimensions, at most 4096 per axis,
  1,000,000 cells total, and cell pixel dimensions at most 4096.
- No reconnect or replay. A changed boot, regressing snapshot, malformed frame,
  mismatched patch, unsolicited response, or response overflow disconnects.
- Dropping the last handle or calling `disconnect()` cancels without joining
  on the GUI thread; dropping the event receiver stops delivery. Explicit
  cancellation closes the receiver without an extra Disconnected event.
- A full event queue intentionally pauses the worker, including writes. Drain
  it continuously in the GUI bridge. Do not clone receivers for broadcast:
  crossbeam receiver clones compete for events.

## Snapshot And Surface

Wire types are re-exported by `herdr_client::protocol` and defined in
`herdr-protocol`. They retain every gen1 bincode enum variant in source order.

- `ClientShellSnapshot`: `boot_id`, `revision`, focused workspace/tab/pane IDs,
  `workspaces`, `tabs`, `panes`, `agents`, `commands`, diagnostics and updates.
- `ClientShellWorkspace`: stable ID, active tab ID, number/label, branch,
  worktree, cwd, focus and agent status. Tabs and panes carry their parent IDs.
- `PaneSurfaceFrame`: `boot_id`, `projection_revision`, `surface_revision`,
  `frame`, pane geometry/scroll/mouse metadata, splits, optional popup, graphics.
- `FrameData`: row-major `cells` of exactly `width * height`, optional cursor,
  hyperlink URI table, and graphics bytes. Popup frames use the same model.
- `CellData`: grapheme `symbol`, packed `fg`/`bg`, `modifier`, `skip`, optional
  hyperlink index. Colors are NOT ARGB: `0..=16` are named colors (0 reset),
  `0x010000XX` is an indexed color, and `0x02RRGGBB` is RGB.
- `CursorState`: zero-based `x/y`, `visible`, DECSCUSR `shape` (0 through 6).
- `ClientPaneInputEvent`: semantic `Key`, `TextCommit`, `Mouse`, or `Paste`.
  Key includes code, modifier bits, kind, repeat count, shifted codepoint,
  generated text, release tracking, physical key ID and optional Windows record.
  Modifier bits: Shift=1, Control=2, Alt=4, Super=8, Hyper=16, Meta=32.

The handshake requests an active surface, with optional surface reuse/delta,
pixel mouse, direct graphics and server-owned keybindings disabled. Baseline
`PaneSurfacePatch` messages still exist in gen1: the client validates and applies
them atomically, then emits a complete immutable `Surface`, not a GUI patch.

Only surfaces matching the latest snapshot's projection revision are emitted.
Future surfaces (including subsequent patches) are retained and emitted after
their matching snapshot arrives.
The GUI keeps a matching snapshot/surface pair on screen while staging one newer
metadata-only snapshot. Input still targets that displayed pane or popup. A
matching surface commits the new pair and its image catalog together. Changes
to boot, focus, displayed pane topology, zoom, keybindings or commands invalidate
the pair immediately; navigation and activation retain their ordered barriers.
Snapshot metadata and cells are never mixed across revisions. Full surfaces can
skip surface revisions; patches cannot.

Popup ownership and geometry are carried by surfaces, not snapshot revisions.
As in the native TUI, an externally opened popup can therefore precede its
presentation: the daemon silently drops blocked pane input and input for retired
popup IDs. The GUI never redirects or replays that input into a new target.
GUI-initiated commands retain the surface barrier before accepting more input.

Herdr sends an image's bytes only in the first surface that places it, so the
worker, which sees every surface, keeps them: each received surface's
`graphics.assets` moves into a per-connection store and the emitted surface
carries placements and retained keys only. Whenever the stored set changes,
`SurfaceImages` (bytes by asset key, each with a process-unique serial) is
emitted before the `Surface` that needs it. The store keeps the keys the newest
received and newest emitted surfaces reference, so a surface waiting for its
snapshot cannot drop pixels from the one on screen. Assets are untrusted: bytes
must match their key's length and raw RGB/RGBA size, sides are at most
`MAX_IMAGE_SIDE`, and placements, images and total bytes are capped
(`MAX_PLACEMENTS`, `MAX_IMAGES`, `MAX_IMAGE_BYTES`). Over-limit or invalid
assets are dropped, not fatal. Direct graphics stay disabled and
`ServerMessage::GraphicsFile` paths are never read. Decoding and painting belong
to the GUI.

Optional delta codecs, local server spawning, discovery of all running sessions,
and automatic reconnect are out of scope. No existing Herdr server or session is modified or started by discovery.

## Saved SSH Hosts

`load_saved_hosts(false)` reads `$XDG_STATE_HOME/herdr/client/endpoints.json`,
falling back to `$HOME/.local/state/herdr/client/endpoints.json` (or the upstream
temporary `herdr-state` directory without HOME). `true` explicitly chooses
`herdr-dev`; build mode and socket overrides do not affect this selection.
The version-1 catalog uses upstream's strict fields, IDs, validation, 64-profile
and 64-KiB limits. Disabled profiles remain in the result. Missing files return
an empty list; malformed catalogs return an error, not a partial list. Selection
files are ignored by this profiles-only API, as in upstream's live-client
`load_profiles`. At startup, `load_saved_host_selection` additionally reads the
adjacent version-1 `endpoint-selection.json`: `None` means Local, otherwise the
value is an enabled profile's opaque ID. Missing, invalid or stale selection
retains the validated legacy catalog selection (normally Local). Refresh only
profiles thereafter so another client's selection cannot hijack the active UI.
`store_saved_host_selection` validates against the current catalog and writes
only selection using a create-new 0600 temporary file, file sync, atomic rename,
and parent-directory sync. Non-file/symlink destinations are refused; failures
are returned without modifying profiles. A directory-sync error can occur after
the new selection has become visible. Call all these APIs on background workers
and serialize a client's writes. Explicit-socket clients should call none of them.

`connect` remains active by default; `connect_with_surface_active(..., false)`
starts with an inactive hello. Inactive local connections and all SSH connections
require `surface_interest`, `presentation_effects_fence`, and the advertised
`client_shell.surface.set` method. SSH additionally requires `health_check`.
`set_surface_active` queues that API with `{"active": bool}` and returns its
request ID. Await the matching response and check its API error before treating
activation as complete. Unsupported or stale requests produce `CommandRejected`.

SSH runs entirely on the connection worker using the system `ssh`, existing
OpenSSH configuration/agent credentials, strict known-host verification, and no
password prompts, agent/X11 forwarding, port forwards, or persistent control
master. It discovers a POSIX remote binary on PATH (excluding mise shims), then
the upstream local-bin, Homebrew, and Nix roots. Each candidate is checked with
`status client --json` for generation and required capabilities before executing
`--session <session> remote-client-bridge`; advertised bridge idle timeouts are
enabled. Welcome negotiation validates the actual running daemon again.
Discovery has a 15-second deadline and bounded output; startup banners are
removed with upstream's output-ready marker. The bridge's SSH stderr is drained
on its own thread for the child's life, keeping only a 4 KB tail. When the bridge
closes before it is ready without naming an incompatible candidate (that is
`Error::BridgeIncompatible`), the tail and the exit code (255 from `ssh`, 127
when no Herdr was found) are reduced to an `SshFailure`: `HostKey`, `Auth`,
`Unreachable`, `HerdrMissing`, or `Other`. The text itself is never retained,
logged, or exposed, as it may name users, keys, and paths. The class is the
source of `Error::SshRefused` and travels on `ClientEvent::Disconnected { ssh }`;
`SshFailure::needs_user` marks the ones retrying cannot fix. Disconnect reasons
are sanitized and capped at 1024 characters.

The worker owns and kills/reaps its SSH child on every exit, including cancellation
and handshake failure; no GUI-thread join occurs. Quiet SSH connections send
`endpoint.health.ping.v1` after five seconds and expire ten seconds after an
unanswered probe. `ClientHandle::check_liveness` sends a probe at once instead,
or brings an outstanding one's deadline forward, and that probe expires after
three seconds; callers use it when a link may have died unnoticed, as across a
sleep. Local connections ignore it. Any complete inbound message satisfies a
probe, independently of the initial-snapshot deadline. As with local connections, continuously drain
events: event backpressure pauses transport processing, including health checks.

`PortForward::start(target, remote_port)` forwards a saved host's loopback port
to `127.0.0.1` and returns at once. A worker thread starts one `ssh -N` master
on a control socket in a fresh owner-only directory, waits for it to
authenticate, and asks it to listen with `ssh -O forward -L`: the preferred port
(`preferred_local_port`), then once more on a port the system picks. That
request succeeds only when this master itself binds the port, so a listener
another process took first is never reported. It reports
`ForwardEvent::Listening`, then `ForwardEvent::Ended` when the master exits,
every port is refused, or the master does not connect within 30 seconds.
`stop`, or dropping the handle, kills the master without waiting; the worker
reaps it and removes its directory. Nothing reconnects. Callers name the remote
port, so discovery stays elsewhere. Windows OpenSSH has no control master, so
there `PortForward::start` returns `Error::SshUnsupported`; `forward_command`,
which holds its forward in its own child, works on Windows too.

On Unix the bridge hands the `ssh` child one end of a socket pair as its
standard streams. On Windows it gives the child a private pipe each way,
driven with the same overlapped read and write deadlines as the daemon's named
pipe, so the handshake, its 16 KiB banner bound, cancellation, and the session
loop behave the same. `ssh` and `ssh-keygen` are
OpenSSH for Windows from `%SystemRoot%\System32\OpenSSH`, or without it the
first absolute `PATH` directory that has them (never Git's MSYS `ssh` ahead of
the system copy), started without a console window. Every
command there also passes `ControlPath=none`: Windows OpenSSH cannot share a
connection, and a configured `ControlPath` fails each one with exit 127, which
would read as a host without Herdr.

Limitations: there is no interactive bootstrap/install/upgrade,
version-specific mise install scanning, or retry/replay.
Shell-initialized PATH entries unavailable to `/bin/sh` are not discovered unless
covered by the known roots. The remote bridge itself can start the named daemon,
as upstream does; disconnect only detaches and never stops the remote daemon.

## Background Host Scripts

`run_script(ScriptHost, script, input, output, ScriptLimits, cancelled)` runs a
POSIX script locally or over a saved SSH target. `run_script_command(Command,
input, output, ScriptLimits, cancelled)` accepts an already-prepared command for
provider-specific transports. The caller supplies authentication, quotes the
remote command, and keeps temporary credential files alive through the call.
Both are blocking background-worker APIs for Linux, macOS, and Windows, where
the local host is refused since it has no `/bin/sh`; all have bounded
streaming output, an idle deadline, cancellation, and owned-child cleanup.
Provider preparation failures can retain their typed I/O cause in
`Error::ScriptConnect`; script execution retains the existing script error types.

## WSL Distributions

On Windows, `ConnectTarget::Wsl { distro, session }` attaches to Herdr inside a
WSL distribution. The worker runs `%SystemRoot%\System32\wsl.exe --distribution
<distro> --cd ~ --exec /bin/sh -c <script>` with the same discovery script,
compatibility checks, output-ready marker, and `remote-client-bridge` handshake
an SSH host uses, over the child's standard streams. `--exec` keeps `wsl.exe`
from expanding `$VARIABLES` in the script. The child has no console window
(`CREATE_NO_WINDOW`, as for every child built with `new_command`), starts in the system directory so a vanished WSL share
cannot be its working directory, and has its stderr discarded; the worker kills
and reaps it on every exit. As over SSH, the bridge starts the session's server
when it is down, and disconnecting never stops it.

`list_distros()` runs `wsl.exe --list --quiet` with `WSL_UTF8=1`, accepts UTF-16LE
output from builds that ignore it, drops Docker Desktop's internal
distributions and any line that is not a distribution name, and returns an empty
list when none is installed. `probe_distro` and `list_distro_sessions` run the
SSH probe and session-list scripts the same way; `delete_distro_session` the
session deletion. Every `wsl.exe` call is bounded, because a distribution
waiting on a first-run prompt makes `wsl.exe` print nothing and never exit.
`RemoteHost` names either kind of remote host and dispatches session listing
and deletion to the right one. Distribution names follow `wsl --import`'s rules
(`valid_distro`) and are validated before anything spawns. Other platforms
return `Error::WslUnsupported`.

Saved distributions live in `gpui-wsl.json` beside the endpoint catalog, never
in upstream's `endpoints.json` or `endpoint-selection.json`, whose strict schemas
cannot name one: `load_wsl_hosts`, `add_wsl_host`, `remove_wsl_host`, and
`store_wsl_selection` read and atomically rewrite that one file, one distribution
per entry, with the same 64-entry and 64-KiB limits.

## SSH File Transfers

```text
upload_files(target: &str, paths: &[PathBuf], cancelled: &AtomicBool,
             progress: impl FnMut(u64, u64)) -> Result<Vec<String>>
remove_uploaded_files(target: &str, paths: &[String]) -> Result<()>
```

This separate **blocking background-worker API** stages arbitrary regular files,
including multi-gigabyte ISOs, without involving Herdr or its clipboard protocol.
It never pastes, starts a daemon, or changes GUI state. Callers must capture and
revalidate their UI target/connection identity before using the returned paths.
Do not call it or join its worker on the UI thread. Progress callbacks execute on
that same worker and must return promptly; publish/coalesce UI updates there.

At most 256 paths are accepted (an empty batch is a no-op). Basenames must be
UTF-8 without control characters. Directories and special files are rejected;
symlinks to regular files are followed. Files are opened once with `O_NONBLOCK`
using the inherited workspace `rustix` safe open API (`NONBLOCK | CLOEXEC`),
and checked through descriptor metadata, so a FIFO or symlink-to-FIFO cannot
block the open. Regular disk/network filesystem operations can still block in
the OS: cancellation is checked around them, not by interrupting kernel I/O.
Open descriptors fix source identity, not contents; callers should avoid changing
files during transfer. Length changes are rejected, but same-size edits are not
detected. Lengths, progress, and checked aggregate size use `u64`. Memory is bounded
to a 64-KiB data chunk, bounded response lines, and at most 256 file/path records,
not the source size. Progress starts at `(0, total)` and counts bytes accepted by
the local SSH socket, **not remote acknowledgement or durable storage**.

Each file gets one dedicated SSH process using the connection bridge's shared
OpenSSH trust/authentication/forwarding policy. Linux/macOS clients are supported;
other clients return `UploadUnsupported`. Remote hosts need a POSIX `/bin/sh`
plus `mktemp`, `cat`, `wc`, `tr`, `rm`, and `rmdir`. Private directories are created
with `mktemp` under canonicalized `${TMPDIR:-/tmp}`, with `umask 077`, preserved
basenames, and exclusive file creation. A readiness handshake validates the
absolute staging directory before any file bytes are sent. A separate created
acknowledgement, including the exact destination path, must arrive after
exclusive file creation before the client streams payload or claims file cleanup
ownership. Before that acknowledgement, rollback only attempts `rmdir`; it never
removes a collided or unacknowledged file. The remote trap owns its created file
before emitting the acknowledgement, including if that response is lost.
EOF terminates each
receive; the remote checks the byte count and reports the exact final path.
Both that response and a successful SSH exit are required for success. Remote
stderr is discarded, not exposed or retained as potentially secret-bearing data.

Nonblocking sockets poll cancellation every 10 ms while stalled. A 30-second
**no-progress** timeout applies, not a whole-transfer deadline: active large
transfers can take arbitrarily long. Setting `cancelled` stops streaming and
kills/reaps only the owned SSH child. The remote trap deletes incomplete files on
EOF, error, or catchable signal. On any batch failure/cancellation, an explicit
cleanup SSH command removes only the validated owned files and empty staging
directories, including earlier successes; no partial path list is returned.
Cleanup intentionally runs despite cancellation, with the same no-progress bound.
Network failure or uncatchable remote termination can prevent cleanup; a failed
explicit rollback returns `UploadCleanup` retaining both the original typed error
and the cleanup error. An unreceived readiness frame can also leave an unknown
empty directory if the remote process cannot run its trap.

On success the returned strings are raw absolute paths, **not shell-quoted paste
text**. Quote each path for the destination shell before pasting it. Successful
files remain after SSH exits and after the GUI detaches. They are temporary,
user-owned files, not daemon-owned objects: OS temporary-directory cleanup or the
user may delete them. There is no automatic expiry, durable-backup guarantee, or
daemon garbage collection.

If success arrives after cancellation, a host switch, or target loss, call
`remove_uploaded_files` on a background worker with the **original upload SSH
target** and the unchanged returned paths. Never use the newly selected host.
This API has no cancellation flag: cleanup must still run after the upload's
cancellation flag is set. It reuses the explicit rollback implementation and
shared SSH trust policy, including its 30-second no-progress timeout. An empty
batch spawns nothing. Unsupported clients still return `UploadUnsupported`.

Before spawning SSH it validates the entire batch: at most 256 paths, at most
4096 bytes each, absolute POSIX paths without controls, empty components, `.` or
`..`, and exactly one basename beneath `herdr-upload.<12 ASCII alphanumeric>`.
Malformed paths return `UploadCleanupPath`. Only the exact files and empty
staging directories are removed; no recursive deletion occurs. Missing files
and directories are already clean. A nonempty or symlink-replaced staging
directory returns failure. Cleanup can partially succeed, so retrying the same
original paths is permitted. A symlink in the final filename is unlinked, not
followed. Path validation proves shape, not provenance: callers must never pass
arbitrary remote paths. The remote account and temporary-directory ancestors
must remain trusted; shell-based cleanup cannot eliminate concurrent path
replacement races by that account.

### One File In Place

```text
fetch_remote_file(file: RemoteFile, out: impl Write, max_bytes: u64,
                  cancelled: &AtomicBool) -> Result<Fetched>
push_remote_file(file: RemoteFile, expect: Expect, access: RemoteFileAccess,
                 len: u64, input: impl Read, cancelled: &AtomicBool) -> Result<()>
```

These copy one file at the host's own path off an SSH host and write it back,
for the GUI's remote file links. `RemoteFile` names the target, the host's
path as the host spells it (absolute, or `~/` for the login user's home), and
the `RemoteShell`: `Posix` runs `/bin/sh` scripts, and `PowerShell` runs
Windows PowerShell 5.1+ scripts through `-EncodedCommand` on Windows OpenSSH
hosts. The path travels quoted in the script; the file's bytes travel only on
stdout and stdin. Both are blocking background-worker APIs, on every client
OS, with the bridge's SSH trust policy and a 30-second no-progress timeout.

A fetch refuses a missing path, a folder or device, and a file over
`max_bytes` (`MAX_REMOTE_FILE_BYTES`, 64 MB, for the GUI) before streaming, and
returns the byte count and SHA-256 of what it wrote. A push writes a temporary
file beside the target and renames it over the target only when it holds
exactly `len` bytes and the host's file still meets `expect`: `Sha256` (the
file as fetched), `Absent`, or `Any`. A symlink or non-regular file is never
replaced. Another writer can still slip in between the check and the rename.

A push never lets anyone read the bytes who could not read the file they came
from, except as noted below. `access` says where a new file's access comes from: `Keep` (the login
umask's default on POSIX, the folder's inherited ACL on Windows) or
`Like(path)`, another host file's. An existing target always keeps its own.
On POSIX the temporary file is 0600 and takes the target's (or the `Like`
file's) group and mode before the rename. The target is written in place
instead when its group cannot be given and its group and other bits differ,
or when `ls` (`+`, or macOS's `@`) or `getfacl` shows an ACL on the target or
the temporary file. That keeps owner, group, ACL and hard links, but is not
atomic. A new `Like` file whose access cannot be copied (an ACL, a group that
cannot be given, a source that is not a plain file) stays 0600. On Windows
the temporary file is created with the target's (or the `Like` file's) DACL,
and `Replace` keeps it; a `Like` source whose DACL cannot be read, or that is
not a file, is refused.

Limitation: on a macOS host, an inheritable folder ACL entry can read the
temporary file while the upload is written, since macOS ACLs are not masked
by its 0600 mode. The final file is still right.

Refusals map to `RemoteFileError` variants (`Missing`, `NotAFile`,
`TooLarge`, `Symlink`, `Conflict`, `Incomplete`, `NoHasher`, and `Access`
when the file's access cannot be kept, before the host's file changes);
anything else is `Script`.

## Local Discovery

`Local` checks `HERDR_SOCKET_PATH` (derive `-client.sock` from its file stem),
then `HERDR_CLIENT_SOCKET_PATH`, then `HERDR_SESSION` (default: `default`).
Release config lives at `$XDG_CONFIG_HOME/herdr` or `$HOME/.config/herdr`.
An application launched without `XDG_CONFIG_HOME` can supply its login shell's
value with `set_login_config_home`; `local_var` reads the environment with that
fallback, and local discovery uses it.
An explicit `Session` bypasses socket overrides; `development: true` selects
`herdr-dev` regardless of this client's compilation profile. Named sessions use
`sessions/<name>/herdr-client.sock`; `default` uses `herdr-client.sock` directly.
`Socket` always means the binary client socket, not the JSON API socket.

## Verification

`cargo test -p herdr-protocol -p herdr-client` runs upstream JSON fixtures,
frozen bincode tags, independent positional source-shape comparisons, framing
and surface limits, and socket-pair/mock-server client tests. It does not build
or modify the sibling Herdr checkout. See `../herdr-protocol/NOTICE.md` and
`../herdr-protocol/LICENSE-APACHE` for upstream provenance and license.

### Opt-In Live Test

The ignored integration test requires an explicit absolute executable path:

```sh
HERDR_TEST_BINARY=/opt/homebrew/bin/herdr \
cargo test -p herdr-client --test live -- --ignored --nocapture
```

`HERDR_TEST_TMPDIR` is optional (defaults to the OS temporary directory); its
parent must exist and be short enough for Unix socket paths. The test creates
a private unique subdirectory using only `std`, with RAII cleanup. It launches
the selected binary's foreground `server` command with a cleared environment,
isolated HOME, XDG config/state/data/cache/runtime directories, explicit config
and socket paths, and a non-login `/bin/sh`. It never discovers an existing
daemon, invokes `server stop`, or signals anything except its own spawned child.
Cleanup kills and waits for that child and removes its temporary directory,
including on assertion failures.

The test exercises the actual stable endpoint welcome, matching snapshots and
surfaces, semantic text plus Enter (checking shell output rather than input
echo), tab/workspace creation and navigation, resize, and detach. After detach
it verifies the daemon is still alive and reconnects to the same boot ID with
the created workspace/tab state intact. Normal `cargo test` leaves it ignored;
explicitly running it without `HERDR_TEST_BINARY` fails rather than falling
back to a personal daemon. No root manifest or additional dependency is needed.
