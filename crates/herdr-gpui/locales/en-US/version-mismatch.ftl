# The notice that a Herdr daemon refused this app's handshake. $place is
# "local" for this device, or "host" for an SSH or WSL device named $host.
# $version is the daemon's version. The command to run, `herdr update`, is
# shown beside the text.

version-mismatch-daemon-title = Herdr needs an update
version-mismatch-client-title = Herdr UI needs an update
version-mismatch-daemon-old = { $place ->
        [local] The Herdr server on this device (version { $version }) is too old for this app. Run this command in a terminal; the app reconnects once the updated server is running.
       *[host] The Herdr server on { $host } (version { $version }) is too old for this app. Run this command on { $host }; the app reconnects once the updated server is running.
    }
# Releases before the endpoint protocol report no version; $minimum is the
# oldest Herdr this app works with.
version-mismatch-daemon-silent = { $place ->
        [local] The Herdr server on this device did not answer this app's handshake. This app needs Herdr { $minimum } or newer. Run this command in a terminal; the app reconnects once the updated server is running.
       *[host] The Herdr server on { $host } did not answer this app's handshake. This app needs Herdr { $minimum } or newer. Run this command on { $host }; the app reconnects once the updated server is running.
    }
version-mismatch-client-old = { $place ->
        [local] The Herdr server on this device (version { $version }) is newer than this app supports. Update this app to connect.
       *[host] The Herdr server on { $host } (version { $version }) is newer than this app supports. Update this app to connect.
    }
version-mismatch-client-old-unknown = { $place ->
        [local] The Herdr server on this device is newer than this app supports. Update this app to connect.
       *[host] The Herdr server on { $host } is newer than this app supports. Update this app to connect.
    }
# $command is the command copied to the clipboard.
version-mismatch-copied = Copied `{ $command }`
version-mismatch-copy = Copy Command
version-mismatch-check = Check for Updates
version-mismatch-dismiss = Dismiss
