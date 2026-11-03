# Links to the app (herdr-gpui://…) that open a device, a folder, or a
# notification's pane: what the window's status line says when one could
# not be followed. The command line reports the same in English.

# $reason is one of the link-refused-* lines, or why a folder could not be
# opened.
link-open-failed = Open link: { $reason }
link-refused-closed = The window closed
link-refused-notification-gone = That notification is no longer known
link-refused-dialog-open = A dialog is open in the window; close it first
# $device is the device ID the link names.
link-refused-no-device = No device { $device }
link-refused-not-connected = Herdr UI is not connected to this computer's Herdr
# A checkout the link names is open, but a dialog keeps the window busy.
link-open-dialog-first = Open link: close the open dialog first
