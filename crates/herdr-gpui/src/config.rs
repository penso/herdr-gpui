//! GUI settings. The daemon's own config is read only where the GUI honors a
//! preference the user already expressed there, never written and never used
//! to change daemon behavior. Managed defaults are refreshed from the binary;
//! `config-gpui.local.toml` holds persistent user overrides.
use crate::{
    Error, Result,
    contrast::Contrast,
    error::ThemeParseError,
    keymap::{Binding, DaemonKeys, Keymap, PaneKeys},
};
pub(crate) mod preferences;
pub(crate) mod sidebar;
pub(crate) mod watch;

use gpui::{Font, FontFallbacks};
use serde::Deserialize;
pub(crate) use sidebar::{
    AgentLayout, AgentToken, Rows, SidebarLayout, SpaceLayout, SpaceToken, TokenStyle,
};
use std::{
    collections::BTreeMap,
    env, fs,
    io::{ErrorKind, Write},
    ops::RangeInclusive,
    path::{Component, Path, PathBuf},
};

const DEFAULT_CONFIG: &str = include_str!("../config-gpui.example.toml");
const FOLLOW_HERDR: &str = "Follow Herdr";
// Compare the first line so Windows checkouts and editors can use CRLF.
const MANAGED_HEADER: &str = "# DO NOT EDIT -- WILL BE OVERWRITTEN";
/// Seeds the overrides file on first launch only. Existing overrides and
/// migrated personal configs are never rewritten, so settings placed here
/// reach new installs without changing what current users see.
const LOCAL_CONFIG: &str = "# Herdr GPUI overrides. Saved changes reload automatically.\n# Unset keys inherit config-gpui.toml; tables merge key by key.\n\n# New installs start with the roomy rounded sidebar. Remove this line for\n# the managed default, or pick another layout listed in config-gpui.toml.\nlayout = \"comfortable-rounded\"\n";

/// Every face is held to this range, whether it comes from the config file or
/// from a runtime adjustment, so the two can never disagree on what is valid.
pub const FONT_SIZE_RANGE: RangeInclusive<f32> = 8.0..=48.0;

/// One logical pixel: the smallest step that can move the terminal cell grid.
pub const FONT_SIZE_STEP: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FontFace {
    Sidebar,
    Tabs,
    Terminal,
    Ui,
}

impl FontFace {
    pub(crate) fn set_size(self, config: &mut Config, size: f32) {
        match self {
            Self::Sidebar => config.sidebar.size = size,
            Self::Tabs => config.tabs.size = size,
            Self::Terminal => config.terminal.size = size,
            Self::Ui => config.ui.size = size,
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Sidebar => "sidebar",
            Self::Tabs => "tabs",
            Self::Terminal => "terminal",
            Self::Ui => "ui",
        }
    }

    pub(crate) fn size(self, config: &Config) -> f32 {
        match self {
            Self::Sidebar => config.sidebar.size,
            Self::Tabs => config.tabs.size,
            Self::Terminal => config.terminal.size,
            Self::Ui => config.ui.size,
        }
    }
}

/// Shared logical-pixel radii for native-style chrome, independent of the
/// terminal grid. Small badges/keycaps retain a tighter curve than controls.
pub(crate) mod corners {
    pub(crate) const PANEL: f32 = 12.;
    pub(crate) const CONTROL: f32 = 8.;
    pub(crate) const SMALL: f32 = 4.;
}

#[derive(Clone, Debug)]
pub struct Config {
    pub theme: String,
    pub confirm_close_tab: bool,
    pub show_agents: bool,
    /// CPU and memory of the selected host in the status bar.
    pub show_system_load: bool,
    /// How far the app's own marks and labels stand off its chrome.
    pub contrast: Contrast,
    pub usage: crate::usage::UsageConfig,
    pub option_as_alt: OptionAsAlt,
    pub open_links_in: LinkTarget,
    /// Whether a terminal selection stays highlighted, and readable by
    /// selection tools, after it is copied.
    pub keep_selection_after_copy: bool,
    pub sidebar: FontConfig,
    pub tabs: FontConfig,
    pub terminal: FontConfig,
    pub ui: FontConfig,
    pub github: GitHubConfig,
    pub features: Features,
    pub notifications: NotificationConfig,
    pub(crate) notification_overrides: NotificationSettings,
    pub clipboard_toast: ClipboardToast,
    pub bell: BellConfig,
    pub layout: Layout,
    /// Daemon sidebar rows, falling back to defaults when invalid.
    pub sidebar_layout: SidebarLayout,
    pub keybindings: Keymap,
    /// The `[keybindings]` table `keybindings` was built from, kept so a
    /// device's server keys can be layered under the same GUI overrides.
    pub(crate) keybinding_overrides: BTreeMap<String, Binding>,
    /// The `[pane_keys]` table `keybindings` was built from, for the same
    /// reason.
    pub(crate) pane_keys: PaneKeys,
    /// Per saved device, by catalog profile ID.
    pub(crate) devices: BTreeMap<String, DeviceSettings>,
    pub palette: crate::palette::PaletteConfig,
    /// Keys the file names that this build does not know, sorted. They are
    /// ignored, as Herdr ignores its own, so a config written by a newer
    /// build or with a typo still loads; `diagnostic` reports them.
    pub unknown_keys: Vec<String>,
}

/// A device list larger than any real catalog is a config mistake.
const MAX_DEVICES: usize = 256;

/// Whose `[keys]` a saved device answers to, as `herdr --remote-keybindings`
/// chooses for the TUI. Local is upstream's default: muscle memory stays the
/// same on every host. Only keybindings follow the server; themes, sidebar,
/// and toasts stay local either way.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum KeybindingSource {
    #[default]
    Local,
    /// The host's published `server_keybindings_toml`.
    Server,
}

/// One `[devices.<profile-id>]` table.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub(crate) struct DeviceSettings {
    pub(crate) keybindings: KeybindingSource,
}

impl Config {
    /// Whose keybindings the endpoint uses. Local and explicit sockets are not
    /// saved devices, so they always use the local ones.
    pub(crate) fn keybinding_source(&self, endpoint_id: &str) -> KeybindingSource {
        crate::endpoint::saved_profile_id(endpoint_id)
            .and_then(|profile| self.devices.get(profile))
            .map(|device| device.keybindings)
            .unwrap_or_default()
    }
}

/// Where a clicked terminal link opens. Alt-click (Option on macOS) opens it
/// in the other one.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum LinkTarget {
    #[default]
    System,
    /// A browser tab in the workspace, where the build can show pages.
    BrowserTab,
}

/// Whether macOS Option sends Alt shortcuts to a pane or types the character
/// the keyboard layout puts on it. Other platforms have no Option layer, so
/// Alt always reaches the pane there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OptionAsAlt {
    /// Alt on the U.S. and ABC layouts, whose Option layer only holds symbols
    /// like `π`; typing elsewhere, where it holds `@`, `[`, or letters.
    #[default]
    Auto,
    Always,
    Never,
}

impl OptionAsAlt {
    /// macOS layouts whose Option characters a terminal user rarely types.
    const ALT_LAYOUTS: [&'static str; 2] = ["com.apple.keylayout.US", "com.apple.keylayout.ABC"];

    /// Whether Option-modified keys go to the pane as Alt under `layout`, the
    /// platform keyboard layout ID.
    pub fn sends_alt(self, layout: &str) -> bool {
        if !cfg!(target_os = "macos") {
            return true;
        }
        match self {
            Self::Auto => Self::ALT_LAYOUTS.contains(&layout),
            Self::Always => true,
            Self::Never => false,
        }
    }
}

impl<'de> Deserialize<'de> for OptionAsAlt {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Value {
            Bool(bool),
            Name(String),
        }
        match Value::deserialize(deserializer)? {
            Value::Bool(true) => Ok(Self::Always),
            Value::Bool(false) => Ok(Self::Never),
            Value::Name(name) if name == "auto" => Ok(Self::Auto),
            Value::Name(name) => Err(serde::de::Error::unknown_variant(&name, &["auto"])),
        }
    }
}

/// Where the "copied to clipboard" flash sits, and whether it appears at all.
/// Resolved from the daemon's `[ui.toast.clipboard]`, then from this GUI's own
/// `[clipboard_toast]`, so one terminal preference covers both clients.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipboardToast {
    pub enabled: bool,
    pub position: ClipboardToastPosition,
}

impl Default for ClipboardToast {
    fn default() -> Self {
        // herdr's own defaults, so an unconfigured pair of clients agrees.
        Self {
            enabled: true,
            position: ClipboardToastPosition::BottomCenter,
        }
    }
}

/// From herdr src/config/model.rs.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ClipboardToastPosition {
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    #[default]
    BottomCenter,
    BottomRight,
}

/// What a pane's terminal bell does. Herdr forwards each bell to its
/// foreground client and leaves the reaction to it, as an outer terminal's
/// own bell settings would, so this is the GUI's `[bell]` alone.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct BellConfig {
    /// Ask for attention (bounce the Dock icon) while the window is inactive.
    pub attention: bool,
    /// Play the system alert sound.
    pub sound: bool,
}

impl Default for BellConfig {
    fn default() -> Self {
        Self {
            attention: true,
            sound: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct NotificationConfig {
    /// In-app toasts, which take precedence over OS notifications.
    pub enabled: bool,
    /// Shared `system` delivery: post daemon notifications to the OS
    /// notification center. Not a native key, so a local override of
    /// `enabled` decides between the two.
    #[serde(skip)]
    pub system: bool,
    #[serde(deserialize_with = "notification_delay")]
    pub delay_seconds: u64,
    pub position: herdr_client::protocol::ToastHerdrPosition,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            system: false,
            delay_seconds: 1,
            position: herdr_client::protocol::ToastHerdrPosition::BottomRight,
        }
    }
}

/// Only explicitly configured GUI keys override the shared Herdr preferences.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct NotificationSettings {
    enabled: Option<bool>,
    #[serde(deserialize_with = "optional_notification_delay")]
    delay_seconds: Option<u64>,
    position: Option<herdr_client::protocol::ToastHerdrPosition>,
}

/// Where a daemon notification that passes the shared policy is presented.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NotificationDelivery {
    Off,
    InApp,
    System,
}

impl NotificationConfig {
    pub(crate) fn delivery(self) -> NotificationDelivery {
        if self.enabled {
            NotificationDelivery::InApp
        } else if self.system {
            NotificationDelivery::System
        } else {
            NotificationDelivery::Off
        }
    }
}

impl NotificationSettings {
    fn resolve(self, base: NotificationConfig) -> NotificationConfig {
        NotificationConfig {
            enabled: self.enabled.unwrap_or(base.enabled),
            system: base.system,
            delay_seconds: self.delay_seconds.unwrap_or(base.delay_seconds),
            position: self.position.unwrap_or(base.position),
        }
    }
}

fn optional_notification_delay<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<u64>, D::Error> {
    notification_delay(d).map(Some)
}

fn notification_delay<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<u64, D::Error> {
    let seconds = u64::deserialize(d)?;
    if seconds > 3600 {
        return Err(serde::de::Error::custom(
            "notifications.delay_seconds must be between 0 and 3600",
        ));
    }
    Ok(seconds)
}

/// Sidebar layout and spacing the config file can adjust.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub mode: LayoutMode,
    /// Blank space between the sidebar and the terminal it borders. Applies
    /// only while the sidebar is on screen, and narrows the terminal, so the
    /// daemon is told about the columns it actually has.
    pub sidebar_gap: f32,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            mode: LayoutMode::default(),
            sidebar_gap: DEFAULT_SIDEBAR_GAP,
        }
    }
}

/// How much the sidebar fits: spacing, indents, and which details show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Density {
    #[default]
    Normal,
    Compact,
    Comfortable,
}

/// How sidebar rows are drawn, independent of how dense they are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    /// Edge-to-edge rows with square highlights and tree lines.
    #[default]
    Flat,
    /// Inset rows with rounded, bordered highlights.
    Rounded,
}

/// A named sidebar layout. Each one draws its rows differently: Herdr's own
/// rows at a density, flat or rounded, or a design of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutMode {
    /// Herdr's rows: `normal`, `compact`, `comfortable`, or any of them with a
    /// `-rounded` suffix.
    Classic { density: Density, style: Style },
    /// Single-line rows with an icon slot and pull request counts.
    Superset,
    /// Rounded cards with a meta line for host, branch, and pull request.
    Orca,
    /// One line per row with only the status and the name.
    Minimal,
}

impl Default for LayoutMode {
    fn default() -> Self {
        Self::new(Density::Normal, Style::Flat)
    }
}

impl LayoutMode {
    /// `ALL`'s names, for errors that list what a config may say.
    const NAMES: &'static [&'static str] = &[
        "normal",
        "compact",
        "comfortable",
        "normal-rounded",
        "compact-rounded",
        "comfortable-rounded",
        "superset",
        "orca",
        "minimal",
    ];

    /// Every named layout, in the order menus list them.
    pub const ALL: [Self; 9] = [
        Self::new(Density::Normal, Style::Flat),
        Self::new(Density::Compact, Style::Flat),
        Self::new(Density::Comfortable, Style::Flat),
        Self::new(Density::Normal, Style::Rounded),
        Self::new(Density::Compact, Style::Rounded),
        Self::new(Density::Comfortable, Style::Rounded),
        Self::Superset,
        Self::Orca,
        Self::Minimal,
    ];

    pub const fn new(density: Density, style: Style) -> Self {
        Self::Classic { density, style }
    }

    /// The spacing the list around the rows uses. Layouts with their own
    /// design fix theirs, so no second setting half-changes them.
    pub const fn density(self) -> Density {
        match self {
            Self::Classic { density, .. } => density,
            Self::Superset | Self::Minimal => Density::Normal,
            Self::Orca => Density::Comfortable,
        }
    }

    /// The highlight shape and heading case the list uses.
    pub const fn style(self) -> Style {
        match self {
            Self::Classic { style, .. } => style,
            Self::Superset | Self::Minimal => Style::Flat,
            Self::Orca => Style::Rounded,
        }
    }

    /// The config value that selects it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Classic { density, style } => match (density, style) {
                (Density::Normal, Style::Flat) => "normal",
                (Density::Compact, Style::Flat) => "compact",
                (Density::Comfortable, Style::Flat) => "comfortable",
                (Density::Normal, Style::Rounded) => "normal-rounded",
                (Density::Compact, Style::Rounded) => "compact-rounded",
                (Density::Comfortable, Style::Rounded) => "comfortable-rounded",
            },
            Self::Superset => "superset",
            Self::Orca => "orca",
            Self::Minimal => "minimal",
        }
    }

    /// How menus title it.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Classic { density, style } => match (density, style) {
                (Density::Normal, Style::Flat) => "Normal",
                (Density::Compact, Style::Flat) => "Compact",
                (Density::Comfortable, Style::Flat) => "Comfortable",
                (Density::Normal, Style::Rounded) => "Normal Rounded",
                (Density::Compact, Style::Rounded) => "Compact Rounded",
                (Density::Comfortable, Style::Rounded) => "Comfortable Rounded",
            },
            Self::Superset => "Superset",
            Self::Orca => "Orca",
            Self::Minimal => "Minimal",
        }
    }
}

impl From<Density> for LayoutMode {
    fn from(density: Density) -> Self {
        Self::new(density, Style::Flat)
    }
}

impl TryFrom<&str> for LayoutMode {
    type Error = Error;

    fn try_from(name: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|mode| mode.name() == name)
            .ok_or_else(|| Error::UnknownLayout(name.to_owned()))
    }
}

impl std::fmt::Display for LayoutMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

impl<'de> Deserialize<'de> for LayoutMode {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Self::try_from(name.as_str())
            .map_err(|_| serde::de::Error::unknown_variant(&name, Self::NAMES))
    }
}

