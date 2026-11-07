# The title bar's Git popup for the focused checkout, and its pull request.

git-menu-checking = Checking working tree...
git-menu-clean = No uncommitted changes
# Lines added and removed, as "+12 -3", and untracked files and folders.
git-menu-changes = { $untracked ->
        [0] +{ $additions } -{ $deletions }
        [one] +{ $additions } -{ $deletions }, { $untracked } untracked entry
       *[other] +{ $additions } -{ $deletions }, { $untracked } untracked entries
    }
git-menu-commit = Commit...
git-menu-push = Push
git-menu-open-pull-request = Open pull request #{ $number }
git-menu-create-pull-request = Create pull request
git-menu-checks-and-comments = Checks and comments
git-menu-comment = Comment...
git-menu-merge = Merge...
git-menu-not-committed = { $untracked ->
        [0] Not yet committed
       *[other] Not yet committed ({ $untracked } untracked)
    }
git-menu-draft = Draft — not ready for review
git-menu-ready = Ready for review
# Headings beside the pull request's review state, mergeability, and checks.
git-menu-review-heading = Review
git-menu-merge-heading = Merge
git-menu-checks-heading = Checks
git-menu-loading-pull-request = Loading pull request...
git-menu-open-in-browser = Open in browser
