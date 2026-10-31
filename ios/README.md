# Herdr Remote for iOS

The native app for Herdr's remote control (`docs/ios-app-plan.md`).
- **What it shows:** the chats and terminal tabs running in Herdr on every
  paired computer, merged into one list, with what needs you first.
- **What you can do:** answer questions in place, open a chat or a terminal,
  and send prompts.
- **Live Activities:** they put running chats on the lock screen and in the
  Dynamic Island.
- **Launch:** it draws the last state from its cache at once, then catches up
  over one live socket per computer.

| Path | What |
| --- | --- |
| `project.yml` | XcodeGen spec for the `HerdrRemote` app and the `HerdrRemoteWidgets` extension: iOS 26, Swift 6 with complete concurrency checking, iPhone and iPad |
| `HerdrRemoteCore/` | Swift package with no UIKit: the transcript model and its reducer, the v2 API's messages, `ComputerLink` (address racing, resubscription, backoff), pairing links, fingerprints, session signatures, sealed notifications |
| `HerdrRemote/` | The app: `Fleet` (one link per computer), `Cache`, the pinned HTTPS and WebSocket client, pairing, Keychain, and `Views/` |
| `HerdrRemoteWidgets/` | The lock screen and Dynamic Island layouts |
| `Shared/` | The Live Activity's attributes, compiled into both targets |
| `Mockups/` | The SwiftUI mockups the screens were picked from (`just ios-mockups`) |
| `Spikes/` | Phase 0 spikes (`docs/ios-spikes.md`) |
| `tools/` | The simulator loop's control client and the TestFlight upload script |

## Build and test

