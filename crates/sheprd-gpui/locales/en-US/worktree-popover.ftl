# The popover a workspace row in the sidebar opens: a grid of action tiles,
# plain rows for rarer actions, and the destructive action last. Rows ending
# in "..." open a dialog.

worktree-popover-new-worktree = New worktree
# The header's toggle for the workspace's notifications: its accessible
# name, then the short word it shows.
worktree-popover-mute = Mute notifications
worktree-popover-unmute = Unmute notifications
worktree-popover-mute-button = Mute
worktree-popover-unmute-button = Unmute
worktree-popover-fan-out = Fan out prompt...
# Opens the comparison of a fan-out already launched.
worktree-popover-fan-out-comparison = Fan-out comparison...
worktree-popover-rename = Rename
# Focuses the copy this workspace was teleported to.
worktree-popover-go-to-teleported = Go to teleported copy
worktree-popover-clear-teleported = Clear teleported mark
worktree-popover-teleport = Teleport...
# Moves the work back to the host it was teleported from.
worktree-popover-teleport-back = Teleport back
worktree-popover-close = Close
# Closes the workspace with the linked worktrees it heads.
worktree-popover-close-group = Close group
worktree-popover-checkpoints = Checkpoints...
worktree-popover-note = Note...
worktree-popover-orchestrator = Issues and pull requests
worktree-popover-open-worktree = Open worktree...
worktree-popover-sandbox-images = Build sandbox images
worktree-popover-run-script = Run script
worktree-popover-run-setup-script = Run setup script
worktree-popover-expand-group = Expand group
worktree-popover-collapse-group = Collapse group
worktree-popover-delete-worktree = Delete worktree checkout

# Tile captions, shorter where the tile's icon says the rest.
worktree-popover-tile-new-worktree = New worktree
worktree-popover-tile-fan-out = Fan out
worktree-popover-tile-comparison = Comparison
worktree-popover-tile-rename = Rename
worktree-popover-tile-go-to-teleported = Go to copy
worktree-popover-tile-clear-teleported = Clear mark
worktree-popover-tile-teleport = Teleport
worktree-popover-tile-teleport-back = Teleport back
worktree-popover-tile-close = Close
worktree-popover-tile-close-group = Close group

# Why the new worktree shortcut found nothing to branch from.
worktree-popover-new-disconnected = Not connected, so no worktree can be created
worktree-popover-new-no-workspace = No workspace is focused to create a worktree from
worktree-popover-not-git = This workspace is not a Git repository
worktree-popover-new-main-closed = Open this repository's main checkout to create a worktree
worktree-popover-new-detached = This checkout has no branch to start a worktree from

# Why the open or delete worktree shortcut has nothing to act on.
worktree-popover-change-disconnected = Not connected, so no worktree can be changed
worktree-popover-no-workspace = No workspace is focused
worktree-popover-open-main-closed = Open this repository's main checkout to open a worktree
worktree-popover-main-checkout = Only a linked worktree can be removed, not a main checkout

worktree-popover-note-unavailable = Notes are kept on Git checkouts with a branch

# Building the repository's cloud sandbox images.
worktree-popover-images-no-checkout = This repository has no checkout to build an image from
worktree-popover-images-failed = Not built: { $error }
# $repository is the repository's name; $providers names the cloud providers,
# such as "Docker and Railway".
worktree-popover-images-building = Building { $repository }'s images for { $providers }; Settings > Cloud devices shows their progress
# Two names in a list.
worktree-popover-and = { $first } and { $second }
