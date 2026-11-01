# Plan: Herdr Remote for iOS

Turn `ios/` from a Live Activity companion into the main way to use remote
control on an iPhone or iPad: everything the web app does, natively, plus
what only a native app can do. Pairing works over Bluetooth when the phone
is near the computer, over the local network without a camera, or with the
QR code as today. Bluetooth also keeps working after pairing.

This supersedes "It does not duplicate the chat UI" in
`docs/remote-control-plan.md` (iOS Live Activities). The web app stays for
Android and for phones without the app. Both talk to the same daemon,
`herdr-remote-control`.

## Where things stand

| Piece | Today |
| --- | --- |
| `ios/` | About 1,600 lines of Swift. QR pairing, a Secure Enclave device key, pinned TLS, push token upload, and Live Activity layouts. Tapping anything opens the web app in Safari. iOS 17.2, Swift 5, iPhone only. |
| Daemon web app | Server-rendered HTML fragments (`/f/...`) plus an SSE stream of fragment patches. JSON is used only for commands, pairing, and push. A native client has nothing typed to read. |
| Typed data | `herdr-chat-protocol` already has serde JSON for `Transcript`, `Entry`, `Change`, and a shared reducer. The hub keeps a `Layout` (workspaces, chats, terminals) and per-chat `View`s. The fragments render from these. |
| APNs | `src/apns` signs tokens, decides start/update/end, and posts. It is not yet wired to `POST /api/apns` or to the notifier. |
| Moltis iOS (`~/code/moltis/apps/ios`) | XcodeGen plus `just` recipes. An actor-based WebSocket RPC with continuation maps and typed events. Capped backoff with readable connection diagnostics. Bonjour discovery with a TLS trust probe. A Live Activity "step stack" (current plus previous two steps, debounced). A tok/s footer, and tool names mapped to SF Symbols. No tests, no APNs, no markdown, no Observation, and not Swift 6. Borrow the first five ideas, not the gaps. |

## Decisions

Settled with the user on 2026-10-28: iOS 26 minimum; cache the last
state and open instantly; Bluetooth pairing on macOS only for now
(Linux and Windows followed in phase 8);
presence holds phone alerts by default once it is on; TestFlight under
the user's team, built and uploaded by CI.


- **iOS 26 minimum, Swift 6 language mode, complete strict concurrency.**
  This brings Liquid Glass, `@Observable`, on-device `SpeechAnalyzer`,
  interactive Live Activities, Controls, and current App Intents. It is a
  self-built or TestFlight developer tool, so the oldest iOS versions do not
  need to be reached.
- **iPhone and iPad** from the start (`TARGETED_DEVICE_FAMILY: "1,2"`). The
  iPad uses a three-column split view and works with a hardware keyboard.
  Apple Watch comes later.
- **One typed API, shared by contract.** The daemon gains a JSON model API
  and one WebSocket per computer. The Swift types mirror the Rust serde
  types. The JSON fixtures are checked by both the Rust and the Swift tests,
  as `ChatActivityState` already is. There is no code generator.
- **Port the change reducer to Swift**, so the app applies the same
  `Change`s the desktop and `herdr-acp` apply, against shared fixtures.
- **The trust model stays as it is.** The daemon's CA is pinned by
  fingerprint, the device key stays in the Secure Enclave, and a person
  approves on both screens. The native app needs no certificate profile and
  no Safari handoff.
- **Few dependencies**: Apple's `swift-markdown` (cmark-gfm) and nothing
  else. Everything else is first-party (CryptoKit, Network, CoreBluetooth,
  ActivityKit, AppIntents).

## Architecture

### Targets (XcodeGen, `ios/project.yml`)

| Target | What |
| --- | --- |
| `HerdrRemote` | The app |
| `HerdrRemoteWidgets` | Live Activities, Home and Lock Screen widgets, and Controls |
| `HerdrRemoteNotifications` | Notification Service Extension: decrypts pushes and attaches the agent icon and thumbnails |
| `HerdrRemoteShare` | Share extension: send text, a URL, or images to a chat or a new chat |
| `HerdrRemoteCore` (SPM) | Pure Swift, no UIKit: API types, the reducer, pairing and SAS crypto, address racing, ANSI runs, and the markdown model. `swift test` runs it on the Mac. |
| `HerdrRemoteKit` (SPM) | Shared by the app and its extensions: Keychain and Secure Enclave, the connection actor, session signing, and the App Intents |

One App Group and one Keychain access group are shared by all targets, so
the extensions can sign requests (an Allow from the lock screen) without
opening the app.

### State and concurrency

- One `actor Computer` per paired computer. It owns the address race, the
  session token, the WebSocket, its subscriptions, and reconnection with
  capped backoff (borrowed from Moltis). Each failure is turned into a typed
  diagnostic ("Herdr is not running", "TLS answered as another computer",
  "no route").
- The views read `@Observable @MainActor` stores (`HomeStore`, `ChatStore`,
  `TerminalStore`). They are fed from the actor through `AsyncStream`s with
  latest-value coalescing, never an unbounded queue. This is the same rule
  as Herdr UI's mailbox.
- Requests are correlated by ID through `CheckedContinuation` maps, with a
  bounded number in flight. Cancelling a request never replays it.

### Daemon: the native API (`crates/herdr-remote-control/src/native/`)

This is a new module beside `web/`. It reads the same hub and uses the same
`Device` bearer auth and limits.

- `GET /api/v2/home`: computers, workspaces, chats, terminals, and status
  marks, as serde types next to `hub::layout`. Rendering only becomes an
  HTML concern inside `web/`.
- `GET /api/v2/chat/{pane}`: the transcript snapshot (`herdr_chat_protocol::Transcript`)
  with its revision.
- `GET /api/v2/live` (WebSocket):
  - The client sends `subscribe {home}`, `{chat, pane, after_revision}`, or
    `{term, pane, size?}`, and their `unsubscribe`s, and `term_scroll
    {pane, lines}`.
  - The server sends `home` (whole value, coalesced at 400 ms),
    `chat_changes {pane, revision, changes: [Change]}` (coalesced at 120 ms),
    `term_screen {pane, rows: [[Run]]}` (only when it changed, 300 ms), and
    `reload {view}` when the client lagged past the bounded queue.
  - Commands keep the existing `POST` routes. The socket carries
    subscriptions and data, not writes, so one rate-limit and `Origin`
    policy still covers every write.
  - It is one socket per computer instead of one SSE stream per view.
    Switching views is then a message, not a new connection with a new TLS
    handshake. This is a concrete need, not a speculative transport: axum's
    `ws` feature, under the existing Tokio.
- Terminal screens are sent as styled runs `{text, fg, bg, bold, italic,
  underline, inverse}`. They come from the parser `web/render/ansi.rs`
  already has, which moves into a shared `ansi` module both renderers use.
  Swift draws runs and never parses escapes.
- `POST /api/v2/notify-key`: the device's notification key-agreement public
  key (see Notifications).
