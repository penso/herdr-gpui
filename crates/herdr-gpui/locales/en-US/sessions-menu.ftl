# The sessions popup: this machine's Herdr sessions and each saved device's,
# and adding or deleting one. Device and session names are shown as they are.

# Whether a session's daemon runs.
sessions-menu-running = running
sessions-menu-stopped = stopped
# Section headings, in capitals. $device is the device's name, in capitals.
sessions-menu-current-device = { $device } · CURRENT DEVICE
sessions-menu-local = LOCAL SESSIONS
sessions-menu-remote-devices = REMOTE DEVICES
sessions-menu-other-devices = OTHER DEVICES
sessions-menu-no-devices = No saved devices. Add one from the device picker.
sessions-menu-no-device-sessions = No sessions on this device.
sessions-menu-disabled = Disabled
sessions-menu-checking = Checking…
sessions-menu-no-local-sessions = No sessions on this machine.
sessions-menu-stopped-note = Stopped sessions start their daemon when selected.
sessions-menu-deleting = Deleting…

sessions-menu-add-session = Add session…
sessions-menu-create-and-connect = Create and connect
# --socket and --dev are command line options; keep them as they are.
sessions-menu-unavailable-local = Unavailable with --socket or --dev
sessions-menu-enable-device = Enable this device to manage sessions
sessions-menu-device-offline = Available once this device is online
sessions-menu-wsl-windows-only = WSL is only available on Windows

# Why a session cannot be deleted.
sessions-menu-wait-for-delete = Wait for session deletion to finish.
sessions-menu-select-to-delete = Select a session to delete.
sessions-menu-default-undeletable = Herdr cannot delete the default session.
sessions-menu-management-unavailable = Session management is unavailable for explicit sockets, development catalogs, or disabled devices.
sessions-menu-refresh-first = Refresh the session list before deleting.
sessions-menu-profile-uses = A saved device profile uses this session. Remove or reconfigure that profile before deleting it.
sessions-menu-wait-for-list = Wait for this device's session list before deleting.
sessions-menu-not-listed = This session is not in the device's latest list.
sessions-menu-device-changed = The device changed. Reopen the session picker.
sessions-menu-device-unavailable = The device changed or is unavailable. Reopen the session picker.

sessions-menu-name-placeholder = Session name
sessions-menu-invalid-name = Use 1–64 ASCII letters, digits, dots, underscores or hyphens. The names ‘.’ and ‘..’ are not allowed.
sessions-menu-exists = That session already exists. Select it from the list.
sessions-menu-add-title = Add session
sessions-menu-this-machine = this machine
# $distribution is a WSL distribution's name, such as Ubuntu.
sessions-menu-wsl = WSL { $distribution }
# $destination is "this machine", an SSH host, or a WSL distribution.
sessions-menu-on = On { $destination }
sessions-menu-add-note = Creates and connects to a headless session. An existing name connects to that session.
sessions-menu-delete-title = Delete session?
# "default" is the default session's name; keep it as it is.
sessions-menu-delete-note = Herdr will stop this session, terminate its running processes, and remove its saved state. If you are using it, this window switches to default. This cannot be undone.
sessions-menu-cancel = Cancel
sessions-menu-delete-permanently = Delete permanently
