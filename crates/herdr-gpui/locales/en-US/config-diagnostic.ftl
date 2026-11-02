# The text of the app's own config diagnostic cards, one line each: what
# this build ignored in config-gpui.local.toml, and fonts it could not use.
# File names and config keys stay as written.

# $keys lists unknown config keys, separated by
# commas; $more counts the keys left out of the list.
config-diagnostic-unknown-keys =
    { $more ->
        [0] config-gpui.local.toml: ignoring unknown keys { $keys }
       *[other] config-gpui.local.toml: ignoring unknown keys { $keys } and { $more } more
    }
config-diagnostic-sidebar-gap = config-gpui.local.toml: layout.sidebar_gap is deprecated; use terminal.padding_x
# $fonts lists configured font families, each in quotes, separated by
# commas; $more counts those left out of the list.
config-diagnostic-missing-fonts =
    { $more ->
        [0] Configured fonts not installed: { $fonts }
       *[other] Configured fonts not installed: { $fonts } and { $more } more
    }
# $family is a font family; $weight its configured weight as the config
# names it, such as "medium (500)" or "450".
config-diagnostic-missing-weight = "{ $family }" has no { $weight } face; using its nearest weight

# The notice shown once a prompt draws an icon no installed font has.
# Fonts are never bundled; the notice names how to install one.
config-diagnostic-icon-font = Prompt icons need a Nerd Font, and none is installed.
# On macOS, above the Homebrew command the card shows and copies.
config-diagnostic-icon-font-homebrew = Install one with Homebrew, then restart Herdr UI:
# Elsewhere. "Symbols Nerd Font Mono" is the font's name.
config-diagnostic-icon-font-download = Install Symbols Nerd Font Mono from nerdfonts.com, then restart Herdr UI.
