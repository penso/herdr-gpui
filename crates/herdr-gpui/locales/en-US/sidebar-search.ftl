# The search field that heads the sidebar's list, and the results that
# replace the list while it holds a query.

# What a screen reader calls the field, and its placeholder while focused.
sidebar-search-spaces-label = Search spaces
sidebar-search-spaces-placeholder = Devices, worktrees, branches, notes
sidebar-search-devices-label = Search devices
sidebar-search-devices-placeholder = Search devices and agents
sidebar-search-clear = Clear search
sidebar-search-no-matches = no matches
# Headings over each group of results, lowercase like the sidebar's.
sidebar-search-devices = devices
sidebar-search-worktrees = worktrees
sidebar-search-branches = branches
sidebar-search-notes = notes
# Beside a matching device: how many workspaces it has.
sidebar-search-spaces = { $count ->
    [one] { $count } space
   *[other] { $count } spaces
}
