//! GUI settings. The daemon's own config is read only where the GUI honors a
//! preference the user already expressed there, never written and never used
//! to change daemon behavior. Managed defaults are refreshed from the binary;
//! `config-gpui.local.toml` holds persistent user overrides.
use crate::{
    Error, Result,
    contrast::Contrast,
    keymap::{Binding, DaemonKeys, Keymap, PaneKeys},
};
mod accent;
mod agents_placement;
mod azure;
pub(crate) mod backdrop;
mod bitmap_fonts;
mod boat;
pub(crate) mod chat;
mod cloud;
mod coder;
mod container;
mod daemon;
mod daytona;
mod devices;
mod edit;
mod features;
mod files;
mod font_weight;
mod fonts;
mod github;
mod keybindings;
mod layout;
mod notifications;
mod omarchy;
mod opensandbox;
mod option_as_alt;
mod padding;
pub(crate) mod preferences;
pub(crate) use herdr_remote_control::app::config as remote;
pub(crate) mod sidebar;
mod sidebar_style;
mod sprites;
pub(crate) mod status_bar;
mod status_icons;
pub(crate) mod tab_layout;
#[cfg(any(
    feature = "azure",
    feature = "boat",
    feature = "coder",
    feature = "container",
    feature = "daytona",
    feature = "opensandbox",
    feature = "sprites"
))]
mod table;
pub(crate) use sidebar_style::parse_hex;
mod theme;
mod updates;
pub(crate) mod watch;
mod worktrees;
mod zen;

pub use accent::Accent;
pub use agents_placement::AgentsPlacement;
pub use backdrop::{Backdrop, Material};
pub use chat::ChatLayoutMode;
pub use cloud::CloudConfig;
#[cfg(test)]
use daemon::MAX_DAEMON_CONFIG_BYTES;
pub(crate) use daemon::daemon_config_path;
use daemon::{Daemon, daemon_settings};
use devices::MAX_DEVICES;
pub(crate) use devices::{DeviceSettings, KeybindingSource};
pub(crate) use edit::{Layer, parse_value};
pub use features::{Feature, Features};
use files::write_config;
pub use font_weight::bold as bold_font_weight;
pub(crate) use fonts::FontFace;
use fonts::FontSettings;
#[cfg(any(test, feature = "integration-test"))]
pub use fonts::symbol_fallbacks;
pub use fonts::{FONT_SIZE_RANGE, FONT_SIZE_STEP, FontConfig};
pub(crate) use github::MAX_ACCOUNTS;
pub use github::{GitHubAccount, GitHubConfig};
pub use herdr_remote::config::AzureConfig;
#[cfg(feature = "azure")]
pub(crate) use herdr_remote::config::AzureFields;
pub use herdr_remote::config::BoatConfig;
#[cfg(feature = "boat")]
pub(crate) use herdr_remote::config::BoatFields;
pub use herdr_remote::config::CoderConfig;
#[cfg(feature = "coder")]
pub(crate) use herdr_remote::config::CoderFields;
pub use herdr_remote::config::ContainerConfig;
#[cfg(feature = "container")]
pub(crate) use herdr_remote::config::ContainerFields;
pub use herdr_remote::config::DaytonaConfig;
#[cfg(feature = "daytona")]
pub(crate) use herdr_remote::config::DaytonaFields;
pub use herdr_remote::config::OpenSandboxConfig;
#[cfg(feature = "opensandbox")]
pub(crate) use herdr_remote::config::OpenSandboxFields;
pub use herdr_remote::config::SpritesConfig;
#[cfg(feature = "sprites")]
pub(crate) use herdr_remote::config::SpritesFields;
pub use layout::{Density, Layout, LayoutMode, Style};
pub(crate) use notifications::NotificationDelivery;
pub use notifications::{
    BellConfig, ClipboardToast, ClipboardToastPosition, NotificationConfig, SoundConfig,
};
use notifications::{ClipboardToastSettings, NotificationSettings};
pub use option_as_alt::{OptionAsAlt, OptionKeys};
pub use padding::Padding;
pub(crate) use padding::{Axis, MAX_PADDING, Side, Sides};
pub use remote::RemoteConfig;
use serde::Deserialize;
pub(crate) use sidebar::{
    AgentLayout, AgentToken, MAX_ROWS as MAX_SIDEBAR_ROWS, Rows, SidebarConfigError, SidebarLayout,
    SidebarScope, SpaceLayout, SpaceToken, TokenStyle,
};
pub(crate) use sidebar_style::{ChangeCounts, DeviceFooter};
pub use sidebar_style::{SelectMode, SidebarOverrides, SidebarStyle};
pub use status_bar::StatusBar;
pub use status_icons::StatusIcons;
use std::{
    collections::BTreeMap,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};
pub use tab_layout::TabLayoutMode;
pub use theme::Theme;
pub(crate) use theme::ThemeName;
pub(crate) use theme::mix;
pub use updates::{UpdateChannel, UpdatesConfig};
pub use zen::{Zen, ZenWidth};

const DEFAULT_CONFIG: &str = include_str!("../config-gpui.example.toml");
// Compare the first line so Windows checkouts and editors can use CRLF.
const MANAGED_HEADER: &str = "# DO NOT EDIT -- WILL BE OVERWRITTEN";
/// Seeds the overrides file on first launch only. Existing overrides and
/// migrated personal configs are never rewritten, so settings placed here
/// reach new installs without changing what current users see.
const LOCAL_CONFIG: &str = "# Herdr UI overrides. Saved changes reload automatically.\n# Unset keys inherit config-gpui.toml; tables merge key by key.\n\n# New installs start with the roomy rounded sidebar. Remove this line for\n# the managed default, or pick another layout listed in config-gpui.toml.\nlayout = \"comfortable-rounded\"\n";

/// Shared logical-pixel radii for native-style chrome, independent of the
/// terminal grid. Small badges/keycaps retain a tighter curve than controls.
pub(crate) mod corners {
    pub(crate) const PANEL: f32 = 12.;
    pub(crate) const CONTROL: f32 = 8.;
    pub(crate) const SMALL: f32 = 4.;
}

#[derive(Clone, Debug)]
pub struct Config {
    // keep-sorted start
    /// Where the app's own accent color comes from.
    pub accent: Accent,
    /// Snapshot a checkout's files each time one of its agents starts or
    /// finishes a turn, so they can be rolled back.
    pub agent_checkpoints: bool,
    /// Where the sidebar lists agents while `show_agents` is on.
    pub agents_placement: AgentsPlacement,
    /// What shows behind the main window's terminals and chrome.
    pub azure: AzureConfig,
    pub backdrop: Backdrop,
    pub bell: BellConfig,
    pub boat: BoatConfig,
    /// What chats started from the app run with.
    pub chat: chat::ChatConfig,
    /// Only the keys this GUI's own file sets, so Settings can tell an
    /// override from the Herdr value it shadows.
    pub(crate) clipboard_overrides: ClipboardToastSettings,
    pub clipboard_toast: ClipboardToast,
    /// What a new cloud device runs the first time it connects.
    pub cloud: CloudConfig,
    pub code: CodeConfig,
    pub coder: CoderConfig,
    pub confirm_close_pane: bool,
    pub confirm_close_tab: bool,
    /// Whether a workspace close shows its dialog when the check finds no
    /// unpublished work. Uncommitted or unpushed work always asks.
    pub confirm_close_workspace: bool,
    /// How new containers on this computer's Docker or Podman engine are made.
    pub container: ContainerConfig,
    /// How far the app's own marks and labels stand off its chrome.
    pub contrast: Contrast,
    pub daytona: DaytonaConfig,
    /// Per saved device, by catalog profile ID.
    pub(crate) devices: BTreeMap<String, DeviceSettings>,
    /// A bar on the leading edge of each working, blocked, or done row,
    /// whichever status icons are drawn.
    pub edge_rail: bool,
    /// How the editor starts; the pane's `$VISUAL` or `$EDITOR` when unset.
    pub(crate) editor_command: Option<crate::editor::EditorCommand>,
    pub features: Features,
    /// Focus the split terminal pane under the pointer without clicking.
    pub focus_follows_mouse: bool,
    pub github: GitHubConfig,
    /// Font resolution searched for an icon font for the terminal and found
    /// none installed, so Private Use Area glyphs draw as missing-glyph boxes.
    /// Never set when the config names the terminal's `fallback` itself.
    pub(crate) icon_font_missing: bool,
    /// Whether a terminal selection stays highlighted, and readable by
    /// selection tools, after it is copied.
    pub keep_selection_after_copy: bool,
    /// The `[keybindings]` table `keybindings` was built from, kept so a
    /// device's server keys can be layered under the same GUI overrides.
    pub(crate) keybinding_overrides: BTreeMap<String, Binding>,
    pub keybindings: Keymap,
    pub layout: Layout,
    /// Configured families, sorted, that font resolution found no installed
    /// family for; `diagnostic` reports them.
    pub(crate) missing_fonts: Vec<String>,
    /// The terminal's configured weight, when its family has no face at it.
    pub(crate) missing_weight: Option<font_weight::MissingWeight>,
    pub(crate) notification_overrides: NotificationSettings,
    pub notifications: NotificationConfig,
    /// Where a link-modifier click on a printed file path opens it.
    pub open_files_in: FileTarget,
    pub open_links_in: LinkTarget,
    /// The self-hosted OpenSandbox server whose sandboxes are devices.
    pub opensandbox: OpenSandboxConfig,
    pub option_as_alt: OptionAsAlt,
    pub palette: crate::palette::PaletteConfig,
    /// The `[pane_keys]` table `keybindings` was built from, for the same
    /// reason.
    pub(crate) pane_keys: PaneKeys,
    pub(crate) px0: crate::px0::Config,
    /// Remote control from a phone, through the `herdr-remote-control` daemon.
    pub remote: RemoteConfig,
    /// Whether a linked worktree whose pull request is merged is removed, and
    /// its workspace closed, without asking. The focused workspace still asks.
    pub remove_merged_worktrees: bool,
    pub show_agents: bool,
    /// The "Agents" title row above the agents list, which also holds the sort
    /// toggle. The sort then follows Herdr's `ui.agent_panel_sort` unless it
    /// was toggled earlier, unless a plugin sets the agent view, which keeps the
    /// header visible. Set in Settings > Sidebar or with
    /// `herdr-gpui config set show_agents_header false`.
    pub show_agents_header: bool,
    /// The mark before a device's name in the sidebar, in the layouts that
    /// draw one: where the device runs and how it is reached.
    pub show_device_icons: bool,
    /// Ports each workspace listens on, in the sidebar and the status bar.
    pub show_listening_ports: bool,
    /// Latency and loss from this machine, by timed TCP connects, in the status bar.
    pub show_network_quality: bool,
    /// The repository owner's avatar, or the GitHub mark, before a space's name.
    pub show_repository_icons: bool,
    /// The search field at the top of the sidebar's spaces list.
    pub show_sidebar_search: bool,
    /// CPU and memory of the selected host in the status bar.
    pub show_system_load: bool,
    /// Draw minimize, maximize, and close buttons in the menu bar on Linux.
    /// When true the window needs no title bar from the window manager.
    pub show_window_controls: bool,
    pub sidebar: FontConfig,
    /// Daemon sidebar rows, falling back to defaults when invalid.
    pub sidebar_layout: SidebarLayout,
    /// Spacing overrides, host colours, and selection marking for the sidebar.
    pub sidebar_style: SidebarStyle,
    /// Wheel scrollback draws panes between rows, following the OS deltas.
    pub smooth_scroll: bool,
    /// How loud agent notification sounds play.
    pub sound: SoundConfig,
    /// The Fly Sprites account whose sprites are devices.
    pub sprites: SpritesConfig,
    /// Which status bar items show, and how compactly.
    pub status_bar: StatusBar,
    /// The set agent status is drawn with.
    pub status_icons: StatusIcons,
    /// How tab strips draw their tabs, from View > Tab Layout.
    pub tab_layout: TabLayoutMode,
    pub tabs: FontConfig,
    pub terminal: FontConfig,
    /// Blank space around every terminal's cells.
    pub terminal_padding: Padding,
    pub theme: String,
    pub ui: FontConfig,
    /// Keys the file names that this build does not know, sorted. They are
    /// ignored, as Herdr ignores its own, so a config written by a newer
    /// build or with a typo still loads; `diagnostic` reports them.
    pub unknown_keys: Vec<String>,
    /// Which releases the in-app updater offers.
    pub updates: UpdatesConfig,
    pub usage: crate::usage::UsageConfig,
    /// How linked worktrees are marked apart from their main checkout.
    pub worktrees: crate::worktree_identity::Worktrees,
    /// What Toggle Zen Mode does beyond hiding the chrome.
    pub zen: Zen,
    // keep-sorted end
}

