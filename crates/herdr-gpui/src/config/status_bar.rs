//! What the status bar shows and how much room each item takes. Whether
//! usage, CPU/memory and the GPU appear at all stays with `[usage] show`,
//! `show_system_load` and `show_gpu_load`, which also gate their sampling.
use serde::Deserialize;

/// How much of a reading an item spells out.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Detail {
    /// Meters and every figure.
    #[default]
    Detailed,
    /// One number per item.
    Compact,
}

/// What the cup keeps awake.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AwakeScope {
    /// The display and the system, so the screen never locks.
    #[default]
    Display,
    /// Only the system, like `caffeinate -i`; the display may sleep and lock.
    System,
}

impl AwakeScope {
    fn name(self) -> &'static str {
        match self {
            Self::Display => "display",
            Self::System => "system",
        }
    }
}

/// How a status bar button appears.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Button {
    /// Icon and name.
    #[default]
    Label,
    /// Icon alone, named by its tooltip.
    Icon,
    Hidden,
}

impl Button {
    pub(crate) fn shown(self) -> bool {
        self != Self::Hidden
    }

    fn name(self) -> &'static str {
        match self {
            Self::Label => "label",
            Self::Icon => "icon",
            Self::Hidden => "hidden",
        }
    }
}

impl Detail {
    fn name(self) -> &'static str {
        match self {
            Self::Detailed => "detailed",
            Self::Compact => "compact",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct StatusBar {
    /// The bar at all; `toggle_status_bar` hides or shows it for the session.
    pub show: bool,
    pub usage: Detail,
    pub system_load: Detail,
    /// The selected host's GPU and video memory, while `show_gpu_load` is on.
    pub gpu_detail: Detail,
    pub keep_awake: bool,
    pub keep_awake_scope: AwakeScope,
    pub theme: Button,
    pub shortcuts: Button,
    pub report_issue: Button,
}

impl Default for StatusBar {
    fn default() -> Self {
        Self {
            show: true,
            usage: Detail::default(),
            system_load: Detail::default(),
            gpu_detail: Detail::default(),
            keep_awake: true,
            keep_awake_scope: AwakeScope::default(),
            theme: Button::default(),
            shortcuts: Button::default(),
            report_issue: Button::default(),
        }
    }
}

/// One `[status_bar]` key, as the Settings window saves it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Edit {
    Usage(Detail),
    SystemLoad(Detail),
    GpuDetail(Detail),
    KeepAwake(bool),
    KeepAwakeScope(AwakeScope),
    Theme(Button),
    Shortcuts(Button),
    ReportIssue(Button),
}

impl Edit {
    pub(super) fn entry(self) -> (&'static str, toml_edit::Value) {
        match self {
            Self::Usage(detail) => ("usage", detail.name().into()),
            Self::SystemLoad(detail) => ("system_load", detail.name().into()),
            Self::GpuDetail(detail) => ("gpu_detail", detail.name().into()),
            Self::KeepAwake(shown) => ("keep_awake", shown.into()),
            Self::KeepAwakeScope(scope) => ("keep_awake_scope", scope.name().into()),
            Self::Theme(button) => ("theme", button.name().into()),
            Self::Shortcuts(button) => ("shortcuts", button.name().into()),
            Self::ReportIssue(button) => ("report_issue", button.name().into()),
        }
    }
}
