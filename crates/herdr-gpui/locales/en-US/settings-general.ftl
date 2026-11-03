# Settings > General, beyond the rows search finds (settings.ftl).

## Groups on the page.

settings-general-section-interface = Interface
settings-general-section-features = Features
settings-general-section-clipboard = Clipboard
settings-general-section-worktrees = Worktrees
settings-general-section-browser-skill = Browser skill
settings-general-section-startup = Startup
settings-general-section-updates = Updates

## Rows.

# The language picker's first choice. $language is the name, in itself, of
# the language the system's preferences pick (English, עברית).
settings-general-language-system = System ({ $language })
# A tag the `language` setting names that has no messages. $tag is the BCP 47
# tag (fr-CA); $language names, in itself, the language shown instead.
settings-general-language-unavailable = { $tag }, shown in { $language }

settings-general-window-controls = Show window controls
settings-general-window-controls-description = Draw minimize, maximize, and close buttons in the menu bar. Disable server-side decorations in your window manager to avoid a double bar.
settings-general-chat-layout = Chat layout
settings-general-chat-layout-description = How chat views draw messages and the composer: on a timeline, as bubbles like Claude's app, as Claude Code's terminal screen, as a document, as one card per turn, as Messages bubbles, or in one quiet column like iA Writer. The button in a chat's header steps through them too.
settings-general-chat-launcher = Chat launcher
settings-general-chat-launcher-description = Where tab strips offer a one-click chat with each installed agent. "+" keeps opening a terminal tab in every placement.
settings-general-chat-glide = Smooth chat scrolling
settings-general-chat-glide-description = Scroll smoothly to a reply's new text as it streams in. Off, the chat jumps to it as each chunk lands.
settings-general-chat-full-width = Full-width chat
settings-general-chat-full-width-description = Use all available space for messages and the composer.
settings-general-chat-width = Chat width
settings-general-chat-width-description = Maximum reading width in logical pixels. Narrow windows fit automatically.
settings-general-browser-skill-missing = Not installed.
settings-general-browser-skill-install = Install
settings-general-browser-skill-remove = Remove
# Windows' own Startup switch has turned the entry off.
settings-general-launch-at-login-disabled = Turned off in Settings > Apps > Startup. Turn it back on there.
settings-general-error-launch-at-login = Launch at login: { $error }
