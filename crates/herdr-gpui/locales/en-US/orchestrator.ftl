# The activity bar's Pull requests and Issues sections: the issues, pull
# requests, and agent runs of every repository the window's workspaces hold,
# an item's page, and the dialogs that start or stop an agent on one.

orchestrator-label = Orchestrator
orchestrator-pull-requests = Pull requests
orchestrator-issues = Issues
orchestrator-no-workspace = Open a workspace first
orchestrator-activity-bar-off = Pull requests and issues show in the activity bar, which is turned off
orchestrator-needs-host = Pull requests and issues need a local or SSH host
orchestrator-opening = Opening the repositories…

# Field placeholders.
orchestrator-filter-placeholder = Filter: title, id, author, label, body…
orchestrator-message-placeholder = Send a message to the agent…
orchestrator-comment-placeholder = Leave a comment…
orchestrator-branch-placeholder = Branch name
orchestrator-extra-placeholder = Extra instructions, appended verbatim (optional)
orchestrator-model-placeholder = Agent default

# The list's header, filters, and sort.
orchestrator-no-repositories-yet = No repositories yet
orchestrator-projects = { $count ->
    [one] 1 project
   *[other] { $count } projects
}
orchestrator-no-github-remote = no GitHub remote
orchestrator-dispatch-agent = Dispatch agent
orchestrator-refresh = Refresh
orchestrator-filters = Filters
orchestrator-filter-open = Open
orchestrator-filter-mine = Mine
orchestrator-has-agent = Has agent
orchestrator-blocked = Blocked
orchestrator-clear-search = Clear search
orchestrator-showing-only = Showing only
orchestrator-facets-state = STATE
orchestrator-facets-labels = LABELS
orchestrator-sort-updated = Recently updated
orchestrator-sort-newest = Newest
orchestrator-sort-oldest = Oldest
orchestrator-sort-priority = Priority
orchestrator-sort-title = Title
# A backend's chip after its name ($source, such as GitHub or Beads).
orchestrator-source-failed = failed
orchestrator-source-sign-in = sign in
orchestrator-source-unsupported = not supported yet
# A failed sync's banner when the error does not name its backend.
orchestrator-source-error = { $source }: { $error }
orchestrator-capped = { $source } has more than { $count } items; the newest are shown and not cached.
orchestrator-newer-schema = Shared data is schema { $found }; this herdr-gpui understands up to { $known }. Read-only until updated.
# A banner led by its project's name, while several projects are listed.
orchestrator-banner-of = { $project }: { $text }

# The list's column heads, in capitals as the list shows them.
orchestrator-column-age = AGE
orchestrator-column-source = SRC
orchestrator-column-run = RUN
orchestrator-column-project = PROJECT
orchestrator-column-title = TITLE
orchestrator-column-author = AUTHOR
orchestrator-column-pull-request = PR
orchestrator-column-title-branch = TITLE · BRANCH
orchestrator-column-review = REVIEW
orchestrator-column-checks = CHECKS
orchestrator-column-size = SIZE
# A row's project when checkouts on several hosts hold it.
orchestrator-project-hosts = { $project } · { $count } hosts

# The list with nothing to show. $query is what the user typed.
orchestrator-no-repositories = No repositories
orchestrator-no-repositories-about = Pull requests and issues come from the Git checkouts your workspaces are in, on this machine or an SSH host. Open a workspace in one to list them here.
orchestrator-loading = Loading…
orchestrator-no-issues-match = No open issues match “{ $query }”
orchestrator-no-pull-requests-match = No pull requests match “{ $query }”
orchestrator-no-issues-filtered = No open issues match the filters
orchestrator-no-pull-requests-filtered = No pull requests match the filters
orchestrator-no-issues = No open issues
orchestrator-no-issues-about = This lists the open issues of every repository your workspaces hold, from GitHub, GitLab, and Beads. Pick one to read it or to start an agent on it.
orchestrator-no-pull-requests = No pull requests
orchestrator-no-pull-requests-about = This lists the pull requests of every repository your workspaces hold, with their reviews and checks. Pick one to read it or to start an agent on it.
orchestrator-clear-filter = Clear filter

# The list's group headings.
orchestrator-next-review = Review requested
orchestrator-next-fix = Fix
orchestrator-next-merge = Ready to merge
orchestrator-next-wait = Waiting
orchestrator-next-closed = Closed and merged
# A Beads priority: P0 is the most urgent.
orchestrator-priority = P{ $priority } · { $priority ->
    [0] urgent
    [1] high
    [2] normal
    [3] low
   *[other] backlog
}
orchestrator-no-priority = No priority