impl<'de> Deserialize<'de> for Layout {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        // Keep shipped [layout] spacing settings readable alongside named
        // layouts. A visitor rather than an untagged enum, so a key this build
        // does not know is reported as ignored instead of buffered away.
        #[derive(Default, Deserialize)]
        #[serde(default)]
        struct Options {
            mode: LayoutMode,
            sidebar_gap: Option<f32>,
        }

        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Layout;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a layout name or a [layout] table")
            }

            fn visit_str<E: serde::de::Error>(self, name: &str) -> std::result::Result<Layout, E> {
                let mode = LayoutMode::try_from(name)
                    .map_err(|_| E::unknown_variant(name, LayoutMode::NAMES))?;
                Ok(Layout {
                    mode,
                    ..Layout::default()
                })
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> std::result::Result<Layout, A::Error> {
                let Options { mode, sidebar_gap } =
                    Options::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                Ok(Layout {
                    mode,
                    sidebar_gap: sidebar_gap.unwrap_or(DEFAULT_SIDEBAR_GAP),
                })
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

/// Optional behaviors the config file turns on. Every flag is off by default,
/// so a missing or empty `[features]` table is the shipped experience.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Features {
    /// Open a space's menu when the pointer rests on its sidebar row.
    pub sidebar_hover_menu: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct GitHubConfig {
    pub oauth_client_id: Option<String>,
    pub allow_plaintext_credentials: bool,
}

impl GitHubConfig {
    pub fn client_id(&self) -> Result<Option<String>> {
        self.client_id_with_override(env::var_os("HERDR_GITHUB_OAUTH_CLIENT_ID").as_deref())
    }

    fn client_id_with_override(&self, value: Option<&std::ffi::OsStr>) -> Result<Option<String>> {
        let (id, source) = match value {
            Some(value) => (
                Some(value.to_str().ok_or(Error::ClientIdEncoding)?),
                "HERDR_GITHUB_OAUTH_CLIENT_ID",
            ),
            None => (
                Some(
                    self.oauth_client_id
                        .as_deref()
                        .unwrap_or("Iv23liurUcwxPjrdIFYT"),
                ),
                "github.oauth_client_id",
            ),
        };
        if let Some(id) = id
            && (id.is_empty()
                || id.len() > 256
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')))
        {
            return Err(Error::InvalidClientId(source));
        }
        Ok(id.map(str::to_owned))
    }
}

/// Keep the terminal flush with the divider unless spacing is requested.
const DEFAULT_SIDEBAR_GAP: f32 = 0.;

/// A gap wider than this stops reading as spacing and starts eating columns the
/// terminal needs, so the config file is held to a band a window can afford.
const MAX_SIDEBAR_GAP: f32 = 64.;

/// The daemon's config is read for a handful of keys, so a file far larger
/// than any hand-written config is skipped rather than parsed on every load.
const MAX_DAEMON_CONFIG_BYTES: u64 = 1 << 20;

/// Upper bound on a configured cascade. Every entry is searched for each
/// uncovered codepoint, so a long list costs shaping time and covers nothing a
/// short one does not. Names that are not installed are ignored by the platform.
const MAX_FONT_FALLBACKS: usize = 8;

/// Nerd Font patches keep this marker in every patched family name, so matching
/// it finds the installed icon faces without naming individual fonts.
const SYMBOL_FAMILY_MARKER: &str = "nerd font";

/// A cascade is searched in order for every uncovered codepoint, so automatic
/// detection keeps only the best-ranked few families.
const MAX_DETECTED_FALLBACKS: usize = 3;

#[derive(Clone, Debug)]
pub struct FontConfig {
    pub family: String,
    pub size: f32,
    /// Families searched, nearest first, for glyphs `family` lacks. `None`
    /// until the config names them or [`Config::resolve_font_fallbacks`]
    /// detects them; an empty list opts out of any cascade.
    pub fallbacks: Option<Vec<String>>,
}

impl FontConfig {
    pub fn line_height(&self) -> f32 {
        self.size * 20.0 / 14.0
    }

    /// The shaping font for this face. Terminal prompts draw powerline
    /// separators and Nerd Font icons from the Private Use Area, which no text
    /// face and no platform default cascade covers, so those cells shape to the
    /// missing-glyph box unless the cascade names an icon font explicitly.
    pub fn font(&self) -> Font {
        let mut font = gpui::font(self.family.clone());
        font.fallbacks = self
            .fallbacks
            .as_ref()
            .filter(|families| !families.is_empty())
            .map(|families| FontFallbacks::from_fonts(families.clone()));
        font
    }
}

/// Ranks an installed Nerd Font family for the automatic cascade. Symbols-only
/// faces carry the icon ranges without replacing any text glyph, and `Mono`
/// variants keep every icon inside a single terminal cell, so both come first.
fn fallback_rank(family: &str) -> u8 {
    let lowercase = family.to_lowercase();
    let symbols = lowercase.starts_with("symbols nerd font");
    let mono = lowercase.ends_with(" mono");
    match (symbols, mono) {
        (true, true) => 0,
        (true, false) => 1,
        (false, true) => 2,
        (false, false) => 3,
    }
}

/// Picks the installed icon families to search for Private Use Area glyphs.
/// Ranking then alphabetical order keeps one machine's font set mapping to one
/// cascade, so a rendering report describes a reproducible configuration.
pub fn symbol_fallbacks(installed: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut families: Vec<String> = installed
        .into_iter()
        .filter(|family| family.to_lowercase().contains(SYMBOL_FAMILY_MARKER))
        .collect();
    families.sort_unstable();
    families.dedup();
    families.sort_by_key(|family| fallback_rank(family));
    families.truncate(MAX_DETECTED_FALLBACKS);
    families
}

impl Default for Config {
    fn default() -> Self {
        let (monospace, ui) = if cfg!(target_os = "linux") {
            ("DejaVu Sans Mono", "DejaVu Sans")
        } else {
            ("Menlo", ".SystemUIFont")
        };
        let font = |family: &str, size| FontConfig {
            family: family.into(),
            size,
            fallbacks: None,
        };
        Self {
            theme: "Default".into(),
            github: GitHubConfig::default(),
            confirm_close_tab: true,
            show_agents: true,
            show_system_load: true,
            contrast: Contrast::default(),
            usage: crate::usage::UsageConfig::default(),
            option_as_alt: OptionAsAlt::default(),
            open_links_in: LinkTarget::default(),
            keep_selection_after_copy: true,
            features: Features::default(),
            notifications: NotificationConfig::default(),
            notification_overrides: NotificationSettings::default(),
            clipboard_toast: ClipboardToast::default(),
            bell: BellConfig::default(),
            layout: Layout::default(),
            sidebar_layout: SidebarLayout::default(),
            keybindings: Keymap::default(),
            keybinding_overrides: BTreeMap::new(),
            pane_keys: PaneKeys::new(),
            devices: BTreeMap::new(),
            unknown_keys: Vec::new(),
            palette: crate::palette::PaletteConfig::default(),
            sidebar: font(monospace, 12.0),
            // Tabs are terminal chrome, so they read in the monospace face the
            // sidebar and terminal use, as they do in the reference UI.
            tabs: font(monospace, 12.0),
            terminal: font(monospace, 14.0),
            ui: font(ui, 12.0),
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Settings {
    theme: Option<String>,
    confirm_close_tab: Option<bool>,
    show_agents: Option<bool>,
    show_system_load: Option<bool>,
    contrast: Contrast,
    usage: crate::usage::UsageConfig,
    option_as_alt: OptionAsAlt,
    open_links_in: LinkTarget,
    keep_selection_after_copy: Option<bool>,
    sidebar: FontSettings,
    tabs: FontSettings,
    terminal: FontSettings,
    ui: FontSettings,
    github: GitHubConfig,
    features: Features,
    notifications: NotificationSettings,
    clipboard_toast: ClipboardToastSettings,
    bell: BellConfig,
    layout: Layout,
    keybindings: BTreeMap<String, Binding>,
    pane_keys: PaneKeys,
    devices: BTreeMap<String, DeviceSettings>,
    palette: crate::palette::PaletteConfig,
}

/// Each key overrides the daemon's answer on its own, so naming one of them
/// here does not silently reset the other to a GUI default.
#[derive(Default, Deserialize)]
#[serde(default)]
struct ClipboardToastSettings {
    enabled: Option<bool>,
    position: Option<ClipboardToastPosition>,
}

impl ClipboardToastSettings {
    fn resolve(self, base: ClipboardToast) -> ClipboardToast {
        ClipboardToast {
            enabled: self.enabled.unwrap_or(base.enabled),
            position: self.position.unwrap_or(base.position),
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct FontSettings {
    family: Option<String>,
    size: Option<f32>,
    fallback: Option<Vec<String>>,
}

/// Windows sets `USERPROFILE` rather than `HOME`, and upstream Herdr reads both.
pub(crate) fn home() -> Result<PathBuf> {
    let variable = |name| env::var_os(name).filter(|value: &std::ffi::OsString| !value.is_empty());
    variable("HOME")
        .or_else(|| {
            if cfg!(windows) {
                variable("USERPROFILE")
            } else {
                None
            }
        })
        .map(PathBuf::from)
        .ok_or(Error::MissingHome)
}

/// The directory holding this app's `herdr` configuration directory. Upstream
/// Herdr puts it under `%APPDATA%` on Windows, and the GUI config lives beside
/// the daemon's, so the same root has to be used on both sides.
fn config_root() -> Result<PathBuf> {
    match env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        Some(value) => {
            let path = PathBuf::from(value);
            if !path.is_absolute() {
                return Err(Error::RelativeConfigRoot);
            }
            Ok(path)
        }
        None => {
            #[cfg(windows)]
            if let Some(roaming) = env::var_os("APPDATA").filter(|value| !value.is_empty()) {
                return Ok(PathBuf::from(roaming));
            }
            #[cfg(windows)]
            return Ok(home()?.join("AppData").join("Roaming"));
            #[cfg(not(windows))]
            Ok(home()?.join(".config"))
        }
    }
}

/// The daemon's own config file, resolved exactly as herdr resolves it. Every
/// GUI reader of those settings shares this one answer.
pub(crate) fn daemon_config_path(get: impl Fn(&str) -> Option<std::ffi::OsString>) -> PathBuf {
    if let Some(path) = get("HERDR_CONFIG_PATH") {
        return path.into();
    }
    let root = get("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            #[cfg(windows)]
            {
                if let Some(root) = get("APPDATA") {
                    return PathBuf::from(root);
                }
                get("HOME")
                    .or_else(|| get("USERPROFILE"))
                    .map(PathBuf::from)
                    .map(|home| home.join("AppData/Roaming"))
                    .unwrap_or_else(env::temp_dir)
            }
            #[cfg(not(windows))]
            get("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".config"))
                .unwrap_or_else(env::temp_dir)
        });
    // Share production TUI settings even in a debug GUI build or SSH session.
    root.join("herdr/config.toml")
}

/// What the GUI honors from the daemon's own config.
#[derive(Clone, Debug, Default)]
struct Daemon {
    clipboard_toast: ClipboardToast,
    keys: DaemonKeys,
    sidebar_layout: SidebarLayout,
}

/// A config file the GUI does not own can hold anything, including settings
/// from a newer herdr, so only the keys read here matter and anything
/// unreadable, oversized, malformed, or unrecognized leaves the defaults alone.
fn daemon_settings(path: &Path) -> Daemon {
    if fs::metadata(path).is_ok_and(|data| data.len() > MAX_DAEMON_CONFIG_BYTES) {
        return Daemon::default();
    }
    let Some(table) = fs::read_to_string(path)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
    else {
        return Daemon::default();
    };
    Daemon {
        clipboard_toast: daemon_clipboard_toast(&table),
        keys: DaemonKeys::from_table(table.get("keys").and_then(toml::Value::as_table)),
        sidebar_layout: SidebarLayout::from_daemon_config(&table).unwrap_or_default(),
    }
}

fn daemon_clipboard_toast(table: &toml::Table) -> ClipboardToast {
    let mut resolved = ClipboardToast::default();
    let Some(clipboard) = table
        .get("ui")
        .and_then(|ui| ui.get("toast")?.get("clipboard")?.as_table())
    else {
        return resolved;
    };
    if let Some(enabled) = clipboard.get("enabled").and_then(toml::Value::as_bool) {
        resolved.enabled = enabled;
    }
    if let Some(position) = clipboard
        .get("position")
        .cloned()
        .and_then(|position| position.try_into().ok())
    {
        resolved.position = position;
    }
    resolved
}

fn theme_directories() -> Result<Vec<PathBuf>> {
    let root = config_root()?;
    let mut directories = vec![root.join("herdr/themes"), root.join("ghostty/themes")];
    if let Some(resources) = env::var_os("GHOSTTY_RESOURCES_DIR").filter(|value| !value.is_empty())
    {
        directories.push(PathBuf::from(resources).join("themes"));
    }
    directories.push(PathBuf::from(
        "/Applications/Ghostty.app/Contents/Resources/ghostty/themes",
    ));
    if let Some(data) = env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        directories.push(PathBuf::from(data).join("ghostty/themes"));
    } else if let Ok(home) = home() {
        directories.push(home.join(".local/share/ghostty/themes"));
    }
    let data_dirs = env::var_os("XDG_DATA_DIRS")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    directories.extend(env::split_paths(&data_dirs).map(|dir| dir.join("ghostty/themes")));
    Ok(directories)
}

impl Config {
    /// Pure application of a prepared shared snapshot. Native explicit keys win;
    /// system delivery posts OS notifications instead of in-app toasts, and
    /// terminal delivery has no outer terminal to reach from this GUI.
    pub(crate) fn apply_shared_notifications(&mut self, shared: &crate::herdr_settings::Settings) {
        self.notifications = self.notification_overrides.resolve(NotificationConfig {
            enabled: shared.toast_delivery == crate::herdr_settings::ToastDelivery::Herdr,
            system: shared.toast_delivery == crate::herdr_settings::ToastDelivery::System,
            delay_seconds: shared.toast_delay_seconds,
            position: shared.toast_position,
        });
    }

    pub fn path() -> Result<PathBuf> {
        Ok(config_root()?.join("herdr/config-gpui.toml"))
    }

    pub fn local_path() -> Result<PathBuf> {
        Ok(Self::path()?.with_extension("local.toml"))
    }

    /// Gives every face the config left alone an automatic icon-font cascade.
    /// `installed` is consulted only when some face still needs one, because
    /// enumerating system fonts is slow enough to keep off the UI thread.
    pub fn resolve_font_fallbacks<I>(&mut self, installed: impl FnOnce() -> I)
    where
        I: IntoIterator<Item = String>,
    {
        let faces = [
            &mut self.sidebar,
            &mut self.tabs,
            &mut self.terminal,
            &mut self.ui,
        ];
        if faces.iter().all(|face| face.fallbacks.is_some()) {
            return;
        }
        let detected = symbol_fallbacks(installed());
        for face in faces {
            if face.fallbacks.is_none() {
                face.fallbacks = Some(detected.clone());
            }
        }
    }

    /// A one-line warning naming the keys this build ignored, if any.
    pub(crate) fn diagnostic(&self) -> Option<String> {
        const LISTED: usize = 5;
        if self.unknown_keys.is_empty() {
            return None;
        }
        let listed = self.unknown_keys[..self.unknown_keys.len().min(LISTED)].join(", ");
        let more = match self.unknown_keys.len().saturating_sub(LISTED) {
            0 => String::new(),
            more => format!(" and {more} more"),
        };
        Some(format!(
            "config-gpui.local.toml: ignoring unknown keys {listed}{more}"
        ))
    }

    pub fn load() -> Result<Self> {
        Self::load_path(&Self::path()?, &daemon_config_path(|key| env::var_os(key)))
    }

    /// First-frame settings only: no lock, migration, writes, or fsync. The
    /// background load performs maintenance after the window has appeared.
    pub(crate) fn load_startup() -> Result<Self> {
        Self::load_startup_path(&Self::path()?, &daemon_config_path(|key| env::var_os(key)))
    }

    fn load_startup_path(path: &Path, daemon: &Path) -> Result<Self> {
        let local = path.with_extension("local.toml");
        let (text, source) = match fs::read_to_string(&local) {
            Ok(text) => (text, local),
            // Without overrides or a personal config to migrate, maintenance
            // will seed the first-launch overrides; show them from frame one.
            Err(error) if error.kind() == ErrorKind::NotFound => match fs::read_to_string(path) {
                Ok(text) if text.lines().next() != Some(MANAGED_HEADER) => (text, path.to_owned()),
                Ok(_) => (LOCAL_CONFIG.into(), local),
                Err(error) if error.kind() == ErrorKind::NotFound => (LOCAL_CONFIG.into(), local),
                Err(error) => return Err(Error::from(error).at_path(path)),
            },
            Err(error) => return Err(Error::from(error).at_path(&local)),
        };
        Self::parse_layers([DEFAULT_CONFIG, &text], &daemon_settings(daemon))
            .map_err(|error| error.at_path(&source))
    }

    /// `daemon` is the herdr config whose settings this GUI also honors. It is
    /// read for those keys alone and never written; a missing one is normal.
    fn load_path(path: &Path, daemon: &Path) -> Result<Self> {
        let base = daemon_settings(daemon);
        let (_lock, local) = Self::prepare_files(path)?;
        let text =
            fs::read_to_string(&local).map_err(|error| Error::from(error).at_path(&local))?;
        // Validate the override independently so bad types/unknown keys cannot
        // disappear inside the merge. Empty arrays explicitly replace defaults.
        Self::parse_over(&text, &base).map_err(|error| error.at_path(&local))?;
        Self::parse_layers([DEFAULT_CONFIG, &text], &base).map_err(|error| error.at_path(&local))
    }

    /// Serialize migration, defaults refresh, and theme saves across GUI windows
    /// and processes. This is only called by background config workers.
    fn prepare_files(path: &Path) -> Result<(fs::File, PathBuf)> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(|error| Error::from(error).at_path(parent))?;
        let lock_path = path.with_extension("lock");
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|error| Error::from(error).at_path(&lock_path))?;
        lock.lock()
            .map_err(|error| Error::from(error).at_path(&lock_path))?;
        let local = path.with_extension("local.toml");
        let original = match fs::read_to_string(path) {
            Ok(text) => Some(text),
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => return Err(Error::from(error).at_path(path)),
        };
        let legacy = original
            .as_deref()
            .filter(|text| text.lines().next() != Some(MANAGED_HEADER));
        if let Some(text) = legacy {
            // Never replace an old user's file until its exact contents are
            // safely stored in the local file. A conflict needs human resolution.
            Self::parse_over(text, &Daemon::default()).map_err(|error| error.at_path(path))?;
        }
        match fs::read_to_string(&local) {
            Ok(text) if legacy.is_some_and(|legacy| legacy != text) => {
                return Err(Error::ConfigMigrationConflict {
                    original: path.into(),
                    local,
                });
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {
                let mut file = tempfile::NamedTempFile::new_in(parent)
                    .map_err(|error| Error::from(error).at_path(&local))?;
                file.write_all(legacy.unwrap_or(LOCAL_CONFIG).as_bytes())
                    .map_err(|error| Error::from(error).at_path(&local))?;
                file.as_file()
                    .sync_all()
                    .map_err(|error| Error::from(error).at_path(&local))?;
                file.persist_noclobber(&local)
                    .map_err(|error| Error::from(error.error).at_path(&local))?;
            }
            Err(error) => return Err(Error::from(error).at_path(&local)),
        }
        if legacy.is_some() {
            // Windows FlushFileBuffers requires write access, including when
            // resuming a migration whose local copy already exists. Never truncate.
            fs::OpenOptions::new()
                .write(true)
                .open(&local)
                .and_then(|file| file.sync_all())
                .map_err(|error| Error::from(error).at_path(&local))?;
            // Publish the migration copy durably before replacing the only old
            // copy. Windows does not expose directory sync through std::fs.
            #[cfg(unix)]
            fs::File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|error| Error::from(error).at_path(parent))?;
        }
        if original.as_deref() != Some(DEFAULT_CONFIG) {
            write_config(path, DEFAULT_CONFIG)?;
        }
        Ok((lock, local))
    }

    /// The GUI file on its own, with nothing layered under it: the shape the
    /// tests below read, since loading also consults the daemon's config.
    #[cfg(test)]
    fn parse(text: &str) -> Result<Self> {
        Self::parse_over(text, &Daemon::default())
    }

    /// `base` is what the daemon's own config asked for, which every key this
    /// file names overrides.
    fn parse_over(text: &str, base: &Daemon) -> Result<Self> {
        Self::parse_layers([text], base)
    }

    fn parse_layers<'a>(texts: impl IntoIterator<Item = &'a str>, base: &Daemon) -> Result<Self> {
        let mut builder = config_loader::Config::builder();
        for text in texts {
            builder = builder.add_source(config_loader::File::from_str(
                text,
                config_loader::FileFormat::Toml,
            ));
        }
        let loaded = builder.build()?;
        // Config's typed deserializer coerces strings/numbers. Preserve TOML
        // types so existing strict font and theme validation remains intact.
        let value: toml::Value = loaded.try_deserialize()?;
        let mut unknown_keys = Vec::new();
        let mut settings: Settings = serde_ignored::deserialize(value, |path| {
            unknown_keys.push(path.to_string());
        })?;
        settings.keybindings.retain(|name, _| {
            let known = crate::controls::COMMANDS
                .iter()
                .any(|info| info.name == name);
            if !known {
                unknown_keys.push(format!("keybindings.{name}"));
            }
            known
        });
        unknown_keys.extend(settings.usage.retain_known());
        // Only catalog profile IDs name a device; anything else is ignored
        // and reported like any other unknown key.
        settings.devices.retain(|id, _| {
            let known = herdr_client::valid_profile_id(id);
            if !known {
                unknown_keys.push(format!("devices.{id}"));
            }
            known
        });
        // Unknown keys are ignored, but a credential pasted into the file is
        // refused so it is noticed and removed rather than left on disk.
        if let Some(name) = ["client_secret", "private_key", "token"]
            .into_iter()
            .find(|name| {
                unknown_keys
                    .iter()
                    .any(|key| key == &format!("github.{name}"))
            })
        {
            return Err(Error::GitHubSecretInConfig(name));
        }
        unknown_keys.sort();
        unknown_keys.dedup();
        let mut config = Self {
            unknown_keys,
            ..Self::default()
        };
        settings.github.client_id_with_override(None)?;
        config.github = settings.github;
        config.features = settings.features;
        config.notification_overrides = settings.notifications;
        config.notifications = settings
            .notifications
            .resolve(NotificationConfig::default());
        config.clipboard_toast = settings.clipboard_toast.resolve(base.clipboard_toast);
        config.bell = settings.bell;
        config.sidebar_layout = base.sidebar_layout.clone();
        if !settings.layout.sidebar_gap.is_finite()
            || !(0.0..=MAX_SIDEBAR_GAP).contains(&settings.layout.sidebar_gap)
        {
            return Err(Error::InvalidSidebarGap);
        }
        config.layout = settings.layout;
        config.keybindings =
            Keymap::with_overrides(&settings.keybindings, &settings.pane_keys, &base.keys)?;
        config.keybinding_overrides = settings.keybindings;
        config.pane_keys = settings.pane_keys;
        if settings.devices.len() > MAX_DEVICES {
            return Err(Error::TooManyDevices(MAX_DEVICES));
        }
        config.devices = settings.devices;
        settings.palette.validate()?;
        config.palette = settings.palette;
        if let Some(theme) = settings.theme {
            if theme.trim().is_empty() {
                return Err(Error::EmptyTheme);
            }
            config.theme = theme;
        }
        config.confirm_close_tab = settings.confirm_close_tab.unwrap_or(true);
        config.show_agents = settings.show_agents.unwrap_or(true);
        config.show_system_load = settings.show_system_load.unwrap_or(true);
        config.contrast = settings.contrast;
        config.usage = settings.usage;
        config.option_as_alt = settings.option_as_alt;
        config.open_links_in = settings.open_links_in;
        config.keep_selection_after_copy = settings.keep_selection_after_copy.unwrap_or(true);
        for (name, font, settings) in [
            ("sidebar", &mut config.sidebar, settings.sidebar),
            ("tabs", &mut config.tabs, settings.tabs),
            ("terminal", &mut config.terminal, settings.terminal),
            ("ui", &mut config.ui, settings.ui),
        ] {
            if let Some(family) = settings.family {
                font.family = family;
            }
            if let Some(size) = settings.size {
                font.size = size;
            }
            if let Some(fallback) = settings.fallback {
                if fallback.len() > MAX_FONT_FALLBACKS {
                    return Err(Error::TooManyFontFallbacks(name));
                }
                if fallback.iter().any(|family| family.trim().is_empty()) {
                    return Err(Error::EmptyFontFallback(name));
                }
                font.fallbacks = Some(fallback);
            }
            if font.family.trim().is_empty() {
                return Err(Error::EmptyFontFamily(name));
            }
            if !font.size.is_finite() || !FONT_SIZE_RANGE.contains(&font.size) {
                return Err(Error::InvalidFontSize(name));
            }
        }
        Ok(config)
    }

    /// Discover names without parsing every theme. On failure, callers can use
    /// `Theme::BUILTIN_NAMES`, which remain loadable without any directories.
    pub fn available_themes(&self) -> Result<Vec<String>> {
        self.available_themes_in(&theme_directories()?)
    }

    fn available_themes_in(&self, directories: &[PathBuf]) -> Result<Vec<String>> {
        let mut names: Vec<String> = Theme::BUILTIN_NAMES
            .iter()
            .copied()
            .chain([FOLLOW_HERDR])
            .map(str::to_owned)
            .collect();
        for directory in directories {
            let entries = match fs::read_dir(directory) {
                Ok(entries) => entries,
                Err(error) if error.kind() == ErrorKind::NotFound => continue,
                Err(error) => return Err(Error::from(error).at_path(directory)),
            };
            for entry in entries {
                let entry = entry.map_err(|error| Error::from(error).at_path(directory))?;
                // Follow symlinks just as the named theme loader does.
                let metadata = match fs::metadata(entry.path()) {
                    Ok(metadata) => metadata,
                    Err(error) if error.kind() == ErrorKind::NotFound => continue,
                    Err(error) => return Err(Error::from(error).at_path(&entry.path())),
                };
                if metadata.is_file()
                    && let Some(name) = entry.file_name().to_str()
                {
                    names.push(name.to_owned());
                }
            }
        }
        let selected = self.theme.trim();
        if Path::new(selected).is_absolute() || selected.starts_with("~/") {
            names.push(self.theme.clone());
        }
        names.sort_by_cached_key(|name| (name.to_lowercase(), name.clone()));
        names.dedup();
        Ok(names)
    }

    /// Persist only the theme selection, retaining the latest on-disk settings.
    pub fn save_theme(&self, name: &str) -> Result<()> {
        self.save_theme_at(name, &Self::path()?)
    }

    fn save_theme_at(&self, name: &str, path: &Path) -> Result<()> {
        let (_lock, local) = Self::prepare_files(path)?;
        self.save_theme_path(name, &local)
    }

    fn save_theme_path(&self, name: &str, path: &Path) -> Result<()> {
        let selected = Self {
            theme: name.into(),
            ..self.clone()
        };
        selected.theme()?;
        let result = (|| -> Result<()> {
            let text = match fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) if error.kind() == ErrorKind::NotFound => LOCAL_CONFIG.into(),
                Err(error) => return Err(error.into()),
            };
            let mut document = text.parse::<toml_edit::DocumentMut>()?;
            let mut value = toml_edit::Value::from(name);
            if let Some(previous) = document.get("theme").and_then(toml_edit::Item::as_value) {
                *value.decor_mut() = previous.decor().clone();
            }
            document["theme"] = toml_edit::Item::Value(value);
            write_config(path, &document.to_string())?;
            Ok(())
        })();
        result.map_err(|error| error.at_path(path))
    }

    /// Persist only the sidebar layout, retaining the latest on-disk
    /// settings: a `layout = "..."` name is replaced in place, and a
    /// `[layout]` table gets its `mode`.
    pub fn save_layout(mode: LayoutMode) -> Result<()> {
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::save_layout_path(mode, &local)
    }

    fn save_layout_path(mode: LayoutMode, path: &Path) -> Result<()> {
        let result = (|| -> Result<()> {
            let text = match fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) if error.kind() == ErrorKind::NotFound => LOCAL_CONFIG.into(),
                Err(error) => return Err(error.into()),
            };
            let mut document = text.parse::<toml_edit::DocumentMut>()?;
            match document.get_mut("layout") {
                Some(item) if item.is_table_like() => {
                    if let Some(layout) = item.as_table_like_mut() {
                        layout.insert("mode", toml_edit::value(mode.name()));
                    }
                }
                Some(toml_edit::Item::Value(named)) => {
                    let decor = named.decor().clone();
                    *named = toml_edit::Value::from(mode.name());
                    *named.decor_mut() = decor;
                }
                _ => {
                    document.insert("layout", toml_edit::value(mode.name()));
                }
            }
            write_config(path, &document.to_string())
        })();
        result.map_err(|error| error.at_path(path))
    }

    /// Persist a batch of logical pixel sizes without replacing other overrides.
    /// The lock also serializes this edit with migration and other GUI saves.
    pub(crate) fn save_font_sizes(sizes: &[(FontFace, f32)]) -> Result<()> {
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::save_font_sizes_path(sizes, &local)
    }

    /// Persist usage visibility without replacing provider settings.
    pub(crate) fn save_usage_visibility(show: bool) -> Result<()> {
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::save_usage_visibility_path(show, &local)
    }

    fn save_usage_visibility_path(show: bool, path: &Path) -> Result<()> {
        let result = (|| -> Result<()> {
            let text = match fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) if error.kind() == ErrorKind::NotFound => LOCAL_CONFIG.into(),
                Err(error) => return Err(error.into()),
            };
            let mut document = text.parse::<toml_edit::DocumentMut>()?;
            let usage = document
                .entry("usage")
                .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
                .as_table_like_mut()
                .ok_or(Error::InvalidUsageTable)?;
            let mut value = toml_edit::Value::from(show);
            if let Some(previous) = usage.get("show").and_then(toml_edit::Item::as_value) {
                *value.decor_mut() = previous.decor().clone();
            }
            usage.insert("show", toml_edit::Item::Value(value));
            write_config(path, &document.to_string())
        })();
        result.map_err(|error| error.at_path(path))
    }

    /// Persist only the contrast setting, keeping the rest of the local file.
    pub(crate) fn save_contrast(contrast: Contrast) -> Result<()> {
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::save_contrast_path(contrast, &local)
    }

    fn save_contrast_path(contrast: Contrast, path: &Path) -> Result<()> {
        let result = (|| -> Result<()> {
            let text = match fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) if error.kind() == ErrorKind::NotFound => LOCAL_CONFIG.into(),
                Err(error) => return Err(error.into()),
            };
            let mut document = text.parse::<toml_edit::DocumentMut>()?;
            let mut value = toml_edit::Value::from(contrast.name());
            if let Some(previous) = document.get("contrast").and_then(toml_edit::Item::as_value) {
                *value.decor_mut() = previous.decor().clone();
            }
            document["contrast"] = toml_edit::Item::Value(value);
            write_config(path, &document.to_string())
        })();
        result.map_err(|error| error.at_path(path))
    }

    /// Persist one device's keybinding source, keeping the rest of the local
    /// file. Local is the default, so choosing it removes the entry.
    pub(crate) fn save_device_keybindings(profile: &str, source: KeybindingSource) -> Result<()> {
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::save_device_keybindings_path(profile, source, &local)
    }

    fn save_device_keybindings_path(
        profile: &str,
        source: KeybindingSource,
        path: &Path,
    ) -> Result<()> {
        if !herdr_client::valid_profile_id(profile) {
            return Err(Error::InvalidDeviceId(profile.to_owned()));
        }
        let result = (|| -> Result<()> {
            let text = match fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) if error.kind() == ErrorKind::NotFound => LOCAL_CONFIG.into(),
                Err(error) => return Err(error.into()),
            };
            let mut document = text.parse::<toml_edit::DocumentMut>()?;
            match source {
                KeybindingSource::Server => {
                    let mut devices = toml_edit::Table::new();
                    devices.set_implicit(true);
                    let device = document
                        .entry("devices")
                        .or_insert(toml_edit::Item::Table(devices))
                        .as_table_like_mut()
                        .ok_or(Error::InvalidDevicesTable)?
                        .entry(profile)
                        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
                        .as_table_like_mut()
                        .ok_or(Error::InvalidDevicesTable)?;
                    device.insert("keybindings", toml_edit::value("server"));
                }
                KeybindingSource::Local => {
                    let Some(devices) = document
                        .get_mut("devices")
                        .and_then(toml_edit::Item::as_table_like_mut)
                    else {
                        return Ok(());
                    };
                    if let Some(device) = devices
                        .get_mut(profile)
                        .and_then(toml_edit::Item::as_table_like_mut)
                    {
                        device.remove("keybindings");
                        if device.is_empty() {
                            devices.remove(profile);
                        }
                    }
                    if devices.is_empty() {
                        document.remove("devices");
                    }
                }
            }
            write_config(path, &document.to_string())
        })();
        result.map_err(|error| error.at_path(path))
    }

    /// Persist only the Agents section visibility, keeping the rest of the local file.
    pub(crate) fn save_show_agents(show: bool) -> Result<()> {
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::save_show_agents_path(show, &local)
    }

    fn save_show_agents_path(show: bool, path: &Path) -> Result<()> {
        let result = (|| -> Result<()> {
            let text = match fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) if error.kind() == ErrorKind::NotFound => LOCAL_CONFIG.into(),
                Err(error) => return Err(error.into()),
            };
            let mut document = text.parse::<toml_edit::DocumentMut>()?;
            let mut value = toml_edit::Value::from(show);
            if let Some(previous) = document
                .get("show_agents")
                .and_then(toml_edit::Item::as_value)
            {
                *value.decor_mut() = previous.decor().clone();
            }
            document["show_agents"] = toml_edit::Item::Value(value);
            write_config(path, &document.to_string())
        })();
        result.map_err(|error| error.at_path(path))
    }

    /// `None` removes the local override, inheriting the platform's managed default.
    pub(crate) fn save_font_family(face: FontFace, family: Option<&str>) -> Result<()> {
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::save_font_family_path(face, family, &local)
    }

    pub(crate) fn save_all_font_families(family: Option<&str>) -> Result<()> {
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::save_font_families_path(
            &[
                FontFace::Sidebar,
                FontFace::Tabs,
                FontFace::Terminal,
                FontFace::Ui,
            ],
            family,
            &local,
        )
    }

    fn save_font_family_path(face: FontFace, family: Option<&str>, path: &Path) -> Result<()> {
        Self::save_font_families_path(&[face], family, path)
    }

    fn save_font_families_path(
        faces: &[FontFace],
        family: Option<&str>,
        path: &Path,
    ) -> Result<()> {
        if family.is_some_and(|name| name.trim().is_empty()) {
            return Err(Error::EmptyFontFamily("fonts"));
        }
        let result = (|| -> Result<()> {
            let text = fs::read_to_string(path)?;
            let mut document = text.parse::<toml_edit::DocumentMut>()?;
            for face in faces {
                if let Some(family) = family {
                    let font = document
                        .entry(face.name())
                        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
                    let table = font
                        .as_table_like_mut()
                        .ok_or(Error::EmptyFontFamily(face.name()))?;
                    let mut value = toml_edit::Value::from(family);
                    if let Some(previous) = table.get("family").and_then(toml_edit::Item::as_value)
                    {
                        *value.decor_mut() = previous.decor().clone();
                    }
                    table.insert("family", toml_edit::Item::Value(value));
                } else if let Some(table) = document
                    .get_mut(face.name())
                    .and_then(toml_edit::Item::as_table_like_mut)
                {
                    table.remove("family");
                }
            }
            write_config(path, &document.to_string())
        })();
        result.map_err(|error| error.at_path(path))
    }

    fn save_font_sizes_path(sizes: &[(FontFace, f32)], path: &Path) -> Result<()> {
        for &(face, size) in sizes {
            if !size.is_finite() || !FONT_SIZE_RANGE.contains(&size) {
                return Err(Error::InvalidFontSize(face.name()));
            }
        }
        let result = (|| -> Result<()> {
            let text = fs::read_to_string(path)?;
            let mut document = text.parse::<toml_edit::DocumentMut>()?;
            for &(face, size) in sizes {
                let font = document
                    .entry(face.name())
                    .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
                let table = font
                    .as_table_like_mut()
                    .ok_or(Error::InvalidFontSize(face.name()))?;
                let mut value = toml_edit::Value::from(size as f64);
                if let Some(previous) = table.get("size").and_then(toml_edit::Item::as_value) {
                    *value.decor_mut() = previous.decor().clone();
                }
                table.insert("size", toml_edit::Item::Value(value));
            }
            write_config(path, &document.to_string())
        })();
        result.map_err(|error| error.at_path(path))
    }

    pub fn theme(&self) -> Result<Theme> {
        self.theme_with_directories(theme_directories)
            .map(|theme| theme.with_contrast(self.contrast))
    }

    fn theme_with_directories(
        &self,
        directories: impl FnOnce() -> Result<Vec<PathBuf>>,
    ) -> Result<Theme> {
        let name = self.theme.trim();
        if name == FOLLOW_HERDR {
            return crate::herdr_settings::Settings::load()?.theme(false);
        }
        if let Some(theme) = Theme::builtin(name) {
            return Ok(theme);
        }
        let path = if let Some(relative) = name.strip_prefix("~/") {
            home()?.join(relative)
        } else if Path::new(name).is_absolute() {
            PathBuf::from(name)
        } else {
            if name.is_empty()
                || Path::new(name).components().count() != 1
                || !matches!(
                    Path::new(name).components().next(),
                    Some(Component::Normal(_))
                )
            {
                return Err(Error::InvalidThemePath);
            }
            let directories = directories()?;
            let mut found = None;
            for directory in &directories {
                let candidate = directory.join(name);
                match fs::metadata(&candidate) {
                    Ok(metadata) if metadata.is_file() => {
                        found = Some(candidate);
                        break;
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == ErrorKind::NotFound => {}
                    Err(error) => return Err(Error::from(error).at_path(&candidate)),
                }
            }
            found.ok_or_else(|| Error::ThemeNotFound {
                name: name.into(),
                directories,
            })?
        };
        let text = fs::read_to_string(&path).map_err(|error| Error::from(error).at_path(&path))?;
        Theme::parse_ghostty(&text).map_err(|error| error.at_path(&path))
    }
}