/// Where a link-modifier click on a terminal's web link opens it. Holding
/// Alt (Option on macOS) as well opens it in the other one.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum LinkTarget {
    #[default]
    System,
    /// A browser tab in the workspace, where the build can show pages.
    BrowserTab,
}

/// VS Code as a tab of a space's editor groups, served by
/// `code serve-web`. Each space that shows it gets
/// its own page, which starts at the server's address and then goes
/// wherever it navigates.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct CodeConfig {
    /// Where the server comes from. Unset, it is `address` when `url` is
    /// set, as before there was a choice, else `start` once VS Code is found.
    pub(crate) mode: Option<CodeMode>,
    /// The server to use in `address` mode. `None` leaves the panel empty,
    /// with a hint to set it.
    pub(crate) url: Option<crate::browser::WebUrl>,
    /// The port of the server the app starts, picked once and kept: VS Code
    /// keeps its settings in the storage of the page's origin.
    pub(crate) port: Option<std::num::NonZeroU16>,
    /// The user accepted VS Code's server license in the app. Until then
    /// the app starts nothing, since it would accept it for them.
    pub(crate) license_accepted: bool,
}

/// Where VS Code tabs' server comes from.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CodeMode {
    /// The app starts `code serve-web` itself.
    Start,
    /// A server the user runs, at `url`.
    Address,
}

/// One change to the `[code]` table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CodeEdit {
    /// Sets the address, or removes it with `None`.
    Url(Option<crate::browser::WebUrl>),
    Mode(CodeMode),
    Port(std::num::NonZeroU16),
    AcceptLicense,
}

/// Where a clicked file path opens. Alt-click (Option on macOS) opens it in
/// the other one.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FileTarget {
    /// The terminal editor, in a pane beside the one that printed the path.
    /// Folders, images, and PDFs still open in the system's application.
    #[default]
    Editor,
    /// The system's default application.
    System,
}