- Fixtures: `ios/HerdrRemoteCore/Tests/.../Fixtures/v2/*.json`, generated
  from the Rust types in a test and compared byte for byte, then decoded in
  Swift. A field added on one side without the other fails CI.

### The v2 wire, exactly

All JSON, `snake_case`. Chat types are `herdr_chat_protocol`'s own serde
(`Transcript`, `Change`, `Cursor`, `Status`, `Permission`); nothing is
mirrored. Bearer auth as today (`Authorization: Bearer TOKEN`).

`GET /api/v2/home` answers a `Home`:

```json
{
  "computer": "MacBook Pro",
  "fingerprint": "64 uppercase hex",
  "connected": true,
  "waiting": 1,
  "load": {"cpu": 47, "memory": 67},
  "workspaces": [{
    "id": "w1", "label": "herdr-gpui", "branch": "main", "cwd": "/Users/me/code/herdr-gpui",
    "worktree": {"key": "/Users/me/code/herdr-gpui/.git", "label": "herdr-gpui", "is_linked_worktree": false},
    "pull_request": {"number": 870, "url": "https://github.com/...", "tone": "success"},
    "chats": [{
      "pane": "p1", "tab": "t1", "title": "Equalize splits", "agent": "claude",
      "status": "working",
      "summary": {
        "connected": true, "status": {"state": "working"},
        "question": null, "questions": 0,
        "last_text": "Added Equalize Splits…", "turn_started_ms": 1761670000000,
        "usage_percent": 42
      }
    }],
    "terminals": [{
      "tab": "t2", "label": "zsh", "cwd": "/Users/me/code/herdr-gpui", "zoomed": false,
      "panes": [{"pane": "p2", "label": null, "cwd": "/Users/me/code/herdr-gpui", "focused": true}],
      "agent": {"name": "claude", "status": "done", "pane": "p2", "title": "Fix the build"}
    }]
  }]
}
```

- `status` and `agent.status` are Herdr's agent status: `unknown`, `idle`,
  `working`, `blocked`, or `done`. `summary` is absent while the chat's
  connection is not linked yet. `question` is a `Permission`. `waiting` is
  the badge: chats whose transcript has a permission waiting.
- A terminal's `agent` is the agent Herdr detects in the tab, such as
  `claude` run in a shell: `pane` is the pane it runs in and `title` its
  terminal title, both absent from older daemons. Home lists these tabs
  with the chats, by status, and opens the agent's pane.
- `load` is the computer's CPU and memory in whole percentages, read
  every five seconds; it is absent until the first reading and from older
  daemons. A home goes out again when either figure moves by a point.
- `worktree` is Herdr's `ClientShellWorktree`: every checkout of one
  repository shares `key`, and `label` is the repository's name. The
  Workspaces tab lists a repository's own checkouts, then its linked
  worktrees, under that name. It is absent for a folder outside a
  repository and from older daemons.
- `pull_request` is absent when the workspace has none; `tone` is
  `herdr_github::pull_request::Tone` as serde writes it.
- As built: `waiting` counts every waiting permission request, as the web
  app's badge does; a chat's `cursor` is `null` until the daemon has heard
  from its program once; runs also carry `strike` and `hidden`; a device
  holding eight streams and sockets is answered 429.

`GET /api/v2/chat/{pane}` answers a `chat_snapshot` message (below)
without its `type`.

`GET /api/v2/search?q=TEXT` searches what was said in every chat the
daemon follows: user and agent messages, ignoring case and runs of
whitespace. The query is 2 to 100 characters, or the answer is 400. It
answers at most three matches per chat, newest first, and sixty in all;
`truncated` says more matched. A daemon without it answers 404, and the
phone searches its own index of the chats it has cached instead.

```json
{"hits": [{"pane": "p1", "entry": 42, "author": "agent",
           "snippet": "…the words around the match on one line"}],
 "truncated": false}
```

`GET /api/v2/live` upgrades to a WebSocket. Text frames only, one JSON
object each, at most 4 KiB from the phone; binary frames close the socket.

The phone sends:

```json
{"type": "subscribe", "view": "home"}
{"type": "subscribe", "view": "chat", "pane": "p1", "since": {"epoch": "uuid", "seq": 41}}
{"type": "subscribe", "view": "term", "pane": "p2", "size": {"cols": 46, "rows": 30}}
{"type": "unsubscribe", "view": "chat", "pane": "p1"}
{"type": "term_scroll", "pane": "p2", "lines": 3}
```

`since` is optional: the cursor of the transcript the phone has cached.

`size` is optional and only read for `term`: the columns and rows the
phone draws. While the view runs, the daemon holds a direct terminal
attach to Herdr at that size (without taking the terminal from another
attached client), so Herdr resizes the pane's terminal to the phone and
`term_screen` comes back at that width; Herdr UI shows the pane at the
smaller size meanwhile. Unsubscribing, closing the socket, or the view
ending detaches, and Herdr sizes the terminal back to its layout.
Subscribing to the same terminal again with another `size` (a rotation)
resizes the same attach and sends the screen again; again without `size`
releases it. `cols` must be 20 to 400 and `rows` 5 to 200: another size
answers `error` "terminal size out of range" and follows at the layout's
size. A Herdr that refuses the attach (another client holds the terminal,
or an older Herdr) leaves the view at the layout's size, logged by the
daemon and never an error to the phone.

