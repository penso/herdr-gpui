# The sidebar's device footer and the device picker it opens.

# Shown where Add Device cannot open.
devices-unavailable-socket = Device setup is unavailable with an explicit socket.
devices-unavailable-development = Device setup is unavailable with a development catalog.
# The footer's Settings button, with the key that opens Settings.
devices-settings = Settings
devices-settings-shortcut = Settings ({ $shortcut })
devices-all = All Devices
# How many devices the sidebar lists, when the user picked several.
devices-picked = { $count } Devices
# What a screen reader calls the picker button. $device is the picked
# device's name, or "All Devices".
devices-picker-connected = Devices: { $device }, connected
devices-picker-disconnected = Devices: { $device }, disconnected
devices-hosts = { $count ->
        [one] { $count } host
       *[other] { $count } hosts
    }
devices-sessions = Sessions
# The picker's heading.
devices-heading = DEVICES
devices-multi-select-macos = ⌘-click or Space for several
devices-multi-select = Ctrl-click or Space for several
devices-all-detail = { $count ->
        [one] { $count } device · { $connected } connected
       *[other] { $count } devices · { $connected } connected
    }
# A row's detail for this computer. $session is the Herdr session's name.
devices-this-device-session = This device · { $session }
devices-this-device = This device
# A device row's detail: where it is, then its state, such as online.
devices-row-detail = { $place } · { $state }
devices-add = Add Device…
devices-sessions-row = Sessions…
devices-sessions-detail = Switch, add, or delete this device's sessions
# The heading over cloud machines still being created.
devices-adding = ADDING

# The activity card above the picker's list: agents over the last two hours.
devices-activity-heading = AGENT ACTIVITY · 2 H
# Opens the Devices overview.
devices-activity-overview = Overview
devices-activity-working = { $count } working
devices-activity-blocked = { $count } blocked
