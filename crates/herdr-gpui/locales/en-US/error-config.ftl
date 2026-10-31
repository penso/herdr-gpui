# Settings that could not be loaded, checked, or edited. Words such as
# theme, chat.max_width, or devices.<id> are config keys and tables, written
# as the config file writes them. $table is a config table such as font,
# terminal_font, or zen; $error is the failure underneath.

# $device is a saved device's ID.
error-config-device-theme = devices.{ $device }.theme: { $error }
error-config-empty-font-fallback = { $table }.fallback families must not be empty
error-config-empty-font-family = { $table }.family must not be empty
error-config-empty-theme = theme must not be empty
error-config-invalid-chat-width = chat.max_width must be 0 (full width) or 320–4000 logical pixels
error-config-invalid-code-table = code must be a TOML table
# $device is the ID the config named, quoted.
error-config-invalid-device-id = devices.{ $device } is not a saved device ID
error-config-invalid-devices-table = devices and each devices.<id> must be tables
error-config-invalid-favorite-themes = favorite_themes must be a list of theme names
error-config-invalid-font-size = { $table }.size must be finite and between 8 and 48 logical pixels
error-config-invalid-font-weight = { $table }.weight must be a weight name such as "medium" or a number from 100 to 900
# $host is a host's name and $value the colour the config gave it, both quoted.
error-config-invalid-host-color = sidebar.hosts.{ $host } must be a #rgb or #rrggbb colour, not { $value }
error-config-invalid-line-height = { $table }.line_height must be a finite number between 1 and 2 times the font size
error-config-invalid-sidebar-gap = layout.sidebar_gap must be finite and between 0 and 64 logical pixels
# $key is a sidebar config key; $max is its largest value.
error-config-invalid-sidebar-metric = sidebar.{ $key } must be finite and between 0 and { $max } logical pixels
error-config-invalid-sound-volume = sound.volume must be between 0 and 100
# $key is a terminal config key, such as padding.
error-config-invalid-terminal-padding = terminal.{ $key } must be one number or a pair of numbers, each finite and between 0 and 64 logical pixels
error-config-invalid-theme-pair = a theme that follows the system must name both sides: light:NAME,dark:NAME
error-config-invalid-theme-path = theme must be a name, absolute path, or ~/ path
error-config-invalid-usage-table = usage must be a TOML table
error-config-invalid-zen-font-size = zen.font_size must be finite and between 8 and 48 logical pixels
error-config-invalid-zen-width = zen.width must be 20 to 1000 columns, or a percentage from "10%" to "100%"
# $key is a dotted config key.
error-config-key-not-table = { $key }: a part of this key holds a value, not a table; unset that part first
# $original is the old config file; $local is the local file it moves into.
error-config-migration-conflict = Cannot migrate { $original }: { $local } already contains different settings. Merge your old settings into the local file, then remove the old config; it will be regenerated.
error-config-preferences-not-object = preferences must be an object
# A line of a terminal theme file. $line is its number, $key the color it sets.
error-config-theme-line = line { $line }: { $key }: { $error }
# $name is the theme's name, quoted; $directories lists the folders searched.
error-config-theme-not-found = theme { $name } not found in { $directories }
# $limit is the most devices the table may list.
error-config-too-many-devices = [devices] must list at most { $limit } devices
error-config-too-many-font-fallbacks = { $table }.fallback must list at most 8 families
# $keys lists the config keys, separated by commas.
error-config-unknown-keys = Herdr UI has no setting { $keys }; config-gpui.toml lists every setting
# $name is the layout's name as the config wrote it, quoted.
error-config-unknown-layout = Unknown sidebar layout { $name }

# One line of a terminal theme's colors that did not parse.
error-config-theme-parse-invalid-color = expected a six-digit RGB hex color (optionally prefixed by #)
error-config-theme-parse-invalid-hex = invalid hex color
error-config-theme-parse-missing-palette-color = expected index=color
error-config-theme-parse-palette-index = palette index must be between 0 and 255

# The [ssh] table: hosts whose key the app does not verify.
error-config-ssh-invalid-table = ssh must be a table and ssh.skip_host_key_verification a list of SSH targets
# $target is the entry as the config wrote it, quoted.
error-config-ssh-invalid-target = ssh.skip_host_key_verification: { $target } is not an SSH target
# $limit is the most hosts the list may name.
error-config-ssh-too-many = ssh.skip_host_key_verification must list at most { $limit } hosts

# $name is the layout's name as the config wrote it, quoted.
error-config-tab-layout-unknown = Unknown tab layout { $name }