`term_scroll` scrolls a terminal the phone follows with `size`: `lines`
is a signed integer, positive up toward history and negative down. The
daemon sends it over the held attach as one mouse wheel scroll at the
middle of the terminal, so Herdr hands it to an app that reports the
mouse (`less`, `vim`, an agent's TUI) or scrolls the terminal's own
history otherwise, and reads the screen again at once and shortly after,
as after typing. `lines` of 0 answers `error` "scroll out of range"; more
than 50 either way moves 50. A terminal the view follows without an
attach (no `size`, or Herdr refused it) answers `error` "this terminal
cannot scroll from the phone" once per subscription and ignores later
scrolls; scrolling a terminal the socket does not follow answers it each
time.

The daemon sends:

```json
{"type": "home", "home": {}}
{"type": "chat_snapshot", "pane": "p1", "cursor": {"epoch": "uuid", "seq": 41}, "connected": true, "transcript": {}}
{"type": "chat_resumed", "pane": "p1", "cursor": {"epoch": "uuid", "seq": 41}, "connected": true}
{"type": "chat_changes", "pane": "p1", "changes": [{"cursor": {"epoch": "uuid", "seq": 42}, "change": {"change": "append", "id": 7, "text": "…"}}]}
{"type": "chat_connection", "pane": "p1", "connected": false}
{"type": "term_screen", "pane": "p2", "columns": 120, "rows": [[{"text": "PASS", "fg": "green", "bold": true}]]}
{"type": "reload", "view": "chat", "pane": "p1"}
{"type": "error", "message": "no such chat"}
{"type": "bye", "reason": "revoked"}
```

- A subscription answers at once with the current state: `home`; for a
  chat, `chat_resumed` when `since` equals the chat's cursor, otherwise
  `chat_snapshot`; for a terminal, `term_screen`.
- `home` is sent whole after each burst of changes, gathered 400 ms, and
  never for streamed text alone. `chat_changes` gathers 120 ms. Each
  change carries the cursor it leads to. A phone that sees a gap (another
  epoch, or a `seq` that is not one more) subscribes again with its
  cursor.
- `term_screen` is sent only when the screen changed, read every 300 ms
  while subscribed, and at once, 50 ms, and 150 ms after the pane takes
  keys, a paste, or a picture. A run has `text` and, when set, `fg` and
  `bg` (one of the 16 names `black`, `red`, `green`, `yellow`, `blue`,
  `magenta`, `cyan`, `white`, and their `bright_` forms, or `#rrggbb`),
  `bold`, `dim`, `italic`, `underline`, `inverse` (only when true).
- `reload` means the phone lagged or the view went away: it fetches the
  view again (a chat that is gone answers 404). `bye` precedes the close
  when the session expired (`expired`) or the device was revoked
  (`revoked`).
- Bounds: eight subscriptions per socket (one home, the rest chats or
  terminals), 64 messages queued for a slow phone before `reload`, the
  existing per-device stream cap, and a ping every 20 s.

New workspaces and chats (Phase 3) have their own JSON routes, so the
app learns the pane it should open:

- `GET /api/v2/new` answers what the forms offer: `{"agents": [{"id":
  "claude", "name": "Claude"}], "recent": ["/Users/me/code/herdr-gpui"],
  "roots": [{"path": "/Users/me/code", "folders": ["/Users/me/code/a"]}]}`.
  Agents come in Herdr's order; `id` is what a request names.
- `POST /api/v2/workspaces` takes `{"cwd", "agent", "branch"?, "prompt"?}`
  and answers `{"workspace": "w3", "pane": "p7", "prompted": true}`. The
  folder must be one `new` offers, and `branch`, when not blank, opens a
  worktree.
- `POST /api/v2/workspaces/{workspace}/chats` takes `{"agent", "prompt"?}`
  and answers `{"pane": "p7", "prompted": true}`.
- Both wait up to 10 s for the chat's program to report in, then send the
  first prompt if it is not blank. `pane` is `null` when the chat was not
  listed in time; `prompted` is false when there was no prompt or the
  chat refused it, and the phone opens the chat to send it again.
  A prompt over 256 KiB, an unknown agent, a folder not offered, or an
  invalid branch is answered 400 before anything is created.

## Connectivity

- Each computer stores every address it reported: LAN, Tailscale,
  `HOST.local`. The app races them (connections staggered by about 250 ms)
  and keeps the first that answers `/api/ping` with the right fingerprint.
  `NWPathMonitor` reruns the race when the network changes, so moving from
  Wi-Fi to cellular plus Tailscale picks the other address without the user
  noticing.
- **Bonjour**: the daemon advertises `_herdr-remote._tcp` through `mdns-sd`
  (already a workspace dependency) with a TXT record holding a short
  fingerprint hash and the protocol version. The app browses with
  `NWBrowser`. This finds a computer whose DHCP address changed, and it is
  how network pairing discovers computers. It can be turned off in
  Settings > Remote control.
- **Bluetooth** helps when there is no IP path (see Bluetooth after
  pairing).
- In the background, the app keeps no sockets: pushes wake what needs
  waking. In the foreground, the app holds a live socket to the computer it
  shows and polls `home` from the others every 30 s, to keep their badges
  current.

## Pairing

There are three ways in. All end in the same device record, Secure Enclave
key, and pinned CA.

| Way | When | Channel that authenticates the computer |
| --- | --- | --- |
| **Nearby (Bluetooth)** | Phone near the computer, no camera needed | SAS: a six-digit code compared on both screens, with commitments |
| **Same network (Bonjour)** | Bluetooth off or unavailable on the desktop | The same SAS, carried over the daemon's TLS port without pinning |
| **QR code** | Anywhere, including remote over Tailscale | The camera reads the fingerprint off the computer's screen (as today) |

### Why nearby pairing needs a new code exchange

The current matching code mixes in a server nonce chosen *after* the
server sees the device key. Over the QR path that is fine, because pinned
TLS already proves who the server is. Over an unauthenticated channel
(Bluetooth, Bonjour), a man in the middle could run a fake daemon,
relay the claim to the real one, and grind its nonce until the phone shows
the code the desktop shows: a million tries is cheap. Nearby pairing
therefore uses a numeric-comparison SAS with commitments, as Bluetooth LE
Secure Connections does:

1. The phone (A) and the daemon (B) exchange ephemeral P-256 public keys.
   B's hello also carries its CA fingerprint, name, and addresses.
2. B sends `Cb = SHA-256("herdr-sas-commit" ‖ PKb ‖ PKa ‖ Nb)`.
3. A sends `Na`. B reveals `Nb`, and A checks `Cb`.
4. Both show `code = HKDF(Na ‖ Nb ‖ PKa ‖ PKb ‖ fingerprint) mod 10⁶`.
   A man in the middle gets one guess in a million. The fingerprint is part
   of the transcript, so a matching code also authenticates the CA.
5. A key derived from ECDH plus the nonces encrypts the rest (ChaCha20-Poly1305):
   B sends a one-time claim token. A then claims over pinned TLS with its
   Secure Enclave key, exactly like the QR claim, tied to this SAS session.
6. The desktop dialog shows that one code with Allow and Deny, and the phone
   shows the same code. No second code appears.

The daemon runs both sides of the protocol (`src/auth/sas.rs`; `p256`,
`hkdf`, and `aes-gcm` are already there, plus `chacha20poly1305`). Herdr UI
only carries bytes between Bluetooth and the daemon's control socket, so
"Herdr UI never holds secrets" still holds. On iOS everything is CryptoKit.
The protocol is pure functions in `HerdrRemoteCore` and `auth::sas`, tested
against shared vectors, including a relayed man-in-the-middle that must
fail.

### Nearby pairing, exactly

Phone A, daemon B. Keys are P-256, SEC1 uncompressed (65 bytes);
nonces are 32 random bytes; binary values travel as base64url without
padding; `||` concatenates bytes; `fp` is the CA fingerprint's 32 raw
bytes.

1. A sends `start {public_key: PKa}`. B refuses unless a pairing dialog
   is open (no unclaimed pairing: 409 / `NoPairing`). B makes a fresh key
   pair and `Nb`, keeps the session for 60 s (at most 4 at once, per-IP
   rate limit as `/api/pair/claim`), and answers
   `{session, computer, fingerprint (64 hex), public_key: PKb, commit}`
   with `commit = SHA-256("herdr-sas-commit-v1" || PKb || PKa || Nb)`.
2. A sends `reveal {session, nonce: Na}`. B answers `{nonce: Nb, sealed}`
   and forgets the session. `T = PKa || PKb || Na || Nb || fp`.
   - `code = u32_be(SHA-256("herdr-sas-code-v1" || T)[0..4]) mod 10^6`,
     six digits with leading zeros.
   - `key = HKDF-SHA256(ikm: ECDH, salt: SHA-256(T), info: "herdr-sas-key-v1")`, 32 bytes.
   - `sealed = nonce (12) || AES-256-GCM(key, plaintext, aad: "herdr-sas-key-v1")`,
     plaintext `{"token": TOKEN, "origins": [...]}`.
   - B stores `TOKEN`'s digest on the open pairing as its nearby token,
     with `code` as the code its claim will carry (60 s, single use; a
     newer reveal replaces it). One pairing seals at most three nearby
     tokens, kept across QR rotations; the fourth reveal answers 409
     (`NearbyExhausted`) until the dialog is opened again. Each reveal is
     another guess for a man in the middle and keeps the pairing open
     60 s longer, so both stay bounded.
3. A checks `commit`, computes `code` and `key`, opens `sealed`, and shows
   `code`. It then claims over TLS pinned to `fingerprint`, exactly as the
   QR claim does, at the first of `origins` that answers. The claim
   answers with B's `code`, which A requires to equal its own. The
   desktop dialog shows that code, and the person approves.

Transports: over the network, `POST /api/nearby/start` and
`POST /api/nearby/reveal` on the daemon's HTTPS port, which A calls
without pinning (the code is what authenticates); over Bluetooth, the same
JSON as `pair` characteristic frames, which Herdr UI relays to the
daemon's control socket as `nearby_start` and `nearby_reveal`.

Bonjour: `_herdr-remote._tcp` with the computer's name as the instance
and TXT `v=1`, `fp=` the fingerprint's first 16 hex, and `pairing=1`
while a pairing dialog is open (`0` otherwise).
- On macOS, `herdr-nearby` registers it through the system's DNS-SD while
  the dialog is open. A second mDNS responder beside mDNSResponder (the
  daemon's `mdns-sd`) is invisible to browsers on the same Mac and
  contends for its host name, so the daemon does not advertise there.
- On Linux and Windows, the daemon advertises with `mdns-sd`.
- `--no-bonjour` turns the daemon's advertising off.

Shared vectors (`Fixtures/nearby/*.json`) fix every key and nonce and
give `commit`, `code`, `key`, and `sealed`; a relay vector checks that a
man in the middle who substitutes its own keys yields different codes on
the two sides.

### Notifications, exactly

**Registration.** `POST /api/apns` takes two more optional fields beside
`push_to_start` and `activities`. Like `push_to_start`, each replaces the
stored value when present and keeps it when absent.

```json
{ "alert": "9f02…(hex device token)", "notify_key": "BHk3…" }
```

`notify_key` is base64url without padding of the 65-byte uncompressed P-256
point the app opens alerts with. Anything else is refused. A device with
both, on a daemon with the user's APNs key, gets every chat notification
through APNs and none through Web Push. APNs refusing the token
(`BadDeviceToken`, `Unregistered`) drops `alert`.

**A shown alert** is push type `alert`, topic the bundle id,
`apns-collapse-id` the chat's tag (at most 64 bytes), and priority 10 for a
question, 5 otherwise:

```json
{ "aps": { "alert": { "title": "Herdr", "body": "New activity" },
           "mutable-content": 1, "thread-id": "w1:p2",
           "interruption-level": "time-sensitive", "sound": "default" },
  "sealed": "BASE64URL" }
```

`interruption-level` is `active` for anything but a question. The badge is
not in `aps`: each computer knows only its own questions, so it travels
sealed and the app shows the sum over every paired computer.

**`sealed`** is `seal.rs`'s format to `notify_key`:
`ephemeral point (65) || nonce (12) || AES-256-GCM || tag`, keyed by HKDF over
the ECDH secret with info `herdr-remote-notify-v1`. Its plaintext, in this
field order:

```json
{ "v": 1, "kind": "question", "clear": false,
  "computer": "64 uppercase hex CA fingerprint", "pane": "w1:p2",
  "title": "Claude needs an answer", "body": "…", "badge": 2,
  "permission": { "id": 7, "options": [
      { "id": "allow", "kind": "allow_once", "name": "Allow once" } ] } }
```

- `kind` is `question`, `turn_ended`, or `error`.
- `badge` is the questions waiting across this computer's chats. The app
  keeps the last count from each computer and sets the badge to their sum.
- `permission` appears only for a permission request, with at most four
  options in the agent's order.
- The privacy setting decides what `body` quotes.
- `body` is shortened, ending in `…`, until the whole payload fits 4 KB.

**A clear** (the last question was answered from any client) is push type
`background`, priority 5:

```json
{ "aps": { "content-available": 1 }, "sealed": "…" }
```

Its plaintext has `"clear": true`, and the app removes the chat's
notifications (`thread-id` = `pane`).

**Live Activity buttons.** `content-state` also carries:
- in the question phase, `permission` (the waiting permission's id) and
  `options` (its first three, `{id, kind, name}`);
- `previous`, the two steps before `step`, oldest first, for the step stack.

The shared fixtures are `Fixtures/notify/alert.json` (device key, plaintext,
whole payload) and `Fixtures/update.json`.

### Bluetooth on the desktop

The Mac is the peripheral and the phone the central, because iOS supports
central mode best in the background (state restoration, long-lived
connections).

- **macOS: `herdr-nearby`**, a small Swift helper in `macos/herdr-nearby`,
  shipped inside Herdr.app beside `herdr-acp` (`scripts/release/build-nearby.sh`).
  Phase 0 found that `objc2-core-bluetooth` makes every Core Bluetooth
  call an `unsafe fn`, so the peripheral is Swift, not Rust.
  - Herdr UI starts it while the pairing dialog is open and talks to it in
    newline-delimited JSON on stdio: `advertise`, `stop`, and `frame`
    messages each way.
  - It holds no secrets and decides nothing. Herdr UI relays each `pair`
    frame to the daemon's control socket (`nearby_start`,
    `nearby_reveal`) and writes the answer back.
  - Running inside the bundle, it is attributed to Herdr.app for the
    Bluetooth permission (`NSBluetoothAlwaysUsageDescription`).
- **Linux and Windows: `herdr-nearby`**, a Rust helper in
  `crates/herdr-nearby`, shipped beside `herdr-gpui` in every Linux format
  and the Windows zip and installer. It speaks the macOS helper's stdio
  protocol and serves the same GATT service, so neither Herdr UI nor the
  phone tells them apart; Herdr UI runs it the same way (`nearby::Station`).
  - The stdio protocol, the framing, each phone's partial messages and
    UUID-shaped id, and the idle reports are shared and unit-tested; only
    the radio differs.
  - **Linux:** BlueZ over D-Bus with `zbus` on its own executor (no Tokio,
    and not `bluer`, which brings it). An application object tree
    (`ObjectManager`, `GattService1`, three `GattCharacteristic1`)
    registered with `GattManager1`, and an `LEAdvertisement1` carrying
    only the service UUID. Writes carry the phone's device path and MTU;
    notifications go out as `PropertiesChanged` on `Value`. BlueZ sends
    a notification to every subscribed phone, so two phones exchanging at
    once may each read the other's frames and retry. A phone leaving is
    seen from `Device1.Connected`. Idle time comes from GNOME's
    `IdleMonitor` or the freedesktop `ScreenSaver`; where neither
    answers, the helper reports a year, and presence never holds alerts.
  - **Windows:** WinRT's `GattServiceProvider`, advertised connectable and
    discoverable, with `WriteRequested`, `ReadRequested`, and
    `SubscribedClientsChanged`; each notification goes to its one phone
    with `NotifyValueForSubscribedClientAsync`. Idle time comes from
    `GetLastInputInfo`, the helper's one `unsafe` call.
  - Bonjour stays the daemon's on both, which advertises it there itself.
  - Only compiled and unit-tested so far (`just lint-linux`,
    `just lint-windows`); running against BlueZ, Windows, and an iPhone
    is still to do.
- GATT service (one fixed 128-bit UUID):
  - `info` (read): protocol version, and the computer's name only while the
    pairing dialog is open.
  - `pair` (write and notify): SAS messages as JSON, each framed as a
    2-byte big-endian length and the message (at most 4 KiB), cut into
    chunks a write or notification carries.
  - `presence` (write and notify): challenge and signed answer for paired
    devices, and encrypted current addresses.
  - `l2cap` (read): the PSM of the fallback channel, when it is on.
- Advertising runs only while the pairing dialog is open, or while presence
  is on (opt-in) for at least one paired device. The advertisement carries
  only the service UUID, which iOS background scanning needs. The
  computer's name and identity never appear unauthenticated.

### Flow on the phone

1. Tap **+ Add computer**. Nearby computers appear (Bluetooth or Bonjour,
   merged by fingerprint hash) with a "Scan QR" button below.
2. Tap one. The desktop's pairing dialog must already be open: an
   advertisement exists only then, and Bonjour pairing is refused
   otherwise. The phone then shows the six-digit code with a haptic tick.
3. Allow on the desktop. The phone shows the computer's home at once.

## Bluetooth after pairing

All of this is opt-in per device, under Settings > Remote control on both
sides.

- **Presence: notifications follow you.**
  - The phone keeps a background Bluetooth connection to each paired
    computer it is near, and proves itself by signing the `presence`
    challenge with its device key.
  - Herdr UI reports to the daemon whether the phone is "near" (connected,
    RSSI above a threshold, with hysteresis) and whether the desktop is in
    use (input in the last N minutes).
  - The notifier then holds back phone alerts while you are at the desk,
    and sends them when you walk away. This hold is on by default once
    presence is on. Live Activities still update quietly.
  - This is the "Away" behaviour the orchestrator left for later, with a
    real signal instead of a timer.
- **Address refresh and wake.**
  - The `presence` characteristic sends the computer's current addresses,
    encrypted to the device. A computer whose IP changed is found again
    even where Bonjour is filtered.
  - Core Bluetooth state restoration relaunches the app in the background
    when the computer comes back in range. The app then refreshes its home
    badges and Live Activity tokens.
- **Fallback transport (after a spike).**
  - An L2CAP channel, piped by Herdr UI to the daemon's loopback port. The
    phone runs its ordinary pinned TLS through a loopback `NWListener`, so
    there is no second protocol and the encryption is end-to-end.
  - This is for when there is no IP path: a plane, hotel Wi-Fi with client
    isolation, or a laptop without network.
  - The bandwidth suits chats, questions, and approvals. Terminals fall
    back to one screen per second.
  - Shown as "Connected over Bluetooth" in the connection banner.

### Presence, exactly

Opt-in on the desktop with `[remote] presence = true` (Settings > Remote
control, "Hold phone alerts while I'm here"); macOS only for now.

- **Advertising.** While remote control runs with presence on, Herdr UI
  keeps `herdr-nearby` running and sends it
  `{"type":"advertise_presence","on":true}`. The advertisement carries
  only the service UUID. `info` reads `{"v":1}`, without the name or
  fingerprint, outside the pairing dialog.
- **GATT.** `presence` (write and notify), UUID
  `6E3A4F21-8C0B-4D7E-9A55-2F1C0B7D4E93`, framed like `pair`: a 2-byte
  big-endian length, then at most 4 KiB of JSON, cut into chunks.
- **Phone to Mac:**
  - `{"type":"hello","hint":B64URL}`.
    - The hint is the first 16 bytes of `SHA-256("herdr-presence-hint-v1"
      || device id (UTF-8) || day)`, where `day` is the Unix time divided
      by 86400, UTC, as a big-endian u32.
    - It changes daily, so a listener cannot follow a phone by its id.
      The daemon accepts yesterday's, today's, or tomorrow's.
    - A phone that does not know which paired computer this is sends the
      hint for each computer it holds.
  - `{"type":"prove","nonce":B64URL,"signature":B64URL}`: the nonce
    names the device it was given for.
  - `{"type":"near","near":BOOL}`: the phone's own RSSI decision, with
    hysteresis.
- **Mac to phone:**
  - `{"type":"challenge","nonce":B64URL,"certificate":B64URL,"signature":B64URL}`.
    - `certificate` is the daemon's CA certificate (DER).
    - `signature` is its CA key's ECDSA P-256 / SHA-256 signature, r||s,
      over `"herdr-presence-challenge-v1" || nonce || device id (UTF-8)`.
    - The phone checks that `SHA-256(certificate)` is the fingerprint it
      pinned at pairing, and that the signature verifies with the
      certificate's key, before it signs anything.
    - So a peripheral that is not that computer gets no signature.
    - A daemon on a user-supplied certificate holds no CA key and refuses
      presence.
  - `{"type":"proven","addresses":B64URL|null}`.
  - `{"type":"error","message":"presence was not accepted"}`, the same
    text for every failure (unknown hint, spent or late nonce, bad
    signature, malformed frame).
- **Proof.**
  - ECDSA P-256 with SHA-256, signed by the device key that signs
    sessions, encoded r||s (IEEE P1363) as base64url.
  - The signed bytes are `"herdr-presence-v1" || CA fingerprint (32 raw
    bytes) || nonce (32 raw bytes)`. So a session signature never passes
    as a presence one, and a proof for one computer is worth nothing to
    another.
  - A nonce is single use and good for 30 s.
  - Up to 4 wait per device, the oldest dropped beyond that, so a forged
    hello cannot cancel the real phone's challenge.
  - Herdr UI takes at most 8 hellos a minute from one phone, and a phone
    may prove only a nonce given to it.
- **Addresses.** On a valid proof, the daemon seals `{"origins":[...]}` to
  the device's registered `notify_key`, in the alert seal's format with
  `herdr-remote-addresses-v1` as HKDF info and AAD. With no key, it sends
  `null`.
- **Control socket (Herdr UI only).**
  - `presence_challenge {hint}` answers `presence_challenge {nonce,
    certificate, signature}`.
  - `presence_prove {nonce, signature}` answers `presence_proven {device,
    addresses}`.
  - `presence_set {device, near}` answers `done`.
- **Near.**
  - Herdr UI marks a device near while a phone that proved it says
    `near` and the desktop saw input in the last 300 s. `herdr-nearby`
    reports idle seconds every 30 s.
  - Herdr UI sends each change, and renews a near mark every 30 s. A mark
    lapses after 120 s without renewal, so a crashed Herdr UI never holds
    alerts for long.
  - A disconnect lifts the mark.
- **Hold.**
  - While a device is near, its alerts are held instead of sent, through
    APNs and Web Push alike. At most 32 are held, the newest per chat
    wins, and they go in order once the mark is lifted or lapses.
  - A clear drops the held alert for its chat and still goes.
  - Live Activity pushes are never held.
- **Fixture.** `Fixtures/presence/prove.json` holds both sides from fixed
  values.
  - **The hello:** `device_id`, `hint_day`, and `hint`.
  - **The computer's side:**
    - `ca_certificate` and `ca_fingerprint`;
    - `challenge_message` and `challenge_signature`, made with a fixed CA
      key.
  - **The phone's side:** its key, `nonce`, the signed `message`, and a
    `signature` the daemon's verifier accepts.
  - **Addresses** sealed to a fixed notify key.
  - The certificate's own signature is randomized, so the test checks it
    by its key rather than byte for byte.

## Feature parity with the web app

Every row ships before the app is called complete. Native mockup picks
(round 1, `ios/Mockups/`, chosen 2026-10-28), all B:

- **Pairing B**: a proximity card, like AirPods setup, rises when a Mac in
  pairing mode is near (Pair / Not now, Bluetooth or Wi-Fi badge), and the
  same card turns into the six-digit code with "They don't match".
- **Home B**: attention first. "Needs you" cards answer inline (Allow once,
  Deny, Always…, or full-width options), then "Working" with progress, then
  "Recent". A tab bar holds Home, Board, Computers, and Search; the board
  (web pick D) becomes its own tab and Computers lists every computer's
  workspaces (the web's grouped home).
- **Chat B**: a document. The prompt as a heading, steps folded to one
  summary line ("5 steps · 3 files changed · +140 −3"), the running step
  pinned under the header as a glass capsule, diffs inline, earlier turns
  summed up at the end.
- **Live Activity B**: the step stack (current step with the previous two
  fading above), plan progress, elapsed time and cost, and the computer's
  counts. A question or permission still adds its answer buttons below.

Web picks that carry over: questions A (bottom sheet), terminal B/A/D.

Round 2 picks (2026-10-28):

- **Computers tab C**: an outline (sidebar list style) from computer to
  workspace to chats and terminals, with CPU and counts on each computer
  row and New chat inside each workspace.
- **New workspace A**: one form sheet: computer, project (opens the
  searchable folder list), new worktree with its branch, an agent grid,
  and an optional first prompt; Create in the toolbar.
- **Session sheet C**: cards: a large context ring with model, tokens, cost,
  and tok/s; mode tiles 2x2; an effort slider; auto-accept as a red card
  that asks for Face ID.
- **Terminal A**: full bleed with the two key rows docked; paste and the
  action sheet in the header.
- **Settings A**: one grouped list: computers, notifications (encrypted,
  hold at desk, lock screen detail, Live Activities), presence, Face ID,
  storage, about and diagnostics.
- **iPad B**: the attention list and chat B, with the pane's terminal in an
  inspector beside the chat.

Workspaces round (2026-10-29) replaces the Computers tab with a
**Workspaces** tab laid out like Herdr UI's sidebar (`spaces-Mix` in
`ios/Mockups`):

- Search on top, then every computer: a header with its name, connection,
  and CPU and memory, then its repositories as inset sections (initial,
  name, and + for a new worktree of it). Rows show the most pressing
  agent's mark, the checkout's branch or the worktree's name, its agent
  and terminal counts, and the pull request's number in its tone.
- A workspace opens on its own screen: branch, pull request, and place on
  top, its tabs (chats, agents in terminals, terminals) below, and a glass
  row docked at the bottom that starts a terminal or an agent with one
  tap; a long press on an agent opens the form with a first prompt.
- GPU use is not sent yet: the desktop's GPU reader lives in Herdr UI, and
  sharing it with the daemon means moving it to a crate both can use.

| Web app | Native |
| --- | --- |
| Home: needs-you strip, computers > workspaces > tabs, status marks, ⊕ new chat, collapse | `List` with sections per computer. Swipe actions (mute, close tab), context menus (new chat here, open on board), status marks from `look`, pull to refresh, search with `.searchable` |
| Attention board | The same three groups. Inline Allow once and Deny with haptics, matched-geometry transition into the chat |
| Chat focus layout: latest turn, step timeline, folded earlier turns, markdown, tool calls, diffs, queued prompts | `swift-markdown` into native blocks. Code blocks scroll sideways with Copy. Diffs show added and removed lines. Tool rows use SF Symbols (borrowed from Moltis). Text appended to the streaming answer is rendered without a reflow jump. Text is selectable, and long-press copies a block. |
| Header: title, status, context ring, session line, session sheet | Toolbar title menu that opens a sheet with detents. Effort is a segmented control, the mode list shows descriptions, and auto-accept asks for confirmation (Face ID optional) |
| Questions bottom sheet | `.sheet` at a medium detent that opens by itself. Full-width option buttons, with "always" options set apart |
| Composer: send and stop, photos, pasted screenshots, `/` completion | Native text view with the keyboard toolbar, `PhotosPicker`, the camera, paste including images (`UIPasteControl`, no Safari prompt), `/` completion, and **dictation** with `SpeechAnalyzer` on device |
| New workspace and new chat | The project picker with search (recent roots, then folders), a branch for a worktree, and an agent grid with the agents' icons |
| Terminal tabs | A Core Text view of the runs, fit to width, pinch to zoom, and selection. Two fixed key rows (B) above the keyboard. Hardware keys (Esc, Ctrl, arrows, ⌘V) through `UIKeyCommand` on iPad. The action sheet (A). Paste and image upload as today |
| Several computers, SSH hosts | All computers merged on home, not one at a time. The SSH and cloud machines a computer's Herdr UI connects to are paired directly once that computer allows the phone (`docs/remote-control-plan.md`, "Other machines"), and show "via" that computer |
| Settings | Notifications (per chat and per computer, privacy), presence, Bonjour, Face ID, the paired computers with their fingerprints, unpair |
| Reconnecting banner | A thin glass banner that says why, such as "No route to MacBook (LAN, Tailscale tried)" |

## Beyond the web app

- **Interactive Live Activities.**
  - Finish the APNs wiring (the "still to do" in the remote-control plan).
  - When a question or a permission waits, the lock screen and the expanded
    Dynamic Island show Allow once, Deny, or up to three answers as
    `Button(intent:)`. A `LiveActivityIntent` signs and posts from the
    app's process without opening it.
  - Add the step stack (current step plus the previous two) from Moltis.
- **Actionable, encrypted notifications** (APNs, replacing Web Push for
  this device):
  - Categories: permission (Allow once, Deny, and Always, which needs
    authentication), question (up to four answers), and finished (a text
    Reply that becomes the next prompt).
  - Encryption: the payload is encrypted to a second Secure Enclave key, for
    key agreement, in the shared access group. The Notification Service
    Extension decrypts it, so Apple sees only "Herdr" and ciphertext.
  - The existing privacy setting decides how much the decrypted text shows.
  - Thread identifiers are set per chat and interruption levels are set per
    kind (a question is time-sensitive).
  - Badges show the needs-you count.
- **Widgets and Controls.**
  - Home and Lock Screen widgets: needs-you count and working chats per
    computer.
  - Controls (Control Center, Lock Screen, Action button): open the board,
    new chat on the last computer, and dictate a prompt.
- **App Intents, Siri, Shortcuts, Spotlight.**
  - `AppEntity` types for computers, workspaces, and chats.
  - Intents: "Ask Claude in herdr-gpui to …", "What needs me?", "Stop the
    running chat", and "Answer the question".
  - `IndexedEntity` makes chats searchable in Spotlight.
- **Share extension**: send a page, text, or screenshots to a chat as a
  prompt, or to a new chat in a chosen workspace.
- **Face ID** gate, optional: on opening the app, and always before "Allow
  always" or turning on auto-accept.
- **iPad**:
  - `NavigationSplitView` with computers and workspaces, then chats, then
    the chat or terminal.
  - Keyboard shortcuts with the ⌘ discoverability overlay, pointer hover,
    and Stage Manager windows (one per chat, through `openWindow`).
- **Liquid Glass** toolbars and banners. The app follows the desktop theme
  sent with `look`, with system light and dark, Dynamic Type, VoiceOver
  labels on every status mark, and Reduce Motion.
- **Later**: an Apple Watch app (needs-you list, approve with a tap, and
  dictated answers) and the terminal's "Size for phone" once Herdr has an
  active non-sizing surface.

## Data and privacy

- **Keychain**:
  - Device keys live in the Secure Enclave. The signing key is
    non-exportable and stays this-device-only, and it is never synced to
    iCloud: each device pairs itself.
  - Sessions and fingerprints use `AfterFirstUnlockThisDeviceOnly`, so the
    extensions can sign while the phone is locked.
- **Cache, for speed**: the last `home` of every computer and the last 200
  entries of recently opened chats (bounded by size, oldest chat evicted
  first), as one file per computer under the App Group with
  `NSFileProtectionComplete`. Launch draws from it in the first frame, with
  a thin "Updating" mark, then the live socket resumes each chat from its
  cached revision, so only the changes since cross the network. Images go
  to a bounded disk cache keyed by content hash. Unpairing a computer
  deletes its files; one switch in Settings clears all of it.
- **Logs**: `os.Logger` with private fields redacted. "Send diagnostics"
  posts the last bounded log to `/api/log` (which exists) on the user's
  request only.

## Herdr UI changes

- The pairing dialog gets a "Nearby" state: advertising while it is open,
  then the SAS code with Allow and Deny. The QR stays in the same dialog.
- Settings > Remote control:
  - Each device shows its kind (iOS app or web app) and whether it is near.
  - New switches: Bluetooth presence, "Hold phone notifications while I'm
    at this computer", and Bonjour advertising.
- Control protocol: Bluetooth relay frames, and presence and desktop-in-use
  reports to the daemon.
- CLI:
  - `herdr-gpui remote devices --json` gains `kind` and `near`.
  - `herdr-gpui remote pair` gains `--nearby`, which opens the same dialog.
    The user still approves in the dialog.
  - Both go in `docs/cli.md`.

## Engineering

- The 1,000-line limit already covers Swift (`just check-file-size`). Split
  by responsibility.
- Swift Testing (`@Test`) in the packages. `ViewInspector` is not needed:
  test the stores and reducers, and look at the views in the simulator.
- `just` recipes (borrowed from Moltis):
  - `ios-generate` (xcodegen)
  - `ios-test` (`swift test` for both packages)
  - `ios-build` (simulator build, unsigned)
  - `ios-lint` (`swift-format lint --strict`)
  - `ios-sim` (the headless simulator loop below)
- CI: a macOS job runs `ios-test` and `ios-build` when `ios/**` or the
  fixtures change. The Rust side checks the same fixtures in `cargo test`.
- **The simulator loop** (from the earlier phone work):
  - Boot a headless simulator and start a sandbox daemon.
  - Pair through `simctl openurl` with a QR link, and approve over the
    control socket.
  - `simctl io screenshot` plus `daemon.log`, with no taps on the shared
    display.
  - Bluetooth does not work in the simulator, so nearby pairing is tested
    by its protocol vectors plus a manual device check.
- `SWIFT_VERSION: 6.0` and `SWIFT_STRICT_CONCURRENCY: complete` from the
  first change. No `@unchecked Sendable` without a comment stating the
  invariant.

## TestFlight from CI

A `ios-testflight` job in `.github/workflows/ios.yml` builds and uploads on
every push to `main` that touches `ios/**` or the v2 fixtures, and on each
release tag; pull requests run only `ios-test` and the unsigned build.

- `macos-26` runner with Xcode 26 (or later) selected explicitly.
- `xcodegen`, then `xcodebuild archive` with automatic signing through an
  App Store Connect API key (`-allowProvisioningUpdates
  -authenticationKeyPath ... -authenticationKeyID ... -authenticationKeyIssuerID ...`),
  so no certificates or profiles are stored in the repository or secrets.
- `xcodebuild -exportArchive` with an `ExportOptions.plist` whose
  `destination` is `upload` and `method` is `app-store-connect`: Xcode
  uploads to TestFlight itself, without fastlane or `altool`.
- `CURRENT_PROJECT_VERSION` is the workflow's run number;
  `MARKETING_VERSION` follows the app's release version.
- Secrets (environment `testflight`, required reviewers off for `main`):
  `ASC_KEY_ID`, `ASC_ISSUER_ID`, `ASC_KEY_P8`, `APPLE_TEAM_ID`. The key is
  written to a temporary file and removed in an `always()` step.
- `aps-environment` is `production` for the archive, `development` for
  local runs, set by configuration in `project.yml`.
- Internal testers get each build at once; an external group is a manual
  step in App Store Connect.

## Phases

Status, 2026-10-28: phase 0 ran (`docs/ios-spikes.md`; the device checks
wait for a signed build), and phase 1 is built: the daemon's `native/`
API, the Swift reducer and `ComputerLink`, and the app on iOS 26 with the
picked home, board, computers outline, search, a document chat with
questions and a composer, and a read-only terminal, checked end to end in
the simulator against a sandbox daemon.

Phase 2 (nearby pairing) is built: Bonjour pairing ran end to end in the
simulator, and Bluetooth waits for a device. Phase 3 (chat parity) is
built and checked in the simulator against a sandbox daemon and a
scripted agent:
- **Answers:** native Markdown blocks (`MarkdownBlock` in
  `HerdrRemoteCore`).
- **Composer:** photos, the camera, and pasted images (uploaded at once),
  on-device dictation with `SpeechAnalyzer`, and `/` completion.
- **Queue:** queued prompts can be removed.
- **Session sheet C:** the context ring, mode tiles, an effort slider,
  the other settings, and auto-accept behind Face ID.
- **Creating:** new workspace A and new chat, through `/api/v2/new`,
  `/api/v2/workspaces`, and `/api/v2/workspaces/{id}/chats`.
- **Still to check on a phone:** dictation, the camera, paste, and Face
  ID. A token-rate readout (tok/s) waits for the agents to report one.

Phase 4 (terminal parity) is built and checked in the simulator against
a sandbox daemon:
- **Typing:** keys typed on the phone reach the shell in order
  (`TermInput` in `HerdrRemoteCore`, one request at a time with keys
  batched between).
- **Key rows B:** sticky ctrl and alt, with ^C set apart.
- **Screen:** fit to width, pinch to zoom, and double-tap to reset; it
  scrolls sideways only once pinched wider than the screen.
- **Hardware keyboard:** arrows, Escape, Tab, Control and Option
  combinations, and ⌘V.
- **Header:** paste through `PasteButton` (no prompt) and a picture
  typed in as a path, the tab's panes as chips, and the action sheet A
  (split, zoom, new tab, new chat, close).
- **Actions:** they now also answer the `pane` to show next.
- **Still to do:** the screen is SwiftUI text per row, not yet the Core
  Text view the table above names, and hardware keys need an iPad or a
  phone with a keyboard to check.

Phase 5 (notifications) is built:
- **Daemon:** sealed APNs alerts with collapse ids, time-sensitive
  questions, badges, and background clears. Live Activity state carries
  the permission, three options, and the previous two steps.
- **App:** a Notification Service Extension that opens alerts with the
  App Group key and adds per-answer categories, actions that answer or
  reply without opening the app, Live Activity answer buttons
  (`AnswerIntent`), and the step stack.
- **Checked:**
  - Swift opens the daemon's own sealed fixture.
  - An alert sealed by a separate Python implementation, pushed with
    `simctl push`, opened in the app with the right actions.
- **Needs a device:** the extension itself (the simulator does not run
  it for `simctl push`), the lock-screen buttons, and APNs delivery.

Phase 6 (Bluetooth after pairing) is built, without the L2CAP fallback.
That fallback waits for its throughput spike to run on a device.
- **Presence:** the phone proves itself on the `presence` characteristic
  and reports near from signal strength with hysteresis. A Mac
  peripheral cannot read a central's signal, so the phone measures it
  and its claim only affects its own alerts.
- **Hold:** the daemon holds that phone's alerts while it is near and the
  desktop saw input in the last five minutes. The hold lapses after
  120 s without renewal.
- **Addresses:** a computer's current addresses come back sealed to the
  phone, under their own info.
- **Wake:** state restoration relaunches the app when a computer comes
  back in range.
- **Settings:** off by default on both sides, a switch per computer on
  the phone and `[remote] presence` on the desktop.
- **Checked:** against shared fixtures (the signed bytes, the signature
  encoding, the sealed addresses) and unit tests on each side.
- **Needs a device:** the Bluetooth path itself.

Phase 7 (system) is built:
- **Widgets:** Needs You in home and lock screen sizes, drawn from a
  summary file in the App Group.
- **Controls:** Board and New Workspace.
- **App Intents:**
  - What needs me, Ask a chat, Stop a chat (both need an unlocked phone),
    and Open the board.
  - `ChatEntity` is an `IndexedEntity`, so Spotlight indexes it.
- **Share extension:** sends to an existing chat. New chat from the share
  sheet waits.
- **Face ID:** an optional app lock, and before "always" answers.
- **Keychain:** the paired computers moved to the App Group's keychain
  group, with a migration that copies before it deletes.
- **Checked in the simulator:** the migration, and the summary written
  from a live home.
- **Needs a device or hands:** widgets, Controls, Siri, Spotlight, and
  the share sheet need a tap or a device to see.

Phase 8 has begun with the iPad, checked in the iPad simulator against
a sandbox daemon:
- **Layout (B):** the attention list beside the chat, with the
  workspace's newest terminal in an inspector (⌘⌥I).
- **Windows:** "Open in New Window" for chats and terminals, which Stage
  Manager shows; the Face ID lock covers every window.
- **Keyboard:** ⌘N, ⌘⇧B, ⌘, and ⌘⌥↑ / ⌘⌥↓ to move through chats.

0. **Spikes**, each with a written result:
   - A macOS GATT peripheral from Herdr.app through objc2, including the
     TCC prompt.
   - iOS background central with state restoration that reconnects to it.
   - L2CAP throughput between iPhone and Mac.
   - A `LiveActivityIntent` that posts over pinned TLS from the lock screen.
   - A Notification Service Extension that decrypts with a Secure Enclave
     key from the shared access group.
1. **Foundation**:
   - Daemon `native/`: v2 home and chat, the live WebSocket, and v2
     fixtures.
   - Swift 6 and iOS 26 shell, and the `Computer` actor with address racing.
   - The reducer port, QR pairing carried over, and read-only home and
     board across every computer.
2. **Nearby pairing**: SAS in `auth::sas` and Swift, Bonjour advertising
   and pairing, and the macOS Bluetooth backend with the dialog's Nearby
   state.
3. **Chat parity**: transcript, composer with photos and dictation, slash
   completion, session sheet, questions, queue, new workspace, and new chat.
4. **Terminal parity**: runs, key rows, hardware keys, actions, paste, and
   upload.
5. **Notifications**: APNs alerts with encryption and actions,
   interactive Live Activities, and badges. Web Push stays for the web app.
6. **Bluetooth after pairing**: presence and the notification hold, address
   refresh and wake, then the L2CAP fallback if its spike holds up.
7. **System**: widgets, Controls, App Intents, Spotlight, the share
   extension, and Face ID.
8. **iPad** polish (split view, windows, keyboard), then the Linux and
   Windows Bluetooth backends, then Apple Watch.

## Tests

- Shared fixtures for every v2 type and every `Change`, decoded and
  re-encoded on both sides. The reducer is run against the same change
  sequences in Rust and Swift, and the results are compared.
- SAS vectors, including the relayed man-in-the-middle with nonce grinding
  (it must fail), a wrong commitment, replay, an oversized frame, and a
  truncated frame.
- `Computer` actor:
  - Address race order.
  - Fingerprint mismatch skipped.
  - Lag that yields `reload`.
  - Cancel without replay.
  - Session expiry mid-stream.
  - Revocation closes the socket.
- Daemon: the WebSocket's auth, subscription bounds, coalescing, lag, and
  per-device stream caps; Bonjour TXT contents; pairing refused while no
  dialog is open.
- Manual, on devices:
  - Bluetooth pairing.
  - Presence walking away and back.
  - Push on a locked phone, and actions from the lock screen.
  - Live Activity buttons.
  - Wi-Fi to cellular plus Tailscale.
  - Revoke while streaming.
  - iPad with a keyboard.

## Open questions

- The App Store Connect API key and team ID, added as the `testflight`
  environment's secrets.
