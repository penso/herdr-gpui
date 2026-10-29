# Settings > Theme: appearance, the theme browser, the window backdrop, and
# terminal padding. Row titles are in settings.ftl; theme names are data.

settings-theme-section-mode = Mode
settings-theme-section-themes = Themes
settings-theme-section-window = Window
settings-theme-section-padding = Terminal padding
settings-theme-light = Light
settings-theme-dark = Dark
settings-theme-system = System
settings-theme-mode-hint = Light, dark, or follow the system. Pick a theme below to change it.
# $light and $dark are theme names.
settings-theme-side-hint = Which side of the pair the themes below change. Light: { $light } · Dark: { $dark }
# Shown for the theme value that uses Herdr's own theme.
settings-theme-follow-herdr = Follow Herdr
settings-theme-library-all = All
settings-theme-library-ghostty = Ghostty
settings-theme-library-herdr = Herdr
settings-theme-library-read-only = Herdr's themes are read-only on this platform.
settings-theme-read-only = Herdr's themes are read-only on this platform. Choose This app or Follow Herdr instead.
settings-theme-open-file = Open theme file
settings-theme-search-placeholder = Search themes...
settings-theme-count =
    { $total ->
        [one] { $shown } of { $total } theme
       *[other] { $shown } of { $total } themes
    }
settings-theme-count-discovering =
    { $total ->
        [one] { $shown } of { $total } theme / discovering installed themes...
       *[other] { $shown } of { $total } themes / discovering installed themes...
    }
settings-theme-empty = No matching themes. Try a shorter search.

## Where a theme card's theme comes from, and whether it is drafted or saved.

settings-theme-kind-herdr = Herdr
settings-theme-kind-built-in = Built-in
settings-theme-kind-file = File
settings-theme-kind-ghostty = Ghostty
# $source is one of the settings-theme-kind- messages.
settings-theme-card-draft = { $source } / Draft
settings-theme-card-saved = { $source } / Saved
settings-theme-thumbnail-unavailable = Unavailable
settings-theme-thumbnail-loading = Loading...
settings-theme-catalog-error = Catalog discovery failed: { $error }. Built-ins remain available.
settings-theme-retry-catalog = Retry discovery
settings-theme-preview-error = Preview unavailable: { $error }. Keeping the last valid preview.
settings-theme-retry-preview = Retry preview
settings-theme-error-load = Load theme: { $error }

## The window backdrop.

settings-theme-backdrop-solid = Solid
settings-theme-backdrop-translucent = Translucent
settings-theme-backdrop-picture = Picture
settings-theme-backdrop-solid-hint = The window paints as it always has.
settings-theme-backdrop-translucent-hint = Colors a program chooses, popups, and dialogs stay opaque.
settings-theme-backdrop-picture-hint = The window stays opaque over the picture; lower opacity shows more of it.
settings-theme-backdrop-blur = Blur behind
settings-theme-backdrop-mica = Mica
settings-theme-backdrop-mica-hint = Mica tints the window with the wallpaper rather than the windows behind it.
settings-theme-backdrop-change = Change
settings-theme-backdrop-remove = Remove
# The button of the file chooser that picks a picture.
settings-theme-backdrop-choose = Choose
settings-theme-backdrop-opacity = Background opacity
settings-theme-backdrop-percent = { $percent }%

## Terminal padding.

settings-theme-padding-left-hint = Beside the sidebar. A deprecated sidebar_gap stands in while unset.
