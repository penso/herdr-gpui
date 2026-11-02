# Keybindings that could not be used. keybindings and pane_keys are config
# tables; cmd, ctrl, alt, and fn are modifier key names. $keystroke is a key
# as the config writes it, quoted, such as "cmd-t". $command is a command's
# config name, such as new_tab.

# $first and $second are command names.
error-keymap-duplicate-keystroke = keybindings: { $keystroke } is bound to both { $first } and { $second }
error-keymap-duplicate-pane-key = pane_keys: { $keystroke } is listed twice
error-keymap-empty-pane-key-text = pane_keys.{ $keystroke }: text: needs something to send
error-keymap-invalid-keystroke = keybindings.{ $command }: invalid keystroke { $keystroke }
error-keymap-invalid-pane-key = pane_keys: invalid keystroke { $keystroke }
error-keymap-keystroke-without-modifier = keybindings.{ $command }: { $keystroke } needs a cmd, ctrl, alt, or fn modifier so typing still reaches the terminal
error-keymap-pane-key-bound = pane_keys: { $keystroke } is also bound to keybindings.{ $command }
# $limit is a size in bytes.
error-keymap-pane-key-text-too-long = pane_keys.{ $keystroke }: text may be at most { $limit } bytes
error-keymap-pane-key-without-modifier = pane_keys: { $keystroke } needs a cmd, ctrl, alt, or fn modifier so typing still reaches the terminal
error-keymap-too-many-keystrokes = keybindings.{ $command } must list at most 8 keystrokes
# $limit is the most keys the table may list.
error-keymap-too-many-pane-keys = pane_keys must list at most { $limit } keystrokes
error-keymap-unknown-keybinding = keybindings.{ $command } is not a command; see the keybindings list in config-gpui.toml
# $from is the key pressed and $to the key a pane would be sent, both quoted.
error-keymap-unsendable-pane-key = pane_keys.{ $from }: a pane cannot receive { $to }

# The keybindings a host publishes. [keys] is a table in them.
error-keymap-daemon-missing = the host did not publish its keybindings
error-keymap-daemon-no-keys = the host's keybindings have no [keys] table
# $error is the TOML parser's text.
error-keymap-daemon-parse = the host's keybindings are not valid TOML: { $error }
# $max is a size in bytes.
error-keymap-daemon-too-large = the host's keybindings exceed { $max } bytes
