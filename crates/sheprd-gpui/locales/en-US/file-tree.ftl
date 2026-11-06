# The file tree tab: a checkout's files, each marked with how Git sees it.
# File names are never translated.

file-tree-filter = Filter files…
file-tree-refresh = Refresh
file-tree-reading = Reading files…
file-tree-no-matches = No file matches
file-tree-empty = This checkout has no files
file-tree-local-only = The file tree shows local checkouts only
file-tree-no-checkout = The focused workspace has no Git checkout
file-tree-no-workspace = Open a workspace first
file-tree-too-many-tabs = Too many tabs are open
# How many files the tree lists.
file-tree-count = { $count ->
    [one] 1 file
   *[other] { $count } files
}
# The same, when the checkout has more than the tree lists.
file-tree-count-truncated = First { $count } files
