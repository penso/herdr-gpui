# Plan: a push relay for the iOS app

Let people who install the TestFlight or App Store app turn on Live Activities
and sealed alerts, without shipping the APNs key to their computers. A small
Rust service in this repository, `sheprd-push-relay`, holds the key and forwards
pushes from each user's `sheprd-remote-control` daemon to Apple.

- **Opt-in.** It is off until the user turns it on, in a setting that explains
  what the relay sees.
- **Cheap to run.** One Docker image built by CI, on any small host, with no database.
  TestFlight has about 120 downloads, so nothing here needs to scale yet.
- **Little to steal.** The server stores no push tokens and no chat content, and
  it has no route to anyone's computer.

This extends `docs/remote-control-plan.md` (iOS Live Activities) and
`docs/ios-app-plan.md` (Notifications, exactly). Bringing your own key stays
supported for people who sign the app themselves.

## Where things stand

| Piece | Today |
| --- | --- |
| Credentials | Each daemon loads `STATE_DIR/apns/AuthKey.p8` and `config.json`. Without them, `state.apns` is `None`, chat changes are skipped, and `POST /api/apns` answers 503. |
| Sending | `apns::Sender` posts straight to Apple over HTTP/2, in `crates/sheprd-apns/src/send.rs`. The docs say a device can never make the daemon post anywhere else. |
| Alerts | Sealed to the phone. APNs sees "Sheprd", "New activity", the thread id, and ciphertext. |
| Live Activities | Plaintext: agent, chat title, workspace, computer, step, question, and answer options. |
| Live Activity detail | Each computer and each phone chooses Detailed or Generic, and the stricter one wins (`apns/redact.rs`). `Privacy` also applies: question text and failure messages need Excerpt. |
| Token upload | `ActivityTokens.sync` treats the 503 like any failure: it forgets the session and sets `lastError`, which Settings shows. |

### Why bring-your-own-key cannot work for most users

APNs accepts a key only from the Apple team that owns the app. Tokens from our
TestFlight or App Store build belong to `so.pen.herdr.remote`, so only our key
can reach them. A user's own key works only with a copy of the app signed by
their own team.

### Why the key cannot ship in the app

Anything in a desktop binary can be extracted. Whoever has the key can push as
Sheprd to any token they hold, and revoking it cuts off every user until a new
build ships. Sealed alerts resist forgery because a forged one will not open;
Live Activities have no such protection.

## Decision: build a small relay

No open source project does this job as it stands. The APNs sending is the easy
part, and we already have it. What is missing everywhere is the part that makes
a public relay safe: proving a caller is our app, limiting who can reach which
phone, and fixing the topic on the server.

Checked against each project's source on 2026-11-04, except where noted.

