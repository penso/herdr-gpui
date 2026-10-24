//! The `[sidebar]` table beyond its font: spacing that overrides the layout
//! preset, a colour per host, how a selection marks its host's group, when
//! the device footer is drawn, whose change counts a row shows, and whether
//! the activity bar leads it.

use super::Error;
use super::fonts::FontSettings;
use crate::Result;
use serde::Deserialize;
use std::collections::BTreeMap;

/// Spacing the config file sets over the active layout's numbers. Unset keys
/// keep the preset's value, so a layout still looks like itself. The card
/// layouts, superset and orca, copy another app's rows and read only the keys
/// that fit them: `row_padding` has no effect there, and orca keeps its own
/// gap too.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SidebarOverrides {
    /// Pixels each nesting level steps in: workspaces under their host,
    /// worktrees under their repository.
    pub indent: Option<f32>,
    /// Vertical padding around a row's text.
    pub row_padding: Option<f32>,
    /// Space between a row's status dot and its label.
    pub gap: Option<f32>,
    /// Space between a host header's parts.
    pub host_gap: Option<f32>,
}

impl SidebarOverrides {
    /// Every key with the most it may be set to; the least is always 0.
    pub(crate) const BANDS: [(&'static str, f32); 4] = [
        ("indent", 48.),
        ("row_padding", 16.),
        ("gap", 32.),
        ("host_gap", 48.),
    ];

    fn validate(&self) -> Result<()> {
        for ((key, max), value) in
            Self::BANDS
                .into_iter()
                .zip([self.indent, self.row_padding, self.gap, self.host_gap])
        {
            if let Some(value) = value
                && (!value.is_finite() || !(0.0..=max).contains(&value))
            {
                return Err(Error::InvalidSidebarMetric { key, max }.into());
            }
        }
        Ok(())
    }
}

/// What selecting a workspace marks beyond its own row.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
pub enum SelectMode {
    /// Only the row, as before.
    #[default]
    #[serde(rename = "row")]
    Row,
    /// The row, and a stronger wash over its whole host group.
    #[serde(rename = "group")]
    Group,
    /// As `group`, with every other host's rows dimmed.
    #[serde(rename = "group-dim")]
    GroupDim,
}

/// When the sidebar draws its device footer: the device picker, the
/// connection mark, and the Sessions and Settings buttons.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DeviceFooter {
    #[default]
    Always,
    /// Only while the picker has a choice to offer: two or more devices, or
    /// a cloud machine still being added, whose count the footer carries.
    Auto,
    Never,
}

impl DeviceFooter {
    /// The value written to the config file; matches the serde name.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::Auto => "auto",
            Self::Never => "never",
        }
    }

    pub(crate) fn shown(self, devices: usize, adding: usize) -> bool {
        match self {
            Self::Always => true,
            Self::Auto => devices > 1 || adding > 0,
            Self::Never => false,
        }
    }
}

/// Whose `+added/-deleted` line counts a worktree row shows. Only one set,
/// so the two are never mistaken for each other.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
pub enum ChangeCounts {
    /// The cached pull request's, as GitHub reports them.
    #[default]
    #[serde(rename = "pr")]
    PullRequest,
    /// The checkout's uncommitted changes against HEAD. Listed checkouts are
    /// counted on the dirty probe's slow, rate-limited schedule.
    #[serde(rename = "local")]
    Local,
}

impl ChangeCounts {
    /// The value written to the config file; matches the serde name.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::PullRequest => "pr",
            Self::Local => "local",
        }
    }
}

