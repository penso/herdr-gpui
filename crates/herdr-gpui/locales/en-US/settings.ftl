# The Settings window: its frame, section list, search, and footer, and the
# title and description of every row search can find (catalog.rs), which
# the pages show the rows by too. Each page's other text has its own file.

settings-title = Settings
settings-search-placeholder = Search settings
settings-sections-label = Settings sections
settings-sections = Sections
settings-results = Results
settings-search-empty = No settings match.
# A search result: the page it is on, then the row's title.
settings-search-result = { $section } › { $title }
settings-loading = Loading preferences...
settings-finishing = Finishing preferences before closing...
settings-saved = All changes saved.
settings-saved-cloud = Account fields save with Save; sign-in and removal apply at once.
# $drafts lists the unsaved parts, each one of the settings-draft- messages.
settings-unsaved = Unsaved: { $drafts } — saved when this window closes
settings-draft-theme = theme
settings-draft-layout = layout
settings-draft-sidebar = sidebar
settings-save-now = Save now
settings-reload = Reload
settings-error-system-appearance = Apply system appearance: { $error }
# Which app a setting belongs to, as a tag beside it.
settings-source-herdr = Herdr
settings-source-herdr-ui = Herdr UI
# The footer after a save: what went wrong saving or reading the files back.
settings-saved-unreloaded = Saved; could not reload current preferences
settings-save-failed-reloaded = Save failed; reloaded current preferences
settings-save-failed = Save failed; reload before editing again
settings-saved-daemon-unqueued = Saved; daemon reload not queued: { $error }
settings-error-save = Save settings: { $error }
settings-error-reload = Could not reload settings; keeping current preferences: { $error }
settings-error-load-herdr = Load Herdr settings: { $error }

## Shared by the pages' controls.

# Why rows that edit Herdr's own file are disabled.
settings-shared-read-only = Herdr's settings are read-only on this platform.
settings-shared-unavailable = Herdr's settings are unavailable. Reload to retry.
# The first entry of a dropdown that can follow Herdr's file, with the
# value Herdr has; the other entries override it.
settings-use-herdr = Use Herdr setting · { $value }
settings-on = On
settings-off = Off
# Where a notification appears in the window.
settings-position-top-left = Top left
settings-position-top-center = Top center
settings-position-top-right = Top right
settings-position-bottom-left = Bottom left
settings-position-bottom-center = Bottom center
settings-position-bottom-right = Bottom right
# Under a number field with steppers: the values it takes.
settings-number-range = { $min } to { $max }
# Units after a number field's value.
settings-unit-percent = %
settings-unit-pixels = px
settings-unit-seconds = s
# A host with no colour of its own, in place of a colour's hex code.
settings-swatch-none = none

## Groups of pages in the section list.

settings-group-appearance = Appearance
settings-group-behaviour = Behaviour
settings-group-connect = Connect
settings-group-advanced = Advanced

## Pages, and the line under each page's title.

settings-section-general = General
settings-section-theme = Theme
settings-section-sidebar = Sidebar
settings-section-indicators = Indicators
settings-section-fonts = Fonts
settings-section-status-bar = Status bar
settings-section-tabs-and-panes = Tabs & panes
settings-section-code = Code
settings-section-notifications = Notifications
settings-section-integrations = Integrations
settings-section-plugins = Plugins
settings-section-cloud-devices = Cloud devices
settings-section-remote-control = Remote control
settings-section-configuration = Configuration
settings-lead-theme = Hover a theme to try it in the app; click to keep it.
settings-lead-sidebar = Changes apply to every open window immediately.
settings-lead-indicators = How agent status shows in the sidebar, tabs and the collapsed rail. Working always moves; reduced motion holds it still.
settings-lead-fonts = Each face can have its own family and size.
settings-lead-status-bar = Which items the bottom bar shows, and how compactly.
settings-lead-code = Review code and diffs beside your terminals through a Visual Studio Code server.
settings-lead-integrations = Agents Herdr can launch and track.
settings-lead-plugins = Show what your plugins report in the sidebar.
settings-lead-cloud-devices = Create machines from your cloud accounts and use them as devices.
settings-lead-remote-control = Use your chats from a phone, over HTTPS on your network or through Tailscale.
settings-lead-configuration = Files behind these settings. Saved edits reload automatically.

## Rows, in catalog order: each title, then its description.

