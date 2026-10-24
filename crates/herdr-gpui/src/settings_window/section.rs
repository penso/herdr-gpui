//! The Settings pages and the groups the section list shows them under.

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

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Behaviour => "Behaviour",
            Self::Connect => "Connect",
            Self::Advanced => "Advanced",
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

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Theme => "Theme",
            Self::Sidebar => "Sidebar",
            Self::Indicators => "Indicators",
            Self::Fonts => "Fonts",
            Self::StatusBar => "Status bar",
            Self::General => "General",
            Self::Tabs => "Tabs & panes",
            Self::Code => "Code",
            Self::Notifications => "Notifications",
            Self::Integrations => "Integrations",
            Self::Plugins => "Plugins",
            #[cfg(feature = "cloud")]
            Self::CloudDevices => "Cloud devices",
            Self::RemoteControl => "Remote control",
            Self::Configuration => "Configuration",
        }
    }

    pub(super) fn lead(self) -> Option<&'static str> {
        match self {
            Self::Theme => Some("Hover a theme to try it in the app; click to keep it."),
            Self::Sidebar => Some("Changes apply to every open window immediately."),
            Self::Indicators => Some(
                "How agent status shows in the sidebar, tabs and the collapsed rail. \
                 Working always moves; reduced motion holds it still.",
            ),
            Self::Fonts => Some("Each face can have its own family and size."),
            Self::StatusBar => Some("Which items the bottom bar shows, and how compactly."),
            Self::Code => Some(
                "Review code and diffs beside your terminals through a Visual Studio Code server.",
            ),
            Self::Integrations => Some("Agents Herdr can launch and track."),
            Self::Plugins => Some("Show what your plugins report in the sidebar."),
            #[cfg(feature = "cloud")]
            Self::CloudDevices => {
                Some("Create machines from your cloud accounts and use them as devices.")
            }
            Self::RemoteControl => Some(
                "Use your chats from a phone, over HTTPS on your network or through Tailscale.",
            ),
            Self::Configuration => {
                Some("Files behind these settings. Saved edits reload automatically.")
            }
            Self::General | Self::Tabs | Self::Notifications => None,
        }
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

    pub(super) fn selector(self) -> String {
        format!(
            "settings-section-{}",
            self.label()
                .to_lowercase()
                .replace(' ', "-")
                .replace('&', "and")
        )
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