/// Sidebar styling the config file adds to a layout preset.
#[derive(Clone, Debug, PartialEq)]
pub struct SidebarStyle {
    /// `activity_bar`: the icon column that picks what the window shows:
    /// the sidebar and terminals, or an orchestrator section. On unless
    /// turned off.
    pub activity_bar: bool,
    /// `automations`: the bar's Automations section, and Herdr UI reading
    /// and starting each host's `herdr-automate`. On unless turned off.
    pub automations: bool,
    pub change_counts: ChangeCounts,
    pub device_footer: DeviceFooter,
    /// A colour per host, keyed by the host's display name, as `0xRRGGBB`.
    pub hosts: BTreeMap<String, u32>,
    pub overrides: SidebarOverrides,
    pub select: SelectMode,
}

impl Default for SidebarStyle {
    fn default() -> Self {
        Self {
            activity_bar: true,
            automations: true,
            change_counts: ChangeCounts::default(),
            device_footer: DeviceFooter::default(),
            hosts: BTreeMap::new(),
            overrides: SidebarOverrides::default(),
            select: SelectMode::default(),
        }
    }
}

/// `#rgb` or `#rrggbb`, in either case.
pub(crate) fn parse_hex(value: &str) -> Option<u32> {
    let digits = value.strip_prefix('#')?;
    let expanded: String = match digits.len() {
        3 => digits.chars().flat_map(|c| [c, c]).collect(),
        6 => digits.to_owned(),
        _ => return None,
    };
    if !expanded.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(&expanded, 16).ok()
}

/// The raw `[sidebar]` table: font keys beside the style keys. Deserialized
/// by hand so a key this build does not know is still handed to the ignored
/// key reporter, which a flattened struct would swallow.
#[derive(Default)]
pub(super) struct SidebarSettings {
    pub(super) font: FontSettings,
    pub(super) overrides: SidebarOverrides,
    pub(super) select: SelectMode,
    pub(super) device_footer: DeviceFooter,
    pub(super) change_counts: ChangeCounts,
    pub(super) hosts: BTreeMap<String, String>,
    pub(super) activity_bar: Option<bool>,
    pub(super) automations: Option<bool>,
}

impl SidebarSettings {
    /// The validated style, with host colours parsed.
    pub(super) fn style(&self) -> Result<SidebarStyle> {
        self.overrides.validate()?;
        let hosts = self
            .hosts
            .iter()
            .map(|(host, value)| {
                parse_hex(value)
                    .map(|color| (host.clone(), color))
                    .ok_or_else(|| Error::InvalidHostColor {
                        host: host.clone(),
                        value: value.clone(),
                    })
            })
            .collect::<Result<_, Error>>()?;
        Ok(SidebarStyle {
            activity_bar: self.activity_bar.unwrap_or(true),
            automations: self.automations.unwrap_or(true),
            change_counts: self.change_counts,
            device_footer: self.device_footer,
            hosts,
            overrides: self.overrides,
            select: self.select,
        })
    }
}

impl<'de> Deserialize<'de> for SidebarSettings {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = SidebarSettings;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a [sidebar] table")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<SidebarSettings, A::Error> {
                let mut font = toml::Table::new();
                let mut settings = SidebarSettings::default();
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "family" | "size" | "fallback" | "line_height" => {
                            font.insert(key, map.next_value()?);
                        }
                        "indent" => settings.overrides.indent = Some(map.next_value()?),
                        "row_padding" => settings.overrides.row_padding = Some(map.next_value()?),
                        "gap" => settings.overrides.gap = Some(map.next_value()?),
                        "host_gap" => settings.overrides.host_gap = Some(map.next_value()?),
                        "select" => settings.select = map.next_value()?,
                        "device_footer" => settings.device_footer = map.next_value()?,
                        "change_counts" => settings.change_counts = map.next_value()?,
                        "activity_bar" => settings.activity_bar = Some(map.next_value()?),
                        "automations" => settings.automations = Some(map.next_value()?),
                        "hosts" => settings.hosts = map.next_value()?,
                        _ => {
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                settings.font = FontSettings::deserialize(toml::Value::Table(font))
                    .map_err(serde::de::Error::custom)?;
                Ok(settings)
            }
        }

        deserializer.deserialize_map(Visitor)
    }
}
