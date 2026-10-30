# Why a Teleport move, which carries a worktree, its tabs, and its agents to
# another computer, could not finish. $host is a computer's name, $path a
# folder on it, $branch a Git branch, and $agent an agent's name, such as
# Claude Code. $error is the failure underneath.

# $step is the part of the move that failed.
error-remote-teleport-script = { $step ->
    [discover] finding matching repositories failed: { $error }
    [review] reviewing the move failed: { $error }
    [clone] copying the repository to the destination failed: { $error }
    [open] opening the repository on the destination failed: { $error }
    [handoff] asking agents for handoff notes failed: { $error }
    [capture] capturing commits and changes failed: { $error }
    [transfer] copying changes to the destination failed: { $error }
    [fetch] fetching the branch on the destination failed: { $error }
    [create-worktree] creating the destination worktree failed: { $error }
    [restore] restoring uncommitted changes failed: { $error }
    [tabs] recreating tabs failed: { $error }
    [retire] stopping programs in the source workspace failed: { $error }
    [credentials] installing GitHub credentials on the destination failed: { $error }
    [sessions] moving agent sessions failed: { $error }
   *[launch] starting programs failed: { $error }
}
# $step is the part of the move whose script printed something unexpected.
error-remote-teleport-decode = { $step ->
    [discover] finding matching repositories failed: unexpected output
    [review] reviewing the move failed: unexpected output
    [clone] copying the repository to the destination failed: unexpected output
    [open] opening the repository on the destination failed: unexpected output
    [handoff] asking agents for handoff notes failed: unexpected output
    [capture] capturing commits and changes failed: unexpected output
    [transfer] copying changes to the destination failed: unexpected output
    [fetch] fetching the branch on the destination failed: unexpected output
    [create-worktree] creating the destination worktree failed: unexpected output
    [restore] restoring uncommitted changes failed: unexpected output
    [tabs] recreating tabs failed: unexpected output
    [retire] stopping programs in the source workspace failed: unexpected output
    [credentials] installing GitHub credentials on the destination failed: unexpected output
    [sessions] moving agent sessions failed: unexpected output
   *[launch] starting programs failed: unexpected output
}
error-remote-teleport-unsupported-host = Teleport needs a local session, SSH host, or cloud device; custom socket and WSL endpoints are unsupported
error-remote-teleport-development-session = Host scripts cannot reach a development (--dev) session
error-remote-teleport-workspace-gone = The workspace is no longer open on its host
error-remote-teleport-pane-gone = The pane is no longer open on its host
error-remote-teleport-repository-not-open = The repository is not open on { $host }
error-remote-teleport-source-moved = The checkout here has new commits since the review; review the move again
error-remote-teleport-outside-checkout = The pane is in { $path }, outside its space's checkout; change back into it first
error-remote-teleport-destination-directory-missing = The directory { $path } is missing on the destination; create it there before moving (no files are copied)
error-remote-teleport-outside-home = The pane is in { $path }, outside its host's home directory; only home-relative directories can move
error-remote-teleport-session-agent-working = The agent is working. Move it once it is idle.
error-remote-teleport-session-cwd-unavailable = The pane's working directory could not be read
error-remote-teleport-session-not-started = { $agent } did not start on { $host }; the pane here keeps running
error-remote-teleport-session-program-missing = { $agent } is not installed on { $host }; a session-only move needs the same agent
error-remote-teleport-session-required = Only an agent with a transferable session can move without a Git repository
# $workspace is the ID of the workspace the move opened on the destination.
error-remote-teleport-session-workspace-left-behind = { $error }; the new workspace { $workspace } could not be closed
error-remote-teleport-source-session-changed = The pane's session or directory changed since the review; review the move again
error-remote-teleport-worker-stopped = The worker running this move stopped before it finished
error-remote-teleport-space-gone = The space is no longer open on { $host }
error-remote-teleport-space-changed = The space at { $path } has uncommitted changes; commit or stash them there first
error-remote-teleport-space-diverged = The space is on { $branch }, which this pane's commits do not continue
error-remote-teleport-left-behind = { $error }; the new worktree at { $path } could not be removed
# $reference is a Git reference, such as a commit hash or branch name.
error-remote-teleport-no-commit = { $reference } does not name a commit on the source
error-remote-teleport-detached-head = The checkout is not on a branch (detached HEAD)
error-remote-teleport-branch-diverged = Branch { $branch } already exists on the destination with commits this checkout lacks
error-remote-teleport-branch-checked-out = Branch { $branch } is already checked out on the destination at { $path }
error-remote-teleport-session-missing = Session file for { $agent } was not found on the source host
error-remote-teleport-local-file = Could not use a local temporary file
error-remote-teleport-invalid-token = The GitHub CLI returned an unexpected token
error-remote-teleport-cancelled = Teleport cancelled
