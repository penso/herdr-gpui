# The main window's own cards over the terminal area: the reconnecting card
# and the daemon's product announcement. Host names and the announcement's
# title and body come from the user and the daemon.

# The card over a dropped connection; $host is the device's name.
window-reconnecting = Reconnecting to { $host }
window-reconnect-now = Reconnect now
# Shown when SSH refused the device's host key, to see it and trust it.
window-review-host-key = Review host key
# The same card while the device is being removed on purpose.
window-removing = Removing { $host }
window-device-being-removed = It is being removed.
# The announcement's caption; $version is the Herdr release it is about.
window-announcement = Herdr { $version }
window-announcement-preview = Herdr { $version } · preview
window-dismiss = Dismiss
# $error is why the daemon could not mark the announcement seen.
window-dismiss-announcement-failed = Dismiss announcement: { $error }
