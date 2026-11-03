# Pairing a phone for remote control: the dialog with the QR code, then the
# step that asks to allow the phone. $device is the name the phone gave.

remote-pair-title = Remote control
remote-pair-close = Close
remote-pair-unavailable-here = Remote control is not available in this window.
remote-pair-off = Remote control is off.
remote-pair-off-detail = Turn it on in Settings > Remote control to pair a phone.
remote-pair-open-settings = Open Settings
remote-pair-approved = { $device } can now control your chats.
remote-pair-approved-detail = Revoke it any time in Settings > Remote control.
remote-pair-done = Done
remote-pair-denied = Not paired.
remote-pair-denied-detail = The device was turned away and its code no longer works.
remote-pair-expired = The request from { $device } expired.
remote-pair-expired-detail = Nothing was granted. Scan a new code to try again.
remote-pair-new-code = New code
# The reason follows on its own line.
remote-pair-failed = Pairing is unavailable.
remote-pair-try-again = Try again
# A phone that gave no name.
remote-pair-unnamed = A device

remote-pair-scan = Scan with your phone's camera
remote-pair-choose-address = Choose how your phone reaches this computer. Up/Down changes the QR address.
# $seconds until the QR code is replaced.
remote-pair-countdown = New code in { $seconds } s
remote-pair-asking = Asking for a pairing code…
# $fingerprint is the certificate authority's fingerprint.
remote-pair-fingerprint = CA { $fingerprint }
# Herdr Remote is the iPhone app's name.
remote-pair-nearby-mac = Or open Herdr Remote on an iPhone nearby: it finds this Mac over Bluetooth.
remote-pair-nearby = Or open Herdr Remote on an iPhone nearby: it finds this computer over Bluetooth.
remote-pair-nearby-allow-mac = To pair nearby over Bluetooth, allow Herdr in System Settings > Privacy & Security > Bluetooth.
remote-pair-nearby-allow = To pair nearby over Bluetooth, allow Herdr to use Bluetooth in your system's settings.

# The phone's name is drawn bold.
remote-pair-wants = { $device } wants remote control of your chats.
# $app is the address the phone's app was installed from.
remote-pair-app = It asks from the Herdr app at { $app }. Allow it only if you are adding this computer to that app.
remote-pair-same-code = Allow it only if the phone shows the same code.
remote-pair-deny = Deny
remote-pair-allow = Allow
remote-pair-allowing = Allowing…

# The addresses a phone may reach this computer at, each with the kind of
# network it is on.
remote-pair-network-bridge = Network bridge
remote-pair-network-local = Local network
remote-pair-network-loopback = This computer only
remote-pair-network-other = Network address
remote-pair-network-tailscale = Tailscale
remote-pair-hint-tailscale = Connect both devices to the same Tailscale network. Works across networks when access is allowed.
remote-pair-hint-local = Same local network; network isolation may block access.
remote-pair-hint-bridge = Local bridge; phone reachability is not verified.
remote-pair-hint-loopback = Only reachable on this computer, not from your phone.
remote-pair-hint-other = Phone reachability depends on your network.
# The network interface's name was not reported.
remote-pair-no-interface = Interface unavailable
remote-pair-older-daemon = Network labels need the updated remote-control daemon. Restart it in Settings > Remote control, then reopen pairing.
remote-pair-recommended = Recommended
# The kind of network the QR code's address is on.
remote-pair-qr-uses = QR uses { $network }
remote-pair-hide-addresses = Hide other addresses
remote-pair-show-addresses = Show other addresses
remote-pair-other-addresses = Other addresses ({ $count })
# What a screen reader says for an address: the network's kind, the
# interface, and the address.
remote-pair-address-selected = Selected: { $network } · { $interface } · { $address }
remote-pair-address-use = Use { $network } · { $interface } · { $address }

# Other machines the phone can be paired with at the same time.
remote-pair-hosts = Also pair it with these machines, so it reaches them while this computer sleeps:
