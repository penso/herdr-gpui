//! Every Settings row, in page order, so search can find it from any page.
//! Ids are the rows' own selectors; a test renders each page and checks they
//! still exist. Titles and descriptions are message IDs in `settings.ftl`,
//! which the pages show the rows by too.
// i18n: migrated
use super::Section;
use crate::{config::Feature, i18n};

pub(super) struct Entry {
    pub section: Section,
    pub id: &'static str,
    /// The message naming the row.
    pub title: &'static str,
    /// The message describing it, or empty when it has none.
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
    pub(super) fn title(&self) -> String {
        i18n::message(self.title, &[])
    }

    /// The description in the current locale, empty when the row has none.
    pub(super) fn description(&self) -> String {
        if self.description.is_empty() {
            return String::new();
        }
        i18n::message(self.description, &[])
    }

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
        feature.message(),
        feature.description_message(),
    )
}

pub(super) const CATALOG: &[Entry] = &[
    entry(
        Section::Theme,
        "theme-mode",
        "settings-theme-mode",
        "settings-theme-mode-description",
    ),
    entry(
        Section::Theme,
        "theme-side",
        "settings-theme-side",
        "settings-theme-side-description",
    )
    .only(Availability::FollowingSystemTheme),
    entry(
        Section::Theme,
        "theme-follow",
        "settings-theme-follow",
        "settings-theme-follow-description",
    ),
    entry(
        Section::Theme,
        "theme-contrast",
        "settings-theme-contrast",
        "settings-theme-contrast-description",
    ),
    entry(
        Section::Theme,
        "theme-source",
        "settings-theme-source",
        "settings-theme-source-description",
    ),
    entry(
        Section::Theme,
        "theme-backdrop",
        "settings-theme-backdrop",
        "settings-theme-backdrop-description",
    ),
    entry(
        Section::Theme,
        "theme-padding-left",
        "settings-theme-padding-left",
        "settings-theme-padding-left-description",
    ),
    entry(
        Section::Theme,
        "theme-padding-right",
        "settings-theme-padding-right",
        "settings-theme-padding-right-description",
    ),
    entry(
        Section::Theme,
        "theme-padding-top",
        "settings-theme-padding-top",
        "settings-theme-padding-top-description",
    ),
    entry(
        Section::Theme,
        "theme-padding-bottom",
        "settings-theme-padding-bottom",
        "settings-theme-padding-bottom-description",
    ),
    entry(
        Section::Sidebar,
        "sidebar-layout",
        "settings-sidebar-layout",
        "settings-sidebar-layout-description",
    ),
    entry(
        Section::Sidebar,
        "sidebar-select",
        "settings-sidebar-select",
        "settings-sidebar-select-description",
    ),
    entry(
        Section::Sidebar,
        "sidebar-device-footer",
        "settings-sidebar-device-footer",
        "settings-sidebar-device-footer-description",
    ),
    entry(
        Section::Sidebar,
        "sidebar-change-counts",
        "settings-sidebar-change-counts",
        "settings-sidebar-change-counts-description",
    ),
    entry(
        Section::Sidebar,
        "sidebar-show-agents",
        "settings-sidebar-show-agents",
        "settings-sidebar-show-agents-description",
    ),
    entry(
        Section::Sidebar,
        "sidebar-agents-placement",
        "settings-sidebar-agents-placement",
        "settings-sidebar-agents-placement-description",
    ),
    entry(
        Section::Sidebar,
        "sidebar-show-agents-header",
        "settings-sidebar-show-agents-header",
        "settings-sidebar-show-agents-header-description",
    ),
    entry(
        Section::Sidebar,
        "sidebar-show-search",
        "settings-sidebar-show-search",
        "settings-sidebar-show-search-description",
    ),
    entry(
        Section::Sidebar,
        "sidebar-repository-icons",
        "settings-sidebar-repository-icons",
        "settings-sidebar-repository-icons-description",
    ),
    entry(
        Section::Sidebar,
        "sidebar-device-icons",
        "settings-sidebar-device-icons",
        "settings-sidebar-device-icons-description",
    ),
    entry(
        Section::Sidebar,
        "sidebar-gpu-load",
        "settings-sidebar-gpu-load",
        "settings-sidebar-gpu-load-description",
    ),
    entry(
        Section::Sidebar,
        "sidebar-indent",
        "settings-sidebar-indent",
        "settings-sidebar-indent-description",
    ),
    entry(
        Section::Sidebar,
        "sidebar-row-padding",
        "settings-sidebar-row-padding",
        "settings-sidebar-row-padding-description",
    ),
    entry(
        Section::Sidebar,
        "sidebar-row-gap",
        "settings-sidebar-row-gap",
        "settings-sidebar-row-gap-description",
    ),
    entry(
        Section::Sidebar,
        "sidebar-host-gap",
        "settings-sidebar-host-gap",
        "settings-sidebar-host-gap-description",
    ),
    entry(
        Section::Sidebar,
        "sidebar-host-colours",
        "settings-sidebar-host-colours",
        "settings-sidebar-host-colours-description",
    ),
    entry(
        Section::Indicators,
        "indicators-edge-rail",
        "settings-indicators-edge-rail",
        "settings-indicators-edge-rail-description",
    ),
    entry(
        Section::Indicators,
        "indicators-sets",
        "settings-indicators-sets",
        "settings-indicators-sets-description",
    ),
    entry(
        Section::Fonts,
        "fonts-terminal",
        "settings-fonts-terminal",
        "",
    ),
    entry(
        Section::Fonts,
        "fonts-sidebar",
        "settings-fonts-sidebar",
        "",
    ),
    entry(Section::Fonts, "fonts-tabs", "settings-fonts-tabs", ""),
    entry(Section::Fonts, "fonts-ui", "settings-fonts-ui", ""),
    entry(
        Section::Fonts,
        "fonts-all-row",
        "settings-fonts-all-row",
        "",
    ),
    entry(
        Section::StatusBar,
        "status-usage",
        "settings-status-usage",
        "settings-status-usage-description",
    ),
    entry(
        Section::StatusBar,
        "status-system-load",
        "settings-status-system-load",
        "settings-status-system-load-description",
    ),
    entry(
        Section::StatusBar,
        "status-gpu",
        "settings-status-gpu",
        "settings-status-gpu-description",
    ),
    entry(
        Section::StatusBar,
        "status-network-quality",
        "settings-status-network-quality",
        "settings-status-network-quality-description",
    ),
    entry(
        Section::StatusBar,
        "status-keep-awake",
        "settings-status-keep-awake",
        "settings-status-keep-awake-description",
    ),
    entry(
        Section::StatusBar,
        "status-theme",
        "settings-status-theme",
        "",
    ),
    entry(
        Section::StatusBar,
        "status-shortcuts",
        "settings-status-shortcuts",
        "",
    ),
    entry(
        Section::StatusBar,
        "status-report-issue",
        "settings-status-report-issue",
        "",
    ),
    entry(
        Section::General,
        "general-ports",
        "settings-general-ports",
        "settings-general-ports-description",
    ),
    entry(
        Section::General,
        "general-checkpoints",
        "settings-general-checkpoints",
        "settings-general-checkpoints-description",
    ),
    entry(
        Section::General,
        "general-smooth-scroll",
        "settings-general-smooth-scroll",
        "settings-general-smooth-scroll-description",
    ),
    // One per `[features]` flag; a test checks none is missing.
    // keep-sorted start
    feature(Feature::AcpChats),
    feature(Feature::QaMenu),
    feature(Feature::SidebarHoverMenu),
    // keep-sorted end
    entry(
        Section::General,
        "general-copy-on-select",
        "settings-general-copy-on-select",
        "settings-general-copy-on-select-description",
    ),
    entry(
        Section::General,
        "general-copied-toast",
        "settings-general-copied-toast",
        "",
    ),
    entry(
        Section::General,
        "general-clipboard-position",
        "settings-general-clipboard-position",
        "",
    ),
    entry(
        Section::General,
        "general-merged-worktrees",
        "settings-general-merged-worktrees",
        "settings-general-merged-worktrees-description",
    ),
    entry(
        Section::General,
        "general-browser-skill",
        "settings-general-browser-skill",
        "settings-general-browser-skill-description",
    ),
    entry(
        Section::General,
        "general-launch-at-login",
        "settings-general-launch-at-login",
        "settings-general-launch-at-login-description",
    )
    .only(Availability::Windows),
    entry(
        Section::General,
        "general-update-beta",
        "settings-general-update-beta",
        "settings-general-update-beta-description",
    ),
    entry(
        Section::Tabs,
        "tabs-layout",
        "settings-tabs-layout",
        "settings-tabs-layout-description",
    ),
    entry(Section::Tabs, "tabs-position", "settings-tabs-position", ""),
    entry(
        Section::Tabs,
        "tabs-hide-single",
        "settings-tabs-hide-single",
        "",
    ),
    entry(
        Section::Tabs,
        "tabs-confirm-tab",
        "settings-tabs-confirm-tab",
        "",
    ),
    entry(
        Section::Tabs,
        "tabs-confirm-pane",
        "settings-tabs-confirm-pane",
        "",
    ),
    entry(
        Section::Tabs,
        "tabs-confirm-workspace",
        "settings-tabs-confirm-workspace",
        "",
    ),
    entry(
        Section::Tabs,
        "tabs-focus-follows-mouse",
        "settings-tabs-focus-follows-mouse",
        "settings-tabs-focus-follows-mouse-description",
    ),
    entry(
        Section::Tabs,
        "tabs-pane-history",
        "settings-tabs-pane-history",
        "settings-tabs-pane-history-description",
    ),
    entry(
        Section::Code,
        "code-mode",
        "settings-code-mode",
        "settings-code-mode-description",
    ),
    entry(
        Section::Notifications,
        "notif-delivery",
        "settings-notif-delivery",
        "settings-notif-delivery-description",
    ),
    entry(
        Section::Notifications,
        "notif-finished",
        "settings-notif-finished",
        "settings-notif-finished-description",
    ),
    entry(
        Section::Notifications,
        "notif-show",
        "settings-notif-show",
        "",
    ),
    entry(
        Section::Notifications,
        "notif-delay",
        "settings-notif-delay",
        "settings-notif-delay-description",
    ),
    entry(
        Section::Notifications,
        "notif-position",
        "settings-notif-position",
        "",
    ),
    entry(
        Section::Notifications,
        "notif-sound",
        "settings-notif-sound",
        "settings-notif-sound-description",
    ),
    entry(
        Section::Notifications,
        "notif-volume",
        "settings-notif-volume",
        "settings-notif-volume-description",
    ),
    entry(
        Section::Integrations,
        "integrations-search",
        "settings-integrations-search",
        "settings-integrations-search-description",
    ),
    entry(
        Section::Plugins,
        "plugins-values",
        "settings-plugins-values",
        "settings-plugins-values-description",
    ),
    #[cfg(feature = "cloud")]
    entry(
        Section::CloudDevices,
        "cloud-accounts",
        "settings-cloud-accounts",
        "settings-cloud-accounts-description",
    ),
    #[cfg(feature = "cloud")]
    entry(
        Section::CloudDevices,
        "cloud-setup",
        "settings-cloud-setup",
        "settings-cloud-setup-description",
    ),
    entry(
        Section::RemoteControl,
        "remote-enabled",
        "settings-remote-enabled",
        "settings-remote-enabled-description",
    ),
    entry(
        Section::RemoteControl,
        "remote-keep-running",
        "settings-remote-keep-running",
        "settings-remote-keep-running-description",
    ),
    entry(
        Section::RemoteControl,
        "remote-port",
        "settings-remote-port",
        "settings-remote-port-description",
    ),
    entry(
        Section::RemoteControl,
        "remote-bind",
        "settings-remote-bind",
        "settings-remote-bind-description",
    ),
    entry(
        Section::RemoteControl,
        "remote-addresses",
        "settings-remote-addresses",
        "settings-remote-addresses-description",
    ),
    entry(
        Section::RemoteControl,
        "remote-certificate",
        "settings-remote-certificate",
        "settings-remote-certificate-description",
    ),
    entry(
        Section::RemoteControl,
        "remote-notifications",
        "settings-remote-notifications",
        "settings-remote-notifications-description",
    ),
    entry(
        Section::RemoteControl,
        "remote-idle",
        "settings-remote-idle",
        "settings-remote-idle-description",
    ),
    entry(
        Section::RemoteControl,
        "remote-pair-card",
        "settings-remote-pair-card",
        "settings-remote-pair-card-description",
    ),
    entry(
        Section::RemoteControl,
        "remote-devices",
        "settings-remote-devices",
        "settings-remote-devices-description",
    ),
    entry(
        Section::Configuration,
        "config-local",
        "settings-config-local",
        "settings-config-local-description",
    ),
    entry(
        Section::Configuration,
        "config-shared",
        "settings-config-shared",
        "settings-config-shared-description",
    ),
    entry(
        Section::Configuration,
        "config-reload",
        "settings-config-reload",
        "settings-config-reload-description",
    ),
];

/// Indices into [`CATALOG`] of the entries whose title, description or page
/// name contains `query`, ignoring case, in catalog order. Each is matched in
/// the current locale and in English, as the palette matches commands.
pub(super) fn search(query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    CATALOG
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            [entry.title, entry.description, entry.section.message()]
                .iter()
                .filter(|id| !id.is_empty())
                .flat_map(|id| [i18n::message(id, &[]), i18n::english(id, &[])])
                .any(|text| text.to_lowercase().contains(&query))
        })
        .map(|(index, _)| index)
        .collect()
}
