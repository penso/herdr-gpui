# The dialogs a workspace row's popover opens: rename, new and open
# worktree, new tab and workspace, note, close, and delete. $workspace,
# $host, and $branch are names; $machine is a new cloud machine, such as
# "a new Docker sandbox".

workspace-dialog-rename-title = Rename workspace
workspace-dialog-rename = Rename
workspace-dialog-rename-note = Edit the workspace label.
workspace-dialog-new-worktree-title = New worktree
workspace-dialog-open-worktree-title = Open worktree
workspace-dialog-open = Open
workspace-dialog-new-tab-title = New tab
workspace-dialog-new-tab-note = Name the tab, or leave the suggestion for Herdr to number it.
workspace-dialog-new-workspace-title = New workspace
workspace-dialog-new-workspace-note = Name the workspace, or leave the suggestion for Herdr to name it.
workspace-dialog-note-title = Note
workspace-dialog-note-note = A reminder kept only in this app, on this checkout and branch. Save it empty to remove it.
workspace-dialog-save = Save
workspace-dialog-force-delete-title = Force delete checkout?
workspace-dialog-force-remove = Force remove
workspace-dialog-delete-title = Delete worktree checkout?
workspace-dialog-remove = Remove
workspace-dialog-create = Create
# Creates the worktree or workspace on another host.
workspace-dialog-create-on = Create on { $host }
workspace-dialog-create-in = Create in { $machine }
workspace-dialog-cancel = Cancel
# The submit button while the daemon works.
workspace-dialog-waiting = Waiting...
# What a screen reader says for the submit button while the daemon works.
workspace-dialog-waiting-spoken = Waiting

# New workspace on another host.
workspace-dialog-opens-on = Opens the repository's main checkout on { $host }.
workspace-dialog-opens-on-clone = Opens the repository's main checkout on { $host }, cloning the repository there first.
workspace-dialog-opens-in = Makes { $machine }, clones the repository there, and opens it.

workspace-dialog-close-note = { $count ->
        [one] Closes { $count } workspace and terminates its running terminals. Checkout files and branches are not deleted.
       *[other] Closes { $count } workspaces and terminates their running terminals. Checkout files and branches are not deleted.
    }

# New worktree. $base is the branch or commit it starts from, such as HEAD.
workspace-dialog-name = Name
workspace-dialog-branch = Branch
# Caption of the row choosing what the new worktree's first pane shows.
workspace-dialog-lands-in = Opens in
workspace-dialog-creates-on = Creates a worktree from { $base } on { $host }, without granting repository trust:
workspace-dialog-creates-on-clone = Creates a worktree from { $base } on { $host }, cloning the repository there first, without granting repository trust:
workspace-dialog-creates-in = Makes { $machine } and creates a worktree from { $base } there, cloning the repository first, without granting repository trust:
workspace-dialog-creates-here = Creates this folder from { $base }, without granting repository trust:
# Where the checkout goes, before a branch is typed or when no folder is known.
workspace-dialog-daemon-names = The daemon names the checkout.
workspace-dialog-daemon-chooses = The daemon chooses the checkout.
workspace-dialog-host-chooses = { $host }'s daemon chooses the checkout.
workspace-dialog-its-daemon-chooses = Its daemon chooses the checkout.

# Delete worktree checkout.
workspace-dialog-merged = Pull request #{ $number } was merged.
workspace-dialog-force-removes = This force removes the checkout folder:
workspace-dialog-removes = This removes the checkout folder:
workspace-dialog-path-pending = Waiting for the daemon to name the checkout...
workspace-dialog-force-note = Modified and untracked files, including submodule contents, are discarded. The branch is not deleted. The Herdr workspace will close.
workspace-dialog-delete-note = The branch is not deleted. The Herdr workspace will close.
workspace-dialog-archive-checking = Checking for an archive script...
workspace-dialog-archive-failed = The archive script cannot run: { $error }
workspace-dialog-archive-trusted = The repository's archive script runs first, in a new tab. The checkout is removed only if it succeeds.
workspace-dialog-archive-untrusted = The repository has an archive script. You can review it before anything runs.

# $status says what a dispatch to another host is doing, in English.
workspace-dialog-job-waiting = { $status } Dismissing does not cancel it.
workspace-dialog-daemon-waiting = Waiting for the daemon. Dismissing does not cancel it.

workspace-dialog-remove-failed = Remove worktree: { $error }
workspace-dialog-unexpected = Unexpected daemon response. Review current workspace state before retrying.
workspace-dialog-setup-failed = The setup script did not start: { $error }
