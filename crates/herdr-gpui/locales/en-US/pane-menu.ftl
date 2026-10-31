# A pane's menu, from a right-click on the pane or its agent's sidebar row,
# and its pages: rename, move, move to another device, and processes.

pane-menu-rename = Rename
pane-menu-split-right = Split Right
pane-menu-split-down = Split Down
pane-menu-swap = Swap with Focused Pane
# The pane's tab is zoomed; the row turns zoom off.
pane-menu-unzoom = Unzoom
pane-menu-zoom = Zoom
pane-menu-move-to-new-tab = Move to New Tab
pane-menu-move-to-new-workspace = Move to New Workspace
# Opens the list of every place the pane can move to.
pane-menu-move-to = Move to…
pane-menu-edit-scrollback = Open Scrollback in Editor
# Right-clicks currently reach the pane's program; the row gives them back to
# this menu.
pane-menu-right-click-menu = Open This Menu on Right-Click
pane-menu-right-click-pane = Send Right-Clicks to Pane
pane-menu-processes = Processes
pane-menu-close = Close

pane-menu-rename-placeholder = Pane name (blank clears)
pane-menu-rename-title = Rename pane
pane-menu-rename-hint = Leave blank to clear the custom label.
pane-menu-cancel = Cancel
pane-menu-renaming = Renaming...
pane-menu-rename-submit = Rename

# The move picker.
pane-menu-moving = Moving pane…
pane-menu-move-heading = Move pane to
pane-menu-new-tab = New Tab
pane-menu-new-workspace = New Workspace

# Moving the pane to another device. $device is a device's name.
pane-menu-carry-session-only = Agent Session Only
pane-menu-carry-new-worktree = New Worktree
pane-menu-carry-checking = Checking the agent session and home directory…
pane-menu-carry-failed = Moving the pane failed: { $error }
# $destination names the device and place the pane moved to.
pane-menu-carry-moved = Moved to { $destination }
# $first is the first warning, in English from the move.
pane-menu-carry-moved-warnings = { $count ->
        [one] Moved to { $destination } with a warning: { $first }
       *[other] Moved to { $destination } with { $count } warnings: { $first }
    }
pane-menu-carry-title = Move pane to { $destination }
pane-menu-carry-reviewing = Checking the destination and the pane's program…
pane-menu-carry-move = Move
# $directory is the agent's working directory on the other device.
pane-menu-carry-session-cwd = New workspace in { $directory }
pane-menu-carry-session-note = Only the agent session moves. No files are copied.
pane-menu-carry-closes = The pane here closes once it runs there.
pane-menu-carry-branch = Branch { $branch }, in a new worktree
pane-menu-carry-branch-in-use = A new branch from { $branch }, which is in use there
pane-menu-carry-branch-commit = A new branch from this commit
# $checkout is a workspace on the other device.
pane-menu-carry-tab-fast-forward = A new tab in { $checkout }, fast-forwarded to this commit first
pane-menu-carry-tab = A new tab in { $checkout }
# $repository is the repository's name.
pane-menu-carry-into = Into { $repository }
# What the checkout holds that the move copies.
pane-menu-carry-state = { $unpushed ->
        [one] { $unpushed } unpushed commit
       *[other] { $unpushed } unpushed commits
    }, { $changed ->
        [one] { $changed } changed file
       *[other] { $changed } changed files
    }, { $untracked } untracked
pane-menu-carry-copied = The changes are copied: this checkout keeps them. The pane here closes once it runs there.
pane-menu-carry-starting = Starting
pane-menu-carry-keeps-going = Closing this menu does not stop the move.
pane-menu-carry-moving = Moving…
pane-menu-carry-close = Close

# The processes under the pane.
pane-menu-processes-title = Processes
# $pane is the pane's custom label.
pane-menu-processes-in = Processes in { $pane }
# The CPU share and memory of every process, as "4.2% · 120 MB".
pane-menu-processes-total = { $cpu } · { $memory }
pane-menu-processes-megabytes = { $size } MB
pane-menu-processes-gigabytes = { $size } GB
pane-menu-processes-listing = Listing processes…
# Marks the process the pane's terminal shows.
pane-menu-processes-foreground = foreground
pane-menu-processes-hidden = { $count } more not shown
pane-menu-processes-ending = Ending processes…
pane-menu-processes-end = { $count ->
        [0] End Process…
        [one] End { $count } Process…
       *[other] End { $count } Processes…
    }
pane-menu-processes-confirm-title = { $count ->
        [one] End { $count } process?
       *[other] End { $count } processes?
    }
pane-menu-processes-pid = pid { $pid }
pane-menu-processes-confirm-note = Each is asked to quit (SIGTERM), and may lose unsaved work. Processes they started keep running unless chosen too.
pane-menu-processes-cancel = Cancel
pane-menu-processes-confirm = { $count ->
        [one] End Process
       *[other] End { $count } Processes
    }
# What became of the processes the user chose to end, each sentence shown
# only when its count is above zero.
pane-menu-processes-signalled = { $count ->
        [one] Asked { $count } process to quit.
       *[other] Asked { $count } processes to quit.
    }
pane-menu-processes-exited = { $count } had already exited.
pane-menu-processes-moved = { $count } no longer under this pane, left running.
pane-menu-processes-failed = { $count } could not be signalled.