# A row.
# What a screen reader says for a row; $state is the provider's word.
orchestrator-row-label = { $identifier } { $title }, { $state }
orchestrator-start-agent = Start agent
orchestrator-badge-blocked = blocked
orchestrator-badge-draft = draft
orchestrator-badge-fork = fork
orchestrator-review-yours = your review
orchestrator-review-approved = approved
orchestrator-review-changes = changes requested
orchestrator-review-required = review required
orchestrator-review-dismissed = dismissed
# How long ago a row changed, in a narrow column.
orchestrator-age-now = now
orchestrator-age-minutes = { $count }m
orchestrator-age-hours = { $count }h
orchestrator-age-days = { $count }d
orchestrator-age-weeks = { $count }w

# A run's state.
orchestrator-status-starting = starting
orchestrator-status-working = working
orchestrator-status-needs-input = needs input
orchestrator-status-idle = idle
orchestrator-status-done = done
orchestrator-status-failed = failed
orchestrator-status-stopped = stopped
# $agent is the agent's name, such as claude; $status one of the above.
orchestrator-run-status = { $agent } { $status }

# An item's page.
orchestrator-tab-conversation = Conversation
orchestrator-tab-checks = Checks
orchestrator-tab-description = Description
orchestrator-tab-agent = Agent
orchestrator-tab-details = Details
# Where the open item is in the list.
orchestrator-position = { $at } of { $count }
orchestrator-previous = Previous
orchestrator-next = Next
orchestrator-open-in-browser = Open in browser
orchestrator-close-details = Close details
orchestrator-back = Back
# $parent is the parent item's identifier.
orchestrator-child-of = child of { $parent }
orchestrator-no-description-short = No description.
orchestrator-no-runs = No agent has worked on this yet.
orchestrator-earlier-runs = EARLIER RUNS
orchestrator-runs-note = Status comes from each host's Herdr snapshot. Runs started by agent-launcher are shown here; it alone updates them.
# A run on this computer, which names no host.
orchestrator-host-local = local
orchestrator-details-key = Key
orchestrator-details-identifier = Identifier
orchestrator-details-state = State
orchestrator-details-url = URL
orchestrator-details-base = Base
orchestrator-details-head = Head
orchestrator-details-head-repository = Head repository
orchestrator-details-comments = Comments
orchestrator-details-review-comments = Review comments
orchestrator-details-commits = Commits
orchestrator-details-created = Created
orchestrator-details-updated = Updated
orchestrator-delete-bead = Delete bead…
orchestrator-open-pane = Open pane
orchestrator-send = Send
orchestrator-stop = Stop
orchestrator-remove = Remove

# The status box above an item's page.
orchestrator-blocked-by = by { $items }
orchestrator-status-sign-in = Sign in to GitHub to read its status
orchestrator-status-reading = Reading the pull request…
orchestrator-checks-failing = { $failing } of { $total } checks failing
orchestrator-checks-running = Checks running
orchestrator-checks-passed = Checks passed
orchestrator-checks-none = No checks reported
# $state is GitHub's word for the pull request, such as Open; $base a branch.
orchestrator-merge-into = { $state } into { $base }
orchestrator-no-agent = No agent yet
orchestrator-review-with-agent = Review with agent
orchestrator-open-agent = Open agent
orchestrator-merge-menu = Merge ▾
# Facts after the title, joined with " · ". $ago is how long ago, such as
# "3 min ago" or "yesterday".
orchestrator-opened-by = { $author } opened { $ago }
orchestrator-opened = opened { $ago }
orchestrator-updated = updated { $ago }
orchestrator-pull-request-size = +{ $additions } −{ $deletions } in { $files ->
    [one] 1 file
   *[other] { $files } files
}

# The Checks tab. $summary is GitHub's summary of the checks, in capitals.
orchestrator-checks-sign-in = Sign in to GitHub to read the checks.
orchestrator-checks-reading = Reading the checks…
orchestrator-checks-heading = CHECKS · { $summary }
orchestrator-checks-review = REVIEW
orchestrator-checks-merge = MERGE
orchestrator-outcome-passed = passed
orchestrator-outcome-failed = failed
orchestrator-outcome-pending = pending
orchestrator-outcome-skipped = skipped
orchestrator-no-such-pull-request = GitHub reports no such pull request.
orchestrator-no-merge-method = This repository allows no merge method.
# The merge methods' buttons, as GitHub words them.
orchestrator-merge-commit = Create a merge commit
orchestrator-merge-squash = Squash and merge
orchestrator-merge-rebase = Rebase and merge

