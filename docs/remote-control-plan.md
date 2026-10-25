# Plan: Remote control

A phone (or any browser) controls the chats running on this machine: it lists
workspaces and their tabs, shows `herdr-acp` chats, sends prompts, answers the
agents' questions, and gets push notifications when an agent needs an answer
or finishes. It is reached over HTTPS from the local network or, when
Tailscale runs on the computer, from anywhere. Read `AGENTS.md` first; its
rules apply throughout.

## Decisions

| Question | Decision |
| --- | --- |
| Scope | Chats only. Workspaces are listed, switched, and created. Terminal tabs are listed but disabled ("Not available yet"). |
| Server stack | axum + Tokio, isolated in a new `herdr-remote-control` crate. No Tokio in `herdr-client` or `herdr-gpui`. |
| Lifecycle | A user service that outlives Herdr UI. The settings toggle installs and starts it, or stops and removes it. |
| Pairing | Dual approval: the phone scans a short-lived QR, then the desktop confirms the device with a matching code. Both sides prove themselves cryptographically. |
| TLS | The daemon's own certificate authority, trusted once on the phone. Works by IP address on the LAN and over Tailscale; no DNS name is required. |
| Notifications | Web Push from an installed PWA. |
| UI | Mobile-first server-rendered HTML: Tailwind, Alpine (CSP build), SSE. Mockups: `docs/mockups/remote-control/index.html`. Picked: home F (needs-you strip over lists grouped by computer, Ghostty-style), workspaces and tabs D (attention board), chat C (focus), questions A (bottom sheet). |
| Computers | One phone app shows every paired computer, plus the SSH hosts a paired computer's Herdr UI connects to. |
| iOS Live Activities | A PWA cannot start them, so a small native companion app (`ios/`) adds them, updated by the daemon through APNs. |

## Architecture

```
phone PWA ──HTTPS──▶ herdr-remote-control ──private endpoint──▶ herdr-acp (one per chat pane)
                         │  ▲
                         │  └── control endpoint ◀── Herdr UI (toggle, QR, approval, devices)
                         └──── Herdr daemon API (snapshot, workspace.create, tab.create)
```

### `crates/herdr-remote-control`

A library plus a `herdr-remote-control` binary. It depends on `herdr-chat-protocol`
and `herdr-client`, never on GPUI, keeping the UI → client → protocol
direction.

- **Runtime**: Tokio and axum, TLS through `tokio-rustls` (or `axum-server`)
  on the workspace's `rustls` with the ring provider. The blocking chat and
  daemon connections run on dedicated std threads and meet the async side
  through bounded `tokio::sync::mpsc` and `broadcast` channels.
- **Chat discovery**: reads pane metadata from the local daemon, takes panes
  whose `acp` token holds a valid `EndpointId`, and derives the address with
  `endpoint_address(id)`. It never connects to a path read from daemon data:
  the same trust boundary as `herdr-gpui/src/chat/discovery.rs`.
- **Per chat**: one upstream attach connection shared by every phone
  watching it, with a new `ClientKind` (`Remote`; older programs see `Other`).
  It keeps the transcript with `Transcript::apply`, the protocol's only
  reducer, and fans changes out to subscribers. A lagging subscriber is
  dropped and resumes, as the protocol already specifies.