fn write_config(path: &Path, text: &str) -> Result<()> {
    let result = (|| -> std::io::Result<()> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(text.as_bytes())?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|error| error.error)?;
        Ok(())
    })();
    result.map_err(|error| Error::from(error).at_path(path))
}

/// Colors are packed 24-bit RGB, without an alpha channel.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub background: u32,
    pub foreground: u32,
    pub cursor: u32,
    pub surface: u32,
    pub active: u32,
    pub muted: u32,
    /// Herdr's optional `sidebar_bg`, which colors only the sidebar. Unset,
    /// the sidebar stays on [`Self::surface`].
    pub sidebar: Option<u32>,
    pub palette: [u32; 256],
    /// Applied by [`Theme::with_contrast`]; every theme loads as `Standard`.
    pub contrast: Contrast,
}

impl Default for Theme {
    fn default() -> Self {
        let mut palette = [0; 256];
        palette[..16].copy_from_slice(&[
            0x000000, 0x800000, 0x008000, 0x808000, 0x000080, 0x800080, 0x008080, 0xc0c0c0,
            0x808080, 0xff0000, 0x00ff00, 0xffff00, 0x0000ff, 0xff00ff, 0x00ffff, 0xffffff,
        ]);
        for (index, color) in palette.iter_mut().enumerate().skip(16) {
            let n = index as u32;
            *color = if n < 232 {
                let n = n - 16;
                let level = |v| if v == 0 { 0 } else { 55 + v * 40 };
                (level(n / 36) << 16) | (level(n / 6 % 6) << 8) | level(n % 6)
            } else {
                (8 + (n - 232) * 10) * 0x010101
            };
        }
        Self {
            background: 0x101419,
            foreground: 0xd8dee9,
            cursor: 0xd8dee9,
            surface: 0x1c1c22,
            active: 0x2b2933,
            muted: 0x827e91,
            sidebar: None,
            palette,
            contrast: Contrast::Standard,
        }
    }
}