# The Conversation tab.
orchestrator-conversation-sign-in = Sign in to GitHub to read the conversation.
orchestrator-conversation-reading = Reading the conversation…
orchestrator-no-comments = No comments yet.
orchestrator-no-description = No description provided.
orchestrator-commented = commented
orchestrator-reviewed = { $count ->
    [0] reviewed
    [one] reviewed · 1 comment
   *[other] reviewed · { $count } comments
}
orchestrator-summary-approved = { $count } approved
orchestrator-summary-changes = { $count } requested changes
orchestrator-summary-open-threads = { $count ->
    [one] 1 open thread
   *[other] { $count } open threads
}
orchestrator-summary-resolved = { $count } resolved
orchestrator-summary-comments = { $comments ->
    [one] 1 comment
   *[other] { $comments } comments
} from { $people ->
    [one] 1 person
   *[other] { $people } people
}
# A resolved thread folded away; Show unfolds it.
orchestrator-thread-show = { $replies ->
    [0] Show
    [one] 1 reply · Show
   *[other] { $replies } replies · Show
}
orchestrator-thread-resolved = Resolved
orchestrator-thread-state-resolved = resolved
orchestrator-thread-state-open = open
orchestrator-replies = { $count ->
    [one] 1 reply
   *[other] { $count } replies
}
orchestrator-comment = Comment
# Marks an author that is an app, not a person.
orchestrator-bot = bot
# Stand-ins for media in a description or comment.
orchestrator-markdown-image = [image]
orchestrator-markdown-video = [video]

# Dispatching an agent.
orchestrator-on-host = on { $host }
orchestrator-this-machine = this machine
orchestrator-close = Close
orchestrator-cancel = Cancel
orchestrator-dispatch = Dispatch
orchestrator-section-prompt = PROMPT
orchestrator-section-agent = AGENT
orchestrator-section-host = HOST
orchestrator-section-model = MODEL
orchestrator-section-branch = BRANCH
orchestrator-section-extra = EXTRA INSTRUCTIONS
# The prompt that ships with the app, beside the repository's own profiles.
orchestrator-profile-built-in = built-in
orchestrator-agents-failed = Could not discover agents. Select the host to retry.
orchestrator-agents-looking = Looking for installed agents…
orchestrator-agents-none = No supported agent is installed on this host.
orchestrator-problem-agent = Pick an agent installed on this host.
orchestrator-problem-branch = That branch name is not one Git accepts.
orchestrator-problem-extra = Extra instructions are limited to 16 KiB.
orchestrator-problem-model = A model name has no spaces and does not start with a dash.
# The hosts a dispatch could go to.
orchestrator-host-best = best
orchestrator-host-working = { $count } working
orchestrator-host-has-repository = repository here
orchestrator-host-clones = clones it first

# Confirmations. $agent is an agent's name; $branch a branch; $id a bead's.
orchestrator-confirm-stop-title = Stop { $agent }?
orchestrator-confirm-stop-body = Sends Esc, then Ctrl-C to the agent. The worktree is kept.
orchestrator-confirm-remove-run-title = Remove run?
orchestrator-confirm-remove-run-body = Forgets this failed run. It has no worktree to remove.
orchestrator-confirm-remove-worktree-title = Remove worktree?
orchestrator-confirm-remove-worktree-body = Removes the checkout of { $branch }, discarding uncommitted changes. Committed work stays on the branch.
# $action is a merge method's button, such as Squash and merge.
orchestrator-confirm-merge-title = { $action } #{ $number }?
# $head is the start of the head commit's hash.
orchestrator-confirm-merge-body = Merges pull request #{ $number } at head commit { $head }; GitHub refuses if the branch moved since.
orchestrator-merge = Merge
orchestrator-confirm-delete-bead-title = Delete { $id } permanently?
orchestrator-confirm-delete-bead-body = Runs bd delete --force. This cannot be undone and removes it for everyone using this .beads.
orchestrator-delete = Delete
orchestrator-dismiss = Dismiss

# What a finished action did, in the banner.
orchestrator-done-started = Agent started
orchestrator-done-stopped = Agent stopped
orchestrator-done-sent = Message sent
orchestrator-done-removed = Run removed
orchestrator-done-bead-deleted = Bead deleted