| Project | License, activity | Live Activities | Fit |
| --- | --- | --- | --- |
| [Gorush](https://github.com/appleboy/gorush) | MIT, active | Yes: `content-state`, `event`, `attributes`, `attributes-type`, and a caller-set `push_type` | The closest match, but built for a trusted backend. The caller chooses `topic`, `production`, and raw tokens, and `config.go` and `router/server.go` show no API auth. `core.sync` defaults to false, so APNs refusals need `sync` or the feedback hook. It rebuilds payloads from typed fields rather than forwarding bytes. |
| [Bark server](https://github.com/Finb/bark-server) | MIT, active | No: alert and background only | Shows the risk. Its private key is committed in `apns/apns_certs.go`, the design we are rejecting. |
| [Sygnal](https://github.com/element-hq/sygnal) | AGPL-3.0, active | One `push_type` per configured app | Speaks the Matrix push gateway API, and needs a separate app entry per push type. A good reference for rejected-key handling, not something to run. |
| [ntfy](https://github.com/binwiederhier/ntfy) | Apache-2.0, active | Not supported as far as we found | Its [`upstream-base-url`](https://docs.ntfy.sh/config/#ios-instant-notifications) forwards only a message id and a topic hash, then the phone fetches the content. A good pattern for privacy, but a Live Activity has to carry its content in the push. |
| [PushGo Gateway](https://github.com/AldenClark/pushgo-gateway) | MIT, active, Rust | Not established: the matches seen were prototype validation files | A complete messaging platform with channels, MQTT, and MCP. Far more than we need. Reviewed only briefly. |

Commercial services (OneSignal, Firebase) were not evaluated: they would put
chat metadata with another party. Azure Notification Hubs was reported in 2023
as [accepting `liveactivity` pushes without delivering them](https://learn.microsoft.com/en-us/answers/a/1164208),
so test any intermediary on a real device.

**Why not Gorush behind a proxy?** That was the serious alternative. We would
still write the attestation, the handles, the topic enforcement, and the rate
limits as a front service, then run a second Go service to reach code we
already have in Rust. It remains a reasonable fallback if extracting
`apns::Sender` turns out to be harder than expected.

## Design

```text
iPhone ── attest once ──────────▶ relay ── credential ──▶ iPhone
iPhone ── handle (sealed to relay) ─▶ daemon                    (POST /api/apns)
daemon ── handle + payload ─────▶ relay ── token + payload ──▶ APNs ──▶ iPhone
```

Traffic runs one way. Daemons call out to the relay, and the relay calls out to
Apple. The relay never connects to a computer or a phone, and answers from a
phone still go straight to its daemon.

### 1. Attest once

The app proves it is genuine with App Attest and receives a **device
credential**, valid for 30 days, matching the daemon's `device_idle_days`
default. With it comes the relay's current public key, signed by an offline root
key pinned in the app, so the relay key can be replaced without an app release.

### 2. Make handles on the phone

A handle is the push token and the credential, sealed to the relay's public key
with the token's kind and a random handle id inside.

- The phone makes handles locally, so a new activity token needs no relay round
  trip, and the relay being down does not block pairing or token upload.
- Daemons never see a raw token. A daemon can reach only phones paired with it.
- The handle holds no computer fingerprint, so the relay cannot tell which
  computer a phone is paired with, beyond the IP addresses it sees.
- The scheme is `sheprd_apns::seal` with its own `info` string, so a handle cannot be
  passed off as an alert. The key id travels with the handle.

### 3. Send

The daemon sends `POST /v1/push` with the handle, push type, priority, collapse
id, and payload, and `POST /v1/revoke` with a handle when a phone unpairs. One
relay instance serves one APNs environment, and refuses a credential for the
other. The relay:

1. Applies a generous rate limit per source IP, before any cryptography, so junk
   requests cost no key agreement. Many computers can share one address, so this
   limit only stops floods. Addresses stay in memory and are never logged.
2. Opens the handle in memory and checks the credential's expiry and revocation.
3. Checks that the kind permits the push type: `push_to_start` and `activity`
   allow `liveactivity`; `alert` allows `alert` and `background`.
4. Checks the payload is JSON within `MAX_PAYLOAD` (4,096 bytes).
5. Applies rate limits per device and per handle id.
6. Sets the topic and host itself, from its own config and the credential's
   environment. Callers never supply either.
7. Posts to APNs and maps the reply: success, `gone` (the daemon forgets the
   handle), `limited`, or `unavailable`.

Recording the environment in the credential also removes the sandbox mismatch
that `ios/README.md` warns about, where a wrong `sandbox` flag makes the daemon
forget good tokens.

### 4. Storage

Memory by default, so a droplet needs nothing but the image and its secrets. The
`Store` trait has three implementations, chosen with `--store`:

| `--store` | Keeps | For |
| --- | --- | --- |
| `memory` (default) | Nothing across a restart | One small host |
| `sqlite:FILE` | A file on a volume | One host where revocations must survive |
| `postgres` (feature) | `postgres-url` in the secrets, and `postgres-ca.pem` beside it for a private CA | Several relays sharing state |

Postgres connections always use TLS, unless the URL says `sslmode=disable` for a
database on the same machine. Revoking has its own tight limit per device, since
each revocation stays until its credential expires.

What a store holds:

| Table | Holds |
| --- | --- |
| `revoked` | Handle ids and device ids, removed once they would have expired |
| `devices` (with App Attest, not built) | Attested public key, assertion counter, APNs environment, first and last seen dates |

**Tokens and payloads never go in a store or the logs.** That rule is what keeps
a stolen disk from being useful, and a test enforces it: it sends a push through
the relay, then searches the database and captured logs for the token and payload.

Rate limits live in memory and reset on restart.

### Restarts

The relay holds nothing a push depends on: each push carries its own handle, and
a credential proves itself with the relay's secret. So after a restart on the
memory store the next push simply works, with no phone or computer doing anything.

| After a restart on `memory` | Effect |
| --- | --- |
| Pushes | Keep working at once |
| Revocations | Forgotten. A revoked handle works again until its credential expires, at most 30 days on. Such a handle reaches only its own phone, and an alert sent with it shows only the fixed cover text. Use `sqlite:` or `postgres` to rule it out. |
| Attested keys, once App Attest exists | Forgotten. A phone attests again the next time it renews, in the foreground. |
| Rate limits | Reset |

**The secrets must survive.** `relay-key.pem` and `credential-secret` live on the
host, outside any store. Lose or replace them, as a rebuilt droplet would, and every
handle stops working: the relay answers `refused`, the daemon reports the handle
stale, and each phone repairs itself when it next opens.

### Staying connected

Every step can be repeated, so recovery needs no coordination:

- **Phone.** Sends everything (`POST /api/apns`) each time it comes to the
  foreground. The daemon replaces what it held, so repeating it changes nothing.
- **Daemon.** `RelayClient` holds no state about the relay. A dropped connection is
  opened again, and a failed call is tried twice more within seconds. A repeated push
  shows once, since pushes for a chat share a collapse id. `Stale` means only
  the phone can help, so it waits for the next registration.
- **Revoking** twice is harmless, so a daemon may repeat it until it hears back.

### 5. Revoke

Unpairing a computer revokes that computer's handles for the device. Deleting
the app lets the credential expire, and APNs reports the tokens gone.

## Opt-in

Relay delivery is off by default. A daemon with no relay setting makes no
connection to the relay, and the iOS app makes no attest call until the user
turns it on.

The choice is made in two places, because each side holds something different:

- **Sheprd**, Remote Control settings: `push_relay = false` in `[remote]`, for
  each computer. This allows content to leave that computer.
- **iOS app**, Settings > Live Activities: this allows the phone to attest and
  give out handles. When the user first pairs, or when a paired computer has
  the relay on, the app offers this with the same explanation.

Both must be on for pushes to flow. Turning either off stops them, and turning
off the phone's setting also revokes its handles.

Both settings show the same explanation. A draft:

> **Push through the Sheprd relay**
>
> Get Live Activities and alerts when you are away from this computer. Apple only
> accepts pushes for this app from Sheprd, so they go through a small server we
> run.
>
> **What the relay sees:** your phone's push token while it sends a push, the
> time and size of each push, and the IP addresses of your computer and phone.
>
> **What it never sees:** your prompts, chats, code, file names, or terminal
> output. Alerts and Live Activities are encrypted so that only your phone can
> read them.
>
> **What it cannot do:** connect to your computer, run anything, or answer for
> you. Tokens are not stored or logged.
>
> Apple also sees the push token, timing, and size of every push.

Keep this text true as the design changes: a test should check that a relay
payload carries nothing the text says the relay never sees. The Sheprd copy goes through `fl!`.

## Live Activity privacy

Relay users get the full Live Activity from the first release: agent, chat title,
workspace, current step, plan progress, the question, and answer buttons. All of
it travels sealed, so neither the relay nor Apple can read it.

**What is sealed.** A `sealed` field in `content-state`, and one in `attributes`
on a start event, using the per-computer key alerts already use. The widget
extension opens them to render. If a payload cannot be opened, the widget shows a
fixed "Sheprd" activity and no buttons, and never uses text from the push.

**What stays readable.** Only what iOS needs: `event`, `timestamp`,
`attributes-type`, dates, and the `alert` on a start event, which iOS draws itself
and so stays generic ("Sheprd", "New activity"). `computerId` is sealed too. The
app needs it to route activity tokens, and it reads it from the opened attributes.

**Size.** APNs allows 4,096 bytes. `ios/Spikes/seal-activity-push.swift` seals
the largest content the daemon writes:

| Push | ASCII | 3-byte text (CJK) |
| --- | --- | --- |
| Update | 1,654 B | 3,574 B |
| Start | 1,458 B | 2,525 B |

Both fit. Text made entirely of 4-byte characters such as emoji would not:
`excerpt` counts characters, not bytes. Today `Live::payload` refuses an oversized
push and it is lost, so relay mode must shorten the text to fit instead.

Own-key mode keeps plaintext payloads, since only the user and Apple are
involved. It can move to sealed payloads later.

**What the user chooses.** Sealing protects content from the relay. These settings
decide what the lock screen shows, which matters however the push travels:

| Shown | Detailed | Generic |
| --- | --- | --- |
| Agent, phase, plan counts, elapsed time | Yes | Yes |
| Chat title, workspace, steps, usage summary | Yes | No |
| Question, its buttons, failure message | With Excerpt | No |

Detailed is the default. Sheprd has `live_activities` in `[remote]`, and the iOS
app has Keep Generic in Settings, sent as `activity_detail`. A phone can ask for
less than its computer allows, never more. Buttons go with the question, because
nobody should answer from the lock screen without seeing what they are answering.

This depends on spike 2, which runs first. If the widget cannot open a sealed
payload on a locked phone, the fallbacks are, in order:

1. A software key in the App Group's keychain instead of a Secure Enclave one.
2. Sealing one key per activity at start, so updates need only AES-GCM.
3. Stop and decide. Plaintext through the relay would change what the opt-in text
   says, so it is not a silent fallback.

## If the server is hacked

The aim is that a full compromise of the relay gives an attacker very little,
and only for as long as they stay undetected.

| What they get | What it allows |
| --- | --- |
| A store, if one is used | Revocations, and attested public keys and dates. No tokens, content, or secrets. |
| The APNs key | Nothing on its own: Apple also needs a device token, and tokens exist only inside handles, which daemons hold. It is scoped to this app's topic (spike 1), so other apps on the team are out of reach. |
| The relay's private key | Opens handles, but the server stores none. To use it they would also need a handle from a daemon or the wire. |
| Live access to the running process | The real exposure. They see tokens, timing, and size for pushes sent while they are in. |

With tokens seen during that window, they could:

- Send plain alerts with their own text. An alert without `mutable-content` skips
  the Notification Service Extension, so the app cannot stop it. This is the one
  thing that cannot be designed away: lock screen spam or phishing text.
- Start activities that show only the fixed "Sheprd" fallback, with no buttons,
  since the widget uses no text it cannot open.

They cannot, even then:

- **Reach a computer.** The relay has no connection to any computer. Replies to
  daemons are bounded and can only cause a handle to be forgotten.
- **Answer or act for the user.** Actions go from the phone to the daemon. Live
  Activity buttons recheck the real chat, and sealed content cannot be forged,
  because only each computer holds the public key its content is sealed to.
- **Read alerts or Live Activities.** They are sealed to the phone.
- **Stop the rest of Sheprd.** They can drop pushes. Chats and terminals are
  unaffected.

`NotificationService` replaces an alert it cannot open with the fixed cover text,
dropping the push's actions and data. That closes the `mutable-content` variant of
forged text, though not the plain alert.

### Hardening the host

- The image has no shell or package manager, runs as a non-root user with a
  read-only filesystem, drops all capabilities, and sets `no-new-privileges`.
- The default store writes nothing. With `sqlite:`, the only writable path is its
  volume. Secrets are mounted files.
- Only ports 80 and 443 are open, with SSH by key only, or none where the host
  offers a console.
- Egress goes to APNs only, if the host allows filtering it. Checking an
  attestation should need only Apple's root certificate (spike 3).
- Request bodies, connections, and attest calls per IP are all bounded.

### Recovery

Written as a runbook in `deploy/push-relay/README.md`, and tried once on the
sandbox relay:

1. Revoke the APNs key in the Apple Developer site.
2. Deploy a clean image on a new host, with a new APNs key and new relay key.
3. Sign the new relay key with the offline root key.
4. Phones pick it up at their next attest. Old handles stop working.

The offline root key never touches the server or CI.

## Deployment

**Image.** `crates/sheprd-push-relay/Dockerfile`, multi-stage:

- Builds `-p sheprd-push-relay --locked` with the `rust-toolchain.toml` version,
  statically for musl, into a distroless `static` image. Base images are pinned
  by digest.
- The workspace's `[patch.crates-io]` and members need their manifests present,
  so the build context is the repository, trimmed by a `.dockerignore` that drops
  `ios/`, `site/`, `website/`, and `design/`. `Cargo.lock` has no git sources.
- Builds `linux/amd64` and `linux/arm64`, so cheap ARM hosts work too.

**CI.** A new `.github/workflows/push-relay.yml`, following the repository's
rules: actions pinned by SHA, `permissions: {}` at the top, the repository guard
`supply-chain.yml` uses, and zizmor clean.

- Pull requests that touch the relay build the image without publishing.
- On `main`, it pushes to `ghcr.io/sheprd-labs/sheprd-push-relay`, tagged by commit.
- The image is signed with cosign keyless, using the version `release.yml` pins,
  and gets a build provenance attestation and an SBOM.
- CI holds no relay or APNs secrets. It builds; it does not deploy.

**Running it.** `deploy/push-relay/compose.yml` holds the relay and Caddy, which
gets the TLS certificate itself. Any host with Docker works, such as a small
DigitalOcean droplet.

- Deploys pin the image by digest, after `cosign verify`, so a compromised
  registry tag cannot replace it.
- Each push costs one key agreement, one AES-GCM open, and one HTTP/2 request,
  and the daemon's pacer holds updates to one per ten seconds per chat. At this
  size the smallest instance is plenty. Measure it in phase 4 rather than trust
  this estimate.
- `GET /healthz` for an uptime monitor, and counters with no per-user labels.

## Changes by component

**New crate, `crates/sheprd-apns`.** The JWT, `Host`, `Outcome`, and HTTP/2
sender move here from `sheprd-remote-control`, so the daemon's own-key mode and
the relay share one implementation. No GPUI.

**New crate, `crates/sheprd-push-relay`.** The relay binary, on Tokio and axum
as `sheprd-remote-control` is, with stores behind a trait. It has its own
`Error`, bounded bodies, and tests with a mock APNs. Keys, the credential secret,
and deployment config stay on the host and out of the repository.

**`sheprd-remote-control`.**
- A `Delivery` enum with `Off`, `OwnKey(Sender)`, and `Relay(RelayClient)`
  replaces `Option<Sender>`, so `server/live.rs` and `server/notifier.rs` have
  one path. The own key wins when both are configured.
- `Registration` holds a recipient that is a token or a handle, as a closed
  enum with its own length bounds. `DeviceToken`'s hex check stays for tokens.
- The relay URLs are compiled in, so a paired device still cannot redirect the
  daemon: `push.sheprd.org`, and `push-sandbox.sheprd.org` for Xcode builds. The
  handle's environment picks one. Update the guarantee in `apns.rs` to name them.
- In relay mode, Live Activity payloads are sealed with `sheprd_apns::seal` under their
  own `info` string, after `Live::redacted`, and shortened to fit.
- `RelayClient` accepts only a sealed payload type, built by the sealing code, so
  sending plain text through the relay is a compile error, not a code review
  catch.
- `[remote]` gains `push_relay = false`, with a daemon flag and a `--help` entry.
- A `gone` reply forgets the handle, as `BadDeviceToken` forgets a token today.
- A relay outage must not stall the notifier. Alerts fall back to Web Push where
  a subscription exists, and activity updates are dropped, not queued.

**iOS.**
- The opt-in setting and explanation. Nothing below runs until it is on.
- App Attest registration and credential renewal, in the main app only.
- `ActivityTokens.sync` sends handles, and sends tokens only in own-key mode.
- A daemon reply of "push not configured" stops being an error: it no longer
  forgets the session or sets `lastError`.
- Settings says which delivery is active.
- The widget extension opens sealed activity content, in `ChatActivityState` and
  `ChatActivityFacts`, with shared fixtures for the Rust and Swift tests.

**Sheprd.**
- The opt-in setting and explanation in Remote Control settings, using the app's
  usual confirmation before it turns on.
- Status shown in the remote control view (off, own key, or relay, and whether
  the last push worked), with a `sheprd` command to read it, per `AGENTS.md`.
  The command only reads: turning the relay on stays in the settings.

**Docs.** `ios/README.md`, `crates/sheprd-remote-control/README.md` (the off,
own-key, and relay modes), `docs/remote-control-plan.md`, the App Store privacy
details (the relay receives device tokens), and a privacy policy entry.

## Phases

**0. Spikes.** Answer the four questions below before building. Spike 2 goes
first: the launch depends on it.

**1. Fixes that stand alone.** Done. They help whether or not the relay goes ahead:
- Live Activities honor `Privacy`, with Detailed and Generic on desktop and iOS.
- `ActivityTokens.sync` no longer treats the 503 as a session failure.
- Alerts that cannot be opened show fixed cover text.

**2. Extract and build.** Started. `sheprd-apns` is extracted with no behavior
change, and `sheprd-push-relay` runs against a mock APNs, covering malformed
handles, expiry, revocation, kind checks, limits, each APNs reply, and the test
that no token or payload reaches disk or logs. Still to build, after spike 3: App
Attest, `POST /v1/attest`, the signed relay key, and the `devices` table. Until
then nothing issues credentials, so the relay cannot serve a real phone. Also
still to add: the counters. Stores and `RelayClient` are built; the daemon uses
the client once `Delivery` lands in phase 4.

**3. Image and host.** Add the Dockerfile, the workflow, and the compose file.
Bring up a sandbox relay on a small host.

**4. Connect it.** Add `Delivery`, handles in `Registration`, the opt-in settings,
iOS attestation and handles, and sealed Live Activities. Test on a real device
against the sandbox relay, including a locked phone, and measure memory and CPU.

**5. Roll out.** Deploy production and offer it to TestFlight users. Watch
delivery rates, 429s, and `gone` replies. Try the recovery runbook on the sandbox
relay before announcing it.

## Acceptance

Before phase 5 offers the relay to anyone, from issue #1056:

- [ ] A TestFlight install, paired with a daemon that has no APNs key, gets sealed
      alerts and Live Activity updates through the relay.
- [ ] The relay never receives a plain-text alert body or Live Activity content.
- [ ] A handle from one phone cannot push to another phone.
- [ ] A daemon with its own key keeps sending directly, as today.
- [ ] Turning the relay off, in Sheprd or on the phone, stops every send through it.
- [ ] No token or payload reaches the relay's disk or logs.
- [ ] `ios/README.md` and `crates/sheprd-remote-control/README.md` document the
      three modes.

## Spikes

1. **Key scope.** Apple now offers [topic-specific keys](https://developer.apple.com/documentation/usernotifications/establishing-a-token-based-connection-to-apns),
   limited to chosen topics and one environment. Confirm one works for
   `so.pen.herdr.remote.push-type.liveactivity`. If it does, use one per
   environment instead of a team-scoped key.
2. **Sealed Live Activity (first, and blocking).** Built and waiting for a phone:
   see `docs/ios-spikes.md`. Payload size is measured above. Can the widget
   extension open sealed content on a locked phone? The alert key is `AfterFirstUnlockThisDeviceOnly` and the
   Notification Service Extension already uses it, but a Secure Enclave
   operation during a widget render is untested. Measure memory, render time,
   and how a real payload fits 4,096 bytes after base64.
3. **App Attest.** The details here came from general knowledge, because
   Apple's page could not be read while writing this. Confirm the attestation
   flow, how a TestFlight build is identified, and how its App Attest
   environment relates to `aps-environment`. Decide what the simulator does,
   since App Attest is not expected to work there.
4. **Background time.** Confirm the app can seal a new activity token and reach
   the daemon within the hold window in `server/live.rs`, on a real device with
   the app closed. Check what happens when the credential has lapsed.

## Decided

- Build it in Rust in this repository, with Docker images from CI.
- Run it on a cheap host, such as DigitalOcean.
- Opt-in, in settings, with an explanation of what the relay sees.
- Memory by default, with SQLite and Postgres behind the `Store` trait for when
  revocations must survive a restart or instances share state.
- Full Live Activities with meaningful detail at launch, sealed end to end.
- The relay lives on `sheprd.org`: `push.sheprd.org`, and
  `push-sandbox.sheprd.org` for development.

## Decisions needed

- **Self-hosted relays.** Anyone can run the code, but only for an app they sign
  themselves, for the reason given above. Say so in the docs rather than offering
  a relay URL setting.
