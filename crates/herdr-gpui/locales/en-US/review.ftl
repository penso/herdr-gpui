# The Review tab: a checkout's changes, notes on their lines, and sending
# the notes to the agent.

review-title = Review changes
# $agent is the agent's name, as it reports it.
review-notes-go-to = Notes go to { $agent }
review-notes-go-to-agent = Notes go to the agent
review-no-agent = No agent in this workspace; notes can be copied
# Before the base branch button: "feature against [main ▾]". $branch is the
# branch under review.
review-against = { $branch } against
review-reading = Reading…
review-reading-changes = Reading changes…
review-find = Find in changes
review-hide-whitespace = Hide whitespace changes
review-layout-unified = Unified
review-layout-split = Side by side
review-scope-uncommitted = Uncommitted
review-scope-branch = Branch
review-no-uncommitted = No uncommitted changes
review-no-branch-changes = No changes on this branch

# The base branch list.
review-automatic = Automatic
review-current = Current
review-reading-branches = Reading branches…
# The base button before any base was found.
review-choose-base = choose a base

# Placeholders of the tab's fields.
review-describe = Describe the change…
review-filter = Filter files…
review-find-placeholder = Find in changes…

# The changed files list.
review-changed-files = Changed files
review-hide-viewed = Hide viewed
review-files-changed = { $count ->
    [one] 1 file changed
   *[other] { $count } files changed
}

# A file's header and the line standing for its lines.
review-viewed = Viewed
review-status-added = new
review-status-deleted = deleted
review-status-renamed = renamed
review-status-untracked = untracked
review-binary = Binary file not shown
# $added and $removed are signed counts, such as +120 and −4.
review-large-change = Large change not shown: { $added } { $removed } lines
review-large-file = Large file not shown
review-load-diff = Load diff
review-try-again = Try again
review-cut-short = { $count ->
    [one] Cut short after 1 line
   *[other] Cut short after { $count } lines
}
review-no-lines = No changes to show
review-unreadable-change = Could not read the change
review-unreadable-file = Could not read the file
review-unreadable-not-file = Not a regular file
review-unreadable-too-large = Too large to show
# Unchanged lines folded between two hunks.
review-more-lines = { $count ->
    [one] ↕ 1 more line
   *[other] ↕ { $count } more lines
}

# Find in changes. $current is the match shown, $count how many were found;
# $capped is yes when the search stopped counting.
review-search-none = No matches
review-search-count = { $capped ->
    [yes] { $current } of { $count }+
   *[no] { $current } of { $count }
}

# The notes panel.
review-notes = Notes
review-notes-empty = Click a line or a file name to note what should change.
review-save-note = Save note
review-add-note = Add note
review-withdraw = Withdraw
review-copy = Copy
review-clear-sent = Clear sent
# Where a note points, $place being a path and line, such as src/lib.rs:3.
review-note-place-removed = { $place } (removed)
review-note-sent = { $place } · sent

# Flashes.
review-no-checkout = No local checkout to review
review-no-workspace = Open a workspace first
review-too-many-tabs = Too many tabs are open
review-notes-full = Clear sent notes or remove some before adding more
review-note-empty = Write what should change first
review-send-pending = The previous notes are still pending delivery
review-withdraw-late = The agent already has those notes
review-withdrawn = Notes withdrawn
review-notes-copied = Notes copied
review-copied = Copied to clipboard
review-path-copied = Path copied
review-hunk-copied = Hunk copied