settings-theme-mode = Appearance
settings-theme-mode-description = Pick a theme for light and dark, or follow the system.
settings-theme-side = Editing
settings-theme-side-description = Which side of the pair the themes below change.
settings-theme-follow = Follow Herdr's theme
settings-theme-follow-description = Use the theme set in Herdr's own config instead of a GUI choice.
settings-theme-contrast = High contrast
settings-theme-contrast-description = Raise text and marker contrast on every theme.
settings-theme-source = Source
settings-theme-source-description = Ghostty themes apply to this app; Herdr themes are shared.
settings-theme-backdrop = Backdrop
settings-theme-backdrop-description = What shows behind the main window: solid, translucent or a picture.
settings-theme-padding-left = Left
settings-theme-padding-left-description = Terminal padding beside the sidebar.
settings-theme-padding-right = Right
settings-theme-padding-right-description = Terminal padding on the right.
settings-theme-padding-top = Top
settings-theme-padding-top-description = Terminal padding above the terminal.
settings-theme-padding-bottom = Bottom
settings-theme-padding-bottom-description = Terminal padding below the terminal.
settings-sidebar-layout = Layout preset
settings-sidebar-layout-description = Row shape and density; spacing below starts from the preset.
settings-sidebar-select = Selection marks
settings-sidebar-select-description = What selecting a workspace highlights beyond its own row.
settings-sidebar-device-footer = Device footer
settings-sidebar-device-footer-description = The device picker and its buttons. Auto shows it with two or more devices.
settings-sidebar-change-counts = Change counts
settings-sidebar-change-counts-description = The +added/-deleted lines under a worktree: its pull request's, or its uncommitted changes against HEAD.
settings-sidebar-show-agents = Show agents
settings-sidebar-show-agents-description = List running agents in the sidebar.
settings-sidebar-agents-placement = Agents placement
settings-sidebar-agents-placement-description = Their own section below the workspaces, or under the workspace each one runs in.
settings-sidebar-show-agents-header = Show agents header
settings-sidebar-show-agents-header-description = The Agents title row with the sort toggle.
settings-sidebar-show-search = Show search
settings-sidebar-show-search-description = The search field above the workspaces.
settings-sidebar-repository-icons = Show repository icons
settings-sidebar-repository-icons-description = The repository owner's avatar at the start of each workspace row.
settings-sidebar-device-icons = Show device icons
settings-sidebar-device-icons-description = A mark before each device's name: this machine, a cloud provider or SSH.
settings-sidebar-gpu-load = Show GPU and video memory
settings-sidebar-gpu-load-description = Each host's GPU busy share and VRAM on a line under its CPU and memory, or a GPU gauge after its name in compact layouts. Off by default.
settings-sidebar-indent = Indent
settings-sidebar-indent-description = Pixels each nesting level steps in.
settings-sidebar-row-padding = Row padding
settings-sidebar-row-padding-description = Vertical padding around a row's text. No effect on card layouts.
settings-sidebar-row-gap = Row gap
settings-sidebar-row-gap-description = Space between a row's status dot and its label.
settings-sidebar-host-gap = Host gap
settings-sidebar-host-gap-description = Space between a host header's parts.
settings-sidebar-host-colours = Host colours
settings-sidebar-host-colours-description = Tint a host's header, workspaces, worktrees and agents.
settings-indicators-edge-rail = Edge rail
settings-indicators-edge-rail-description = A bar on the leading edge of each working, blocked, or done row, whichever icons you pick.
settings-indicators-sets = Icons
settings-indicators-sets-description = Dots, Solid, Outline, Tiles, Wave, Braille or Orca status icons. Working always moves.
settings-fonts-terminal = Terminal
settings-fonts-sidebar = Sidebar
settings-fonts-tabs = Tabs
settings-fonts-ui = Interface
settings-fonts-all-row = All faces
settings-status-usage = Usage
settings-status-usage-description = Plan usage for signed-in AI services, detailed or compact.
settings-status-system-load = CPU and memory
settings-status-system-load-description = Hiding CPU and memory also removes them from sidebar host rows.
settings-status-gpu = GPU and video memory
settings-status-gpu-description = The selected host's GPU busy share and VRAM, from nvidia-smi, the amdgpu driver, or ioreg on a Mac. Off by default.
settings-status-network-quality = Network latency and loss
settings-status-network-quality-description = Times one TCP connect to 1.1.1.1:443 every 15 seconds from this machine, about a megabyte a day.
settings-status-keep-awake = Keep awake
settings-status-keep-awake-description = The cup keeps the machine from sleeping. System only lets the display sleep and lock, like caffeinate -i. A cup already on keeps its scope until it is turned off and on again.
settings-status-theme = Theme
settings-status-shortcuts = Shortcuts
settings-status-report-issue = Report issue
settings-general-ports = Show listening ports
settings-general-ports-description = Port chips on workspaces with a dev server.
settings-general-checkpoints = Checkpoint agent turns
settings-general-checkpoints-description = Snapshot the worktree after each agent turn.
settings-general-smooth-scroll = Smooth scrolling
settings-general-smooth-scroll-description = Scroll terminal history between rows; off steps whole rows.
settings-general-copy-on-select = Copy on select
settings-general-copy-on-select-description = Off: ⌘C / Ctrl-C copies the highlighted selection.
settings-general-copied-toast = Copied notification
settings-general-clipboard-position = Notification position
settings-general-merged-worktrees = Remove worktrees of merged PRs
settings-general-merged-worktrees-description = Deletes a clean linked worktree and closes its workspace once its PR is merged. The focused workspace asks first.
settings-general-browser-skill = Teach local agents to use browser tabs
settings-general-browser-skill-description = Installed in ~/.claude and ~/.agents, kept up to date.
settings-general-launch-at-login = Launch at login
settings-general-launch-at-login-description = Open Herdr UI when you sign in to Windows.
settings-general-update-beta = Install beta releases
settings-general-update-beta-description = Betas ship before they are promoted to stable. Turning this off never downgrades; Homebrew installations update in place while it is on.
settings-tabs-layout = Tab layout
settings-tabs-layout-description = How tab strips draw their tabs. Also under View > Tab Layout.
settings-tabs-position = Position
settings-tabs-hide-single = Hide with a single tab
settings-tabs-confirm-tab = Confirm before closing a tab
settings-tabs-confirm-pane = Confirm before closing a pane
settings-tabs-confirm-workspace = Confirm before closing a workspace
settings-tabs-focus-follows-mouse = Focus follows mouse
settings-tabs-focus-follows-mouse-description = The pointer chooses the active pane within a split terminal tab.
settings-tabs-pane-history = Restore scrollback after Herdr restarts
settings-tabs-pane-history-description = Experimental. Herdr saves pane output to session-history.json and replays it when the daemon restarts.
settings-code-mode = VS Code server
settings-code-mode-description = Start VS Code automatically, or use the address of a server you run.
settings-notif-delivery = Deliver agent notifications
settings-notif-delivery-description = Herdr shows them in-app. System uses OS notifications; clicking one opens its pane. Terminal is for other clients only.
settings-notif-finished = Finished notifications
settings-notif-finished-description = Off drops the notice at the end of each agent turn; needs attention still shows.
settings-notif-show = Show toasts
settings-notif-delay = Delay
settings-notif-delay-description = Wait before a toast appears, so quick turns stay quiet.
settings-notif-position = Position
settings-notif-sound = Play agent sounds
settings-notif-sound-description = Uses Herdr's sound paths and per-agent overrides.
settings-notif-volume = Volume
settings-notif-volume-description = Sounds and Test play at this level, even when sounds are off.
settings-integrations-search = Integrations
settings-integrations-search-description = Agents Herdr can launch and track.
settings-plugins-values = Sidebar values
settings-plugins-values-description = Show a value a plugin reports as its own sidebar row.
settings-cloud-accounts = Cloud accounts
settings-cloud-accounts-description = Accounts for each cloud provider, and the machines saved as devices.
settings-cloud-setup = New device setup
settings-cloud-setup-description = Agents such as Claude Code, Codex, Gemini CLI, or OpenCode, and a setup script, installed when a new cloud device first connects.
settings-remote-enabled = Remote control
settings-remote-enabled-description = Serve your chats to paired phones over HTTPS, as a user service.
settings-remote-keep-running = Keep running when Herdr UI quits
settings-remote-keep-running-description = Phones keep working and notifications keep coming after the app quits.
settings-remote-port = Port
settings-remote-port-description = The HTTPS port phones connect to.
settings-remote-bind = Listen on
settings-remote-bind-description = Every interface, or only one network's address.
settings-remote-addresses = Addresses
settings-remote-addresses-description = The addresses phones reach this computer at: the LAN, and Tailscale when it runs.
settings-remote-certificate = Certificate
settings-remote-certificate-description = The daemon's own certificate authority and its fingerprint, or your own files.
settings-remote-notifications = Notification text
settings-remote-notifications-description = Whether phone notifications include a short excerpt of the message.
settings-remote-idle = Forget idle devices after
settings-remote-idle-description = A day, a week, a month, or never: how long a paired device may stay away.
settings-remote-pair-card = Pair a phone
settings-remote-pair-card-description = Show a QR code to scan with a phone, then approve it.
settings-remote-devices = Paired devices
settings-remote-devices-description = Devices that can control your chats, with when each was last seen. Revoke one here.
settings-config-local = Herdr UI settings
settings-config-local-description = The file holding Herdr UI's own settings on this machine.
settings-config-shared = Herdr configuration
settings-config-shared-description = Herdr's configuration file, which its terminal client reads too.
settings-config-reload = Reload configuration
settings-config-reload-description = Re-reads both files. Does not restart the daemon.
