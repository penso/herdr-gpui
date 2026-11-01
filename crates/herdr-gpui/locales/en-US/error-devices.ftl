# Why adding, saving, removing, or discovering a device failed.
# $status is how Herdr's command exited, in the system's words; $detail is
# what it printed.

error-devices-adding = This host is already being added, in another Add Device dialog or a “Set up” workspace that is still running. Finish or close it, then try again.
# $label is the name the device was saved under.
error-devices-exists = This host and session are already saved as “{ $label }”.
error-devices-remove = Removing the device failed ({ $status })
error-devices-remove-detail = Removing the device failed ({ $status }): { $detail }
error-devices-rename = Renaming the device failed ({ $status })
error-devices-rename-detail = Renaming the device failed ({ $status }): { $detail }
error-devices-server-restart = The Herdr server on this host was started in a way that may not survive a dropped SSH connection, so Herdr has to restart it before saving the device. Restarting stops the panes running there.
error-devices-setup = Saving the device failed ({ $status })
error-devices-setup-detail = Saving the device failed ({ $status }): { $detail }
error-devices-setup-no-profile = This device has no saved profile.
error-devices-setup-label = Device names are at most 128 bytes, without control characters.
error-devices-setup-target = Enter an SSH target such as user@hostname or an SSH alias.
error-devices-setup-session = Session names use letters, digits, dots, underscores, or hyphens (at most 64 bytes), excluding '.' and '..'.
error-devices-setup-state-directory = Herdr UI has no state directory.
error-devices-setup-paths = The setup paths must be UTF-8.
error-devices-setup-windows = Setting up a device in a terminal is unavailable on Windows.
error-devices-setup-timeout = Saving the device timed out
error-devices-tailscale-json = Tailscale returned an unreadable status: { $error }
error-devices-tailscale-timeout = Tailscale did not answer in time