- **Workspaces**: listed from the daemon snapshot; switching is client-side
  (the phone's own view, not `workspace.focus`, so the desktop is not moved).
  Creating one uses `workspace.create` (or `worktree.create` for a new branch)
  and then `tab.create` running `herdr-acp` with the chosen agent, the same
  path `chat new` takes in Herdr UI. Only directories the user already has
  workspaces in, or below configured project roots, can be chosen.
- **Control endpoint**: a private endpoint (`herdr_client::private_endpoint`)
  that Herdr UI uses to read status, mint pairing tokens, approve or deny a
  pending device, list and revoke devices, and stop the service. Only this
  user can reach it.

### Lifecycle

- The toggle in Settings > Remote control installs a user service (LaunchAgent
  on macOS, systemd user unit on Linux, a logon task on Windows), started
  through `herdr_client::new_command`. Turning it off stops and removes it.
- It keeps running when Herdr UI quits, so the phone works and push keeps
  flowing. Pairing needs Herdr UI, because only its dialog approves devices.
- Without a running Herdr daemon it serves the app shell with a "Herdr is not
  running" state; it never starts or installs a daemon.

## TLS without DNS

The phone must trust the certificate fully: service workers, PWA install,
and Web Push all refuse an untrusted one. A name like `*.ts.net` is not
required.

- On first start `herdr-remote-control` creates its own **certificate authority**
  (`rcgen`, P-256, key 0600 in its private state directory) and issues a
  server certificate whose SANs list every address it serves on: the LAN
  IPv4/IPv6 addresses, `HOSTNAME.local`, and, when Tailscale is up, the
  Tailscale `100.x` address (no MagicDNS name). When a new address appears
  (new Wi-Fi, Tailscale toggled), it reissues the leaf from the same CA and
  reloads it without restarting; the phone trusts the CA, so nothing changes
  there. The leaf is saved beside the CA (`tls/leaf.pem`, 0600) and kept
  across restarts and when an address goes away, so a phone that only
  accepted Safari's warning keeps working until a new address or renewal.
- The service worker never answers `/pair` from its cache: a phone that no
  longer trusts the certificate must see Safari's warning, not a cached page
  whose calls fail.
- The daemon logs to `daemon.log`: one line per request (the route, never
  the path or query, which carry claim IDs and handoff tokens), refused TLS
  handshakes, and errors phones report through `POST /api/log` (bounded,
  rate limited per address, unauthenticated because pairing fails before a
  session exists).
- The CA is name-constrained to private address ranges, `.local`, and
  `localhost` (not `ts.net`, which would cover every tailnet), and carries
  the serverAuth EKU, so a stolen CA key cannot vouch for public sites or
  sign anything but TLS servers.
- On macOS with the application firewall on, the first start asks whether
  `herdr-remote-control` may accept incoming connections; until the user allows it,
  LAN and Tailscale requests hang (loopback works). The settings page says
  so.
- The phone installs the CA once during pairing: a `.mobileconfig` profile on
  iOS (then Settings > General > About > Certificate Trust Settings), a CA
  file on Android. The pairing page walks through it and the QR carries the
  CA fingerprint, which the page shows beside the desktop's.
- A user-supplied certificate and key remain an option for people with their
  own domain.

### One app, many addresses

An origin includes the host, so a PWA installed from `https://192.168.1.20:7443`
is a different app from one at `https://100.64.0.3:7443`. To stay one app:

- The PWA is installed from the address used at pairing, its **home origin**.
  The service worker caches the app shell, so the app opens from any network.
- The shell then races the daemon's known addresses (it learns them at
  pairing and on every connection) and uses the first that answers. The API
  sends CORS headers for the home origin only.
- Requests authenticate with short-lived bearer tokens, not cookies, so
  cross-origin calls work without third-party cookies.
- Push does not care about the network: the daemon sends to the browser's
  push service, which reaches the phone anywhere.

## Pairing and authentication

Both directions are proven, and a person approves on both screens.

| Direction | Proof |
| --- | --- |
| Phone trusts the computer | TLS chain to the CA the phone installed; CA fingerprint in the QR and shown on both screens. |
| Computer trusts the phone | A non-extractable WebCrypto P-256 key, created at pairing and kept in IndexedDB, signs a server challenge for every session. |
| A person agrees | A 6-digit matching code shown on the phone and in the desktop's approval dialog. |

Passkeys (WebAuthn) would be stronger but need a domain as their relying
party, and this works by IP address. They can come later for users with their
own certificate and domain.

1. Herdr UI, Remote control > Pair device, shows a QR:
   `https://ADDRESS:PORT/pair#t=TOKEN&fp=CA_SHA256`. The token is 256 random
   bits, valid 60 seconds, single use, and rotates while the dialog is open.
   It sits after `#`, so it never reaches a server or proxy log. The daemon
   mints it; Herdr UI never holds secrets.
