# A window's devices: connecting to them, switching between them, and saving
# which one it shows. Errors appear in the main window's status line.

# The reason shown while reconnecting when the connection gave none.
endpoint-connection-lost = connection lost
# A device's state in the device picker. "Herdr update needed" means the
# device's Herdr is too old for this app; "app update needed" means this app
# is too old for it.
endpoint-status =
    { $state ->
        [disabled] disabled
        [stopped] stopped
        [detached] detached
        [daemon-outdated] Herdr update needed
        [client-outdated] app update needed
        [reconnecting] reconnecting
        [online] online
       *[connecting] connecting
    }
# A daemon copied text to the clipboard (OSC 52).
endpoint-copied = copied to clipboard
# $error says why.
endpoint-save-selection-failed = Save host selection: { $error }
endpoint-catalog-failed = Host catalog: { $error }
endpoint-reload-config-failed = Reload config: { $error }
endpoint-activate-failed = Activate: { $error }
# $device is the device's name.
endpoint-activation-failed = { $device }: surface activation failed or timed out
# $reason is what the connection reported.
endpoint-activation-failed-because = { $device }: surface activation failed or timed out: { $reason }
endpoint-surface-switching-unsupported = Current endpoint does not support surface switching
