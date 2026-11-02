# Fan-out: one prompt sent to several agents, each in its own new worktree,
# then the lanes compared and one kept. A lane is one agent's worktree.

fan-out-title = Fan out prompt
# $repo is the repository's name, $host the device it is on.
fan-out-subtitle = { $repo } on { $host }
# $base is the branch the worktrees start from.
fan-out-intro = Each agent gets its own new worktree from { $base }, then the same prompt.
fan-out-looking = Looking for installed agents...
fan-out-no-agents = No supported agent is installed on { $host }.
# $picked agents of at most $max.
fan-out-agents = Agents · { $picked } of { $max }
# $agents lists the agent names, separated by commas.
fan-out-not-installed = Not installed on { $host }: { $agents }
fan-out-spread = Spread agents across hosts
fan-out-most-room = { $host } has the most room
fan-out-start-agent = Start agent
fan-out-start-agents = Start { $lanes } agents
fan-out-start-agents-hosts = Start { $lanes } agents on { $hosts } hosts
fan-out-send-prompt = Send prompt
# Why Start cannot be pressed yet.
fan-out-not-ready-agents = Waiting for the agent list
fan-out-not-ready-prompt = Write a prompt
fan-out-not-ready-pick = Pick at least one agent
fan-out-unscriptable = { $host } cannot be reached by script
fan-out-moved = This workspace has moved to another repository since the fan-out opened

# The prompt, quoted, while the lanes start and are compared.
fan-out-quoted-prompt = “{ $prompt }”
fan-out-closing-keeps-going = Closing this dialog does not stop the launch.
fan-out-starting = Starting...
fan-out-compare-hint = Keep the lane you prefer to remove the others, or forget the fan-out to leave every worktree as it is.
fan-out-keep-ellipsis = Keep...
fan-out-confirm =
    { $others ->
        [one] Removes the other worktree and its uncommitted changes. Its branch stays, with any commits.
       *[other] Removes the other { $others } worktrees and their uncommitted changes. Their branches stay, with any commits.
    }
fan-out-remove-others =
    { $others ->
        [one] Remove { $others } other
       *[other] Remove { $others } others
    }
fan-out-removing = Removing...
fan-out-open = Open
fan-out-keep = Keep
fan-out-forget = Forget
fan-out-cancel = Cancel
fan-out-back = Back
# Work under way carries on; closing only hides it.
fan-out-hide = Hide
fan-out-close = Close

# A lane's state. $status is one of these or an agent state, $host the
# device it runs on.
fan-out-lane-elsewhere = { $status } · { $host }
fan-out-lane-waiting = Waiting
fan-out-lane-setting-up = Setting up the host
fan-out-lane-creating-worktree = Creating worktree
fan-out-lane-starting-agent = Starting agent
fan-out-lane-prompting = Sending prompt
fan-out-lane-prompted = Prompted
fan-out-lane-stopped = Stopped
fan-out-lane-never-started = Never started
fan-out-lane-interrupted-creating = Interrupted creating its worktree; the branch may exist
fan-out-lane-interrupted-created = Interrupted before its agent started
fan-out-lane-interrupted-starting = Interrupted starting its agent; it may be running
fan-out-lane-interrupted-prompting = Interrupted sending the prompt; it may not have arrived
# $path is where its worktree is.
fan-out-lane-lost = { $host } is not the daemon it was, or cannot be scripted here; its worktree stays at { $path }
fan-out-lane-not-found = A lane's workspace is not shown on its host on its branch; it may be gone or reused. Wait for the host to connect, or Forget.
fan-out-prompt-lost = The prompt was too long to keep, so interrupted lanes cannot be finished here.
# What a compared lane's agent is doing.
fan-out-agent-idle = Idle
fan-out-agent-working = Working
fan-out-agent-blocked = Needs input
fan-out-agent-done = Done
fan-out-agent-unknown = Unknown
fan-out-agent-offline = Host offline
fan-out-agent-closed = Closed

# A lane's changes. The app joins these with " · " after +added −removed.
fan-out-no-changes = No changes yet
fan-out-diff-files =
    { $count ->
        [one] { $count } file
       *[other] { $count } files
    }
fan-out-diff-untracked = { $count } untracked
fan-out-diff-commits =
    { $count ->
        [one] { $count } commit
       *[other] { $count } commits
    }
fan-out-reading-changes = Reading changes...
# $changes is the lane's last changes; $reason why they could not be read again.
fan-out-changes-stale = { $changes } · not updated: { $reason }
fan-out-changes-unread = Changes not read: { $reason }
# $count worktrees; $error is the first one's reason.
fan-out-not-removed =
    { $count ->
        [one] { $count } worktree could not be removed: { $error }
       *[other] { $count } worktrees could not be removed: { $error }
    }

# Brief notices over the terminal.
fan-out-restored = Fan-out restored. Compare it from the workspace menu.
fan-out-interrupted =
    { $lanes ->
        [one] A fan-out was interrupted: { $lanes } lane needs you. Review it from the workspace menu.
       *[other] A fan-out was interrupted: { $lanes } lanes need you. Review it from the workspace menu.
    }
fan-out-prompted =
    { $total ->
        [one] Fan-out: { $running } of { $total } agent prompted. Compare them from the workspace menu.
       *[other] Fan-out: { $running } of { $total } agents prompted. Compare them from the workspace menu.
    }
fan-out-kept = Kept one lane; removed the others
