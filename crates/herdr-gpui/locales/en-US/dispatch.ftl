# Sending new work to another device: the host picker in the new workspace
# dialog, and the notices while a checkout is made there.

# A new cloud machine the provider makes first, used inside sentences such
# as "Create in a new container". $noun is a key naming what the provider
# calls its machines; translate each variant, not the keys.
dispatch-new-machine = a new { $noun ->
        [VM] VM
        [container] container
        [sandbox] sandbox
        [sprite] sprite
        [workspace] workspace
       *[machine] machine
    }
# A tile for such a machine.
dispatch-new-sandbox = New { $noun ->
        [VM] VM
        [container] container
        [sandbox] sandbox
        [sprite] sprite
        [workspace] workspace
       *[machine] machine
    }
dispatch-sandbox-own = Its own files and limits
dispatch-sandbox-made = Made when you create

# A host tile's facts.
dispatch-cores-free =
    { $count ->
        [one] { $count } core free
       *[other] { $count } cores free
    }
dispatch-measuring = measuring...
dispatch-agents-working =
    { $count ->
        [0] no agents working
        [one] { $count } agent working
       *[other] { $count } agents working
    }
# A host row's facts: $free cores (or ?) and $working agents.
dispatch-row-detail = { $free } free · { $working } working
# The quiet word a host carries: why it ranks where it does, or what
# choosing it costs.
dispatch-tag-offline = offline
dispatch-tag-best = Best fit
dispatch-tag-current = current
dispatch-tag-clone = will clone
dispatch-tag-usual = usual pick
# The repository is cloned there first.
dispatch-clone = clone
# The field listing the hosts not shown as tiles.
dispatch-other = Other
dispatch-more-hosts =
    { $count ->
        [one] { $count } more host
       *[other] { $count } more hosts
    }

# The job's status line. $host is a device's name or a dispatch-new-machine
# phrase; $step is what it is doing.
dispatch-making = Making { $host }: { $step }
dispatch-starting = Starting on { $host }...
dispatch-step = On { $host }: { $step }...
# Brief notices over the terminal. $error says why.
dispatch-created = Created on { $host }
dispatch-not-created = Not created on { $host }: { $error }
dispatch-setup-gave-up = A new worktree's setup script did not start: its host was not shown
dispatch-setup-not-started = The setup script did not start: { $error }