impl Default for Config {
    fn default() -> Self {
        let fonts::DefaultFonts {
            monospace,
            sans: ui,
            ..
        } = fonts::PLATFORM_FONTS;
        let font = |family: &str, size| FontConfig {
            family: family.into(),
            size,
            fallbacks: None,
            line_height_multiple: None,
            weight: gpui::FontWeight::NORMAL,
        };
        Self {
            // keep-sorted start
            accent: Accent::default(),
            agent_checkpoints: true,
            agents_placement: AgentsPlacement::default(),
            azure: AzureConfig::default(),
            backdrop: Backdrop::default(),
            bell: BellConfig::default(),
            boat: BoatConfig::default(),
            chat: chat::ChatConfig::default(),
            clipboard_overrides: ClipboardToastSettings::default(),
            clipboard_toast: ClipboardToast::default(),
            cloud: CloudConfig::default(),
            code: CodeConfig::default(),
            coder: CoderConfig::default(),
            confirm_close_pane: true,
            confirm_close_tab: true,
            confirm_close_workspace: true,
            container: ContainerConfig::default(),
            contrast: Contrast::default(),
            daytona: DaytonaConfig::default(),
            devices: BTreeMap::new(),
            edge_rail: false,
            editor_command: None,
            features: Features::default(),
            focus_follows_mouse: true,
            github: GitHubConfig::default(),
            icon_font_missing: false,
            keep_selection_after_copy: true,
            keybinding_overrides: BTreeMap::new(),
            keybindings: Keymap::default(),
            layout: Layout::default(),
            missing_fonts: Vec::new(),
            missing_weight: None,
            notification_overrides: NotificationSettings::default(),
            notifications: NotificationConfig::default(),
            open_files_in: FileTarget::default(),
            open_links_in: LinkTarget::default(),
            opensandbox: OpenSandboxConfig::default(),
            option_as_alt: OptionAsAlt::default(),
            palette: crate::palette::PaletteConfig::default(),
            pane_keys: PaneKeys::new(),
            px0: crate::px0::Config::default(),
            remote: RemoteConfig::default(),
            remove_merged_worktrees: false,
            show_agents: true,
            show_agents_header: true,
            show_device_icons: true,
            show_listening_ports: true,
            show_network_quality: false,
            show_repository_icons: true,
            show_sidebar_search: true,
            show_system_load: true,
            show_window_controls: cfg!(target_os = "linux"),
            sidebar: font(monospace, 12.0),
            sidebar_layout: SidebarLayout::default(),
            sidebar_style: SidebarStyle::default(),
            smooth_scroll: true,
            sound: SoundConfig::default(),
            sprites: SpritesConfig::default(),
            status_bar: StatusBar::default(),
            status_icons: StatusIcons::default(),
            tab_layout: TabLayoutMode::default(),
            // Tabs are terminal chrome, so they read in the monospace face the
            // sidebar and terminal use, as they do in the reference UI.
            tabs: font(monospace, 12.0),
            terminal: font(monospace, 14.0),
            terminal_padding: Padding::default(),
            theme: "Default".into(),
            ui: font(ui, 12.0),
            unknown_keys: Vec::new(),
            updates: UpdatesConfig::default(),
            usage: crate::usage::UsageConfig::default(),
            worktrees: crate::worktree_identity::Worktrees::default(),
            zen: Zen::default(),
            // keep-sorted end
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Settings {
    // keep-sorted start
    accent: Accent,
    agent_checkpoints: Option<bool>,
    agents_placement: AgentsPlacement,
    azure: AzureConfig,
    backdrop: Backdrop,
    bell: BellConfig,
    boat: BoatConfig,
    chat: chat::ChatConfig,
    clipboard_toast: ClipboardToastSettings,
    cloud: CloudConfig,
    code: CodeConfig,
    coder: CoderConfig,
    confirm_close_pane: Option<bool>,
    confirm_close_tab: Option<bool>,
    confirm_close_workspace: Option<bool>,
    container: ContainerConfig,
    contrast: Contrast,
    daytona: DaytonaConfig,
    devices: BTreeMap<String, DeviceSettings>,
    edge_rail: Option<bool>,
    editor_command: Option<crate::editor::EditorCommand>,
    features: Features,
    focus_follows_mouse: Option<bool>,
    github: GitHubConfig,
    keep_selection_after_copy: Option<bool>,
    keybindings: BTreeMap<String, Binding>,
    layout: Layout,
    notifications: NotificationSettings,
    open_files_in: FileTarget,
    open_links_in: LinkTarget,
    opensandbox: OpenSandboxConfig,
    option_as_alt: OptionAsAlt,
    palette: crate::palette::PaletteConfig,
    pane_keys: PaneKeys,
    px0: crate::px0::Config,
    remote: RemoteConfig,
    remove_merged_worktrees: Option<bool>,
    show_agents: Option<bool>,
    show_agents_header: Option<bool>,
    show_device_icons: Option<bool>,
    show_listening_ports: Option<bool>,
    show_network_quality: Option<bool>,
    show_repository_icons: Option<bool>,
    show_sidebar_search: Option<bool>,
    show_system_load: Option<bool>,
    show_window_controls: Option<bool>,
    sidebar: sidebar_style::SidebarSettings,
    smooth_scroll: Option<bool>,
    sound: SoundConfig,
    sprites: SpritesConfig,
    status_bar: StatusBar,
    status_icons: Option<status_icons::SavedIcons>,
    tab_layout: TabLayoutMode,
    tabs: FontSettings,
    terminal: FontSettings,
    theme: Option<String>,
    ui: FontSettings,
    updates: UpdatesConfig,
    usage: crate::usage::UsageConfig,
    worktrees: worktrees::WorktreeSettings,
    zen: zen::ZenSettings,
    // keep-sorted end
}

/// Windows sets `USERPROFILE` rather than `HOME`, and upstream Herdr reads both.
pub(crate) fn home() -> Result<PathBuf> {
    herdr_client::local_path::home_dir().ok_or(Error::MissingHome)
}

/// The directory holding this app's `herdr` configuration directory. Upstream
/// Herdr puts it under `%APPDATA%` on Windows, and the GUI config lives beside
/// the daemon's, so the same root has to be used on both sides, including the
/// login shell's `XDG_CONFIG_HOME` for a launch that did not inherit it.
fn config_root() -> Result<PathBuf> {
    use herdr_client::local_path::PathError;
    herdr_client::local_path::config_root().map_err(|error| match error {
        PathError::MissingHome => Error::MissingHome,
        PathError::RelativeConfigRoot => Error::RelativeConfigRoot,
    })
}

impl Config {
    /// The provider tables `herdr-remote` reads, from a full load.
    pub(crate) fn load_remote()
    -> std::result::Result<herdr_remote::config::Sections, herdr_remote::config::LoadError> {
        let config = Self::load()?;
        Ok(herdr_remote::config::Sections {
            azure: config.azure,
            boat: config.boat,
            coder: config.coder,
            container: config.container,
            daytona: config.daytona,
            opensandbox: config.opensandbox,
            sprites: config.sprites,
        })
    }

    pub fn path() -> Result<PathBuf> {
        Ok(config_root()?.join("herdr/config-gpui.toml"))
    }

    pub fn local_path() -> Result<PathBuf> {
        Ok(Self::path()?.with_extension("local.toml"))
    }

    /// Warnings naming the keys this build ignored and the configured fonts
    /// that are not installed, one line each, if any.
    pub(crate) fn diagnostic(&self) -> Option<String> {
        const LISTED: usize = 5;
        let list = |items: &[String]| {
            let listed = items[..items.len().min(LISTED)].join(", ");
            match items.len().saturating_sub(LISTED) {
                0 => listed,
                more => format!("{listed} and {more} more"),
            }
        };
        let mut lines = Vec::new();
        if !self.unknown_keys.is_empty() {
            lines.push(format!(
                "config-gpui.local.toml: ignoring unknown keys {}",
                list(&self.unknown_keys)
            ));
        }
        if self.layout.legacy_sidebar_gap.is_some() {
            lines.push(
                "config-gpui.local.toml: layout.sidebar_gap is deprecated; use terminal.padding_x"
                    .to_owned(),
            );
        }
        if !self.missing_fonts.is_empty() {
            let fonts: Vec<String> = self
                .missing_fonts
                .iter()
                .map(|family| format!("\"{family}\""))
                .collect();
            lines.push(format!("Configured fonts not installed: {}", list(&fonts)));
        }
        if let Some(missing) = &self.missing_weight {
            lines.push(format!(
                "\"{}\" has no {} face; using its nearest weight",
                missing.family,
                font_weight::describe(missing.weight)
            ));
        }
        (!lines.is_empty()).then(|| lines.join("\n"))
    }

    pub fn load() -> Result<Self> {
        Self::load_path(&Self::path()?, &daemon_config_path(herdr_client::local_var))
    }

    /// First-frame settings only: no lock, migration, writes, or fsync. The
    /// background load performs maintenance after the window has appeared.
    pub(crate) fn load_startup() -> Result<Self> {
        Self::load_startup_path(&Self::path()?, &daemon_config_path(herdr_client::local_var))
    }

    fn load_startup_path(path: &Path, daemon: &Path) -> Result<Self> {
        let (text, source) = Self::overrides_text(path)?;
        Self::parse_layers([DEFAULT_CONFIG, &text], &daemon_settings(daemon))
            .map_err(|error| error.at_path(&source))
            .map(|config| config.with_desktop_theme(&text))
    }

    /// The user's overrides as the app reads them now, and the file they
    /// come from, without writing anything.
    pub(super) fn overrides_text(path: &Path) -> Result<(String, PathBuf)> {
        let local = path.with_extension("local.toml");
        Ok(match fs::read_to_string(&local) {
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
        })
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
        Self::parse_layers([DEFAULT_CONFIG, &text], &base)
            .map_err(|error| error.at_path(&local))
            .map(|config| config.with_desktop_theme(&text))
    }

    /// Without a home directory there is no desktop theme to find, and the
    /// managed default stays.
    fn with_desktop_theme(mut self, overrides: &str) -> Self {
        if let Ok(home) = home() {
            self.follow_omarchy(overrides, &home);
        }
        self
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

    /// The files layered as loading layers them, later ones over earlier.
    fn merged<'a>(texts: impl IntoIterator<Item = &'a str>) -> Result<toml::Value> {
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
        Ok(loaded.try_deserialize()?)
    }

    fn parse_layers<'a>(texts: impl IntoIterator<Item = &'a str>, base: &Daemon) -> Result<Self> {
        let value = Self::merged(texts)?;
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
        // Only the terminal grid takes a line height or weight. The other
        // faces draw fixed chrome whose emphasis sets its own weights, so
        // there both are ignored like any other unknown key.
        unknown_keys.extend(
            [
                ("sidebar", &mut settings.sidebar.font),
                ("tabs", &mut settings.tabs),
                ("ui", &mut settings.ui),
            ]
            .into_iter()
            .flat_map(|(name, face)| face.reject_terminal_keys(name)),
        );
        // Unknown keys are ignored, but a credential pasted into the file,
        // `[github]` or one of its accounts, is refused so it is noticed and
        // removed rather than left on disk.
        if let Some(name) = ["client_secret", "private_key", "token"]
            .into_iter()
            .find(|name| {
                unknown_keys.iter().any(|key| {
                    key.strip_prefix("github.")
                        .is_some_and(|rest| rest.rsplit('.').next() == Some(*name))
                })
            })
        {
            return Err(Error::GitHubSecretInConfig(name));
        }
        // A newer build may keep more accounts or owners than this one.
        unknown_keys.extend(settings.github.limit_accounts());
        unknown_keys.extend(settings.cloud.retain_known());
        unknown_keys.sort();
        unknown_keys.dedup();
        let mut config = Self {
            unknown_keys,
            ..Self::default()
        };
        settings.github.client_id_with_override(None)?;
        settings.github.check_accounts()?;
        config.github = settings.github;
        #[cfg(feature = "azure")]
        settings.azure.check()?;
        config.azure = settings.azure;
        #[cfg(feature = "boat")]
        settings.boat.settings_with(|_| None)?;
        config.boat = settings.boat;
        settings.chat.check()?;
        config.chat = settings.chat;
        settings.cloud.check()?;
        config.cloud = settings.cloud;
        // Check what the file says so a bad value is reported at load; the
        // environment may still fill or override keys when Coder is used.
        #[cfg(feature = "coder")]
        settings.coder.check()?;
        config.coder = settings.coder;
        #[cfg(feature = "container")]
        settings.container.settings()?;
        config.container = settings.container;
        #[cfg(feature = "daytona")]
        settings.daytona.settings_with(|_| None)?;
        config.daytona = settings.daytona;
        #[cfg(feature = "opensandbox")]
        settings.opensandbox.settings_with(|_| None)?;
        config.opensandbox = settings.opensandbox;
        #[cfg(feature = "sprites")]
        settings.sprites.settings_with(|_| None)?;
        config.sprites = settings.sprites;
        config.features = settings.features;
        config.notification_overrides = settings.notifications;
        config.notifications = settings
            .notifications
            .resolve(NotificationConfig::default());
        config.clipboard_overrides = settings.clipboard_toast;
        config.clipboard_toast = settings.clipboard_toast.resolve(base.clipboard_toast);
        config.bell = settings.bell;
        config.sound = settings.sound;
        config.updates = settings.updates;
        config.sidebar_layout = base.sidebar_layout.clone();
        let (padding_x, padding_y) = settings.terminal.take_padding();
        config.terminal_padding = Padding::resolve(
            padding_x.as_ref(),
            padding_y.as_ref(),
            settings.layout.legacy_sidebar_gap,
        )?;
        config.layout = settings.layout;
        config.sidebar_style = settings.sidebar.style()?;
        config.keybindings =
            Keymap::with_overrides(&settings.keybindings, &settings.pane_keys, &base.keys)?;
        config.keybinding_overrides = settings.keybindings;
        config.pane_keys = settings.pane_keys;
        if settings.devices.len() > MAX_DEVICES {
            return Err(Error::TooManyDevices(MAX_DEVICES));
        }
        for (id, device) in &settings.devices {
            device.validate(id)?;
        }
        config.devices = settings.devices;
        settings.palette.validate()?;
        config.palette = settings.palette;
        if let Some(theme) = settings.theme {
            if theme.trim().is_empty() {
                return Err(Error::EmptyTheme);
            }
            ThemeName::parse(&theme)?;
            config.theme = theme;
        }
        // keep-sorted start
        config.agent_checkpoints = settings.agent_checkpoints.unwrap_or(true);
        config.confirm_close_pane = settings.confirm_close_pane.unwrap_or(true);
        config.confirm_close_tab = settings.confirm_close_tab.unwrap_or(true);
        config.confirm_close_workspace = settings.confirm_close_workspace.unwrap_or(true);
        config.focus_follows_mouse = settings.focus_follows_mouse.unwrap_or(true);
        config.remove_merged_worktrees = settings.remove_merged_worktrees.unwrap_or(false);
        config.show_agents = settings.show_agents.unwrap_or(true);
        config.show_agents_header = settings.show_agents_header.unwrap_or(true);
        config.show_device_icons = settings.show_device_icons.unwrap_or(true);
        config.show_listening_ports = settings.show_listening_ports.unwrap_or(true);
        config.show_network_quality = settings.show_network_quality.unwrap_or(false);
        config.show_repository_icons = settings.show_repository_icons.unwrap_or(true);
        config.show_sidebar_search = settings.show_sidebar_search.unwrap_or(true);
        config.show_system_load = settings.show_system_load.unwrap_or(true);
        config.show_window_controls = settings
            .show_window_controls
            .unwrap_or(cfg!(target_os = "linux"));
        config.smooth_scroll = settings.smooth_scroll.unwrap_or(true);
        // keep-sorted end
        config.status_bar = settings.status_bar;
        config.tab_layout = settings.tab_layout;
        let (status_icons, legacy_rail) = status_icons::SavedIcons::resolve(settings.status_icons);
        config.status_icons = status_icons;
        config.edge_rail = settings.edge_rail.unwrap_or(legacy_rail);
        config.zen = settings.zen.try_into()?;
        config.worktrees = settings.worktrees.resolve()?;
        settings.backdrop.validate()?;
        config.backdrop = settings.backdrop;
        config.accent = settings.accent;
        config.contrast = settings.contrast;
        config.agents_placement = settings.agents_placement;
        config.usage = settings.usage;
        config.option_as_alt = settings.option_as_alt;
        config.open_links_in = settings.open_links_in;
        config.code = settings.code;
        settings.px0.check()?;
        config.px0 = settings.px0;
        settings.remote.check()?;
        config.remote = settings.remote;
        config.open_files_in = settings.open_files_in;
        config.editor_command = settings.editor_command;
        config.keep_selection_after_copy = settings.keep_selection_after_copy.unwrap_or(true);
        for (name, font, settings) in [
            ("sidebar", &mut config.sidebar, settings.sidebar.font),
            ("tabs", &mut config.tabs, settings.tabs),
            ("terminal", &mut config.terminal, settings.terminal),
            ("ui", &mut config.ui, settings.ui),
        ] {
            settings.apply(name, font)?;
        }
        Ok(config)
    }
}

#[cfg(test)]
mod tests;
