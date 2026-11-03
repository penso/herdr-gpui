# The Devices overview: a tab with every device's agents and load, its
# activity over the last two hours, and the sidebar's device tree search.
# Device, host, and agent names are shown as they are.

devices-overview-title = Devices overview
devices-overview-summary = { $devices ->
        [one] { $devices } device
       *[other] { $devices } devices
    } · { $agents ->
        [one] { $agents } agent
       *[other] { $agents } agents
    }
devices-overview-add-device = Add Device
devices-overview-search = Search devices, agents, workspaces
devices-overview-tree-search = Search devices and agents
devices-overview-no-workspace = Open a workspace first
devices-overview-too-many-tabs = Too many tabs are open

# The totals across every device.
devices-overview-working = Working
devices-overview-working-of = of { $total } agents
devices-overview-blocked = Blocked
devices-overview-waiting = waiting for you
devices-overview-idle-or-done = Idle or done
devices-overview-ready = ready for work
devices-overview-online = Devices online
devices-overview-all-reachable = all reachable
devices-overview-not-connected = { $count } not connected

# The legend of agent states.
devices-overview-legend-working = Working
devices-overview-legend-blocked = Blocked
devices-overview-legend-done = Done
devices-overview-legend-idle = Idle

# The activity chart.
devices-overview-activity = Agent activity
devices-overview-last-hours = last 2 h
devices-overview-working-now = working now
devices-overview-peak = peak { $peak } in 2 h
devices-overview-hide-devices = Hide devices
devices-overview-view-devices = View all devices
devices-overview-axis-two-hours = 2 h ago
devices-overview-axis-hour = 1 h ago
devices-overview-axis-now = now
# A device's lane: agents working now of all its agents.
devices-overview-lane-now = { $working }/{ $total } now
devices-overview-lane-connecting = connecting
devices-overview-lane-offline = offline
devices-overview-lane-disabled = disabled

# The device table.
devices-overview-devices = Devices
devices-overview-column-device = Device
devices-overview-column-agents = Agents
devices-overview-column-cpu = CPU
devices-overview-column-memory = Memory
devices-overview-column-gpu = GPU
devices-overview-column-vram = VRAM
devices-overview-column-disk = Disk free
devices-overview-column-uptime = Uptime
devices-overview-offline = Offline
devices-overview-connecting = Connecting
devices-overview-disabled = Disabled
devices-overview-no-agents = No agents
devices-overview-agents-working = { $working }/{ $total } working
devices-overview-agents-blocked = { $count } blocked
devices-overview-no-match = No device, agent or workspace matches the search.
devices-overview-device-no-agents = No agents on this device.
devices-overview-device-unknown = Not connected, so its agents are unknown.
# How a device is reached, under its name: $name is a Herdr session's
# name, $target an SSH destination, $distro a WSL distribution.
devices-overview-this-machine = this machine
devices-overview-session = session { $name }
devices-overview-ssh = ssh { $target }
devices-overview-wsl = WSL · { $distro }