2. The phone opens `/pair` and claims it at once, asking nothing: it
   generates its key and posts the token, the public key, and a name taken
   from the browser (iPhone, iPad, Android phone). When the browser does not
   yet trust the CA, the page offers it below the code; pairing works
   without it, but the installed app and its notifications need it.
3. The daemon derives the matching code from the token, the device key, and
   a server nonce. Both screens show it. The desktop dialog reads
   "*iPhone* wants remote control of your chats. Code 428 913. Allow / Deny".
   Nothing is granted until Allow. Deny, timeout, or a second claim of the
   same token voids the pairing.
4. After approval the device is stored (public key, name, created, last seen)
   and gets session tokens by signing challenges. Revoking it in Herdr UI
   ends its sessions and drops its push subscription immediately.
5. iOS gives a Home Screen web app storage of its own, so the key saved in
   the browser never reaches it. While the paired browser shows home, it
   mints a single-use handoff token (24 hours, one per device, reused until
   it nears expiry) and reloads once at `/?handoff=TOKEN`: Safari reads the
   manifest when a page loads, so the daemon serves that page with
   `/manifest.webmanifest?handoff=TOKEN`, whose start address carries the
   token too. On first launch the app makes its own key and redeems the
   token: the device keeps its ID and name, takes the new key, and the
   browser's sessions end. No second approval: the desktop approved this
   device already. The daemon keeps only each token's digest, saved in
   `handoffs.json` (0600), so an icon added before the daemon restarts
   still opens paired, and a used token stays used after one.

Protections: constant-time comparisons, per-IP rate limits on `/pair` and
challenges, an `Origin` check on every write, bounded bodies and headers, a
cap on streams per device, and idle device expiry (configurable). Device
records live in the daemon's private state directory, written 0600.

## Push notifications

Standard Web Push (VAPID, RFC 8291 and 8292), implemented in `push` on
RustCrypto (`p256`, `hkdf`, `aes-gcm`) with a blocking `ureq` request that
the server runs in `spawn_blocking`. Endpoints are limited to the known push
services (Apple, Google, Mozilla, Windows), so a device cannot make the
daemon post elsewhere. Payloads are encrypted end to end, so the push services
(Apple, Google, Mozilla) relay them without reading them.

- iOS 16.4 and later deliver push only to an installed PWA (Add to Home
  Screen); the pairing page guides that. Android works installed or not.
- **Triggers**, each switchable per device, and mutable per chat:
  - an agent asks a question or a permission (high urgency);
  - a turn ends (agent, workspace, duration, cost through `display`);
  - an error notice, or the agent stops while blocked.
- **No noise**: nothing while that chat is visible on the phone (the service
  worker checks its clients); one tag per chat so a newer push replaces the
  older one; a follow-up push clears a question answered anywhere, including
  on the desktop; bursts coalesce; a per-device rate limit.
- The app icon shows the number of waiting questions (Badging API).
- **Privacy**: by default a push says "Claude needs an answer in
  herdr-gpui · main" without the message text. A per-device setting adds a
  short excerpt.
- **Later, opt-in**: answering from the notification. Android offers
  notification actions ("Allow once", "Deny"); iOS has none, so a tap opens
  the question directly. Never offered for an "always" option.
- Sending a push is an outward request: it happens only for devices that
  granted the browser permission and with notifications on in settings.

## Mobile UI

Server-rendered with a template crate (`maud`) and `pulldown-cmark` (raw HTML
shown as text, images turned into links, unsafe link schemes dropped). The
server sends a static app shell (navigations cannot carry a bearer token),
and the shell fetches HTML fragments (`/f/...`) and a patch stream
(`/api/stream`, server-sent events read with `fetch` so the token rides in a
header) from whichever of the computer's addresses answers. Each patch
replaces regions by id; the stream's first patch brings the view up to date,
so nothing is missed between the fragment and the stream. Alpine holds only
local state (composer, sheets, folds).
Tailwind is compiled ahead of time by the standalone CLI (`just web-css`) and
committed; Alpine's CSP build is vendored; everything is embedded with
`include_bytes!` under a strict CSP (`default-src 'self'`). No CDN.

