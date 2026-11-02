# Go to Symbol and Go to File: a picker over the focused pane's checkout.

code-search-title-symbols = Go to Symbol
code-search-title-files = Go to File
code-search-placeholder-symbols = Go to a function, type, or module...
code-search-placeholder-files = Go to a file, or file:line...
# The tabs that switch between the two.
code-search-symbols = Symbols
code-search-files = Files
code-search-reading = Reading the checkout...
# How many results are shown of how many there are. $more is "yes" when
# there are more than the picker lists; $partial is "yes" when the checkout
# was too large to read whole.
code-search-count =
    { $partial ->
        [yes]
            { $more ->
                [yes] { $shown }+ of { $total } (checkout too large, some left out)
               *[no] { $shown } of { $total } (checkout too large, some left out)
            }
       *[no]
            { $more ->
                [yes] { $shown }+ of { $total }
               *[no] { $shown } of { $total }
            }
    }
code-search-no-symbols = No matching symbols.
code-search-no-files = No matching files.
# The keys the picker takes. $viewer is the key that opens the viewer,
# ⌘Enter or Ctrl-Enter.
code-search-hints = ↑ / ↓ navigate · Tab symbols/files · Enter editor · { $viewer } viewer · Esc cancel
# A symbol's place, path:line, when its file has uncommitted changes.
code-search-symbol-changed = { $place }  · changed
# What kind of definition a symbol is.
code-search-kind-function = function
code-search-kind-method = method
code-search-kind-type = type
code-search-kind-module = module
code-search-kind-constant = constant
code-search-kind-implementation = impl
code-search-kind-macro = macro
# A changed file's badge: a file Git does not track yet, or one without
# line counts.
code-search-badge-new = new
code-search-badge-binary = binary

# The line under a changed file's path. A new file is one Git does not
# track yet. $ago is how long ago it was last written, such as 5 min ago
# or yesterday.
code-search-new-ago = New { $ago }
code-search-changed-ago = Changed { $ago }
code-search-new = New
code-search-changed = Changed
