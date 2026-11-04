//! The app, its windows, and their chrome.
use serde::{Deserialize, Serialize};

/// What the window shows, picked on the bar. The bar lists them in
/// declaration order, so this order is what users see.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActivityPanel {
    #[default]
    Workspaces,
    Overview,
    PullRequests,
    Issues,
    Security,
    Automations,
    Usage,
}

impl ActivityPanel {
    pub const ALL: &[Self] = &[
        Self::Workspaces,
        Self::Overview,
        Self::PullRequests,
        Self::Issues,
        Self::Security,
        Self::Automations,
        Self::Usage,
    ];

    /// The name the CLI and the preferences file use.
    pub fn name(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Workspaces => "workspaces",
            Self::PullRequests => "pull-requests",
            Self::Issues => "issues",
            Self::Security => "security",
            Self::Automations => "automations",
            Self::Usage => "usage",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|panel| panel.name() == name)
    }
}

/// `on`, `off`, or `toggle`, as several settings accept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Switch {
    On,
    Off,
    #[default]
    Toggle,
}

impl Switch {
    /// The state after applying this to `current`.
    pub fn apply(self, current: bool) -> bool {
        match self {
            Self::On => true,
            Self::Off => false,
            Self::Toggle => !current,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum AppOp {
    /// Bring the app and its window forward.
    Focus,
    Hide,
    /// Quit the app; Herdr and its terminals keep running.
    Quit,
    /// Bring the window forward and open or focus the workspace of a
    /// folder, as the palette's Open Project does. `path` is absolute and
    /// canonical, resolved by the caller, and checked again by the app.
    Open {
        path: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum WindowOp {
    List,
    /// Another window of the first window's launch target, or of a named
    /// local session.
    New {
        #[serde(default)]
        session: Option<String>,
    },
    Focus,
    Fullscreen {
        switch: Switch,
    },
    /// Reports the window's size, or resizes its content when given one.
    /// GPUI cannot move a window, so there is no position to set.
    Size {
        #[serde(default)]
        size: Option<Size>,
    },
    /// Detach from the selected host; it keeps running.
    Detach,
    Reconnect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case", deny_unknown_fields)]
pub enum FontSize {
    Larger,
    Smaller,
    /// Back to the size the config file sets.
    Reset,
    Points {
        points: u16,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum UiOp {
    Sidebar {
        switch: Switch,
    },
    StatusBar {
        switch: Switch,
    },
    FontSize {
        size: FontSize,
    },
    /// Shows an activity bar section.
    Panel {
        panel: ActivityPanel,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum CommandOp {
    /// A command by its `[keybindings]` name, as the palette runs it.
    Run { name: String },
}

/// What `status` reports about the running app.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub version: String,
    pub protocol: u32,
    pub pid: u32,
    /// Whether pages show in tabs; otherwise they open in the system browser.
    pub browser_tabs: bool,
    pub windows: Vec<WindowInfo>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: u64,
    pub title: String,
    pub active: bool,
    pub fullscreen: bool,
    pub width: f32,
    pub height: f32,
    /// The workspace the window shows on its selected host.
    pub workspace: Option<String>,
    pub hosts: Vec<HostInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostInfo {
    pub id: String,
    pub label: String,
    /// "online", "connecting", "reconnecting", "detached", "disabled", ...
    pub status: String,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reply {
    Windows { windows: Vec<WindowInfo> },
    Size { width: f32, height: f32 },
    FontSize { points: f32 },
}

#[cfg(test)]
mod tests;
