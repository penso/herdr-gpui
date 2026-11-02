# Pages an agent on another device asks this app to open, through the
# herdr-gpui command there. These are brief notices over the terminal.

# $device is the device's name.
remote-browser-bad-address = An agent on { $device } asked for an address that cannot open
# $address is the page's address on the device's own loopback.
remote-browser-needs-ssh = An agent on { $device } asked for { $address }, which only a saved SSH host can forward
remote-browser-too-many-forwards = Too many pages are waiting for a forward
remote-browser-forward-timeout = Port { $port } took too long to forward
remote-browser-too-many-tabs = Too many browser tabs are open
