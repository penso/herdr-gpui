# The sidebar beside the terminals: its headings, rows, footer, and the
# compact rail. Names of workspaces, branches, agents, and devices are the
# user's and the daemon's, never messages.

# Headings over the lists. Lowercase in English; a layout with title case
# capitalizes the first letter.
sidebar-header-spaces = spaces
sidebar-header-devices = devices
sidebar-header-agents = agents
# What a screen reader calls each list.
sidebar-label = Sidebar
sidebar-workspaces-label = Workspaces
sidebar-devices-label = Devices
sidebar-agents-label = Agents
# Shown in place of the agents list when it is empty.
sidebar-no-agents = no agents
sidebar-no-matching-agents = no matching agents

# The footer's buttons, lowercase like the headings, and their names for
# screen readers and tooltips.
sidebar-new = new
sidebar-new-workspace = New workspace
sidebar-menu = menu
sidebar-menu-label = Menu

# The agents header's sort, which a click toggles: by workspace, or the
# agents that want attention first.
sidebar-sort-grouped = grouped
sidebar-sort-priority = priority
# A screen reader's name for the sort button; $sort is one of the above.
sidebar-agents-sort = Sort agents: { $sort }
# A plugin's agents view, which names itself; $view is the plugin's name.
sidebar-agents-view = Agents: { $view }

# An agent's or workspace's status, as its row's state text shows it.
# Lowercase in English, matching what the terminal client prints.
sidebar-status-working = working
sidebar-status-blocked = blocked
sidebar-status-done = done
sidebar-status-idle = idle
sidebar-status-unknown = unknown

# A row's name for screen readers is its parts joined by this separator.
sidebar-spoken-separator = {", "}
sidebar-spoken-linked-worktree = linked worktree
sidebar-spoken-connected = connected
sidebar-spoken-disconnected = disconnected
# How many of a device's agents work; $count reads like "2/5".
sidebar-spoken-working = { $count } working

# The compact rail's tooltips. Each wraps the one before it: an agent or
# workspace $name, then where it is, its status, and the device it runs on.
sidebar-rail-in = { $name } in { $place }
sidebar-rail-status = { $text } ({ $status })
sidebar-rail-host = { $host }: { $text }
# A device's letter whose rows are folded away; $host is its name.
sidebar-rail-collapsed = { $host } (collapsed)

# The tooltip on the mark before a device's name: where it runs and how it
# is reached. $provider is a cloud provider's name. $noun is a key naming
# what the provider calls its machines; translate each variant, not the keys.
sidebar-origin-local = This machine
sidebar-origin-ssh = SSH host
sidebar-origin-tailscale = Tailscale host over SSH
sidebar-origin-wsl = WSL distribution
sidebar-origin-cloud = { $provider } { $noun ->
        [VM] VM
        [container] container
        [sandbox] sandbox
        [sprite] sprite
        [workspace] workspace
       *[machine] machine
    }
sidebar-origin-cloud-ssh = { $provider } { $noun ->
        [VM] VM
        [container] container
        [sandbox] sandbox
        [sprite] sprite
        [workspace] workspace
       *[machine] machine
    } over SSH

# $error is why the daemon refused to reorder the workspaces.
sidebar-workspace-move-not-sent = Workspace move not sent: { $error }

# The badge on a repository's own checkout, the first card of its group in
# the Orca layout.
sidebar-primary = primary
sidebar-spoken-muted = notifications muted
