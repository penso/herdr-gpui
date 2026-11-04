//! The Settings pages and the groups the section list shows them under.
// i18n: migrated
use crate::i18n;

/// One Settings page. `ALL` is the section list's order: General alone at the
/// top, then group by group.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Section {
    General,
    #[default]
    Theme,
    Sidebar,
    Indicators,
    Fonts,
    StatusBar,
    Tabs,
    Code,
    Notifications,
    Integrations,
    Plugins,
    #[cfg(feature = "cloud")]
    CloudDevices,
    RemoteControl,
    Configuration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Group {
    Appearance,
    Behaviour,
    Connect,
    Advanced,
}

impl Group {
    pub(super) const ALL: [Self; 4] = [
        Self::Appearance,
        Self::Behaviour,
        Self::Connect,
        Self::Advanced,
    ];

    pub(super) fn label(self) -> String {
        i18n::message(self.message(), &[])
    }

    fn message(self) -> &'static str {
        match self {
            Self::Appearance => "settings-group-appearance",
            Self::Behaviour => "settings-group-behaviour",
            Self::Connect => "settings-group-connect",
            Self::Advanced => "settings-group-advanced",
        }
    }

    /// The group label's selector, the same in every language.
    pub(super) fn slug(self) -> &'static str {
        match self {
            Self::Appearance => "appearance",
            Self::Behaviour => "behaviour",
            Self::Connect => "connect",
            Self::Advanced => "advanced",
        }
    }
}

impl Section {
    /// The section list's order: General first, outside every group, as the
    /// catch-all people look in when they do not know where a setting lives;
    /// then each group's pages together, groups in [`Group::ALL`] order. Users
    /// see this order, so it is not sorted.
    pub(super) const ALL: &[Self] = &[
        Self::General,
        Self::Theme,
        Self::Sidebar,
        Self::Indicators,
        Self::Fonts,
        Self::StatusBar,
        Self::Tabs,
        Self::Code,
        Self::Notifications,
        Self::Integrations,
        Self::Plugins,
        #[cfg(feature = "cloud")]
        Self::CloudDevices,
        Self::RemoteControl,
        Self::Configuration,
    ];

    pub(super) fn label(self) -> String {
        i18n::message(self.message(), &[])
    }

    /// The message naming the page, which search also matches.
    pub(super) fn message(self) -> &'static str {
        match self {
            Self::Theme => "settings-section-theme",
            Self::Sidebar => "settings-section-sidebar",
            Self::Indicators => "settings-section-indicators",
            Self::Fonts => "settings-section-fonts",
            Self::StatusBar => "settings-section-status-bar",
            Self::General => "settings-section-general",
            Self::Tabs => "settings-section-tabs-and-panes",
            Self::Code => "settings-section-code",
            Self::Notifications => "settings-section-notifications",
            Self::Integrations => "settings-section-integrations",
            Self::Plugins => "settings-section-plugins",
            #[cfg(feature = "cloud")]
            Self::CloudDevices => "settings-section-cloud-devices",
            Self::RemoteControl => "settings-section-remote-control",
            Self::Configuration => "settings-section-configuration",
        }
    }

    pub(super) fn lead(self) -> Option<String> {
        let id = match self {
            Self::Theme => "settings-lead-theme",
            Self::Sidebar => "settings-lead-sidebar",
            Self::Indicators => "settings-lead-indicators",
            Self::Fonts => "settings-lead-fonts",
            Self::StatusBar => "settings-lead-status-bar",
            Self::Code => "settings-lead-code",
            Self::Integrations => "settings-lead-integrations",
            Self::Plugins => "settings-lead-plugins",
            #[cfg(feature = "cloud")]
            Self::CloudDevices => "settings-lead-cloud-devices",
            Self::RemoteControl => "settings-lead-remote-control",
            Self::Configuration => "settings-lead-configuration",
            Self::General | Self::Tabs | Self::Notifications => return None,
        };
        Some(i18n::message(id, &[]))
    }

    /// The group the section list shows this page under; `None` for pages
    /// listed above every group label.
    pub(super) fn group(self) -> Option<Group> {
        match self {
            Self::General => None,
            Self::Theme | Self::Sidebar | Self::Indicators | Self::Fonts | Self::StatusBar => {
                Some(Group::Appearance)
            }
            Self::Tabs | Self::Code | Self::Notifications => Some(Group::Behaviour),
            Self::Integrations | Self::Plugins => Some(Group::Connect),
            #[cfg(feature = "cloud")]
            Self::CloudDevices => Some(Group::Connect),
            Self::RemoteControl => Some(Group::Connect),
            Self::Configuration => Some(Group::Advanced),
        }
    }

    pub(super) fn icon(self) -> &'static str {
        match self {
            Self::Theme => "icons/theme.svg",
            Self::Sidebar => "icons/panel-left.svg",
            Self::Indicators => "icons/pulse.svg",
            Self::Fonts => "icons/pencil.svg",
            Self::StatusBar => "icons/status-bar.svg",
            Self::General => "icons/settings.svg",
            Self::Tabs => "icons/tabs.svg",
            Self::Code => "icons/vscode.svg",
            Self::Notifications => "icons/bell.svg",
            Self::Integrations => "icons/agent-generic.svg",
            Self::Plugins => "icons/plug.svg",
            #[cfg(feature = "cloud")]
            Self::CloudDevices => "icons/globe.svg",
            Self::RemoteControl => "icons/phone.svg",
            Self::Configuration => "icons/file.svg",
        }
    }

    /// The page's selector, the same in every language: its message ID.
    pub(super) fn selector(self) -> String {
        self.message().to_owned()
    }

    /// The section's index among the scrolled section list's children, which
    /// are the ungrouped pages, then each group's label followed by its pages.
    pub(super) fn nav_item(self) -> usize {
        let mut item = 0;
        for group in std::iter::once(None).chain(Group::ALL.map(Some)) {
            if group.is_some() {
                item += 1;
            }
            for section in Self::ALL.iter().filter(|section| section.group() == group) {
                if *section == self {
                    return item;
                }
                item += 1;
            }
        }
        item
    }
}