- **Home** (mockup Round 3 F, after Ghostty's mobile app): a horizontal
  strip of pending questions on top, then each computer as a section, and
  under it each workspace as an inset grouped list with ⊕ (new chat there)
  and a collapse chevron. Rows carry the agent's icon with an unread or
  question dot, the chat title, and `•••` while working. Terminal tabs show
  a terminal icon and open the terminal view (see Terminal tabs). A
  floating glass toolbar holds settings, the eye, and ＋ (new workspace),
  with search apart.
- **Workspaces and tabs, the attention board** (mockup 1D, behind the eye):
  chats across every computer grouped by state, "Needs you" first (with
  inline Allow once / Deny for a simple permission), then "Working" with
  progress, then "Idle"; the computer and workspace are labels on each card.
- **Status marks**: every tab row with an agent, a chat or a terminal
  where Herdr detects one, shows its status from the daemon's snapshot in
  the set picked in Herdr UI (`status_icons`), which Herdr UI sends over
  the control socket and the daemon keeps in its state directory. Dots,
  solid, outline, and tiles draw as on the desktop; the animated sets
  drawn in code (wave, braille, Orca) show as dots. Rows show no path.
- **New workspace**: the project is a field that opens a full-screen list
  with a search field (grouped: recent workspace roots, then folders under
  the project roots; one line each, the folder's name over where it sits),
  an optional new branch as a worktree, and the agent as a grid.
- **Chat, focus layout** (mockup 2C): full screen, showing the latest turn:
  its prompt, the plan or tool calls as a step timeline, and the agent's
  answer. Earlier turns fold into one-line summaries (duration, cost) that
  expand into the full transcript with markdown, tool calls, and diffs that
  scroll horizontally inside their block. Queued prompts with unqueue.
  Header with the title, status, and context ring; the line under the
  title sums up the session ("Opus 5.5 · High · Bypass Permissions"), and
  tapping the title (its chevron says so) drops the session sheet from the top: the model
  and other choices as lists, short plain ones (effort) as a segmented
  control, toggles, the mode with its descriptions, and auto-accept (with
  a confirmation). The sheet stays open while the stream redraws it.
- **Questions, bottom sheet** (mockup 3A): within thumb reach, full-width
  option buttons, "always" options set apart, opening by itself when a
  question arrives.
- **Composer**: pinned above the keyboard (`visualViewport`), safe-area
  insets, send and stop sharing one button, photo attachment. Typing `/`
  lists the first commands whose names start with what follows, with
  their descriptions; tapping one fills it in.
- System light or dark, colors exported from the desktop theme as CSS
  variables, 44 px touch targets, Dynamic Type sizes, a thin "Reconnecting"
  banner. Nothing but the app shell is cached at rest.

## Terminal tabs

Mockups: `docs/mockups/remote-control/terminal.html`. Picks: **typing B**
(two fixed key rows above the keyboard), **actions A** (an action sheet),
**size D** (fit to start, size for phone on demand).

Herdr gives each tab one geometry controller, the last active client to
type, focus, or navigate in it, and only that client's size reaches the
PTYs; an inactive client may neither type nor send endpoint requests
(`surface_inactive`). So the daemon does not drive terminals through its
shell connection. It uses Herdr's public API socket (`herdr.sock` beside
`herdr-client.sock`, newline-delimited JSON), whose pane methods neither
focus nor resize anything:

- **Screen**: `pane.read {source: visible, format: ansi}` while a phone
  shows the pane, every 300 ms, sent as a patch only when it changed. The
  ANSI (SGR only) becomes escaped spans on the server. No cursor position
  comes with it.
- **Typing**: one key at a time with `pane.send_keys` (Herdr's key syntax:
  `ctrl+c`, `esc`, `up`, `pageup`), because `pane.send_text` arrives as a
  bracketed paste, which an editor inserts rather than runs; a paste uses
  `pane.send_text`.
- **Actions** (the sheet): `pane.split {direction: right|down, focus:
  false}`, `pane.zoom`, `pane.close`, `tab.create {focus: false}`,
  `tab.close`, and moving between panes on the phone only.
- The same socket now carries New workspace and New chat, which the
  inactive shell connection could never send.

**Size**: the pane keeps its desktop size and the text shrinks to fit the
width, with a zoom toggle. "Size for phone" (an active surface that takes
the tab's geometry while the phone types) is a later step; until then the
phone never disturbs the desktop's layout.

## Several computers

- The phone pairs each computer once; each pairing stores that computer's
  addresses, CA, and device key. Home lists them all, merged.
- Requests already use bearer tokens and CORS for the home origin, so one
  installed app can talk to every computer's daemon.
- Push: a push subscription is bound to one sender key (VAPID), so each
  computer gets its own subscription, through one service worker
  registration per computer under its own scope (`/m/COMPUTER/`). Phase 0
  checks that an installed iOS PWA accepts several registrations; if not,
  the home computer relays the others' notifications.
- SSH hosts a paired computer's Herdr UI connects to appear as their own
  sections, reached through that computer with `herdr-acp connect ID`, with
  no pairing of their own.

## iOS Live Activities

Running chats on the lock screen and in the Dynamic Island (mockup 5): one
activity per running turn showing the current step, elapsed time, and cost;
amber with "Needs an answer" when a question waits; ending with the turn
summary.

A web app cannot do this. Live Activities belong to ActivityKit, so they need
a native companion app:

- A small SwiftUI app plus widget extension, paired the same way as the PWA
  (QR, matching code, device key in the Keychain). It does not duplicate the
  chat UI; tapping an activity opens the PWA's chat.
- The daemon updates activities through APNs with an Apple Developer token
  (`.p8` key) the user provides, using ActivityKit push tokens, including
  push-to-start (iOS 17.2+) so an activity appears when a turn starts while
  the app is closed.
- Distribution needs a developer account: TestFlight or a self-built app.
  Swift files follow the same 1,000-line limit.
- Android has no equivalent; there, an ongoing notification updated by push
  covers the same ground.

### Built

The companion lives in `ios/` (see `ios/README.md` for building, signing, and
the APNs key):

- **Project**: an XcodeGen spec (`ios/project.yml`) for the `HerdrRemote` app
  and the `HerdrRemoteWidgets` extension, iOS 17.2+, bundle
  `so.pen.herdr-remote`. Logic without UIKit sits in the `HerdrRemoteCore`
  Swift package, whose tests run on the Mac with `swift test`.
- **Activity**: `ChatActivityAttributes` (computer name, computer id,
  workspace, chat title, agent, pane) with `ChatActivityState` as its
  content (phase working,
  question, finished, or failed; step; start time as Unix seconds; cost;
  question; summary; plan progress; the computer's working and waiting
  counts for the island). Lock screen, compact, minimal, and expanded
  layouts follow mockup 5; every tap opens `herdr-remote://chat?...`, which
  the app turns into `https://ORIGIN/chat/PANE` in Safari at the first
  address that answers. The computer id is the CA fingerprint (64
  uppercase hex digits, no colons); the app keys paired computers by it and
  routes links and activity tokens by it alone, since the name is whatever
  the computer calls itself.
- **Pairing**: the same QR as the web app (camera or pasted link), a Secure
  Enclave P-256 key (software on the simulator) whose SEC1 point is the
  claim's `public_key`, the matching code, then polling until Allow. The
  app pins the computer's CA by the QR's fingerprint in its
  `URLSessionDelegate`, anchoring the chain to that certificate alone, so it
  needs no profile; Safari still does, for the web app. Sessions sign
  `herdr-remote-session\0 || nonce` and send the P1363 signature, trying
  each stored address in order.
- **Push tokens**: the push-to-start token goes to every paired computer and
  each activity's token to its own, through `POST /api/apns` with
  `{push_to_start, activities: [{pane, token}]}`.
- **Daemon**: `crates/herdr-remote-control/src/apns` loads `STATE_DIR/apns/AuthKey.p8`
  and `config.json` (absent: off), signs the ES256 provider token (renewed
  every fifty minutes), decides start, update, or end from the same inputs as
  `push::decide`, paces step updates to one per ten seconds per chat, and
  posts over one HTTP/2 connection (hyper's client on tokio-rustls) to
  `api.push.apple.com` or `api.sandbox.push.apple.com` only. A
  `BadDeviceToken` or `410` drops the token. JSON fixtures under
  `ios/HerdrRemoteCore/Tests/HerdrRemoteCoreTests/Fixtures` are checked by
  both the Rust and the Swift tests.

Still to do: wire `apns::Sender` into the server (a `POST /api/apns` route
storing `apns::Registration` in a new opaque `apns` slot of the device
record, and a notifier branch calling `apns::decide`), check payloads and
push-to-start on a device with a real key, and an app icon.

## Herdr UI

- **Settings > Remote control**: a Pair a phone card first, then the
  toggle, port, addresses served (LAN, Tailscale when present), certificate
  (own CA or user files) with its fingerprint, notification defaults, device
  idle expiry, and paired devices with last seen and Revoke.
- **Title bar**, before the GitHub account, while remote control runs: a
  "Pair phone" button until a phone is paired, then a phone with the number
  connected now (green while any is). It opens the list of phones, each with
  Disconnect (the revoke confirmation: a phone signs back in with its key,
  so only revoking keeps it out), plus Pair a phone and Settings.
- **Remote control dialog**: the QR drawn as GPUI quads from a `qrcode`
  matrix, a 60-second countdown ring, the URL and CA fingerprint, then the
  approval step with the matching code. Reachable from the palette, the
  title bar, and Settings.
- **CLI** (`docs/cli.md`): `herdr-gpui remote status|pair|devices [--json]`.
  `pair` opens the dialog and never prints a token; `remote revoke DEVICE`
  opens the app's confirmation. Each gets its `control/protocol` request,
  `control/server` handler, `control/client` parsing and output, `--help`
  text, and tests.
- Settings live under `[remote]` in `config.toml` with sorted fields;
  secrets and device keys never do.

## Phases

0. **Spikes**
   - Read pane metadata from the daemon without attaching as a terminal
     client that could resize panes (JSON API or a non-sizing client).
   - A private CA installed on iOS and Android: PWA install, service worker,
     and a Web Push received, over a LAN IP and a Tailscale IP.
   - CORS plus bearer tokens from the home origin to a second address,
     installed PWA on iOS.
   - Several service worker registrations, each with its own push
     subscription, in one installed iOS PWA.
1. **MVP**: `herdr-remote-control` service, own CA and leaf reissue, pairing with
   dual approval, devices and revoke, workspaces and tabs (terminals
   disabled), chat view with every chat command, new workspace with a chat,
   Settings page and dialog, `remote` CLI commands.
2. **PWA and push**: install flow, Web Push triggers and noise rules, badges,
   per-chat mute, notification privacy setting.
3. **Polish**: photo attachments, slash completion, SSH-host chats through
   `herdr-acp connect ID`, opt-in Android notification actions, passkeys for
   users with their own domain.
4. **iOS companion**: Live Activities and Dynamic Island through
   ActivityKit and APNs.
5. **Terminal tabs**: built (see Terminal tabs); "Size for phone" later.

## Tests

- Pure logic without sockets: token TTL, single use, rotation, and
  constant-time checks; matching-code derivation; challenge signatures and
  replay; CORS and `Origin` rules; SAN set and leaf reissue; cursor to
  `Last-Event-ID`; rendering of every `EntryKind` with escaping; request,
  header, and body bounds and malformed input; push trigger and coalescing
  rules.
- Mock peers: a fake `herdr-acp` endpoint for resume after lag, rejected
  `Ack`s, and a permission answered from phone and desktop (first wins); a
  fake daemon for workspace creation.
- Herdr UI: control requests, CLI parsing and output, settings model, QR
  matrix geometry; headless tests for the dialog states.
- Manual: real iOS and Android devices on LAN and Tailscale, PWA install,
  push on a locked phone, revoke while streaming.

## Open questions

- Default port, and whether to serve on every LAN interface or let the user
  pick one.
- Project roots for "New workspace": reuse an existing Herdr UI setting, or
  add one.
- Device idle expiry default (30 days?).
