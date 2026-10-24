# Herdr Remote for iOS

A small companion to Herdr's remote control web app. It shows each running
chat as a Live Activity on the lock screen and in the Dynamic Island, which a
web app cannot do. The web app stays the main app: tapping an activity opens
the chat's page there, in Safari.

| Path | What |
| --- | --- |
| `project.yml` | XcodeGen spec for the `HerdrRemote` app and the `HerdrRemoteWidgets` extension |
| `Shared/ChatActivityAttributes.swift` | The Live Activity's attributes, compiled into both targets |
| `HerdrRemoteCore/` | Swift package with logic that needs no UIKit: pairing links, fingerprints, session signatures, the activity's content, the API's JSON |
| `HerdrRemote/` | The app: pairing, Keychain, the pinned HTTPS client, push token upload |
| `HerdrRemoteWidgets/` | The lock screen and Dynamic Island layouts |

## Build

Requires Xcode 16 or later and [XcodeGen](https://github.com/yonaskolb/XcodeGen).

```sh
cd ios
xcodegen
open HerdrRemote.xcodeproj
```

Command line, without signing:

```sh
xcodebuild -project HerdrRemote.xcodeproj -scheme HerdrRemote \
  -destination 'generic/platform=iOS Simulator' CODE_SIGNING_ALLOWED=NO build
```

The package's tests run on the Mac, without a simulator:

```sh
cd ios/HerdrRemoteCore
swift test
```

The generated `HerdrRemote.xcodeproj`, Info.plists, and entitlements are not
committed; edit `project.yml` and run `xcodegen` again.

## Sign with your own team

Live Activities that a computer starts and updates need APNs, which needs a
paid Apple Developer account.

1. Set `DEVELOPMENT_TEAM` in `project.yml` (or in Xcode, Signing &
   Capabilities, for both targets).
2. If `so.pen.herdr-remote` is taken for your team, change both bundle
   identifiers (`so.pen.herdr-remote` and `so.pen.herdr-remote.widgets`;
   the extension's must start with the app's) and use the new one as
   `topic` below.
3. Keep the Push Notifications capability (the `aps-environment`
   entitlement). Xcode registers the App ID with it when signing
   automatically.
4. Run on a device. Live Activities and the Secure Enclave do not work fully
   in the simulator; there the app uses a software device key.

## Pair

In Herdr, open Remote control > Pair device, then in the app tap +, and scan
the QR (or paste its link). Compare the 6-digit code with the computer's
dialog and choose Allow there. The app keeps the computer's addresses and
its CA fingerprint in the Keychain, with a device key in the Secure Enclave.

The app trusts the computer through the fingerprint in the QR alone, so it
needs no certificate profile.

Opening a chat happens in Safari (or the web app on the Home Screen), which
is a separate device with its own key: pairing the companion does not sign
Safari in. Set Safari up once:

1. In Herdr, open Remote control > Pair device again for a fresh QR.
2. Scan it with the Camera app (not the companion), and open the link in
   Safari.
3. Install the certificate profile the pairing page offers, then turn on
   full trust for it in Settings > General > About > Certificate Trust
   Settings.
4. Back on the pairing page, name the device, compare the 6-digit code with
   the computer's dialog, and choose Allow there.

Settings > Remote control then lists two devices, the companion and Safari;
revoke each on its own.

## APNs key for the daemon

The daemon (`herdr-remote-control`) sends Live Activity pushes with your own APNs
token key. Without it, everything else works and activities never appear.

1. In the Apple Developer site, Certificates, Identifiers & Profiles > Keys,
   create a key with Apple Push Notifications service (APNs) enabled, and
   download `AuthKey_KEYID.p8` (it downloads once).
2. Put it in the daemon's state directory as `apns/AuthKey.p8`, readable only
   by you:

   ```sh
   mkdir -p STATE_DIR/apns
   cp AuthKey_KEYID.p8 STATE_DIR/apns/AuthKey.p8
   chmod 600 STATE_DIR/apns/AuthKey.p8
   ```

3. Beside it, write `STATE_DIR/apns/config.json`:

   ```json
   {
     "team_id": "ABCDE12345",
     "key_id": "KEYID12345",
     "topic": "so.pen.herdr-remote",
     "sandbox": true
   }
   ```

   `team_id` is your ten-character team, `key_id` the key's, and `topic` the
   app's bundle identifier. The daemon adds `.push-type.liveactivity` itself.

4. Restart remote control in Herdr, so the daemon reads them at start.

`STATE_DIR` is the remote control daemon's private state directory (its
`--state-dir`), the one holding `devices.json` and `push/vapid.pem`.

The daemon side is built (`crates/herdr-remote-control/src/apns`) but not yet wired
into the server's `POST /api/apns` route and notifier; until it is, the app
pairs and uploads tokens, and the upload is answered 404.

### Sandbox or production

An app run from Xcode with a development profile gets tokens for the APNs
sandbox: set `"sandbox": true`, and the daemon posts to
`api.sandbox.push.apple.com`. A TestFlight or App Store build gets production
tokens: set `"sandbox": false` (or leave it out), the daemon posts to
`api.push.apple.com`, and change `aps-environment` in `project.yml` to
`production`. A token sent to the wrong one is refused as `BadDeviceToken`,
and the daemon then forgets it, so switch both together.

## What the daemon sends

Payloads follow `ChatActivityState` (`HerdrRemoteCore`) and
`crates/herdr-remote-control/src/apns/payload.rs`; the fixtures in
`HerdrRemoteCore/Tests/HerdrRemoteCoreTests/Fixtures` are tested on both
sides. A turn starting sends `start` through the push-to-start token; tool
calls, plan steps, and questions send `update` (questions at priority 10, with
an alert); the turn's end sends `end`, which stays on the lock screen for
fifteen minutes (a cancelled turn leaves at once).