/// `percent` of `over` blended onto `base`, per channel.
pub(crate) fn mix(base: u32, over: u32, percent: u32) -> u32 {
    let channel = |shift: u32| {
        let base = (base >> shift) & 255;
        let over = (over >> shift) & 255;
        (base * (100 - percent) + over * percent) / 100
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

impl Theme {
    pub const BUILTIN_NAMES: &'static [&'static str] = &[
        "Default",
        "Nord",
        "Dracula",
        "Catppuccin Mocha",
        "Catppuccin Latte",
    ];

    /// The theme's primary accent, used for selection colors that must read as
    /// chosen rather than merely hovered.
    pub fn primary(&self) -> u32 {
        self.palette[5]
    }

    /// The sidebar's fill: Herdr's `sidebar_bg` when set, else the surface.
    pub fn sidebar_background(&self) -> u32 {
        self.sidebar.unwrap_or(self.surface)
    }

    /// Dimmed foreground for rows that are not the current one: upstream's
    /// subtext sits between its text and its muted overlay.
    pub fn subtext(&self) -> u32 {
        self.ink(mix(self.background, self.foreground, 78))
    }

    /// A configured `dim = true` token: the color faded toward the panel.
    pub fn dimmed(&self, color: u32) -> u32 {
        mix(self.surface, color, 55)
    }

    /// A wash of [`Self::primary`] over the chrome, for filled selections such
    /// as the current tab. Large areas of the full accent shout; this keeps the
    /// hue while staying quiet enough to sit behind text all day.
    pub fn primary_wash(&self) -> u32 {
        mix(self.surface, self.primary(), 22)
    }

    /// Whichever of the theme's two text colors contrasts more with `fill`.
    /// A fixed light-or-dark rule breaks on light themes, where the accent and
    /// the background sit on the same side of any threshold.
    pub fn text_on(&self, fill: u32) -> u32 {
        let luminance = |color: u32| {
            let channel = |shift: u32| ((color >> shift) & 255) as f32 / 255.;
            0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
        };
        let fill = luminance(fill);
        if (luminance(self.background) - fill).abs() >= (luminance(self.foreground) - fill).abs() {
            self.background
        } else {
            self.foreground
        }
    }

    /// `color` as a colored mark or label drawn on this theme's chrome: moved
    /// only as far as the contrast setting needs to read on the background,
    /// the surface, and a selected row, keeping its hue. Never for terminal
    /// cells, whose colors belong to the program that wrote them.
    pub fn ink(&self, color: u32) -> u32 {
        crate::contrast::ink_on_chrome(
            color,
            [self.background, self.surface, self.active],
            self.contrast.mark_ratio(),
        )
    }

    /// High contrast parts selected rows further from the surface and raises
    /// dim labels to text contrast. Standard leaves the theme as drawn.
    pub fn with_contrast(mut self, contrast: Contrast) -> Self {
        self.contrast = contrast;
        if contrast == Contrast::High {
            self.active = mix(self.active, self.foreground, 12);
            self.muted = self.ink(self.muted);
        }
        self
    }

    fn derive_chrome(&mut self) {
        let blend = |percent| mix(self.background, self.foreground, percent);
        self.surface = blend(5);
        self.active = blend(12);
        self.muted = blend(55);
    }

    pub(super) fn builtin(name: &str) -> Option<Self> {
        // Small hand-authored palettes; no external theme assets are bundled.
        let (background, foreground, ansi) = match name {
            "Default" => return Some(Self::default()),
            "Nord" => (
                0x2e3440,
                0xd8dee9,
                [
                    0x3b4252, 0xbf616a, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x88c0d0, 0xe5e9f0,
                    0x4c566a, 0xbf616a, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x8fbcbb, 0xeceff4,
                ],
            ),
            "Dracula" => (
                0x282a36,
                0xf8f8f2,
                [
                    0x21222c, 0xff5555, 0x50fa7b, 0xf1fa8c, 0xbd93f9, 0xff79c6, 0x8be9fd, 0xf8f8f2,
                    0x6272a4, 0xff6e6e, 0x69ff94, 0xffffa5, 0xd6acff, 0xff92df, 0xa4ffff, 0xffffff,
                ],
            ),
            "Catppuccin Mocha" => (
                0x1e1e2e,
                0xcdd6f4,
                [
                    0x45475a, 0xf38ba8, 0xa6e3a1, 0xf9e2af, 0x89b4fa, 0xf5c2e7, 0x94e2d5, 0xbac2de,
                    0x585b70, 0xf38ba8, 0xa6e3a1, 0xf9e2af, 0x89b4fa, 0xf5c2e7, 0x94e2d5, 0xa6adc8,
                ],
            ),
            "Catppuccin Latte" => (
                0xeff1f5,
                0x4c4f69,
                [
                    0x5c5f77, 0xd20f39, 0x40a02b, 0xdf8e1d, 0x1e66f5, 0xea76cb, 0x179299, 0xacb0be,
                    0x6c6f85, 0xd20f39, 0x40a02b, 0xdf8e1d, 0x1e66f5, 0xea76cb, 0x179299, 0xbcc0cc,
                ],
            ),
            _ => return None,
        };
        let mut theme = Self {
            background,
            foreground,
            cursor: foreground,
            ..Self::default()
        };
        theme.palette[..16].copy_from_slice(&ansi);
        theme.derive_chrome();
        Some(theme)
    }

    fn parse_ghostty(text: &str) -> Result<Self> {
        let mut theme = Self::default();
        let mut cursor_set = false;
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line.split_once('=').unwrap_or((line, ""));
            let key = key.trim();
            let value = value.trim();
            let error = |source| Error::ThemeLine {
                line: index + 1,
                key: key.into(),
                source,
            };
            let color = |value: &str| -> Result<u32> {
                let hex = value.strip_prefix('#').unwrap_or(value);
                if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return Err(error(ThemeParseError::InvalidColor));
                }
                u32::from_str_radix(hex, 16)
                    .map_err(|source| error(ThemeParseError::InvalidHex(source)))
            };
            match key {
                "background" => theme.background = color(value)?,
                "foreground" => theme.foreground = color(value)?,
                "cursor-color" => {
                    theme.cursor = color(value)?;
                    cursor_set = true;
                }
                "palette" => {
                    let (index, value) = value
                        .split_once('=')
                        .ok_or_else(|| error(ThemeParseError::MissingPaletteColor))?;
                    let index = index
                        .trim()
                        .parse::<usize>()
                        .map_err(|source| error(ThemeParseError::InvalidPaletteIndex(source)))?;
                    if index >= 256 {
                        return Err(error(ThemeParseError::PaletteIndexOutOfRange));
                    }
                    theme.palette[index] = color(value.trim())?;
                }
                _ => {} // Never interpret includes, commands, or unrelated Ghostty settings.
            }
        }
        if !cursor_set {
            theme.cursor = theme.foreground;
        }
        theme.derive_chrome();
        Ok(theme)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn palette_defaults_overrides_and_validation() -> anyhow::Result<()> {
        let defaults = Config::parse("")?;
        assert!(defaults.palette.double_shift);
        assert!(defaults.palette.project_roots.is_empty());
        let config = Config::parse(
            "[palette]\ndouble_shift = false\nproject_roots = ['~/Code', '$HOME/Projects']",
        )?;
        assert!(!config.palette.double_shift);
        assert_eq!(config.palette.project_roots, ["~/Code", "$HOME/Projects"]);
        for text in [
            "[palette]\nproject_roots = ['']",
            "[palette]\nproject_roots = ['   ']",
            "[palette]\nproject_roots = ['x', 1]",
            "[palette]\ndouble_shift = 'yes'",
        ] {
            assert!(Config::parse(text).is_err(), "{text}");
        }
        assert_eq!(
            Config::parse("[palette]\nunknown = true")?.unknown_keys,
            ["palette.unknown"]
        );
        assert!(matches!(
            Config::parse(&format!(
                "[palette]\nproject_roots = [{}]",
                vec!["'x'"; 17].join(",")
            )),
            Err(Error::PaletteProjectRoots)
        ));
        Ok(())
    }
    use super::*;
    use anyhow::Context as _;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn shared_notifications_inherit_without_resetting_session() -> anyhow::Result<()> {
        use crate::herdr_settings::Settings as Shared;
        use herdr_client::protocol::ToastHerdrPosition;

        for mut config in [
            Config::default(),
            Config::parse("")?,
            Config::parse("[notifications]")?,
            Config::parse(DEFAULT_CONFIG)?,
        ] {
            assert_eq!(config.notifications, NotificationConfig::default());
            config.terminal.size = 27.5;
            config.ui.size = 18.;
            config.terminal.fallbacks = Some(vec!["Session Fallback".into()]);
            config.clipboard_toast.enabled = false;
            config.contrast = Contrast::High;
            let session = config.clone();
            for (delivery, enabled, system) in [
                ("herdr", true, false),
                ("off", false, false),
                ("system", false, true),
                ("terminal", false, false),
                ("herdr", true, false),
            ] {
                let shared = Shared::parse_text(&format!(
                    "[ui.toast]\ndelivery = '{delivery}'\ndelay_seconds = 7\n[ui.toast.herdr]\nposition = 'top-left'\n"
                ))?;
                config.apply_shared_notifications(&shared);
                assert_eq!(
                    config.notifications,
                    NotificationConfig {
                        enabled,
                        system,
                        delay_seconds: 7,
                        position: ToastHerdrPosition::TopLeft
                    }
                );
                for (font, original) in [
                    (&config.sidebar, &session.sidebar),
                    (&config.tabs, &session.tabs),
                    (&config.terminal, &session.terminal),
                    (&config.ui, &session.ui),
                ] {
                    assert_eq!(font.family, original.family);
                    assert_eq!(font.size, original.size);
                    assert_eq!(font.fallbacks, original.fallbacks);
                }
                assert_eq!(config.clipboard_toast, session.clipboard_toast);
                assert_eq!(config.layout, session.layout);
                assert_eq!(config.theme, session.theme);
                assert_eq!(config.contrast, session.contrast);
                assert_eq!(
                    config.keybindings.bindings().collect::<Vec<_>>(),
                    session.keybindings.bindings().collect::<Vec<_>>()
                );
            }
            config.apply_shared_notifications(&Shared::parse_text("")?);
            assert_eq!(config.notifications, NotificationConfig::default());
        }
        Ok(())
    }

    #[test]
    fn shared_notifications_respect_each_explicit_native_override() -> anyhow::Result<()> {
        use crate::herdr_settings::Settings as Shared;
        use herdr_client::protocol::ToastHerdrPosition::{BottomLeft, TopRight};
        let shared = Shared::parse_text(
            "[ui.toast]\ndelivery = 'herdr'\ndelay_seconds = 7\n[ui.toast.herdr]\nposition = 'top-right'",
        )?;
        for (text, enabled, delay_seconds, position) in [
            ("enabled = false", false, 7, TopRight),
            ("delay_seconds = 0", true, 0, TopRight),
            ("position = 'bottom-left'", true, 7, BottomLeft),
            (
                "enabled = false\ndelay_seconds = 0\nposition = 'bottom-left'",
                false,
                0,
                BottomLeft,
            ),
        ] {
            let mut config = Config::parse_layers(
                [DEFAULT_CONFIG, &format!("[notifications]\n{text}")],
                &Daemon::default(),
            )?;
            for _ in 0..2 {
                config.apply_shared_notifications(&shared);
                assert_eq!(
                    config.notifications,
                    NotificationConfig {
                        enabled,
                        system: false,
                        delay_seconds,
                        position
                    },
                    "{text}"
                );
            }
        }
        let mut config = Config::parse("[notifications]\nenabled = true")?;
        for delivery in ["off", "terminal", "system"] {
            config.apply_shared_notifications(&Shared::parse_text(&format!(
                "[ui.toast]\ndelivery = '{delivery}'"
            ))?);
            assert!(config.notifications.enabled);
            assert_eq!(config.notifications.delivery(), NotificationDelivery::InApp);
        }
        // A local opt-out of in-app toasts leaves shared system delivery in charge.
        let mut config = Config::parse("[notifications]\nenabled = false")?;
        for (delivery, expected) in [
            ("system", NotificationDelivery::System),
            ("herdr", NotificationDelivery::Off),
            ("terminal", NotificationDelivery::Off),
        ] {
            config.apply_shared_notifications(&Shared::parse_text(&format!(
                "[ui.toast]\ndelivery = '{delivery}'"
            ))?);
            assert_eq!(config.notifications.delivery(), expected, "{delivery}");
        }
        Ok(())
    }

    #[test]
    fn managed_notifications_defer_but_local_and_legacy_keys_win() -> anyhow::Result<()> {
        use crate::herdr_settings::Settings as Shared;
        let shared = Shared::parse_text("[ui.toast]\ndelivery = 'herdr'\ndelay_seconds = 9")?;
        for legacy in [false, true] {
            let temp = TempDirectory::new()?;
            let path = temp.0.join("config-gpui.toml");
            let daemon = temp.0.join("absent.toml");
            if legacy {
                fs::write(&path, "[notifications]\nenabled = false\n")?;
            }
            for mut config in [
                Config::load_startup_path(&path, &daemon)?,
                Config::load_path(&path, &daemon)?,
            ] {
                config.apply_shared_notifications(&shared);
                assert_eq!(config.notifications.enabled, !legacy);
                assert_eq!(config.notifications.delay_seconds, 9);
            }
            fs::write(
                path.with_extension("local.toml"),
                "[notifications]\nenabled = false\ndelay_seconds = 1\nposition = 'bottom-right'\n",
            )?;
            for mut config in [
                Config::load_startup_path(&path, &daemon)?,
                Config::load_path(&path, &daemon)?,
            ] {
                config.apply_shared_notifications(&shared);
                assert_eq!(config.notifications, NotificationConfig::default());
            }
            assert_eq!(fs::read_to_string(&path)?, DEFAULT_CONFIG);
        }
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn shared_windows_config_path_matches_upstream_roaming_layout() {
        let vars = [
            ("USERPROFILE", r"C:\Users\test"),
            ("APPDATA", r"C:\Roaming"),
            ("XDG_CONFIG_HOME", r"C:\xdg"),
            ("HERDR_CONFIG_PATH", r"C:\explicit.toml"),
        ];
        for (count, expected) in [
            (1, r"C:\Users\test\AppData\Roaming\herdr\config.toml"),
            (2, r"C:\Roaming\herdr\config.toml"),
            (3, r"C:\xdg\herdr\config.toml"),
            (4, r"C:\explicit.toml"),
        ] {
            assert_eq!(
                daemon_config_path(|key| vars[..count]
                    .iter()
                    .find(|(name, _)| *name == key)
                    .map(|(_, value)| (*value).into())),
                PathBuf::from(expected)
            );
        }
    }

    #[test]
    fn bell_defaults_to_attention_and_rejects_unknown_keys() -> anyhow::Result<()> {
        let temp = TempDirectory::new()?;
        let gui = temp.0.join("config-gpui.toml");
        let local = gui.with_extension("local.toml");
        let daemon = temp.0.join("config.toml");
        fs::write(&gui, "")?;
        assert_eq!(
            Config::load_path(&gui, &daemon)?.bell,
            BellConfig {
                attention: true,
                sound: false
            }
        );
        fs::write(&local, "[bell]\nsound = true\n")?;
        assert_eq!(
            Config::load_path(&gui, &daemon)?.bell,
            BellConfig {
                attention: true,
                sound: true
            }
        );
        fs::write(&local, "[bell]\nattention = false\n")?;
        assert!(!Config::load_path(&gui, &daemon)?.bell.attention);
        fs::write(&local, "[bell]\nvisual = true\n")?;
        assert!(Config::load_path(&gui, &daemon).is_err());
        Ok(())
    }

    /// The daemon's own answer is the starting point, each GUI key overrides
    /// it alone, and the file this GUI writes for a new user pins neither.
    #[test]
    fn clipboard_toast_layers_the_daemon_config_under_the_gui_config() -> anyhow::Result<()> {
        use ClipboardToastPosition::*;
        let temp = TempDirectory::new()?;
        let gui = temp.0.join("config-gpui.toml");
        let local = gui.with_extension("local.toml");
        let daemon = temp.0.join("config.toml");

        // No files at all: herdr's defaults, so both clients agree.
        fs::write(&gui, "")?;
        let load = |daemon: &Path| Config::load_path(&gui, daemon);
        assert_eq!(
            load(&daemon)?.clipboard_toast,
            ClipboardToast {
                enabled: true,
                position: BottomCenter
            }
        );

        // The daemon config alone decides when the GUI config is silent.
        fs::write(
            &daemon,
            "onboarding = false\n[ui]\nstatus_indicators = \"dots\"\n[ui.toast.clipboard]\nenabled = false\nposition = \"top-right\"\n",
        )?;
        assert_eq!(
            load(&daemon)?.clipboard_toast,
            ClipboardToast {
                enabled: false,
                position: TopRight
            }
        );

        // Each GUI key overrides on its own, leaving the other one alone.
        for (text, expected) in [
            (
                "[clipboard_toast]\nenabled = true",
                ClipboardToast {
                    enabled: true,
                    position: TopRight,
                },
            ),
            (
                "[clipboard_toast]\nposition = \"bottom-left\"",
                ClipboardToast {
                    enabled: false,
                    position: BottomLeft,
                },
            ),
            (
                "[clipboard_toast]\nenabled = true\nposition = \"top-center\"",
                ClipboardToast {
                    enabled: true,
                    position: TopCenter,
                },
            ),
            (
                "[clipboard_toast]",
                ClipboardToast {
                    enabled: false,
                    position: TopRight,
                },
            ),
        ] {
            fs::write(&local, text)?;
            assert_eq!(load(&daemon)?.clipboard_toast, expected, "{text}");
        }

        // A daemon config the GUI cannot use leaves herdr's defaults standing:
        // it belongs to another program and may hold anything.
        fs::write(&local, "")?;
        for text in [
            "not toml",
            "[ui.toast.clipboard]\nenabled = \"yes\"\nposition = 3",
            "[ui.toast.clipboard]\nposition = \"middle\"",
            "[ui.toast]\nclipboard = 7",
            "[ui]\ntoast = false",
            "",
        ] {
            fs::write(&daemon, text)?;
            assert_eq!(
                load(&daemon)?.clipboard_toast,
                ClipboardToast::default(),
                "{text}"
            );
        }
        fs::remove_file(&daemon)?;
        assert_eq!(load(&daemon)?.clipboard_toast, ClipboardToast::default());
        assert_eq!(
            load(&temp.0)?.clipboard_toast,
            ClipboardToast::default(),
            "a directory is not a config"
        );

        // Oversized files are skipped rather than parsed on every config load.
        let mut oversized = "[ui.toast.clipboard]\nenabled = false\n".to_owned();
        oversized.push_str(&"# pad\n".repeat(MAX_DAEMON_CONFIG_BYTES as usize / 6));
        assert!(oversized.len() as u64 > MAX_DAEMON_CONFIG_BYTES);
        fs::write(&daemon, &oversized)?;
        assert_eq!(load(&daemon)?.clipboard_toast, ClipboardToast::default());

        // The file written for a new user must not pin either key, or the
        // daemon config could never reach a GUI that has run once.
        assert_eq!(
            ClipboardToastSettings::default().resolve(ClipboardToast {
                enabled: false,
                position: TopLeft
            }),
            ClipboardToast {
                enabled: false,
                position: TopLeft
            }
        );
        fs::write(&gui, DEFAULT_CONFIG)?;
        fs::write(
            &daemon,
            "[ui.toast.clipboard]\nenabled = false\nposition = \"top-left\"\n",
        )?;
        assert_eq!(
            load(&daemon)?.clipboard_toast,
            ClipboardToast {
                enabled: false,
                position: TopLeft
            }
        );
        Ok(())
    }

    /// The daemon's `state_text` token turns the GUI's status word on for the
    /// agents whose rows name it: an agent's `rows_by_agent` entry replaces
    /// `rows` for that agent only. Rows without it, or a file the GUI cannot
    /// use, leave it off.
    #[test]
    fn daemon_sidebar_state_text_turns_agent_status_words_on() -> anyhow::Result<()> {
        let temp = TempDirectory::new()?;
        let daemon = temp.0.join("config.toml");
        // Expected for Claude, Codex, and an agent the daemon did not identify.
        for (text, expected) in [
            ("", [false; 3]),
            ("[ui]\nstatus_indicators = \"dots\"\n", [false; 3]),
            (
                "[ui.sidebar.agents]\nrows = [[\"state_icon\", \"workspace\", \"tab\"], [\"agent\"]]\n",
                [false; 3],
            ),
            (
                "[ui.sidebar.agents]\nrows = [[\"state_icon\", \"agent\", \"state_text\"], [\"agent\"]]\n",
                [true; 3],
            ),
            (
                "[ui.sidebar.agents]\nrows = [[{ token = \"state_text\", dim = true }]]\n",
                [true; 3],
            ),
            (
                "[ui.sidebar.agents.rows_by_agent]\nclaude = [[\"state_icon\", \"state_text\"]]\n",
                [true, false, false],
            ),
            (
                "[ui.sidebar.agents.rows_by_agent]\nclaude = [[\"agent\"]]\n",
                [false; 3],
            ),
            (
                "[ui.sidebar.agents]\nrows = [[\"state_text\"]]\n\
                 [ui.sidebar.agents.rows_by_agent]\nclaude = [[\"agent\"]]\n",
                [false, true, true],
            ),
            (
                "[ui.sidebar.agents]\nrows = [[\"state_text\", \"bogus\"]]",
                [false; 3],
            ),
            ("not toml", [false; 3]),
        ] {
            fs::write(&daemon, text)?;
            let settings = daemon_settings(&daemon).sidebar_layout.agents;
            assert_eq!(
                [
                    settings.shows_status_text(Some("claude")),
                    settings.shows_status_text(Some("codex")),
                    settings.shows_status_text(None),
                ],
                expected,
                "{text}"
            );
        }
        let off = AgentLayout::default();
        assert_eq!(
            daemon_settings(&temp.0).sidebar_layout.agents,
            off,
            "a directory is not a config"
        );
        assert_eq!(
            daemon_settings(&temp.0.join("absent.toml"))
                .sidebar_layout
                .agents,
            off
        );

        // Oversized files are skipped rather than parsed on every config load.
        let mut oversized = "[ui.sidebar.agents]\nrows = [[\"state_text\"]]\n".to_owned();
        oversized.push_str(&"# pad\n".repeat(MAX_DAEMON_CONFIG_BYTES as usize / 6));
        assert!(oversized.len() as u64 > MAX_DAEMON_CONFIG_BYTES);
        fs::write(&daemon, &oversized)?;
        assert_eq!(daemon_settings(&daemon).sidebar_layout.agents, off);
        Ok(())
    }

    #[test]
    fn sidebar_layout_comes_from_the_daemon_config() -> anyhow::Result<()> {
        let temp = TempDirectory::new()?;
        let daemon = temp.0.join("config.toml");
        let absent = daemon_settings(&temp.0.join("absent.toml"));
        assert_eq!(absent.sidebar_layout, SidebarLayout::default());

        fs::write(
            &daemon,
            "[ui.sidebar.agents]\nrows = [[\"agent\", \"$usage_ctx_ok\"]]\nrow_gap = 1\n[unrelated]\nx = 1\n[ui.toast.clipboard]\nenabled = false\n",
        )?;
        let settings = daemon_settings(&daemon);
        assert_eq!(settings.sidebar_layout.agents.rows.len(), 1);
        assert_eq!(settings.sidebar_layout.agents.row_gap, 1);
        assert_eq!(settings.sidebar_layout.spaces, SpaceLayout::default());
        assert!(!settings.clipboard_toast.enabled);
        let config = Config::parse_layers(["\n"], &settings)?;
        assert_eq!(config.sidebar_layout, settings.sidebar_layout);
        assert_eq!(
            Config::parse_layers(["[usage]\ninline = false"], &settings)?.sidebar_layout,
            settings.sidebar_layout
        );

        fs::write(
            &daemon,
            "[ui.toast.clipboard]\nenabled = false\n[ui.sidebar.agents]\nrows = [[\"bogus\"]]\n",
        )?;
        let settings = daemon_settings(&daemon);
        assert_eq!(settings.sidebar_layout, SidebarLayout::default());
        assert!(!settings.clipboard_toast.enabled);
        fs::write(&daemon, "not toml [")?;
        let settings = daemon_settings(&daemon);
        assert_eq!(settings.sidebar_layout, SidebarLayout::default());
        assert_eq!(settings.clipboard_toast, ClipboardToast::default());

        Ok(())
    }

    #[test]
    fn clipboard_toast_keys_are_strict() {
        for text in [
            "[clipboard_toast]\nenabled = 1",
            "[clipboard_toast]\nenabled = \"true\"",
            "[clipboard_toast]\nposition = \"middle\"",
            "[clipboard_toast]\nposition = \"BottomCenter\"",
            "[clipboard_toast]\nposition = 1",
            "clipboard_toast = true",
        ] {
            assert!(Config::parse(text).is_err(), "{text}");
        }
    }

    #[test]
    fn notification_settings_defaults_bounds_corners_and_strict_types() -> anyhow::Result<()> {
        use herdr_client::protocol::ToastHerdrPosition;
        use std::error::Error as _;
        for text in ["", "[notifications]", DEFAULT_CONFIG] {
            assert_eq!(
                Config::parse(text)?.notifications,
                NotificationConfig::default()
            );
        }
        for delay in [0, 1, 3600] {
            for (name, position) in [
                ("top-left", ToastHerdrPosition::TopLeft),
                ("top-right", ToastHerdrPosition::TopRight),
                ("bottom-left", ToastHerdrPosition::BottomLeft),
                ("bottom-right", ToastHerdrPosition::BottomRight),
            ] {
                let config = Config::parse(&format!(
                    "[notifications]\nenabled=true\ndelay_seconds={delay}\nposition=\"{name}\"\n[layout]\nsidebar_gap=16\n[terminal]\nsize=18"
                ))?;
                assert_eq!(config.layout.sidebar_gap, 16.);
                assert_eq!(config.terminal.size, 18.);
                assert_eq!(
                    config.notifications,
                    NotificationConfig {
                        enabled: true,
                        system: false,
                        delay_seconds: delay,
                        position
                    }
                );
            }
        }
        for field in [
            "enabled=1",
            "enabled=\"true\"",
            "delay_seconds=-1",
            "delay_seconds=3601",
            "delay_seconds=1.5",
            "delay_seconds=\"1\"",
            "position=\"center\"",
        ] {
            let error = Config::parse(&format!("[notifications]\n{field}"))
                .err()
                .ok_or_else(|| anyhow::anyhow!("accepted {field}"))?;
            assert!(matches!(error, Error::Toml(_)), "{field}: {error:?}");
            assert!(error.source().is_some());
        }
        // Delivery is a shared Herdr setting; a native `system` key is unknown.
        assert_eq!(
            Config::parse("[notifications]\nsystem=true")?
                .notifications
                .delivery(),
            NotificationDelivery::Off
        );
        Ok(())
    }

    #[test]
    fn primary_selection_text_contrasts_in_every_builtin_theme() {
        let luminance = |color: u32| {
            let channel = |shift: u32| ((color >> shift) & 255) as f32 / 255.;
            0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
        };
        for name in Theme::BUILTIN_NAMES {
            let theme = Theme::builtin(name).unwrap_or_else(|| panic!("missing theme {name}"));
            assert_eq!(
                theme.primary(),
                theme.palette[5],
                "{name}: accent is ANSI 5"
            );
            // The tab fill is the softened wash, not the raw accent.
            let primary = theme.primary_wash();
            let text = theme.text_on(primary);
            assert_ne!(primary, theme.surface, "{name}: selection must be visible");
            assert_ne!(
                primary, theme.active,
                "{name}: selection must outrank hover"
            );
            assert!(
                text == theme.background || text == theme.foreground,
                "{name}: text must be one of the theme's own colors"
            );
            let gap = (luminance(text) - luminance(primary)).abs();
            let other = if text == theme.background {
                theme.foreground
            } else {
                theme.background
            };
            assert!(gap >= 0.3, "{name}: unreadable selection, gap {gap}");
            assert!(
                gap >= (luminance(other) - luminance(primary)).abs(),
                "{name}: the other text color contrasts more"
            );
        }
    }

    #[test]
    fn errors_retain_paths_categories_and_parser_sources() -> anyhow::Result<()> {
        use std::error::Error as _;

        let temp = TempDirectory::new()?;
        let path = temp.0.join("invalid.toml");
        fs::write(&path, "theme = [")?;
        let error = Config::load_path(&path, &temp.0.join("absent.toml"))
            .err()
            .ok_or_else(|| anyhow::anyhow!("accepted invalid TOML"))?;
        assert!(
            error
                .to_string()
                .starts_with(&format!("{}: ", path.display()))
        );
        let Error::Path {
            path: actual,
            source,
        } = error
        else {
            anyhow::bail!("missing path context");
        };
        assert_eq!(actual, path);
        assert!(matches!(source.as_ref(), Error::ConfigFile { .. }));
        assert!(source.source().is_some());
        assert!(matches!(
            Config::parse("[ui]\nsize = nan"),
            Err(Error::InvalidFontSize("ui"))
        ));

        let error = Theme::parse_ghostty("# ignored\npalette=bad=ffffff")
            .err()
            .ok_or_else(|| anyhow::anyhow!("accepted invalid palette index"))?;
        assert_eq!(
            error.to_string(),
            "line 2: palette: palette index must be between 0 and 255"
        );
        assert!(matches!(
            &error,
            Error::ThemeLine {
                line: 2,
                source: ThemeParseError::InvalidPaletteIndex(_),
                ..
            }
        ));
        assert!(
            error
                .source()
                .and_then(|source| source.source())
                .is_some_and(|source| source.is::<std::num::ParseIntError>())
        );
        assert!(matches!(
            Theme::parse_ghostty("palette=256=ffffff"),
            Err(Error::ThemeLine {
                source: ThemeParseError::PaletteIndexOutOfRange,
                ..
            })
        ));
        Ok(())
    }

    struct TempDirectory(PathBuf);

    impl TempDirectory {
        fn new() -> std::io::Result<Self> {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            loop {
                let path = env::temp_dir().join(format!(
                    "herdr-theme-test-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Ok(Self(path)),
                    Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error),
                }
            }
        }
    }

    impl Drop for TempDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn discovers_sorted_names_and_loads_in_precedence_order() -> anyhow::Result<()> {
        let temp = TempDirectory::new()?;
        let first = temp.0.join("first");
        let second = temp.0.join("second");
        fs::create_dir(&first)?;
        fs::create_dir(&second)?;
        fs::create_dir(first.join("not-a-theme"))?;
        for name in ["zebra", "alpha", "Nord"] {
            fs::write(first.join(name), "background=112233")?;
        }
        fs::write(second.join("Alpha"), "background=445566")?;
        fs::write(second.join("zebra"), "background=445566")?;
        let directories = vec![temp.0.join("missing"), first, second];
        let config = Config {
            theme: "alpha".into(),
            ..Config::default()
        };
        assert_eq!(
            config.available_themes_in(&directories)?,
            vec![
                "Alpha",
                "alpha",
                "Catppuccin Latte",
                "Catppuccin Mocha",
                "Default",
                "Dracula",
                "Follow Herdr",
                "Nord",
                "zebra",
            ]
        );
        assert_eq!(
            config
                .theme_with_directories(|| Ok(directories.clone()))?
                .background,
            0x112233
        );
        let builtin = Config {
            theme: "Nord".into(),
            ..Config::default()
        };
        assert_eq!(
            builtin.theme_with_directories(|| Ok(directories))?,
            Theme::builtin("Nord").context("missing builtin")?
        );
        Ok(())
    }

    #[test]
    fn discovery_includes_explicit_selection_and_reports_errors() -> anyhow::Result<()> {
        let temp = TempDirectory::new()?;
        for name in [
            temp.0.join("custom").to_string_lossy().into_owned(),
            "~/custom".into(),
        ] {
            let config = Config {
                theme: name.clone(),
                ..Config::default()
            };
            assert!(config.available_themes_in(&[])?.contains(&name));
        }
        let not_directory = temp.0.join("file");
        fs::write(&not_directory, "")?;
        assert!(
            Config::default()
                .available_themes_in(&[not_directory])
                .is_err()
        );
        for name in Theme::BUILTIN_NAMES {
            let config = Config {
                theme: (*name).into(),
                ..Config::default()
            };
            assert!(
                config
                    .theme_with_directories(|| Err(Error::MissingHome))
                    .is_ok()
            );
        }
        Ok(())
    }

    #[test]
    fn font_family_saves_and_reset_preserve_other_overrides() -> anyhow::Result<()> {
        let directory = TempDirectory::new()?;
        let path = directory.0.join("config-gpui.local.toml");
        let original = "# keep me\ntheme = 'Nord'\n\n[terminal]\nsize = 18 # size comment\nfamily = 'Old' # family comment\n";
        fs::write(&path, original)?;
        for face in [
            FontFace::Sidebar,
            FontFace::Tabs,
            FontFace::Terminal,
            FontFace::Ui,
        ] {
            Config::save_font_family_path(face, Some("Any Installed Font"), &path)?;
            let text = fs::read_to_string(&path)?;
            let document = text.parse::<toml_edit::DocumentMut>()?;
            assert_eq!(
                document[face.name()]["family"].as_str(),
                Some("Any Installed Font")
            );
            assert!(text.contains("# keep me"));
            assert!(text.contains("size = 18 # size comment"));
            Config::save_font_family_path(face, None, &path)?;
            let text = fs::read_to_string(&path)?;
            let document = text.parse::<toml_edit::DocumentMut>()?;
            assert!(
                document
                    .get(face.name())
                    .and_then(|item| item.get("family"))
                    .is_none()
            );
            assert!(text.contains("# keep me"));
            assert!(text.contains("size = 18 # size comment"));
        }
        Ok(())
    }

    #[test]
    fn all_font_families_save_and_reset_in_one_document() -> anyhow::Result<()> {
        let directory = TempDirectory::new()?;
        let path = directory.0.join("config-gpui.local.toml");
        fs::write(
            &path,
            "# keep\n[terminal]\nsize = 18 # keep size\nfamily = 'Old'\n",
        )?;
        let faces = [
            FontFace::Sidebar,
            FontFace::Tabs,
            FontFace::Terminal,
            FontFace::Ui,
        ];
        Config::save_font_families_path(&faces, Some("Shared"), &path)?;
        let document = fs::read_to_string(&path)?;
        let parsed = document.parse::<toml_edit::DocumentMut>()?;
        for face in faces {
            assert_eq!(parsed[face.name()]["family"].as_str(), Some("Shared"));
        }
        Config::save_font_family_path(FontFace::Tabs, Some("Independent"), &path)?;
        let parsed = fs::read_to_string(&path)?.parse::<toml_edit::DocumentMut>()?;
        assert_eq!(parsed["tabs"]["family"].as_str(), Some("Independent"));
        assert_eq!(parsed["terminal"]["family"].as_str(), Some("Shared"));
        Config::save_font_families_path(&faces, None, &path)?;
        let text = fs::read_to_string(&path)?;
        let parsed = text.parse::<toml_edit::DocumentMut>()?;
        for face in faces {
            assert!(
                parsed
                    .get(face.name())
                    .and_then(|item| item.get("family"))
                    .is_none()
            );
        }
        assert!(text.contains("# keep"));
        assert!(text.contains("size = 18 # keep size"));
        Ok(())
    }

    #[test]
    fn font_size_saves_preserve_other_overrides_and_comments() -> anyhow::Result<()> {
        let temp = TempDirectory::new()?;
        let path = temp.0.join("config.toml");
        let original = "# user settings\ntheme = 'Nord'\nfuture = true\n\n[tabs] # keep table\nsize = 19 # keep size\nfamily = 'Custom'\n";
        fs::write(&path, original)?;
        for (face, size) in [
            (FontFace::Sidebar, 8.),
            (FontFace::Tabs, 20.),
            (FontFace::Terminal, 48.),
            (FontFace::Ui, 14.),
        ] {
            Config::save_font_sizes_path(&[(face, size)], &path)?;
            let text = fs::read_to_string(&path)?;
            let known = text.replace("future = true\n", "");
            assert_eq!(
                face.size(&Config::parse_layers(
                    [DEFAULT_CONFIG, &known],
                    &Daemon::default()
                )?),
                size
            );
            assert!(text.contains("future = true"));
            assert!(text.contains("family = 'Custom'"));
            assert!(text.contains("[tabs] # keep table"));
            assert!(text.contains("size = 20.0 # keep size") || face != FontFace::Tabs);
        }
        let before = fs::read_to_string(&path)?;
        for invalid in [7., 49., f32::NAN, f32::INFINITY] {
            assert!(Config::save_font_sizes_path(&[(FontFace::Tabs, invalid)], &path).is_err());
            assert_eq!(fs::read_to_string(&path)?, before);
        }
        Ok(())
    }

    #[test]
    fn font_size_batches_validate_every_change_before_writing() -> anyhow::Result<()> {
        let temp = TempDirectory::new()?;
        let path = temp.0.join("config-gpui.local.toml");
        let original = "# retained\ntheme = 'Nord'\n[sidebar]\nsize = 12 # retained size\n";
        fs::write(&path, original)?;
        assert!(matches!(
            Config::save_font_sizes_path(&[(FontFace::Sidebar, 14.), (FontFace::Ui, 49.)], &path),
            Err(Error::InvalidFontSize("ui"))
        ));
        assert_eq!(fs::read_to_string(&path)?, original);
        Config::save_font_sizes_path(&[(FontFace::Sidebar, 14.), (FontFace::Ui, 20.)], &path)?;
        let saved = fs::read_to_string(&path)?;
        let config = Config::parse(&saved)?;
        assert_eq!((config.sidebar.size, config.ui.size), (14., 20.));
        assert!(saved.contains("size = 14.0 # retained size"));
        assert!(saved.contains("theme = 'Nord'"));
        Ok(())
    }

    #[test]
    fn usage_visibility_preserves_settings_and_rejects_invalid_tables() -> anyhow::Result<()> {
        let temp = TempDirectory::new()?;
        let path = temp.0.join("config.toml");
        let original = "theme = 'Nord' # keep\n[usage]\nshow = true # visibility\nhide_providers = ['claude']\n";
        fs::write(&path, original)?;
        Config::save_usage_visibility_path(false, &path)?;
        assert_eq!(
            fs::read_to_string(&path)?,
            original.replace("show = true", "show = false")
        );
        assert!(!Config::parse(&fs::read_to_string(&path)?)?.usage.show);
        Config::save_usage_visibility_path(true, &path)?;
        assert_eq!(fs::read_to_string(&path)?, original);
        for original in [
            "theme = 'Nord'\n",
            "usage = { show = true, browser_cookies = false }\n",
        ] {
            fs::write(&path, original)?;
            Config::save_usage_visibility_path(false, &path)?;
            assert!(!Config::parse(&fs::read_to_string(&path)?)?.usage.show);
        }
        fs::write(&path, "usage = false\n")?;
        let error = Config::save_usage_visibility_path(false, &path)
            .err()
            .context("invalid usage table must be rejected")?;
        assert!(matches!(&error, Error::Path { path: failed, source }
            if failed == &path && matches!(**source, Error::InvalidUsageTable)));
        assert!(std::error::Error::source(&error).is_some());
        assert_eq!(fs::read_to_string(&path)?, "usage = false\n");
        Ok(())
    }

    #[test]
    fn contrast_parses_reaches_the_theme_and_saves_in_place() -> anyhow::Result<()> {
        assert_eq!(Config::parse("")?.contrast, Contrast::Standard);
        let high = Config::parse("theme = 'Catppuccin Latte'\ncontrast = 'high'")?;
        assert_eq!(high.contrast, Contrast::High);
        assert_eq!(high.theme()?.contrast, Contrast::High);
        assert!(Config::parse("contrast = 'loud'").is_err());
        assert!(Config::parse("contrast = true").is_err());

        let temp = TempDirectory::new()?;
        let path = temp.0.join("config.toml");
        let original =
            "theme = 'Nord' # keep\ncontrast = 'standard' # mine\n[usage]\nshow = false\n";
        fs::write(&path, original)?;
        Config::save_contrast_path(Contrast::High, &path)?;
        let saved = fs::read_to_string(&path)?;
        assert_eq!(saved, original.replace("'standard'", "\"high\""));
        assert_eq!(Config::parse(&saved)?.contrast, Contrast::High);
        assert!(!Config::parse(&saved)?.usage.show);
        fs::remove_file(&path)?;
        Config::save_contrast_path(Contrast::High, &path)?;
        let created = fs::read_to_string(&path)?;
        assert!(created.starts_with(LOCAL_CONFIG), "{created}");
        assert_eq!(Config::parse(&created)?.contrast, Contrast::High);
        Ok(())
    }

    #[test]
    fn show_agents_saves_in_place_and_keeps_other_settings() -> anyhow::Result<()> {
        let temp = TempDirectory::new()?;
        let path = temp.0.join("config.toml");
        let original = "theme = 'Nord' # keep\nshow_agents = true # mine\n[usage]\nshow = false\n";
        fs::write(&path, original)?;
        Config::save_show_agents_path(false, &path)?;
        let saved = fs::read_to_string(&path)?;
        assert_eq!(
            saved,
            original.replace("show_agents = true", "show_agents = false")
        );
        let config = Config::parse(&saved)?;
        assert!(!config.show_agents);
        assert!(!config.usage.show);
        Config::save_show_agents_path(true, &path)?;
        assert_eq!(fs::read_to_string(&path)?, original);

        // A key added to a file with tables must stay top-level, not join [usage].
        fs::write(&path, "theme = 'Nord'\n[usage]\nshow = true\n")?;
        Config::save_show_agents_path(false, &path)?;
        let config = Config::parse(&fs::read_to_string(&path)?)?;
        assert!(!config.show_agents);
        assert!(config.usage.show);

        fs::remove_file(&path)?;
        Config::save_show_agents_path(false, &path)?;
        let created = fs::read_to_string(&path)?;
        assert!(created.starts_with(LOCAL_CONFIG), "{created}");
        assert!(!Config::parse(&created)?.show_agents);
        Ok(())
    }

    #[test]
    fn high_contrast_parts_selected_rows_and_lifts_dim_labels_on_every_theme() {
        let ratio = crate::contrast::ratio;
        for name in Theme::BUILTIN_NAMES {
            let standard = Theme::builtin(name).unwrap_or_else(|| panic!("missing {name}"));
            assert_eq!(standard.clone().with_contrast(Contrast::Standard), standard);
            let high = standard.clone().with_contrast(Contrast::High);
            // Terminal cells keep the program's colors.
            assert_eq!(high.palette, standard.palette);
            assert_eq!(
                (high.background, high.foreground, high.cursor, high.surface),
                (
                    standard.background,
                    standard.foreground,
                    standard.cursor,
                    standard.surface
                )
            );
            assert!(ratio(high.active, high.surface) > ratio(standard.active, standard.surface));
            for background in [high.background, high.surface, high.active] {
                assert!(ratio(high.muted, background) >= 4.5, "{name} muted");
                assert!(ratio(high.subtext(), background) >= 4.5, "{name} subtext");
                assert!(ratio(high.foreground, background) >= 4.5, "{name} text");
            }
        }
    }

    #[test]
    fn saves_only_theme_and_preserves_latest_settings_and_comments() -> anyhow::Result<()> {
        let temp = TempDirectory::new()?;
        let path = temp.0.join("config.toml");
        let config = Config::default();
        // These on-disk settings differ from the in-memory snapshot, including
        // a setting this version does not understand.
        let original = "# heading\ntheme = 'Default' # selection\nfuture = true\n\n[tabs] # fonts\nsize = 19 # keep\n\n[github] # public only\noauth_client_id = 'Iv1.fixture' # keep ID\n";
        fs::write(&path, original)?;
        config.save_theme_path("Nord", &path)?;
        assert_eq!(
            fs::read_to_string(&path)?,
            original.replace("'Default'", "\"Nord\"")
        );
        assert_eq!(config.theme, "Default");
        assert_eq!(fs::read_dir(&temp.0)?.count(), 1);

        fs::write(
            &path,
            "# no theme\n[tabs]\nsize = 19\n[github]\noauth_client_id = 'Iv1.fixture'\n",
        )?;
        config.save_theme_path("Dracula", &path)?;
        let saved = fs::read_to_string(&path)?;
        let parsed = Config::parse(&saved)?;
        assert_eq!(parsed.theme, "Dracula");
        assert_eq!(parsed.tabs.size, 19.0);
        assert_eq!(
            parsed.github.oauth_client_id.as_deref(),
            Some("Iv1.fixture")
        );
        assert!(saved.contains("# no theme"));
        Ok(())
    }

    #[test]
    fn save_validates_theme_and_toml_before_writing() -> anyhow::Result<()> {
        let temp = TempDirectory::new()?;
        let path = temp.0.join("config.toml");
        let config = Config::default();
        let custom = temp.0.join("custom");
        fs::write(&custom, "background=invalid")?;
        let custom_name = custom.to_str().context("non-UTF8 temporary path")?;
        for name in ["", "../invalid", custom_name] {
            assert!(config.save_theme_path(name, &path).is_err());
            assert!(!path.exists());
        }
        for text in ["theme = [", "theme = 'Nord'\ntheme = 'Dracula'\n"] {
            fs::write(&path, text)?;
            assert!(config.save_theme_path("Nord", &path).is_err());
            assert_eq!(fs::read_to_string(&path)?, text);
            assert_eq!(fs::read_dir(&temp.0)?.count(), 2);
        }
        fs::write(&custom, "background=112233")?;
        let new_path = temp.0.join("nested/config.toml");
        config.save_theme_path(custom_name, &new_path)?;
        assert_eq!(
            Config::parse(&fs::read_to_string(&new_path)?)?
                .theme()?
                .background,
            0x112233
        );
        assert_eq!(
            fs::read_dir(new_path.parent().context("missing parent")?)?.count(),
            1
        );
        Ok(())
    }

    #[test]
    fn defaults_and_partial_settings() -> anyhow::Result<()> {
        // Sidebar, tabs, terminal, ui: only the status bar and modals are sans.
        #[cfg(target_os = "linux")]
        let families = [
            "DejaVu Sans Mono",
            "DejaVu Sans Mono",
            "DejaVu Sans Mono",
            "DejaVu Sans",
        ];
        #[cfg(not(target_os = "linux"))]
        let families = ["Menlo", "Menlo", "Menlo", ".SystemUIFont"];

        for config in [
            Config::default(),
            Config::parse("")?,
            Config::parse(DEFAULT_CONFIG)?,
        ] {
            assert_eq!(config.theme()?, Theme::default());
            assert!(config.github.oauth_client_id.is_none());
            // Every feature ships off, including in the example config.
            assert_eq!(config.features, Features::default());
            assert!(!config.features.sidebar_hover_menu);
            assert_eq!(config.terminal.line_height(), 20.0);
            for ((font, family), size) in [config.sidebar, config.tabs, config.terminal, config.ui]
                .into_iter()
                .zip(families)
                .zip([12.0, 12.0, 14.0, 12.0])
            {
                assert_eq!(font.family, family);
                assert_eq!(font.size, size);
            }
        }

        for settings in ["", "size = 18", "family = 'Custom Font'"] {
            let text = ["sidebar", "tabs", "terminal", "ui"]
                .map(|section| format!("[{section}]\n{settings}\n"))
                .join("\n");
            let config = Config::parse(&text)?;
            for ((font, family), size) in [config.sidebar, config.tabs, config.terminal, config.ui]
                .into_iter()
                .zip(families)
                .zip([12.0, 12.0, 14.0, 12.0])
            {
                assert_eq!(
                    font.family,
                    if settings.starts_with("family") {
                        "Custom Font"
                    } else {
                        family
                    }
                );
                assert_eq!(
                    font.size,
                    if settings.starts_with("size") {
                        18.0
                    } else {
                        size
                    }
                );
            }
        }
        Ok(())
    }

    #[test]
    fn appearance_and_close_options_preserve_defaults() -> anyhow::Result<()> {
        for config in [
            Config::default(),
            Config::parse("")?,
            Config::parse(DEFAULT_CONFIG)?,
        ] {
            assert!(config.confirm_close_tab);
            assert!(config.show_agents);
        }
        let config = Config::parse("confirm_close_tab = false\nshow_agents = false")?;
        assert!(!config.confirm_close_tab);
        assert!(!config.show_agents);
        assert!(Config::parse("confirm_close_tab = 'false'").is_err());
        assert!(Config::parse("show_agents = 0").is_err());
        Ok(())
    }

    #[test]
    fn links_open_in_the_system_browser_unless_configured() -> anyhow::Result<()> {
        assert_eq!(Config::parse("")?.open_links_in, LinkTarget::System);
        assert_eq!(
            Config::parse(DEFAULT_CONFIG)?.open_links_in,
            LinkTarget::System
        );
        assert_eq!(
            Config::parse("open_links_in = \"browser-tab\"")?.open_links_in,
            LinkTarget::BrowserTab
        );
        assert!(Config::parse("open_links_in = \"tab\"").is_err());
        Ok(())
    }

    #[test]
    fn pane_keys_reach_the_keymap_as_written() -> anyhow::Result<()> {
        let config = Config::parse(
            "[pane_keys]\n\"cmd-k\" = \"ctrl-l\"\n\"cmd-.\" = \"alt-.\"\n\"cmd-left\" = \"\"\n",
        )?;
        let keymap = &config.keybindings;
        let sent = |typed| {
            keymap
                .pane_key(&gpui::Keystroke::parse(typed).unwrap_or_default())
                .map(|sent| sent.unparse())
        };
        assert_eq!(sent("cmd-k").as_deref(), Some("ctrl-l"));
        assert_eq!(sent("cmd-.").as_deref(), Some("alt-."));
        assert_eq!(sent("cmd-left"), None);
        assert_eq!(keymap.primary(crate::controls::Command::ClearPane), "");
        assert_eq!(config.pane_keys.len(), 3);
        assert!(config.unknown_keys.is_empty(), "{:?}", config.unknown_keys);
        assert!(matches!(
            Config::parse(
                "[pane_keys]\n\"cmd-k\" = \"ctrl-l\"\n[keybindings]\nclear_pane = \"cmd-k\"\n"
            ),
            Err(Error::PaneKeyBound {
                command: "clear_pane",
                ..
            })
        ));
        Ok(())
    }

    #[test]
    fn selections_stay_after_copy_unless_configured() -> anyhow::Result<()> {
        assert!(Config::parse("")?.keep_selection_after_copy);
        assert!(Config::parse(DEFAULT_CONFIG)?.keep_selection_after_copy);
        assert!(!Config::parse("keep_selection_after_copy = false")?.keep_selection_after_copy);
        assert!(Config::parse("keep_selection_after_copy = 'no'").is_err());
        Ok(())
    }

    #[test]
    fn option_as_alt_accepts_auto_or_a_bool() -> anyhow::Result<()> {
        assert_eq!(Config::parse("")?.option_as_alt, OptionAsAlt::Auto);
        assert_eq!(
            Config::parse(DEFAULT_CONFIG)?.option_as_alt,
            OptionAsAlt::Auto
        );
        for (value, expected) in [
            ("'auto'", OptionAsAlt::Auto),
            ("true", OptionAsAlt::Always),
            ("false", OptionAsAlt::Never),
        ] {
            let config = Config::parse(&format!("option_as_alt = {value}"))?;
            assert_eq!(config.option_as_alt, expected);
        }
        for value in ["'left'", "'true'", "1"] {
            assert!(Config::parse(&format!("option_as_alt = {value}")).is_err());
        }
        Ok(())
    }

    #[test]
    fn every_layout_has_its_own_name_and_label() -> anyhow::Result<()> {
        assert_eq!(LayoutMode::NAMES, LayoutMode::ALL.map(LayoutMode::name));
        let labels: std::collections::HashSet<_> =
            LayoutMode::ALL.iter().map(|mode| mode.label()).collect();
        assert_eq!(labels.len(), LayoutMode::ALL.len());
        for mode in LayoutMode::ALL {
            let name = mode.name();
            assert_eq!(LayoutMode::try_from(name)?, mode);
            assert_eq!(
                Config::parse(&format!("layout = '{name}'"))?.layout.mode,
                mode
            );
            let table = Config::parse(&format!("[layout]\nmode = '{name}'\nsidebar_gap = 4"))?;
            assert_eq!((table.layout.mode, table.layout.sidebar_gap), (mode, 4.));
        }
        // Layouts with a design of their own fix their spacing.
        assert_eq!(
            (LayoutMode::Orca.density(), LayoutMode::Orca.style()),
            (Density::Comfortable, Style::Rounded)
        );
        // A second setting for rows no longer exists: ignored, not applied.
        let config = Config::parse("[layout]\nmode = 'minimal'\nrows = 'orca'")?;
        assert_eq!(config.layout.mode, LayoutMode::Minimal);
        assert_eq!(config.unknown_keys, ["layout.rows"]);
        assert!(Config::parse("layout = 'herdr'").is_err());
        Ok(())
    }

    #[test]
    fn saving_a_layout_keeps_every_other_setting() -> anyhow::Result<()> {
        let temp = TempDirectory::new()?;
        let path = temp.0.join("config-gpui.local.toml");
        let mode = |path: &Path| -> anyhow::Result<Layout> {
            Ok(Config::parse(&fs::read_to_string(path)?)?.layout)
        };
        // A new install's plain name is replaced in place, comments and all,
        // and still layers over the managed file.
        Config::save_layout_path(LayoutMode::Orca, &path)?;
        let text = fs::read_to_string(&path)?;
        assert!(text.contains("layout = \"orca\""), "{text}");
        assert!(text.contains("# New installs start"), "{text}");
        let merged = Config::parse_layers([DEFAULT_CONFIG, text.as_str()], &Daemon::default())?;
        assert_eq!(merged.layout.mode, LayoutMode::Orca);
        // A table gets its mode beside the gap, and keeps its comments.
        fs::write(
            &path,
            "# mine\ntheme = 'Nord'\n\n[layout] # sidebar\nmode = 'compact'\nsidebar_gap = 4\n",
        )?;
        for chosen in LayoutMode::ALL {
            Config::save_layout_path(chosen, &path)?;
            let layout = mode(&path)?;
            assert_eq!((layout.mode, layout.sidebar_gap), (chosen, 4.));
        }
        let text = fs::read_to_string(&path)?;
        assert!(
            text.contains("# mine") && text.contains("# sidebar"),
            "{text}"
        );
        assert_eq!(Config::parse(&text)?.theme, "Nord");
        // Inline tables and files without a layout work too.
        for original in ["layout = { sidebar_gap = 4 }\n", "theme = 'Nord'\n"] {
            fs::write(&path, original)?;
            Config::save_layout_path(LayoutMode::Minimal, &path)?;
            assert_eq!(mode(&path)?.mode, LayoutMode::Minimal, "{original}");
        }
        Ok(())
    }

    #[test]
    fn compact_layout_is_opt_in() -> anyhow::Result<()> {
        for config in [
            Config::default(),
            Config::parse("")?,
            Config::parse(DEFAULT_CONFIG)?,
            Config::parse("[layout]")?,
            Config::parse("layout = 'normal'")?,
        ] {
            assert_eq!(config.layout.mode, LayoutMode::default());
            assert_eq!(config.layout.mode, LayoutMode::from(Density::Normal));
        }
        let config = Config::parse("layout = 'compact'")?;
        assert_eq!(config.layout.mode, LayoutMode::from(Density::Compact));
        assert_eq!(config.layout.sidebar_gap, Layout::default().sidebar_gap);
        assert_eq!(config.sidebar.size, Config::default().sidebar.size);
        let custom = Config::parse("[layout]\nmode = 'compact'\nsidebar_gap = 4")?;
        assert_eq!(custom.layout.mode, LayoutMode::from(Density::Compact));
        assert_eq!(custom.layout.sidebar_gap, 4.);
        for density in [Density::Compact, Density::Normal, Density::Comfortable] {
            for style in [Style::Flat, Style::Rounded] {
                let mode = LayoutMode::new(density, style);
                let name = mode.to_string();
                assert_eq!(LayoutMode::try_from(name.as_str())?, mode);
                assert_eq!(
                    Config::parse(&format!("layout = '{name}'"))?.layout.mode,
                    mode
                );
                let config = Config::parse(&format!("[layout]\nmode = '{name}'\nsidebar_gap = 4"))?;
                assert_eq!(config.layout.mode, mode);
                assert_eq!(config.layout.sidebar_gap, 4.);
            }
        }
        assert_eq!(
            LayoutMode::try_from("compact-rounded")?,
            LayoutMode::new(Density::Compact, Style::Rounded)
        );
        for name in [
            "rounded",
            "-rounded",
            "normal-",
            "Normal",
            "normal-rounded-rounded",
        ] {
            assert!(matches!(
                LayoutMode::try_from(name),
                Err(Error::UnknownLayout(unknown)) if unknown == name
            ));
        }
        for value in ["'unknown'", "'rounded'", "'normal-square'", "true", "1"] {
            assert!(matches!(
                Config::parse(&format!("layout = {value}")),
                Err(Error::Toml(_))
            ));
        }
        Ok(())
    }

    #[test]
    fn sidebar_gap_defaults_to_flush_and_accepts_its_band() -> anyhow::Result<()> {
        for config in [
            Config::default(),
            Config::parse("")?,
            Config::parse(DEFAULT_CONFIG)?,
        ] {
            assert_eq!(config.layout, Layout::default());
            assert_eq!(config.layout.sidebar_gap, 0.);
        }
        // An empty table keeps the default; only a written value replaces it.
        assert_eq!(Config::parse("[layout]")?.layout.sidebar_gap, 0.);
        for (text, gap) in [
            ("[layout]\nsidebar_gap = 0", 0.),
            ("[layout]\nsidebar_gap = 12", 12.),
            ("[layout]\nsidebar_gap = 7.5", 7.5),
            ("[layout]\nsidebar_gap = 64", 64.),
        ] {
            let config = Config::parse(text)?;
            assert_eq!(config.layout.sidebar_gap, gap);
            // Spacing alone leaves every other setting at its default.
            assert_eq!(config.theme, Config::default().theme);
            assert_eq!(config.terminal.size, Config::default().terminal.size);
        }
        assert!(matches!(
            Config::parse("[layout]\nsidebar_gap = 64.1"),
            Err(Error::InvalidSidebarGap)
        ));
        assert!(matches!(
            Config::parse("[layout]\nsidebar_gap = nan"),
            Err(Error::InvalidSidebarGap)
        ));
        Ok(())
    }

    #[test]
    fn rejects_invalid_settings() {
        for text in [
            "theme = ''",
            "[ui]\nfamily = '  '",
            "[tabs]\nsize = 7.9",
            "[terminal]\nsize = 48.1",
            "[sidebar]\nsize = nan",
            "[sidebar]\nsize = inf",
            "[sidebar]\nsize = -inf",
            "[tabs]\nsize = '14'",
            "[tabs]\nfamily = 14",
            "[github]\nclient_secret = 'not-allowed'",
            "[github]\nprivate_key = 'not-allowed'",
            "[github]\ntoken = 'not-allowed'",
            "[github]\noauth_client_id = 123",
            "[github]\noauth_client_id = ''",
            "[github]\noauth_client_id = ' bad-id'",
            "[github]\noauth_client_id = 'bad/id'",
            "[github]\noauth_client_id = '\u{e9}'",
            "[features]\nsidebar_hover_menu = 'true'",
            "[features]\nsidebar_hover_menu = 1",
            "[layout]\nsidebar_gap = -1",
            "[layout]\nsidebar_gap = 65",
            "[layout]\nsidebar_gap = inf",
            "[layout]\nsidebar_gap = '8'",
        ] {
            assert!(Config::parse(text).is_err(), "accepted {text:?}");
        }
        assert!(Config::parse("[tabs]\nsize = 8\n[ui]\nsize = 48").is_ok());
    }

    #[test]
    fn devices_opt_into_server_keybindings_one_by_one() -> anyhow::Result<()> {
        const ID: &str = "0123456789abcdef0123456789abcdef";
        const OTHER: &str = "fedcba9876543210fedcba9876543210";
        let config = Config::parse(&format!(
            "[keybindings]\nnew_tab = 'cmd-y'\n[devices.{ID}]\nkeybindings = 'server'\n[devices.{OTHER}]\nkeybindings = 'local'"
        ))?;
        assert_eq!(
            config.keybinding_source(&format!("ssh:{ID}")),
            KeybindingSource::Server
        );
        assert_eq!(
            config.keybinding_source(&format!("ssh:{OTHER}")),
            KeybindingSource::Local
        );
        // Local, explicit sockets, and unlisted devices keep local keys, and
        // a bare profile ID is not an endpoint ID.
        for endpoint in [
            "local",
            "socket",
            ID,
            "ssh:00000000000000000000000000000000",
        ] {
            assert_eq!(
                config.keybinding_source(endpoint),
                KeybindingSource::Local,
                "{endpoint}"
            );
        }
        assert_eq!(
            Config::parse("")?.keybinding_source(&format!("ssh:{ID}")),
            KeybindingSource::Local
        );
        // The overrides stay available to layer over a server profile.
        assert_eq!(
            config.keybinding_overrides.get("new_tab"),
            Some(&Binding::One("cmd-y".into()))
        );
        // A non-catalog ID, or a key this build does not know, is ignored
        // and reported, as other unknown keys are.
        let ignored = Config::parse(&format!(
            "[devices.box]\nkeybindings = 'server'\n[devices.{ID}]\nkeybindings = 'server'\ntheme = 'Nord'"
        ))?;
        assert_eq!(
            ignored.unknown_keys,
            [format!("devices.{ID}.theme"), "devices.box".to_owned()]
        );
        assert_eq!(ignored.devices.len(), 1);
        assert_eq!(
            ignored.keybinding_source(&format!("ssh:{ID}")),
            KeybindingSource::Server
        );
        for text in [
            format!("[devices.{ID}]\nkeybindings = 'remote'"),
            format!("[devices.{ID}]\nkeybindings = true"),
            "devices = 'server'".into(),
        ] {
            assert!(Config::parse(&text).is_err(), "accepted {text:?}");
        }
        let many: String = (0..=MAX_DEVICES)
            .map(|index| format!("[devices.{index:032x}]\n"))
            .collect();
        assert!(matches!(
            Config::parse(&many),
            Err(Error::TooManyDevices(MAX_DEVICES))
        ));
        Ok(())
    }

    #[test]
    fn device_keybindings_save_in_place_and_local_removes_them() -> anyhow::Result<()> {
        const ID: &str = "0123456789abcdef0123456789abcdef";
        let endpoint = format!("ssh:{ID}");
        let temp = TempDirectory::new()?;
        let path = temp.0.join("config.toml");
        let original = "theme = 'Nord' # keep\n[usage]\nshow = false\n";
        fs::write(&path, original)?;
        Config::save_device_keybindings_path(ID, KeybindingSource::Server, &path)?;
        let saved = fs::read_to_string(&path)?;
        assert!(saved.starts_with(original), "{saved}");
        assert!(
            saved.contains(&format!("[devices.{ID}]\nkeybindings = \"server\"")),
            "{saved}"
        );
        let config = Config::parse(&saved)?;
        assert_eq!(
            config.keybinding_source(&endpoint),
            KeybindingSource::Server
        );
        assert!(!config.usage.show);
        // Saving it again is a no-op, and Local restores the original file.
        Config::save_device_keybindings_path(ID, KeybindingSource::Server, &path)?;
        assert_eq!(fs::read_to_string(&path)?, saved);
        Config::save_device_keybindings_path(ID, KeybindingSource::Local, &path)?;
        assert_eq!(fs::read_to_string(&path)?, original);
        Config::save_device_keybindings_path(ID, KeybindingSource::Local, &path)?;
        assert_eq!(fs::read_to_string(&path)?, original);
        // Another device's entry, and unknown keys, survive.
        let shared = format!(
            "[devices.{ID}]\nkeybindings = 'server'\n[devices.fedcba9876543210fedcba9876543210]\nkeybindings = 'server'\n"
        );
        fs::write(&path, &shared)?;
        Config::save_device_keybindings_path(ID, KeybindingSource::Local, &path)?;
        let kept = fs::read_to_string(&path)?;
        assert!(!kept.contains(ID), "{kept}");
        assert!(kept.contains("fedcba9876543210fedcba9876543210"), "{kept}");
        // An ID that is not a catalog profile never reaches the file.
        assert!(matches!(
            Config::save_device_keybindings_path("../x", KeybindingSource::Server, &path),
            Err(Error::InvalidDeviceId(_))
        ));
        assert_eq!(fs::read_to_string(&path)?, kept);
        fs::remove_file(&path)?;
        Config::save_device_keybindings_path(ID, KeybindingSource::Server, &path)?;
        let created = fs::read_to_string(&path)?;
        assert!(created.starts_with(LOCAL_CONFIG), "{created}");
        assert_eq!(
            Config::parse(&created)?.keybinding_source(&endpoint),
            KeybindingSource::Server
        );
        Ok(())
    }

    #[test]
    fn keybindings_override_defaults_and_reject_bad_entries() -> anyhow::Result<()> {
        use crate::Command;
        let config = Config::parse("")?;
        assert_eq!(config.keybindings.primary(Command::Tab), "cmd-t");
        let config = Config::parse(
            "[keybindings]\nnew_workspace = \"cmd-n\"\nnew_tab = [\"cmd-t\", \"ctrl-t\"]\nquit = \"\"",
        )?;
        let shortcuts = |command| config.keybindings.shortcuts(command).collect::<Vec<_>>();
        assert_eq!(shortcuts(Command::Workspace), ["cmd-n"]);
        assert_eq!(shortcuts(Command::Tab), ["cmd-t", "ctrl-t"]);
        assert!(shortcuts(Command::Quit).is_empty());
        // The managed defaults document the table without setting it.
        let layered = Config::parse_layers(
            [DEFAULT_CONFIG, "[keybindings]\nthemes = \"cmd-k\""],
            &Daemon::default(),
        )?;
        assert_eq!(layered.keybindings.primary(Command::Themes), "cmd-k");
        assert_eq!(layered.keybindings.primary(Command::Tab), "cmd-t");
        // A command this build does not have is ignored, not fatal.
        let config = Config::parse("[keybindings]\nnew_space = \"cmd-n\"\nthemes = \"cmd-k\"")?;
        assert_eq!(config.unknown_keys, ["keybindings.new_space"]);
        assert_eq!(config.keybindings.primary(Command::Themes), "cmd-k");
        assert!(matches!(
            Config::parse("[keybindings]\nnew_tab = \"t\""),
            Err(Error::KeystrokeWithoutModifier { .. })
        ));
        assert!(Config::parse("[keybindings]\nnew_tab = 5").is_err());
        Ok(())
    }

    /// The daemon's `[keys]` reach the GUI keymap under the GUI's own
    /// `[keybindings]`, and a daemon file the GUI cannot use falls back to
    /// Herdr's defaults instead of failing the GUI config.
    #[test]
    fn daemon_keys_layer_under_gui_keybindings() -> anyhow::Result<()> {
        use crate::Command;
        let temp = TempDirectory::new()?;
        let gui = temp.0.join("config-gpui.toml");
        let local = gui.with_extension("local.toml");
        let daemon = temp.0.join("config.toml");
        let load = || Config::load_path(&gui, &daemon);
        fs::write(&gui, "")?;
        let shortcuts = |config: &Config, command| {
            config
                .keybindings
                .shortcuts(command)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };

        // No daemon file: Herdr's defaults.
        assert_eq!(shortcuts(&load()?, Command::Tab), ["cmd-t", "ctrl-b c"]);

        fs::write(
            &daemon,
            "[keys]\nprefix = \"ctrl+a\"\nsplit_vertical = [\"prefix+v\", \"prefix+\\\\\"]\nswitch_tab = [\"prefix+1..9\", \"alt+1..9\"]\n",
        )?;
        let config = load()?;
        assert_eq!(
            shortcuts(&config, Command::SplitRight),
            ["cmd-d", "ctrl-a v", "ctrl-a \\"]
        );
        assert_eq!(
            shortcuts(&config, Command::TabNumber(2)),
            ["cmd-2", "ctrl-a 2", "alt-2"]
        );
        assert!(
            config
                .keybindings
                .bindings()
                .any(|binding| binding == (Command::TabNumber(2), "alt-2"))
        );

        // The GUI's own entry replaces the command's list, daemon chords too.
        fs::write(&local, "[keybindings]\nsplit_right = \"cmd-d\"\n")?;
        assert_eq!(shortcuts(&load()?, Command::SplitRight), ["cmd-d"]);

        fs::write(&local, "")?;
        fs::write(&daemon, "[keys\nprefix = ")?;
        assert_eq!(shortcuts(&load()?, Command::Tab), ["cmd-t", "ctrl-b c"]);
        Ok(())
    }

    #[test]
    fn features_are_opt_in_per_flag() -> anyhow::Result<()> {
        assert!(!Config::parse("[features]")?.features.sidebar_hover_menu);
        let config = Config::parse("[features]\nsidebar_hover_menu = true")?;
        assert!(config.features.sidebar_hover_menu);
        // Turning a flag on leaves the rest of the settings at their defaults.
        assert_eq!(config.theme, Config::default().theme);
        assert!(
            !Config::parse("[features]\nsidebar_hover_menu = false")?
                .features
                .sidebar_hover_menu
        );
        Ok(())
    }

    #[test]
    fn github_public_client_id_and_explicit_environment_precedence() -> anyhow::Result<()> {
        assert!(!Config::default().github.allow_plaintext_credentials);
        assert!(
            Config::parse("[github]\nallow_plaintext_credentials = true")?
                .github
                .allow_plaintext_credentials
        );
        assert!(Config::parse("[github]\nallow_plaintext_credentials = 'true'").is_err());
        let config = Config::parse("[github]\noauth_client_id = 'Iv1.fixture'")?;
        assert_eq!(
            config.github.client_id_with_override(None)?.as_deref(),
            Some("Iv1.fixture")
        );
        assert_eq!(
            config
                .github
                .client_id_with_override(Some("override-fixture".as_ref()))?
                .as_deref(),
            Some("override-fixture")
        );
        assert_eq!(
            config.github.oauth_client_id.as_deref(),
            Some("Iv1.fixture")
        );
        assert_eq!(
            Config::default()
                .github
                .client_id_with_override(None)?
                .as_deref(),
            Some("Iv23liurUcwxPjrdIFYT")
        );
        for id in [
            "",
            " ",
            "bad\nvalue",
            "bad/value",
            "\u{e9}",
            &"a".repeat(257),
        ] {
            assert!(matches!(
                config.github.client_id_with_override(Some(id.as_ref())),
                Err(Error::InvalidClientId("HERDR_GITHUB_OAUTH_CLIENT_ID"))
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            assert!(
                config
                    .github
                    .client_id_with_override(Some(std::ffi::OsStr::from_bytes(b"\xff")))
                    .is_err()
            );
        }
        Ok(())
    }

    #[test]
    fn default_palette_and_builtins() -> anyhow::Result<()> {
        let default = Theme::default();
        assert_eq!(default.palette[16], 0);
        assert_eq!(default.palette[21], 0x0000ff);
        assert_eq!(default.palette[231], 0xffffff);
        assert_eq!(default.palette[232], 0x080808);
        assert_eq!(default.palette[255], 0xeeeeee);
        assert_eq!(default.surface, 0x1c1c22);
        for name in ["Nord", "Dracula", "Catppuccin Mocha", "Catppuccin Latte"] {
            let theme = Config {
                theme: name.into(),
                ..Config::default()
            }
            .theme()?;
            assert_ne!(theme, default);
            assert_ne!(theme.surface, theme.background);
            assert_eq!(theme.palette[255], default.palette[255]);
        }
        Ok(())
    }

    #[test]
    fn ghostty_colors_and_ignored_settings() -> anyhow::Result<()> {
        let theme = Theme::parse_ghostty(
            "# comment\nbackground = #123aBC\nforeground=abcdef\n\
             palette = 0 = #010203\npalette=255=fefefe\npalette=0=040506\n\
             font-size = nonsense\nconfig-file = /do/not/read\nignored line",
        )?;
        assert_eq!(theme.background, 0x123abc);
        assert_eq!(theme.foreground, 0xabcdef);
        assert_eq!(theme.cursor, theme.foreground);
        assert_eq!(theme.palette[0], 0x040506);
        assert_eq!(theme.palette[255], 0xfefefe);
        assert_eq!(
            Theme::parse_ghostty("cursor-color=#ffffff")?.cursor,
            0xffffff
        );
        Ok(())
    }

    #[test]
    fn ghostty_errors_have_line_numbers() {
        for line in [
            "background=red",
            "foreground=#fff",
            "cursor-color=0x123456",
            "palette=256=ffffff",
            "palette=-1=ffffff",
            "palette=0=oops",
            "palette=ffffff",
            "background",
            "foreground=#12345678",
        ] {
            let result = Theme::parse_ghostty(&format!("# comment\n{line}"));
            assert!(
                matches!(result, Err(Error::ThemeLine { line: 2, .. })),
                "{result:?}"
            );
        }
    }

    #[test]
    fn startup_reads_settings_without_writes_or_waiting_for_maintenance() -> anyhow::Result<()> {
        let temp = TempDirectory::new()?;
        let path = temp.0.join("config-gpui.toml");
        let daemon = temp.0.join("absent.toml");
        // A fresh install's first frame already shows the layout its seeded
        // overrides will hold, without writing them yet.
        assert_eq!(
            Config::load_startup_path(&path, &daemon)?.layout.mode,
            LayoutMode::new(Density::Comfortable, Style::Rounded)
        );
        assert_eq!(fs::read_dir(&temp.0)?.count(), 0);
        let legacy = "layout = 'compact'\ntheme = 'Nord'\n[terminal]\nsize = 18\n";
        fs::write(&path, legacy)?;
        let config = Config::load_startup_path(&path, &daemon)?;
        assert_eq!(config.layout.mode, LayoutMode::from(Density::Compact));
        assert_eq!(config.theme, "Nord");
        assert_eq!(config.terminal.size, 18.);
        assert_eq!(fs::read_to_string(&path)?, legacy);
        assert_eq!(fs::read_dir(&temp.0)?.count(), 1);

        let local = path.with_extension("local.toml");
        fs::write(&local, "layout = 'compact'\ntheme = 'Dracula'")?;
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path.with_extension("lock"))?;
        lock.lock()?;
        // Hold the maintenance lock until the read finishes, with a bounded wait
        // so accidentally adding lock acquisition is a deterministic failure.
        let (send, receive) = std::sync::mpsc::channel();
        let (worker_path, worker_daemon) = (path.clone(), daemon.clone());
        let worker = std::thread::spawn(move || {
            let _ = send.send(Config::load_startup_path(&worker_path, &worker_daemon));
        });
        let result = receive.recv_timeout(std::time::Duration::from_secs(5));
        drop(lock);
        worker
            .join()
            .map_err(|_| anyhow::anyhow!("startup reader panicked"))?;
        assert_eq!(result??.theme, "Dracula");
        assert_eq!(fs::read_to_string(&path)?, legacy);
        assert_eq!(
            fs::read_to_string(&local)?,
            "layout = 'compact'\ntheme = 'Dracula'"
        );
        Ok(())
    }

    #[test]
    fn startup_appearance_read_timing() -> anyhow::Result<()> {
        let temp = TempDirectory::new()?;
        let path = temp.0.join("config-gpui.toml");
        let daemon = temp.0.join("absent.toml");
        fs::write(
            path.with_extension("local.toml"),
            "layout = 'compact'\ntheme = 'Nord'",
        )?;
        let mut samples = Vec::new();
        for _ in 0..100 {
            let start = std::time::Instant::now();
            let config = Config::load_startup_path(&path, &daemon)?;
            let theme = config.theme()?;
            samples.push(start.elapsed());
            assert_eq!(config.layout.mode, LayoutMode::from(Density::Compact));
            assert_eq!(Some(theme), Theme::builtin("Nord"));
        }
        let first = samples[0];
        samples.sort();
        eprintln!(
            "Startup config + built-in theme: first={first:?}, median={:?}, p95={:?} (100 reads)",
            samples[50], samples[94]
        );
        // Timing is reported, not gated: filesystem latency is machine-dependent.
        Ok(())
    }

    #[test]
    fn only_new_installs_start_with_the_rounded_comfortable_layout() -> anyhow::Result<()> {
        let rounded = LayoutMode::new(Density::Comfortable, Style::Rounded);
        let daemon = Path::new("absent.toml");
        // The managed defaults keep the flat layout for everyone else.
        assert_eq!(
            Config::parse(DEFAULT_CONFIG)?.layout.mode,
            LayoutMode::default()
        );
        assert_eq!(Config::parse(LOCAL_CONFIG)?.layout.mode, rounded);

        let fresh = TempDirectory::new()?;
        let path = fresh.0.join("config-gpui.toml");
        assert_eq!(
            Config::load_startup_path(&path, daemon)?.layout.mode,
            rounded
        );
        assert_eq!(Config::load_path(&path, daemon)?.layout.mode, rounded);
        assert_eq!(
            fs::read_to_string(path.with_extension("local.toml"))?,
            LOCAL_CONFIG
        );
        // A later launch reads the seeded file, not the first-launch fallback.
        assert_eq!(
            Config::load_startup_path(&path, daemon)?.layout.mode,
            rounded
        );

        // Existing overrides without a layout keep the managed default.
        let existing = TempDirectory::new()?;
        let path = existing.0.join("config-gpui.toml");
        fs::write(path.with_extension("local.toml"), "theme = 'Nord'\n")?;
        for config in [
            Config::load_startup_path(&path, daemon)?,
            Config::load_path(&path, daemon)?,
        ] {
            assert_eq!(config.layout.mode, LayoutMode::default());
        }
        assert_eq!(
            fs::read_to_string(path.with_extension("local.toml"))?,
            "theme = 'Nord'\n"
        );

        // So does a personal config migrated from before local overrides.
        let legacy = TempDirectory::new()?;
        let path = legacy.0.join("config-gpui.toml");
        fs::write(&path, "theme = 'Nord'\n")?;
        for config in [
            Config::load_startup_path(&path, daemon)?,
            Config::load_path(&path, daemon)?,
        ] {
            assert_eq!(config.layout.mode, LayoutMode::default());
        }
        Ok(())
    }

    #[test]
    fn theme_save_updates_only_local_overrides() -> anyhow::Result<()> {
        let temp = TempDirectory::new()?;
        let path = temp.0.join("config-gpui.toml");
        let daemon = temp.0.join("absent.toml");
        let legacy = "# user fonts\n[terminal]\nsize = 19 # keep\n";
        fs::write(&path, legacy)?;
        Config::default().save_theme_at("Nord", &path)?;
        assert_eq!(fs::read_to_string(&path)?, DEFAULT_CONFIG);
        let local = fs::read_to_string(path.with_extension("local.toml"))?;
        assert!(local.contains("# user fonts"));
        assert!(local.contains("size = 19 # keep"));
        let config = Config::load_path(&path, &daemon)?;
        assert_eq!(config.theme, "Nord");
        assert_eq!(config.terminal.size, 19.);
        Ok(())
    }

    #[test]
    fn local_overrides_merge_tables_replace_arrays_and_refresh_defaults() -> anyhow::Result<()> {
        let temp = TempDirectory::new()?;
        let path = temp.0.join("config-gpui.toml");
        let local = path.with_extension("local.toml");
        let daemon = temp.0.join("absent.toml");
        Config::load_path(&path, &daemon)?;
        assert_eq!(
            fs::read_to_string(&path)?.lines().next(),
            Some(MANAGED_HEADER)
        );
        assert_eq!(fs::read_to_string(&local)?, LOCAL_CONFIG);
        let overrides = "# personal settings\nlayout = 'compact'\n[terminal]\nsize = 19\nfallback = []\n[notifications]\nenabled = true\n";
        fs::write(&local, overrides)?;
        fs::write(&path, format!("{MANAGED_HEADER}\ntheme = 'old-default'\n"))?;
        let config = Config::load_path(&path, &daemon)?;
        assert_eq!(config.layout.mode, LayoutMode::from(Density::Compact));
        assert_eq!(config.terminal.size, 19.);
        assert_eq!(config.terminal.fallbacks, Some(vec![]));
        assert!(config.notifications.enabled);
        assert_eq!(config.notifications.delay_seconds, 1);
        assert_eq!(config.theme, "Default");
        assert_eq!(fs::read_to_string(&local)?, overrides);
        assert_eq!(fs::read_to_string(&path)?, DEFAULT_CONFIG);
        // A table can replace the named default without losing layout defaults.
        fs::write(&local, "[layout]\nmode = 'compact'\nsidebar_gap = 3")?;
        assert_eq!(Config::load_path(&path, &daemon)?.layout.sidebar_gap, 3.);
        let merged = Config::parse_layers(
            [
                "[terminal]\nfallback = ['first', 'second']",
                "[terminal]\nfallback = []",
            ],
            &Daemon::default(),
        )?;
        assert_eq!(merged.terminal.fallbacks, Some(vec![]));
        Ok(())
    }

    #[test]
    fn managed_headers_accept_lf_and_crlf_without_migrating_defaults() -> anyhow::Result<()> {
        for newline in ["\n", "\r\n"] {
            for overrides in [None, Some("theme = 'Nord'\r\n")] {
                let temp = TempDirectory::new()?;
                let path = temp.0.join("config-gpui.toml");
                let local = path.with_extension("local.toml");
                let daemon = temp.0.join("absent.toml");
                let managed = format!(
                    "# DO NOT EDIT -- WILL BE OVERWRITTEN{newline}theme = 'Dracula'{newline}"
                );
                fs::write(&path, &managed)?;
                if let Some(text) = overrides {
                    fs::write(&local, text)?;
                }
                let expected_theme = if overrides.is_some() {
                    "Nord"
                } else {
                    "Default"
                };
                assert_eq!(
                    Config::load_startup_path(&path, &daemon)?.theme,
                    expected_theme
                );
                assert_eq!(fs::read_to_string(&path)?, managed);
                assert_eq!(local.exists(), overrides.is_some());
                for _ in 0..2 {
                    assert_eq!(Config::load_path(&path, &daemon)?.theme, expected_theme);
                    assert_eq!(fs::read_to_string(&path)?, DEFAULT_CONFIG);
                    assert_eq!(
                        fs::read_to_string(&local)?,
                        overrides.unwrap_or(LOCAL_CONFIG)
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn legacy_config_migrates_verbatim_and_conflicts_never_overwrite() -> anyhow::Result<()> {
        let temp = TempDirectory::new()?;
        let path = temp.0.join("config-gpui.toml");
        let local = path.with_extension("local.toml");
        let daemon = temp.0.join("absent.toml");
        // A longer comment is not the exact managed marker. Preserve CRLF too.
        let legacy = "# DO NOT EDIT -- WILL BE OVERWRITTEN (personal copy)\r\ntheme = 'Nord'\r\n[layout]\r\nsidebar_gap = 4\r\n";
        fs::write(&path, legacy)?;
        assert_eq!(Config::load_path(&path, &daemon)?.theme, "Nord");
        assert_eq!(fs::read_to_string(&local)?, legacy);
        assert_eq!(fs::read_to_string(&path)?, DEFAULT_CONFIG);
        assert_eq!(Config::load_path(&path, &daemon)?.layout.sidebar_gap, 4.);
        // A crash after the local copy but before refresh is safe to resume.
        fs::write(&path, legacy)?;
        assert_eq!(Config::load_path(&path, &daemon)?.theme, "Nord");
        assert_eq!(fs::read_to_string(&local)?, legacy);
        assert_eq!(fs::read_to_string(&path)?, DEFAULT_CONFIG);
        fs::write(&path, "theme = 'Dracula'")?;
        assert!(matches!(
            Config::load_path(&path, &daemon),
            Err(Error::ConfigMigrationConflict { .. })
        ));
        assert_eq!(fs::read_to_string(&local)?, legacy);
        assert_eq!(fs::read_to_string(&path)?, "theme = 'Dracula'");
        Ok(())
    }

    #[test]
    fn unknown_keys_are_ignored_and_reported() -> anyhow::Result<()> {
        // What a newer build might write: every key it knows still applies.
        let config = Config::parse(
            "future = 1\ntheme = 'Nord'\n[future_table]\nx = 1\n\
             [terminal]\nsize = 18\nligatures = true\n\
             [notifications]\nenabled = true\nsound = 'ping'\n\
             [clipboard_toast]\nduration = 3\n[features]\nnew_flag = true\n\
             [github]\nenterprise = 'x'\n[layout]\nsidebar_gap = 4\nshadow = true\n\
             [usage.providers.future]\ntoken = 'y'",
        )?;
        assert_eq!(config.theme, "Nord");
        assert_eq!(config.terminal.size, 18.);
        assert!(config.notifications.enabled);
        assert_eq!(config.layout.sidebar_gap, 4.);
        assert_eq!(
            config.unknown_keys,
            [
                "clipboard_toast.duration",
                "features.new_flag",
                "future",
                "future_table",
                "github.enterprise",
                "layout.shadow",
                "notifications.sound",
                "terminal.ligatures",
                "usage.providers.future",
            ]
        );
        assert_eq!(
            config.diagnostic().as_deref(),
            Some(
                "config-gpui.local.toml: ignoring unknown keys clipboard_toast.duration, \
                 features.new_flag, future, future_table, github.enterprise and 4 more"
            )
        );
        assert_eq!(Config::parse("")?.diagnostic(), None);
        assert_eq!(
            Config::parse("[sidebar]\nnope = 1")?
                .diagnostic()
                .as_deref(),
            Some("config-gpui.local.toml: ignoring unknown keys sidebar.nope")
        );
        // The managed defaults layered underneath name no unknown keys.
        assert!(
            Config::parse_layers([DEFAULT_CONFIG], &Daemon::default())?
                .unknown_keys
                .is_empty()
        );
        // Credentials stay refused rather than ignored.
        for name in ["client_secret", "private_key", "token"] {
            assert!(matches!(
                Config::parse(&format!("[github]\n{name} = 'x'")),
                Err(Error::GitHubSecretInConfig(found)) if found == name
            ));
        }

        // Loading from disk keeps going too, and leaves the file alone.
        let temp = TempDirectory::new()?;
        let path = temp.0.join("config-gpui.toml");
        let local = path.with_extension("local.toml");
        let daemon = temp.0.join("absent.toml");
        Config::load_path(&path, &daemon)?;
        let text = "theme = 'Dracula'\n[notifications]\nunknown = true\n";
        fs::write(&local, text)?;
        let loaded = Config::load_path(&path, &daemon)?;
        assert_eq!(loaded.theme, "Dracula");
        assert_eq!(loaded.unknown_keys, ["notifications.unknown"]);
        assert_eq!(Config::load_startup_path(&path, &daemon)?.theme, "Dracula");
        assert_eq!(fs::read_to_string(&local)?, text);
        Ok(())
    }

    #[test]
    fn invalid_local_overrides_keep_their_path_and_contents() -> anyhow::Result<()> {
        let temp = TempDirectory::new()?;
        let path = temp.0.join("config-gpui.toml");
        let local = path.with_extension("local.toml");
        let daemon = temp.0.join("absent.toml");
        Config::load_path(&path, &daemon)?;
        for text in [
            "theme = [",
            "[terminal]\nsize = '19'",
            "[notifications]\nenabled = 1",
        ] {
            fs::write(&local, text)?;
            let error = Config::load_path(&path, &daemon)
                .err()
                .context("accepted bad local config")?;
            assert!(
                matches!(&error, Error::Path { path, source } if path == &local
                    && matches!(source.as_ref(), Error::ConfigFile { .. } | Error::Toml(_))),
                "{error:?}"
            );
            assert_eq!(fs::read_to_string(&local)?, text);
        }
        Ok(())
    }

    #[test]
    fn simultaneous_migration_keeps_user_settings() -> anyhow::Result<()> {
        let temp = TempDirectory::new()?;
        let path = temp.0.join("config-gpui.toml");
        fs::write(&path, "theme = 'Nord'")?;
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..4)
                .map(|_| scope.spawn(|| Config::load_path(&path, &temp.0.join("absent.toml"))))
                .collect();
            for handle in handles {
                let config = handle
                    .join()
                    .map_err(|_| anyhow::anyhow!("config loader panicked"))??;
                assert_eq!(config.theme, "Nord");
            }
            anyhow::Ok(())
        })?;
        assert_eq!(
            fs::read_to_string(path.with_extension("local.toml"))?,
            "theme = 'Nord'"
        );
        Ok(())
    }

    #[test]
    fn refreshes_managed_config_and_loads_absolute_theme() -> anyhow::Result<()> {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let directory =
            env::temp_dir().join(format!("herdr-config-{}-{unique}", std::process::id()));
        let path = directory.join("config-gpui.toml");
        let result = (|| {
            let absent = directory.join("config.toml");
            Config::load_path(&path, &absent)?;
            assert_eq!(fs::read_to_string(&path)?, DEFAULT_CONFIG);
            let local = path.with_extension("local.toml");
            fs::write(&local, "theme = 'Nord'")?;
            fs::write(&path, format!("{MANAGED_HEADER}\ntheme = 'Dracula'"))?;
            assert_eq!(Config::load_path(&path, &absent)?.theme, "Nord");
            assert_eq!(fs::read_to_string(&path)?, DEFAULT_CONFIG);
            assert_eq!(fs::read_to_string(&local)?, "theme = 'Nord'");
            let theme_path = directory.join("custom-theme");
            fs::write(&theme_path, "background=112233")?;
            let config = Config {
                theme: theme_path.to_string_lossy().into_owned(),
                ..Config::default()
            };
            assert_eq!(config.theme()?.background, 0x112233);
            Ok(())
        })();
        fs::remove_dir_all(directory)?;
        result
    }

    /// Installed families as macOS reports them, in arbitrary order.
    fn installed() -> Vec<String> {
        [
            "Menlo",
            "Zapfino",
            "JetBrainsMono Nerd Font Propo",
            "Symbols Nerd Font",
            "Hack Nerd Font Mono",
            "Symbols Nerd Font Mono",
            "Agave Nerd Font Mono",
            "Hack Nerd Font Mono",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }

    #[test]
    fn detection_ranks_symbol_and_mono_faces_and_ignores_text_families() {
        // Symbols first, then single-cell Mono faces, alphabetical within each
        // rank, deduplicated, and capped so the cascade stays short.
        assert_eq!(
            symbol_fallbacks(installed()),
            [
                "Symbols Nerd Font Mono",
                "Symbols Nerd Font",
                "Agave Nerd Font Mono",
            ]
        );
        assert!(symbol_fallbacks(["Menlo".to_owned(), "Zapfino".to_owned()]).is_empty());
    }

    #[test]
    fn detection_fills_only_the_faces_the_config_left_alone() -> anyhow::Result<()> {
        let mut config = Config::parse("[terminal]\nfallback = ['Menlo']\n[ui]\nfallback = []")?;
        config.resolve_font_fallbacks(installed);
        assert_eq!(
            config.terminal.fallbacks.as_deref(),
            Some(["Menlo".to_owned()].as_slice())
        );
        // An explicit empty list opts out; it is not "unset".
        assert_eq!(config.ui.fallbacks.as_deref(), Some([].as_slice()));
        assert_eq!(config.ui.font().fallbacks, None);
        let detected = symbol_fallbacks(installed());
        assert_eq!(
            config.sidebar.fallbacks.as_deref(),
            Some(detected.as_slice())
        );
        assert_eq!(config.tabs.fallbacks.as_deref(), Some(detected.as_slice()));
        Ok(())
    }

    #[test]
    fn detection_does_not_enumerate_fonts_when_every_face_is_configured() -> anyhow::Result<()> {
        // Enumerating installed families is slow, so a fully configured file
        // must not pay for it.
        let mut config = Config::parse(
            "[sidebar]\nfallback = []\n[tabs]\nfallback = []\n\
             [terminal]\nfallback = []\n[ui]\nfallback = []",
        )?;
        config.resolve_font_fallbacks(|| -> Vec<String> { panic!("enumerated installed fonts") });
        Ok(())
    }

    #[test]
    fn configured_fallbacks_reach_the_shaping_font_in_order() -> anyhow::Result<()> {
        let config = Config::parse(
            "[terminal]\nfallback = ['Symbols Nerd Font Mono', 'Hack Nerd Font Mono']",
        )?;
        let font = config.terminal.font();
        assert_eq!(font.family, config.terminal.family);
        let fallbacks = font
            .fallbacks
            .ok_or_else(|| anyhow::anyhow!("missing cascade"))?;
        assert_eq!(
            fallbacks.fallback_list(),
            ["Symbols Nerd Font Mono", "Hack Nerd Font Mono"]
        );
        // The default face shapes without a cascade until one is resolved.
        assert_eq!(Config::default().terminal.font().fallbacks, None);
        Ok(())
    }

    #[test]
    fn fallback_lists_are_validated_per_face() {
        assert!(matches!(
            Config::parse("[terminal]\nfallback = ['Menlo', '  ']"),
            Err(Error::EmptyFontFallback("terminal"))
        ));
        let list = |count: usize| {
            (0..count)
                .map(|index| format!("'face{index}'"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        assert!(matches!(
            Config::parse(&format!(
                "[sidebar]\nfallback = [{}]",
                list(MAX_FONT_FALLBACKS + 1)
            )),
            Err(Error::TooManyFontFallbacks("sidebar"))
        ));
        assert!(
            Config::parse(&format!(
                "[sidebar]\nfallback = [{}]",
                list(MAX_FONT_FALLBACKS)
            ))
            .is_ok()
        );
    }
}
