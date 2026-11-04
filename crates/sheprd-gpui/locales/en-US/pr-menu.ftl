# The workspace popover's pull request section: a card for the branch's pull
# request, or a line saying why there is none.

pr-menu-heading = Pull request
pr-menu-files = { $count ->
        [one] { $count } file
       *[other] { $count } files
    }
pr-menu-checking = Checking GitHub...
# The pull request shown is from an earlier answer; $message says why it was
# not refreshed, in English.
pr-menu-stale = Stale: { $message }
pr-menu-none = No PR found for this origin and branch.
pr-menu-workspace-changed = Workspace changed or disconnected. Reopen the menu.