Requires Xcode 26 or later and [XcodeGen](https://github.com/yonaskolb/XcodeGen).

```sh
just ios-test      # Swift package tests on the Mac, no simulator
just ios-build     # unsigned simulator build of the app and its extensions
just ios-generate  # then open ios/HerdrRemote.xcodeproj
```

- **Fixtures:** the package tests decode the JSON fixtures that the
  daemon's Rust tests write from its own types
  (`HerdrRemoteCore/Tests/HerdrRemoteCoreTests/Fixtures/v2`). The reducer is
  checked against the same change sequences on both sides, so the two
  cannot drift apart.
- **Generated files:** the `.xcodeproj`, Info.plists, and entitlements are
  not committed. Edit `project.yml` and run `xcodegen` again.

## The simulator loop

Test against a sandbox daemon in a headless simulator. No taps are needed,
and nothing goes through your phone.

1. **Start the daemon.** Use a short state directory, since Unix socket
   paths are limited:

   ```sh
   cargo build -p herdr-remote-control --locked
   state=$(mktemp -d /tmp/hrc.XXXXXX)
   ./target/debug/herdr-remote-control serve --state-dir "$state" --port 7543 2> "$state/daemon.log" &
   ```

   - It reads the local Herdr's workspaces.
   - The macOS firewall blocks an unsigned debug daemon on LAN and
     Tailscale addresses, so the simulator uses `127.0.0.1`.
2. **Pair without the camera.** Build signed to run locally, so the app
   has its entitlements and the simulator's keychain keeps the pairing
   across launches (`just ios-build` is unsigned, and its keychain calls
   fail with "A required entitlement isn't present"):

   ```sh
   cd ios && xcodebuild -project HerdrRemote.xcodeproj -scheme HerdrRemote \
     -destination 'platform=iOS Simulator,id=DEVICE' -derivedDataPath build \
     CODE_SIGN_IDENTITY=- CODE_SIGN_STYLE=Manual DEVELOPMENT_TEAM= build
   ```

   Then pair. The tool approves as the desktop dialog would:

   ```sh
   link=$(ios/tools/remote_control.py "$state" pair | sed -E 's#https://[^/]+/pair#https://127.0.0.1:7543/pair#')
   xcrun simctl launch DEVICE so.pen.herdr.remote -pairLink "$link"
   ios/tools/remote_control.py "$state" approve
   ```

3. **Drive it with debug launch arguments.**
   - `-openLink 'herdr-remote://chat?id=FINGERPRINT&computer=NAME&pane=PANE'`
     opens a chat, as tapping a Live Activity does.
   - `-openTerm PANE` opens a terminal; `-type TEXT` then types into it
     (a newline presses Enter).
   - `-tab workspaces|settings` opens that tab, and `-openWorkspace ID`
     opens that workspace's screen in it.
   - `-newWorkspace YES` opens the new workspace form.
   - `-syncBar waiting|failed` shows the sync bar as if a change were on
     its way or had failed.
   - With `-openLink`, `-session YES` opens the chat's session sheet and
     `-draft TEXT` types into its composer (`-draft /` lists the slash
     commands).
4. **Read the results.** `xcrun simctl io DEVICE screenshot FILE` captures
   the screen, and `daemon.log` lists every request.
   - The cache sits in the App Group container
     (`xcrun simctl get_app_container DEVICE so.pen.herdr.remote group.so.pen.herdr.remote`).
   - Avoid `simctl openurl`: its confirmation alert needs a tap, and it
     keeps the app inactive until the simulator restarts.

## Sign with your own team

Live Activities and pushes need APNs, which needs a paid Apple Developer
account.

1. Set `DEVELOPMENT_TEAM` in `project.yml` (or in Xcode, Signing &
   Capabilities, for both targets).
2. Rename the bundle identifiers if `so.pen.herdr.remote` is taken for your
   team.
   - Change both `so.pen.herdr.remote` and `so.pen.herdr.remote.widgets`;
     the extension's must start with the app's.
   - Change the App Group `group.so.pen.herdr.remote` too.
   - Use the new identifier as `topic` below.
3. Run on a device. Live Activities and the Secure Enclave do not work fully
   in the simulator; there the app uses a software device key.

## TestFlight

`.github/workflows/ios.yml` uploads every change to `main` that touches the
app, once the repository's `testflight` environment holds these secrets:

| Secret | What |
| --- | --- |
| `APPLE_TEAM_ID` | Your ten-character team |
| `ASC_KEY_ID`, `ASC_ISSUER_ID` | An App Store Connect API key with the App Manager role (Users and Access > Integrations) |
| `ASC_KEY_P8` | The key's `.p8` file, its whole text |

- **How it signs:** Xcode signs automatically with that key, so no
  certificates or profiles are stored.
- **What it uploads:** the export re-signs for distribution and uploads to
  App Store Connect.
- **Build number:** one more than the highest build App Store Connect has
  for the app, asked for with the same key (`ios/tools/next-build.py`), so
  CI and local uploads count up together. Set `BUILD_NUMBER` to choose one.
- **Running it locally:** `just ios-testflight` does the same with the same
  variables. Any it does not find set come from `pass`: the issuer ID from
  `apple/app-store-connect-issuer-id` and the key from
  `herdr-gpui/apple-api-key-KEYID`.

## Pair

1. In Herdr, open Remote control > Pair device.
2. In the app, tap +, and scan the QR (or paste its link).
3. Compare the 6-digit code with the computer's dialog and choose Allow
   there.

The app keeps the computer's addresses and its CA fingerprint in the
Keychain, with a device key in the Secure Enclave. It trusts the computer
through the fingerprint alone, so it needs no certificate profile.

### Nearby, without the camera

While the pairing dialog is open, the computer can also be found nearby:

- **Same network:** `_herdr-remote._tcp` with `pairing=1` in its TXT
  record.
  - On macOS, the `herdr-nearby` helper advertises it through the system.
  - Elsewhere, the daemon advertises it.
- **Bluetooth:** the `herdr-nearby` helper beside Herdr advertises the
  nearby service. On macOS it is the Swift helper; on Linux (BlueZ) and
  Windows (WinRT) it is the Rust crate `crates/herdr-nearby`.

The app shows a card for it (or lists it in the pair sheet). Tap Pair,
compare the six digits with the computer's dialog, and choose Allow
there. The code comes from a commitment-based exchange
(`docs/ios-app-plan.md`, "Nearby pairing, exactly"), so it also vouches
for the CA fingerprint the claim then pins.

## APNs key for the daemon

The daemon (`herdr-remote-control`) sends Live Activity pushes with your own
APNs token key. Without it everything else works, and the app's token upload
is answered 503.

1. **Create the key.** In the Apple Developer site, go to Certificates,
   Identifiers & Profiles > Keys. Create a key with Apple Push Notifications
   service (APNs) enabled, and download `AuthKey_KEYID.p8` (it downloads
   once).
2. **Install it.** Put it in the daemon's state directory as
   `apns/AuthKey.p8`, readable only by you:

   ```sh
   mkdir -p STATE_DIR/apns
   cp AuthKey_KEYID.p8 STATE_DIR/apns/AuthKey.p8
   chmod 600 STATE_DIR/apns/AuthKey.p8
   ```

3. **Configure it.** Beside it, write `STATE_DIR/apns/config.json`:

   ```json
   {
     "team_id": "ABCDE12345",
     "key_id": "KEYID12345",
     "topic": "so.pen.herdr.remote",
     "sandbox": true
   }
   ```

   - `team_id` is your ten-character team.
   - `key_id` is the key's ID.
   - `topic` is the app's bundle identifier. The daemon adds
     `.push-type.liveactivity` itself.
4. **Restart.** Restart remote control in Herdr, so the daemon reads them at
   start.

`STATE_DIR` is the remote control daemon's private state directory (its
`--state-dir`).

