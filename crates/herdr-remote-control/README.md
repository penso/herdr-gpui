# Remote control daemon

`herdr-remote-control` serves this machine's `herdr-acp` chats to paired phones over
HTTPS: a mobile web app (installable as a PWA) that lists workspaces, follows
chats, answers agent questions, and receives push notifications. The plan is
`docs/remote-control-plan.md`.

The daemon is a user service that Herdr UI installs when Remote control is
turned on, and it keeps running when the app quits. It never starts, stops,
or upgrades a Herdr daemon.

## Control protocol

`control` is the wire between Herdr UI and the daemon: one request and one
response per connection, as length-prefixed JSON frames
(`herdr_chat_protocol::write_frame`) over a private endpoint at
`control::address(state_dir)`. Herdr UI depends on this crate with
`default-features = false`, which leaves out the server and Tokio, and calls
`control::request` from a background thread.

## Pairing and authentication

`auth` (server only) holds the pairing the desktop dialog opens, the paired
devices in `devices.json` (0600, replaced atomically), challenge-signed
sessions, and per-address rate limits. It is synchronous and takes every time
as milliseconds since the Unix epoch, so the server calls it under a mutex and
tests pass explicit times.

The pairing QR prefers a Tailscale address when one is available, then a LAN
address. The desktop dialog highlights the Tailscale connection and reveals
alternatives under **Other addresses**. Each address names its interfaces on
the daemon's host (for example `utun12`, `en0`, or `bridge100`), with a network
label and reachability guidance. A private IP alone does not identify Wi-Fi
versus Ethernet, and a CGNAT address alone does not prove Tailscale. Unknown
interfaces, including those from older daemons, are labelled unavailable.
After a development rebuild, restart the background daemon in Settings >
Remote control and reopen pairing to get the new metadata. An older daemon's
offer shows this instruction in the dialog; rebuilding the GUI alone does not
replace an already-running daemon.
The same metadata is included in `remote status` and its JSON output.
The desktop dialog lets you select any advertised HTTPS address and
keeps that choice when the QR token refreshes. Use Up/Down to select an address
with the keyboard, or Home/End to select the first/last; the selected row stays
visible, automatically revealing alternatives. Right/Left expand/collapse the
other addresses without changing the selection. Escape closes the dialog. For pairing across networks,
connect both devices to the same Tailscale network and allow access to the
remote-control port (7443 by default). Local addresses still work on the same
network. Pair and install the phone app from the stable Tailscale address to
avoid tying its browser origin to temporary Wi-Fi. Bluetooth is not used.

The native pairing dialog, shown with synthetic addresses and an inactive QR:

| Light | Dark |
| --- | --- |
| ![Tailscale-first pairing](../../docs/screenshots/remote-pairing-light.png) | ![Selected IPv6 address with alternatives collapsed](../../docs/screenshots/remote-pairing-dark.png) |

[Expanded alternatives in a narrow native window](../../docs/screenshots/remote-pairing-narrow.png).

## Push notifications

`push` (server feature only) sends Web Push: a VAPID key created once at
`STATE_DIR/push/vapid.pem` (0600), RFC 8291 `aes128gcm` encryption, the
HTTPS post to the phone's push service, the triggers that turn chat changes
into questions, finished turns, and errors, and a per-device throttle. The
service worker receives `push::Message` as JSON.

## Errors

The crate exports `Error` and `Result<T>`.
