//! Every Settings row, in page order, so search can find it from any page.
//! Ids are the rows' own selectors; a test renders each page and checks they
//! still exist.
use super::Section;
use crate::config::Feature;

pub(super) struct Entry {
    pub section: Section,
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// When the page renders this row; search offers it only then.
    pub availability: Availability,
}

/// The state a row needs to be rendered at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Availability {
    Always,
    /// Only while the drafted theme follows the system as a light/dark pair.
    FollowingSystemTheme,
    /// Only on Windows, which has the only platform code for it.
    Windows,
}

impl Entry {
    const fn only(self, availability: Availability) -> Self {
        Self {
            availability,
            ..self
        }
    }
}

const fn entry(
    section: Section,
    id: &'static str,
    title: &'static str,
    description: &'static str,
) -> Entry {
    Entry {
        section,
        id,
        title,
        description,
        availability: Availability::Always,
    }
}

/// A `[features]` flag's row on the General page.
const fn feature(feature: Feature) -> Entry {
    entry(
        Section::General,
        feature.setting_id(),
        feature.label(),
        feature.description(),
    )
}

pub(super) const CATALOG: &[Entry] = &[
    entry(
        Section::Theme,
        "theme-mode",
        "Appearance",
        "Pick a theme for light and dark, or follow the system.",
    ),
    entry(
        Section::Theme,
        "theme-side",
        "Editing",
        "Which side of the pair the themes below change.",
    )
    .only(Availability::FollowingSystemTheme),
    entry(
        Section::Theme,
        "theme-follow",
        "Follow Herdr's theme",
        "Use the theme set in Herdr's own config instead of a GUI choice.",
    ),
    entry(
        Section::Theme,
        "theme-contrast",
        "High contrast",
        "Raise text and marker contrast on every theme.",
    ),
    entry(
        Section::Theme,
        "theme-source",
        "Source",
        "Ghostty themes apply to this app; Herdr themes are shared.",
    ),
    entry(
        Section::Theme,
        "theme-backdrop",
        "Backdrop",
        "What shows behind the main window: solid, translucent or a picture.",
    ),
    entry(
        Section::Theme,
        "theme-padding-left",
        "Left",
        "Terminal padding beside the sidebar.",
    ),
    entry(
        Section::Theme,
        "theme-padding-right",
        "Right",
        "Terminal padding on the right.",
    ),
    entry(
        Section::Theme,
        "theme-padding-top",
        "Top",
        "Terminal padding above the terminal.",
    ),
    entry(
        Section::Theme,
        "theme-padding-bottom",
        "Bottom",
        "Terminal padding below the terminal.",
    ),
    entry(
        Section::Sidebar,
        "sidebar-layout",
        "Layout preset",
        "Row shape and density; spacing below starts from the preset.",
    ),
    entry(
        Section::Sidebar,
        "sidebar-select",
        "Selection marks",
        "What selecting a workspace highlights beyond its own row.",
    ),
    entry(
        Section::Sidebar,
        "sidebar-device-footer",
        "Device footer",
        "The device picker and its buttons. Auto shows it with two or more devices.",
    ),
    entry(
        Section::Sidebar,
        "sidebar-change-counts",
        "Change counts",
        "The +added/-deleted lines under a worktree: its pull request's, or its uncommitted \
         changes against HEAD.",
    ),
    entry(
        Section::Sidebar,
        "sidebar-show-agents",
        "Show agents",
        "List running agents in the sidebar.",
    ),
    entry(
        Section::Sidebar,
        "sidebar-agents-placement",
        "Agents placement",
        "Their own section below the workspaces, or under the workspace each one runs in.",
    ),
    entry(
        Section::Sidebar,
        "sidebar-show-agents-header",
        "Show agents header",
        "The Agents title row with the sort toggle.",
    ),
    entry(
        Section::Sidebar,
        "sidebar-show-search",
        "Show search",
        "The search field above the workspaces.",
    ),
    entry(
        Section::Sidebar,
        "sidebar-repository-icons",
        "Show repository icons",
        "The repository owner's avatar at the start of each workspace row.",
    ),
    entry(
        Section::Sidebar,
        "sidebar-device-icons",
        "Show device icons",
        "A mark before each device's name: this machine, a cloud provider or SSH.",
    ),
    entry(
        Section::Sidebar,
        "sidebar-indent",
        "Indent",
        "Pixels each nesting level steps in.",
    ),
    entry(
        Section::Sidebar,
        "sidebar-row-padding",
        "Row padding",
        "Vertical padding around a row's text. No effect on card layouts.",
    ),
    entry(
        Section::Sidebar,
        "sidebar-row-gap",
        "Row gap",
        "Space between a row's status dot and its label.",
    ),
    entry(
        Section::Sidebar,
        "sidebar-host-gap",
        "Host gap",
        "Space between a host header's parts.",
    ),
    entry(
        Section::Sidebar,
        "sidebar-host-colours",
        "Host colours",
        "Tint a host's header, workspaces, worktrees and agents.",
    ),
    entry(
        Section::Indicators,
        "indicators-edge-rail",
        "Edge rail",
        "A bar on the leading edge of each working, blocked, or done row, whichever icons you pick.",
    ),
    entry(
        Section::Indicators,
        "indicators-sets",
        "Icons",
        "Dots, Solid, Outline, Tiles, Wave, Braille or Orca status icons. Working always moves.",
    ),
    entry(Section::Fonts, "fonts-terminal", "Terminal", ""),
    entry(Section::Fonts, "fonts-sidebar", "Sidebar", ""),
    entry(Section::Fonts, "fonts-tabs", "Tabs", ""),
    entry(Section::Fonts, "fonts-ui", "Interface", ""),
    entry(Section::Fonts, "fonts-all-row", "All faces", ""),
    entry(
        Section::StatusBar,
        "status-usage",
        "Usage",
        "Plan usage for signed-in AI services, detailed or compact.",
    ),
    entry(
        Section::StatusBar,
        "status-system-load",
        "CPU and memory",
        "Hiding CPU and memory also removes them from sidebar host rows.",
    ),
    entry(
        Section::StatusBar,
        "status-network-quality",
        "Network latency and loss",
        "Times one TCP connect to 1.1.1.1:443 every 15 seconds from this machine, about a megabyte a day.",
    ),
    entry(
        Section::StatusBar,
        "status-keep-awake",
        "Keep awake",
        "The cup keeps the machine from sleeping. System only lets the display sleep and lock, like caffeinate -i. A cup already on keeps its scope until it is turned off and on again.",
    ),
    entry(Section::StatusBar, "status-theme", "Theme", ""),
    entry(Section::StatusBar, "status-shortcuts", "Shortcuts", ""),
    entry(
        Section::StatusBar,
        "status-report-issue",
        "Report issue",
        "",
    ),
    entry(
        Section::General,
        "general-ports",
        "Show listening ports",
        "Port chips on workspaces with a dev server.",
    ),
    entry(
        Section::General,
        "general-checkpoints",
        "Checkpoint agent turns",
        "Snapshot the worktree after each agent turn.",
    ),
    entry(
        Section::General,
        "general-smooth-scroll",
        "Smooth scrolling",
        "Scroll terminal history between rows; off steps whole rows.",
    ),
    // One per `[features]` flag; a test checks none is missing.
    // keep-sorted start
    feature(Feature::SidebarHoverMenu),
    // keep-sorted end
    entry(
        Section::General,
        "general-copy-on-select",
        "Copy on select",
        "Off: \u{2318}C / Ctrl-C copies the highlighted selection.",
    ),
    entry(
        Section::General,
        "general-copied-toast",
        "Copied notification",
        "",
    ),
    entry(
        Section::General,
        "general-clipboard-position",
        "Notification position",
        "",
    ),
    entry(
        Section::General,
        "general-merged-worktrees",
        "Remove worktrees of merged PRs",
        "Deletes a clean linked worktree and closes its workspace once its PR is merged. The focused workspace asks first.",
    ),
    entry(
        Section::General,
        "general-browser-skill",
        "Teach local agents to use browser tabs",
        "Installed in ~/.claude and ~/.agents, kept up to date.",
    ),
    entry(
        Section::General,
        "general-launch-at-login",
        "Launch at login",
        "Open Herdr UI when you sign in to Windows.",
    )
    .only(Availability::Windows),
    entry(
        Section::General,
        "general-update-beta",
        "Install beta releases",
        "Betas ship before they are promoted to stable. Turning this off never downgrades; Homebrew installations update in place while it is on.",
    ),
    entry(
        Section::Tabs,
        "tabs-layout",
        "Tab layout",
        "How tab strips draw their tabs. Also under View > Tab Layout.",
    ),
    entry(Section::Tabs, "tabs-position", "Position", ""),
    entry(
        Section::Tabs,
        "tabs-hide-single",
        "Hide with a single tab",
        "",
    ),
    entry(
        Section::Tabs,
        "tabs-confirm-tab",
        "Confirm before closing a tab",
        "",
    ),
    entry(
        Section::Tabs,
        "tabs-confirm-pane",
        "Confirm before closing a pane",
        "",
    ),
    entry(
        Section::Tabs,
        "tabs-confirm-workspace",
        "Confirm before closing a workspace",
        "",
    ),
    entry(
        Section::Tabs,
        "tabs-focus-follows-mouse",
        "Focus follows mouse",
        "The pointer chooses the active pane within a split terminal tab.",
    ),
    entry(
        Section::Tabs,
        "tabs-pane-history",
        "Restore scrollback after Herdr restarts",
        "Experimental. Herdr saves pane output to session-history.json and replays it when the daemon restarts.",
    ),
    entry(
        Section::Code,
        "code-mode",
        "VS Code server",
        "Start VS Code automatically, or use the address of a server you run.",
    ),
    entry(
        Section::Notifications,
        "notif-delivery",
        "Deliver agent notifications",
        "Herdr shows them in-app. System uses OS notifications; clicking one opens its pane. Terminal is for other clients only.",
    ),
    entry(
        Section::Notifications,
        "notif-finished",
        "Finished notifications",
        "Off drops the notice at the end of each agent turn; needs attention still shows.",
    ),
    entry(Section::Notifications, "notif-show", "Show toasts", ""),
    entry(
        Section::Notifications,
        "notif-delay",
        "Delay",
        "Wait before a toast appears, so quick turns stay quiet.",
    ),
    entry(Section::Notifications, "notif-position", "Position", ""),
    entry(
        Section::Notifications,
        "notif-sound",
        "Play agent sounds",
        "Uses Herdr's sound paths and per-agent overrides.",
    ),
    entry(
        Section::Notifications,
        "notif-volume",
        "Volume",
        "Sounds and Test play at this level, even when sounds are off.",
    ),
    entry(
        Section::Integrations,
        "integrations-search",
        "Integrations",
        "Agents Herdr can launch and track.",
    ),
    entry(
        Section::Plugins,
        "plugins-values",
        "Sidebar values",
        "Show a value a plugin reports as its own sidebar row.",
    ),
    #[cfg(feature = "cloud")]
    entry(
        Section::CloudDevices,
        "cloud-accounts",
        "Cloud accounts",
        "Accounts for each cloud provider, and the machines saved as devices.",
    ),
    #[cfg(feature = "cloud")]
    entry(
        Section::CloudDevices,
        "cloud-setup",
        "New device setup",
        "Agents such as Claude Code, Codex, Gemini CLI, or OpenCode, and a setup script, installed when a new cloud device first connects.",
    ),
    entry(
        Section::Configuration,
        "config-local",
        "Herdr UI settings",
        "The file holding Herdr UI's own settings on this machine.",
    ),
    entry(
        Section::Configuration,
        "config-shared",
        "Herdr configuration",
        "Herdr's configuration file, which its terminal client reads too.",
    ),
    entry(
        Section::Configuration,
        "config-reload",
        "Reload configuration",
        "Re-reads both files. Does not restart the daemon.",
    ),
];

/// Indices into [`CATALOG`] of the entries whose title, description or page
/// name contains `query`, ignoring case, in catalog order.
pub(super) fn search(query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    CATALOG
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            [entry.title, entry.description, entry.section.label()]
                .iter()
                .any(|text| text.to_lowercase().contains(&query))
        })
        .map(|(index, _)| index)
        .collect()
}
