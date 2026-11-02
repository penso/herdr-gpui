# The new worktree dialog's source tabs (a new branch, a checkout not open
# in Herdr, a local branch, a pull request, an issue), and the open worktree
# picker.

# What a screen reader calls the row of tabs.
worktree-source-tabs = Worktree source
# The tabs, short and lowercase: they sit side by side in one row.
worktree-source-tab-new = new
worktree-source-tab-existing = existing
worktree-source-tab-branch = branch
worktree-source-tab-pull-requests = PR
worktree-source-tab-issues = issues
# A tab's name with how many rows match the search, for screen readers.
worktree-source-tab-hits = { $tab } { $hits }
worktree-source-sign-in-pull-requests = Sign in to GitHub in Herdr (account button, top right) to list pull requests.
worktree-source-sign-in-issues = Sign in to GitHub in Herdr (account button, top right) to list issues.

worktree-source-search = Search checkouts, branches, pull requests, issues...
# The new checkout's name field, empty until the user types one.
worktree-source-default-name = Default name
worktree-source-name = Name

# How many rows the search shows, of all there are.
worktree-source-count-existing = { $total ->
        [one] { $shown } of { $total } checkout not open in Herdr
       *[other] { $shown } of { $total } checkouts not open in Herdr
    }
worktree-source-count-branches = { $total ->
        [one] { $shown } of { $total } local branch without a checkout
       *[other] { $shown } of { $total } local branches without a checkout
    }
worktree-source-count-pull-requests = { $total ->
        [one] { $shown } of { $total } open pull request
       *[other] { $shown } of { $total } open pull requests
    }
worktree-source-count-issues = { $total ->
        [one] { $shown } of { $total } open issue
       *[other] { $shown } of { $total } open issues
    }
worktree-source-no-matching-checkouts = No matching checkouts.
worktree-source-all-checkouts-open = Every checkout of this repository is already open.
worktree-source-no-matching-branches = No matching branches.
worktree-source-all-branches-checked-out = Every branch already has a checkout.
worktree-source-no-pull-requests = No open pull requests match.
worktree-source-no-issues = No open issues match.
worktree-source-loading = Loading...
# $name is a checkout's path, a branch, or a pull request or issue.
worktree-source-opening = Opening { $name }. Dismissing does not cancel it.
worktree-source-creating = Creating a checkout for { $name }. Dismissing does not cancel it.
worktree-source-existing-note = Opening adds the checkout to Herdr; nothing is created.
worktree-source-branch-note = The checkout uses the local branch as it is.
# Tags beside a row's title.
worktree-source-detached = detached
worktree-source-draft = draft
# A pull request from a fork, whose branch cannot be checked out here.
worktree-source-from-fork = from a fork - check out manually
worktree-source-no-checkout-path = The daemon did not report the new checkout path, so no agent context note was written.

# The open worktree picker.
worktree-source-search-worktrees = Search worktrees...
worktree-source-count-worktrees = { $total ->
        [one] { $shown } of { $total } worktree
       *[other] { $shown } of { $total } worktrees
    }
# Beside a checkout a Herdr workspace already has open.
worktree-source-open = open
worktree-source-loading-worktrees = Loading worktrees...
worktree-source-no-matching-worktrees = No matching worktrees. Try a shorter search.
worktree-source-worktrees-unavailable = Worktrees unavailable.
worktree-source-no-worktrees = No Git worktrees found for this repository.
