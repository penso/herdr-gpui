# Teleport: moving a worktree, its tabs, and its agents to another host.
# Host, repository, branch, tab, and program names are data, as are
# commands the destination runs.

teleport-title = Teleport
# Under the title: $worktree is the worktree moved, $host where it is now.
teleport-subtitle = { $worktree } from { $host }
teleport-no-hosts = No other host is saved. Add one from Devices, then try again.
teleport-host-connected = Connected
teleport-host-ssh = Not connected; reached over SSH
# $host is the destination being checked.
teleport-checking = Checking { $host } and the running programs...
# The review's first line: where the work goes and how. $host is the
# destination, $repository the repository open there, $path a checkout.
teleport-to-origin = To { $host } · { $repository }. Same origin remote.
teleport-to-remote = To { $host } · { $repository }. Shares a remote.
teleport-to-name = To { $host } · { $repository }. Same name only; remotes differ.
teleport-to-reclaim = To { $host } · { $repository }. Brings the work back to the checkout it left.
teleport-to-existing = To { $host } · { $path }. Opens the checkout already there.
teleport-to-clone = To { $host } · { $path }. Clones the repository there first.
teleport-to-copy = To { $host } · { $path }. Copies the repository there first.
teleport-state = Branch { $branch } · { $unpushed ->
    [one] { $unpushed } unpushed commit
   *[other] { $unpushed } unpushed commits
} · { $changed ->
    [one] { $changed } changed file
   *[other] { $changed } changed files
}, { $untracked } untracked
teleport-github-token = { $host } cannot reach GitHub by itself, so your GitHub CLI token is installed for this repository there, for git pull and push.
teleport-github-unavailable = { $host } cannot reach GitHub, and the GitHub CLI here is not signed in, so pull and push will not work there.
teleport-name-only = Matched by name only: this repository's remotes differ there.
# A tab of the move, numbered from 1, with the label the user gave it.
teleport-tab = Tab { $number }
teleport-tab-labeled = Tab { $number }: { $label }
# What a pane runs on the destination. $program is a program's name,
# $agent the agent that continues, $command the command line.
teleport-pane-shell = Shell
teleport-pane-resume = { $agent }: resume session
teleport-pane-handoff-again = { $program }: handoff note, then start again
teleport-pane-handoff-to = { $program }: handoff note, continued by { $agent }
teleport-pane-start = Start { $command }
teleport-pane-run = Run { $command }
teleport-pane-missing = { $program }: not installed there, skipped
teleport-left-behind = Ignored files such as .env stay behind. The workspace here then closes; its checkout stays and can be reopened with Open worktree.
teleport-moving-to = Moving to { $host }...
# The move's steps, as the progress line names them.
teleport-step-starting = Starting
teleport-step-discover = finding matching repositories
teleport-step-review = reviewing the move
teleport-step-clone = copying the repository to the destination
teleport-step-open = opening the repository on the destination
teleport-step-handoff = asking agents for handoff notes
teleport-step-capture = capturing commits and changes
teleport-step-transfer = copying changes to the destination
teleport-step-fetch = fetching the branch on the destination
teleport-step-create-worktree = creating the destination worktree
teleport-step-restore = restoring uncommitted changes
teleport-step-tabs = recreating tabs
teleport-step-retire = stopping programs in the source workspace
teleport-step-credentials = installing GitHub credentials on the destination
teleport-step-sessions = moving agent sessions
teleport-step-launch = starting programs
teleport-keeps-moving = Closing this dialog does not stop the move.
teleport-source-closes-later = The source workspace closes only once the destination is ready.
teleport-review = Review
teleport-moving = Moving...
teleport-close = Close
teleport-hide = Hide
teleport-cancel = Cancel
# Flashes when the move ends with the dialog closed.
teleport-failed = Teleport failed: { $error }
teleport-done = Teleported to { $host }
# $first is the first warning, from the move itself.
teleport-done-warnings = { $count ->
    [one] Teleported to { $host } with one warning: { $first }
   *[other] Teleported to { $host } with { $count } warnings: { $first }
}
