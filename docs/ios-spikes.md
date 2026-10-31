# Phase 0 spikes: results

The risky pieces of `docs/ios-app-plan.md`, tried before building on them.
Spike code: `ios/Spikes/` (an iPhone app with a Notification Service
Extension and a widget extension, plus `mac/`, the Mac's Bluetooth side),
`ios/HerdrRemoteCore` (`NotificationSeal`), and
`crates/herdr-remote-control/src/apns/seal.rs`.

| Spike | Result | Evidence |
| --- | --- | --- |
| Sealed notifications: daemon seals, phone opens | **Works** | Rust (`apns::tests::seal`) produces byte for byte the payload CryptoKit sealed in `Fixtures/seal.json`; Swift `SealTests` opens it and refuses a changed byte, another device's key, and a truncated payload. |
| Key in an App Group keychain access group | **Works on the simulator** | The spike app made and read back its key-agreement key in `group.so.pen.herdr-spikes` (ad-hoc signed build, no team prefix). |
| Notification Service Extension opens a push | **Not shown on the simulator; needs a device** | `simctl push` delivered the notification, but with provisional authorization (the only kind granted without a tap) alerts are disabled and the system did not run the extension; the notification kept its placeholder text. |
| Live Activity with `Button(intent:)` | **Starts; tap needs a device** | `Activity.request` succeeded with the widget extension's `LiveActivityIntent` buttons. A headless simulator cannot tap the lock screen, and its screenshots did not show the island. |
| Intent calls the computer over pinned TLS | **Builds; needs a device** | `SpikeAnswerIntent.call()` uses the app's pinning; "Run the intent here" runs it in the app. |
| Bluetooth peripheral on the Mac from Rust | **Possible, but not recommended** | In `objc2-core-bluetooth` 0.3.2 every Core Bluetooth method is an `unsafe fn`, and delegate conformances are `unsafe impl`. A Rust peripheral would be dozens of unsafe calls, against the repository's safe-Rust rule. |
| Bluetooth peripheral on the Mac in Swift | **Builds; needs the user's permission and an iPhone** | `ios/Spikes/mac/Peripheral.swift`: GATT service, echo characteristic, L2CAP channel. `run.sh` wraps it in an app bundle with `NSBluetoothAlwaysUsageDescription`, which macOS requires before Bluetooth access. |
| iPhone central: restoration, latency, throughput | **Builds; needs a device** | The simulator has no Bluetooth. `App/Central.swift` logs relaunches by state restoration, 20 echo round trips, and 1 MiB each way over L2CAP. |
| Core Bluetooth under Swift 6 | **Settled** | Core Bluetooth has no Sendable annotations. A `@MainActor` class with `@preconcurrency` delegate conformances and `queue: nil` compiles under complete checking and asserts the main queue at run time. |

## Decision proposed by the spikes

**The Mac's Bluetooth side becomes a small Swift helper, `herdr-nearby`,
inside the app bundle.**
- Herdr UI starts it and exchanges newline-delimited JSON with it on stdio:
  advertise, stop, frames in and out, and presence. The SAS crypto stays in
  the daemon, and the helper only carries bytes, as the plan already says
  of Herdr UI.
- Swift keeps Core Bluetooth safe and idiomatic.
- A helper inside the bundle is attributed to Herdr.app for the Bluetooth
  permission.
- The cost is a `swiftc` step in macOS packaging.

Linux (BlueZ over D-Bus) and Windows (WinRT) stay as planned, in Rust.

## On a device

Needs the user's Apple team in `ios/Spikes/project.yml` (`DEVELOPMENT_TEAM`),
an iPhone on iOS 26, and this Mac.

1. **Bluetooth**
   - Run `ios/Spikes/mac/run.sh /tmp/herdr-ble-spike.log` on the Mac and
     allow Bluetooth when macOS asks.
   - Install HerdrSpikes on the iPhone, open it, and allow Bluetooth.
   - Read the Bluetooth tab and the log: connect time, echo median and
     worst, and L2CAP KiB/s each way.
2. **State restoration**
   - Leave the app in the background and walk out of range for a minute,
     then come back.
   - The log should show `RESTORED by the system` and a reconnect, without
     the app being opened.
3. **Sealed notifications**
   - In the Notify tab, Register, allow notifications, and copy the key and
     the APNs token.
   - Send `swift ios/Spikes/seal-push.swift KEY` as a sandbox push with the
     daemon's APNs key (or the Push Notifications Console).
   - The banner must show the decrypted title and body, which proves the
     extension can use the Secure Enclave key from the shared group.
4. **Live Activity button**
   - In the Activity tab, paste a pairing link from Herdr's Remote control
     dialog (only its address and fingerprint are used), Save, and Start.
   - Lock the phone and press Allow once.
   - The activity should show `HTTP 200 in N ms`.
