# The main window's find bar, copy mode, and copying a selection from a
# pane. $error says why something failed, in the error's own words.

# The count beside a search: none found, $current of $total, or $total
# found while no match is current.
window-find-no-results = No results
window-find-match = { $current } of { $total }
window-find-found = { $total } found

## The find bar

window-find-placeholder = Find
window-find-needs-newer = Find needs a newer Herdr daemon
window-find-scroll-failed = Scroll not sent: { $error }

## Copy mode

window-find-copy-mode = Copy mode
# $search is the search marker, query, and count, such as "/error 3 of 17".
window-find-copy-mode-search = Copy mode  { $search }
window-find-copy-search-placeholder = Search
window-find-copy-mode-needs-newer = Copy mode needs a newer Herdr daemon
window-find-copy-search-needs-newer = Copy mode search needs a newer Herdr daemon
# $request is "motion" (moving the copy cursor) or "search".
window-find-copy-request-not-sent =
    { $request ->
        [motion] Copy mode motion not sent: { $error }
       *[search] Copy mode search not sent: { $error }
    }
window-find-copy-request-failed = Copy mode request failed: { $error }
window-find-copy-scroll-failed = Scroll not sent: { $error }

## Copying a selection

# The flash after a selection is copied, when the clipboard notice is on.
window-find-copied = copied to clipboard
window-find-selection-not-copied = Selection not copied: { $error }
window-find-selection-scroll-failed = Scroll not sent: { $error }