### Sandbox or production

The two must match, so switch both together.

- **Xcode builds:** an app run from Xcode gets sandbox tokens. Set
  `"sandbox": true`.
- **TestFlight and App Store builds:** these get production tokens. Set
  `"sandbox": false`.
- **A mismatch:** a token sent to the wrong service is refused as
  `BadDeviceToken`, and the daemon then forgets it.

## Sealed alerts

With the APNs key installed, questions, finished turns, and errors reach
the phone as APNs alerts instead of Web Push. These alerts are sealed to
the phone: Apple sees only "Herdr", "New activity", and ciphertext.

1. **Registration.** The app asks to show alerts after the first pairing.
   It then registers its APNs token with each computer, along with the
   public half of that computer's own key-agreement key: a Secure Enclave
   key in the App Group's keychain group, or a software key in the
   simulator.
   - An alert counts as from the computer whose key opens it, and is
     dropped if its text names another computer.
   - So one paired computer cannot send an alert whose actions reach
     another.
2. **Opening.** The Notification Service Extension
   (`HerdrRemoteNotifications`) opens each alert. It shows the chat's
   title and text, groups the alert with the chat's other notifications,
   and adds the actions.
3. **Actions.**
   - **Questions:** up to four answers. "Always" answers ask for Face ID
     first.
   - **Finished turns:** Reply, whose text becomes the next prompt. Reply
     asks for Face ID or the passcode on a locked phone.
   - **Where they run:** an action runs in the app without opening it,
     and signs in with the device key.
4. **Clears and badges.** An answer given elsewhere sends a background
   push that removes the chat's notifications. Badges show the questions
   waiting.

A Live Activity in the question phase also offers up to three answers
(never the "always" ones) as buttons that answer from the lock screen.
Any computer can start an activity that names another computer's chat.
So a button answers only after the app reads that chat and finds the
same question still waiting, with the same answer offered.

The simulator delivers `simctl push` payloads but does not run the
extension. For the simulator loop:
- `-provisionalAlerts YES` grants quiet permission without the prompt.
- `-exportNotifyKey FINGERPRINT` writes that computer's alert key to the
  App Group, where a script can seal a payload to it.
- `-dumpNotifications YES` writes what was delivered, and opens each
  alert in the app as the extension would.

## Around the system

- **Widgets:**
  - Needs You: small, medium (the chats as links), and the lock screen
    sizes. It is drawn from `summary.json` in the App Group, which the app
    rewrites as homes change.
  - The file is readable after the first unlock, so lock screen widgets
    draw. The full cache stays readable only while unlocked.
- **Controls:** Herdr Chats and New Herdr Workspace, for Control Center,
  the Lock Screen, and the Action button.
- **Siri and Shortcuts:**
  - What needs me, Ask a chat, Stop a chat, and Open the chats.
  - Asking and stopping need an unlocked phone.
  - Chats are `ChatEntity`s that Spotlight indexes. Tapping one opens the
    chat.
- **Share sheet:** "Herdr" sends text, a link, or up to four pictures to
  a chat as a prompt. It signs in like the app does, because the paired
  computers' keychain item lives in the App Group's keychain group. An
  item from before moves there on first launch.
- **Face ID:**
  - Optional: Settings > Privacy locks the app and hides it in the app
    switcher.
  - "Always" answers and auto-accept ask either way.

## iPad and Apple Watch

- **iPad:**
  - The attention list sits beside the chat, with the workspace's terminal
    in an inspector (⌘⌥I).
  - A chat or terminal opens in its own window from its context menu.
  - Keyboard: ⌘N, ⌘⇧B, ⌘, and ⌘⌥↑ / ⌘⌥↓.
- **Apple Watch:**
  - Shows what needs you and what is running, and answers with a tap or a
    dictated reply.
  - It holds no keys. Every request goes to the phone over
    WatchConnectivity, and the phone checks it against the chats it last
    saw before sending it with its device key.
  - "Always" answers stay on the phone.
  - With the Face ID lock on, the phone refuses Watch requests, as it
    refuses Siri's.
  - For the simulator: `-sample YES` shows made-up chats, and
    `-openFirst YES` opens the first.

## What the daemon sends

Live Activity payloads follow `ChatActivityState` (`HerdrRemoteCore`) and
`crates/herdr-remote-control/src/apns/payload.rs`; both sides test the
fixtures under `HerdrRemoteCore/Tests/HerdrRemoteCoreTests/Fixtures`.

- **Start:** a turn starting sends `start` through the push-to-start token.
- **Update:** tool calls, plan steps, and questions send `update`. Questions
  go at priority 10, with an alert.
- **End:** the turn's end sends `end`, which stays on the lock screen for
  fifteen minutes. A cancelled turn leaves at once.
