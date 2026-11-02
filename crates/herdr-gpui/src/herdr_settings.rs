//! Shared release-namespace Herdr settings. `load` and `save` do blocking I/O:
//! callers MUST run them on a background worker, never in render/input handlers.
//!
//! Schema/defaults and palette data adapted from https://github.com/herdrdev/herdr
//! revision 8ac9542757292f7a8d42a2d532bc6a8a33c7ffce (Apache-2.0), specifically
//! src/config/{model,sound,theme}.rs, src/app/{state,mod}.rs and
//! src/client/shell.rs. This adaptation
//! uses GPUI RGB colors, strict bounded reads, and targeted optimistic saves.
// i18n: migrated

#[path = "herdr_settings/palette.rs"]
mod palette;
#[path = "herdr_settings/persistence.rs"]
#[cfg(unix)]
mod persistence;
#[cfg(windows)]
#[path = "herdr_settings/persistence_windows.rs"]
mod persistence;
mod remote;
mod sidebar;
#[cfg(test)]
mod tests;

pub(crate) use crate::config::ClipboardToastPosition as ClipboardPosition;
use gpui::Modifiers;
use herdr_client::protocol::AgentStatus;
pub(crate) use herdr_client::protocol::ToastHerdrPosition as ToastPosition;
pub(crate) use remote::RemotePaneHistory;
use serde::{Deserialize, Deserializer};
use std::{env, num::NonZeroU16, path::PathBuf};
use toml_edit::{DocumentMut, Item, Value};

pub(crate) const THEME_NAMES: &[&str] = &[
    "catppuccin",
    "catppuccin-latte",
    "terminal",
    "tokyo-night",
    "tokyo-night-day",
    "dracula",
    "nord",
    "gruvbox",
    "gruvbox-light",
    "one-dark",
    "one-light",
    "solarized",
    "solarized-light",
    "kanagawa",
    "kanagawa-lotus",
    "rose-pine",
    "rose-pine-dawn",
    "vesper",
];

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("shared Herdr config I/O failed")]
    Io(#[from] std::io::Error),
    #[cfg(windows)]
    #[error("saving shared Herdr settings is unsupported on Windows; edit config.toml manually")]
    Unsupported,
    #[cfg(unix)]
    #[error("shared Herdr config was replaced but completion failed; reload before saving again")]
    Committed(#[source] std::io::Error),
    #[error("invalid shared Herdr TOML")]
    Parse(#[from] toml::de::Error),
    #[error("cannot edit shared Herdr TOML")]
    Edit(#[from] toml_edit::TomlError),
    #[cfg(unix)]
    #[error("shared Herdr config changed; reload before saving")]
    Conflict,
    #[cfg(unix)]
    #[error("shared Herdr config is busy; retry saving")]
    Busy,
    #[error(
        "shared Herdr config must be a single-link regular file you own, in directories only you or root can write, reached only through symlinks you or root own"
    )]
    UnsafePath,
    #[cfg(unix)]
    #[error(
        "shared Herdr config path {path:?} is writable by group or others (mode {mode:04o}); remove group/other write permission with chmod go-w on this path, then Reload settings"
    )]
    InsecurePermissions { path: PathBuf, mode: u32 },
    #[error("shared Herdr config exceeds the 1 MiB limit")]
    TooLarge,
    #[error("ui.toast.delay_seconds must be between 0 and 3600")]
    ToastDelay,
    #[error("unknown Herdr theme: {0}")]
    Theme(String),
    #[error("cannot edit non-table config field {0}")]
    Table(&'static str),
    #[error("invalid sidebar token")]
    SidebarToken(#[from] crate::config::SidebarConfigError),
    #[error("could not reach the host's Herdr config")]
    Remote(#[source] herdr_client::Error),
    #[error("the host's Herdr config changed; reload before saving")]
    RemoteConflict,
    #[error("the host's Herdr config is not a regular file; symlink targets are refused")]
    RemoteUnsafePath,
    #[error("the host's Herdr config exceeds the 64 KiB limit for remote edits")]
    RemoteTooLarge,
    #[error("the host's Herdr config is not UTF-8")]
    RemoteUtf8(#[source] std::string::FromUtf8Error),
    #[error("unexpected output reading the host's Herdr config")]
    RemoteOutput,
    #[cfg(unix)]
    #[error("could not remove shared config temporary file: {cleanup}")]
    Cleanup {
        #[source]
        source: Option<Box<Error>>,
        cleanup: std::io::Error,
    },
}

// Keep the concrete source through the existing boxed config-error boundary.
// A dedicated root HerdrSettings(#[from] herdr_settings::Error) is optional.
impl From<Error> for crate::Error {
    fn from(source: Error) -> Self {
        Self::ConfigFile {
            uri: None,
            source: Box::new(source),
        }
    }
}

/// Whether interactive creation asks for a name first, as Herdr's
/// `ui.prompt_new_tab_name` and `ui.prompt_new_workspace_name` decide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NamePrompts {
    pub tab: bool,
    pub workspace: bool,
}

/// Herdr's defaults, also used before the shared config has loaded.
impl Default for NamePrompts {
    fn default() -> Self {
        Self {
            tab: true,
            workspace: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ToastDelivery {
    #[default]
    Off,
    Herdr,
    Terminal,
    System,
}

/// Where the desktop tab row sits, as Herdr's `ui.tab_bar_position` places it.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TabBarPosition {
    #[default]
    Top,
    Bottom,
}

/// What collapsing the sidebar leaves, as Herdr's `ui.sidebar_collapsed_mode`
/// chooses it: a narrow rail of status marks, or nothing.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SidebarCollapsedMode {
    #[default]
    Compact,
    Hidden,
}

/// Herdr's default for `ui.mouse_scroll_lines`.
pub(crate) const DEFAULT_MOUSE_SCROLL_LINES: NonZeroU16 = NonZeroU16::MIN.saturating_add(2);

/// Herdr's `ui.right_click_passthrough_modifier`: the exact modifier
/// combination that sends a right-click to a mouse-aware pane application
/// instead of opening the pane menu. `None` disables it: unset, empty, `off`,
/// `none`, `disabled`, or a combination naming `meta` or `hyper`, which GPUI
/// cannot report, so such a combination could never be held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RightClickPassthrough(Option<Modifiers>);

impl RightClickPassthrough {
    /// Herdr's grammar: `+`-separated `ctrl`/`control`, `alt`/`option`,
    /// `cmd`/`command`/`super`, `meta` and `hyper`, case-insensitive. `None`
    /// for a value Herdr rejects, including any combination with `shift`.
    fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if value.is_empty()
            || ["off", "none", "disabled"]
                .iter()
                .any(|off| value.eq_ignore_ascii_case(off))
        {
            return Some(Self(None));
        }
        let mut modifiers = Modifiers::default();
        let mut unreportable = false;
        for token in value.split('+') {
            match token.trim().to_ascii_lowercase().as_str() {
                "ctrl" | "control" => modifiers.control = true,
                "alt" | "option" => modifiers.alt = true,
                "cmd" | "command" | "super" => modifiers.platform = true,
                "meta" | "hyper" => unreportable = true,
                _ => return None,
            }
        }
        Some(Self((!unreportable).then_some(modifiers)))
    }

    /// Whether `held` is exactly the configured combination. The application
    /// then receives the click without it, as Herdr strips it.
    pub(crate) fn matches(self, held: Modifiers) -> bool {
        self.0 == Some(held)
    }
}

impl<'de> Deserialize<'de> for RightClickPassthrough {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value)
            .ok_or_else(|| serde::de::Error::custom("unsupported right_click_passthrough_modifier"))
    }
}

// Playback paths and per-agent policy are parsed by the sound backend, not this editor.
#[derive(Deserialize)]
#[serde(default)]
struct Sound {
    #[serde(deserialize_with = "crate::lenient::or_true")]
    enabled: bool,
}

impl Default for Sound {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Edit {
    Theme(String),
    Sound(bool),
    Toasts(ToastDelivery),
    CopyOnSelect(bool),
    TabBarPosition(TabBarPosition),
    HideSingleTabBar(bool),
    PaneHistory(bool),
    /// Adds `token` (config spelling, such as `$summary`) as its own row of
    /// `[ui.sidebar.<scope>].rows`, or removes every occurrence of it there.
    SidebarToken {
        scope: crate::config::SidebarScope,
        token: String,
        shown: bool,
    },
}

/// Debug is derived: the original document's text, which may hold private
/// settings this adapter does not read, is redacted by `persistence::Snapshot`.
#[derive(Clone, Debug)]
pub(crate) struct Settings {
    // keep-sorted start
    /// The agents panel's starting order until the user toggles it.
    pub agent_sort: crate::preferences::AgentSort,
    pub clipboard: ClipboardToast,
    /// Whether releasing a mouse selection copies it. When off, the
    /// selection stays highlighted until Cmd-C or Ctrl-C copies it.
    pub copy_on_select: bool,
    pub hide_tab_bar_when_single_tab: bool,
    /// Lines one wheel notch scrolls, Herdr's `ui.mouse_scroll_lines`.
    pub mouse_scroll_lines: NonZeroU16,
    pub name_prompts: NamePrompts,
    original: persistence::Snapshot,
    palettes: [palette::Palette; 2],
    /// Herdr's `experimental.pane_history`: the daemon saves pane scrollback
    /// to `session-history.json` and replays it after a full restart. The
    /// GUI never stores terminal output itself.
    pub pane_history: bool,
    pub path: PathBuf,
    pub right_click_passthrough: RightClickPassthrough,
    pub sidebar_collapsed_mode: SidebarCollapsedMode,
    pub sidebar_start_collapsed: bool,
    pub sound_enabled: bool,
    pub tab_bar_position: TabBarPosition,
    pub theme_name: String,
    pub toast_delay_seconds: u64,
    pub toast_delivery: ToastDelivery,
    pub toast_position: ToastPosition,
    // keep-sorted end
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Parsed {
    #[serde(deserialize_with = "crate::lenient::or_default")]
    theme: palette::ThemeConfig,
    #[serde(deserialize_with = "crate::lenient::or_default")]
    ui: Ui,
    #[serde(deserialize_with = "crate::lenient::or_default")]
    experimental: Experimental,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Experimental {
    #[serde(deserialize_with = "crate::lenient::or_default")]
    pane_history: bool,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Ui {
    #[serde(deserialize_with = "crate::lenient::or_default")]
    agent_panel_sort: crate::preferences::AgentSort,
    #[serde(deserialize_with = "crate::lenient::or_default")]
    sound: Sound,
    #[serde(deserialize_with = "crate::lenient::or_default")]
    toast: RawToast,
    #[serde(deserialize_with = "crate::lenient::or_default")]
    accent: Option<String>,
    // Herdr defaults this one on, unlike the derived `false`.
    #[serde(deserialize_with = "crate::lenient::or_default")]
    copy_on_select: Option<bool>,
    #[serde(deserialize_with = "crate::lenient::or_default")]
    tab_bar_position: TabBarPosition,
    #[serde(deserialize_with = "crate::lenient::or_default")]
    hide_tab_bar_when_single_tab: bool,
    #[serde(deserialize_with = "crate::lenient::or_default")]
    sidebar_collapsed_mode: SidebarCollapsedMode,
    #[serde(deserialize_with = "crate::lenient::or_default")]
    sidebar_start_collapsed: bool,
    #[serde(deserialize_with = "crate::lenient::or_default")]
    prompt_new_tab_name: Option<bool>,
    #[serde(deserialize_with = "crate::lenient::or_default")]
    prompt_new_workspace_name: Option<bool>,
    // Zero, negative, and oversized counts keep Herdr's default.
    #[serde(deserialize_with = "crate::lenient::or_default")]
    mouse_scroll_lines: Option<NonZeroU16>,
    #[serde(deserialize_with = "crate::lenient::or_default")]
    right_click_passthrough_modifier: RightClickPassthrough,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct RawToast {
    #[serde(deserialize_with = "crate::lenient::or_default")]
    delivery: Option<ToastDelivery>,
    #[serde(deserialize_with = "crate::lenient::or_default")]
    enabled: Option<bool>,
    #[serde(deserialize_with = "crate::lenient::or_default")]
    delay_seconds: Option<u64>,
    #[serde(deserialize_with = "crate::lenient::or_default")]
    herdr: HerdrToast,
    #[serde(deserialize_with = "crate::lenient::or_default")]
    clipboard: ClipboardToast,
}

#[derive(Deserialize)]
#[serde(default)]
struct HerdrToast {
    #[serde(deserialize_with = "bottom_right")]
    position: ToastPosition,
}

fn bottom_right<'de, D: Deserializer<'de>>(deserializer: D) -> Result<ToastPosition, D::Error> {
    Ok(crate::lenient::value(deserializer)?.unwrap_or(ToastPosition::BottomRight))
}

impl Default for HerdrToast {
    fn default() -> Self {
        Self {
            position: ToastPosition::BottomRight,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub(crate) struct ClipboardToast {
    #[serde(deserialize_with = "crate::lenient::or_true")]
    pub enabled: bool,
    #[serde(deserialize_with = "crate::lenient::or_default")]
    pub position: ClipboardPosition,
}

impl Default for ClipboardToast {
    fn default() -> Self {
        Self {
            enabled: true,
            position: ClipboardPosition::BottomCenter,
        }
    }
}

impl Settings {
    #[cfg(test)]
    pub(crate) fn parse_text(text: &str) -> Result<Self, Error> {
        #[cfg(unix)]
        let snapshot = persistence::Snapshot {
            text: Some(text.into()),
            ..Default::default()
        };
        #[cfg(windows)]
        let snapshot = persistence::Snapshot {
            text: Some(text.into()),
        };
        Self::parse(PathBuf::from("config.toml"), snapshot)
    }

    /// Background-only. A missing file yields defaults without creating anything.
    pub(crate) fn load() -> crate::Result<Self> {
        let path = crate::config::daemon_config_path(herdr_client::local_var);
        let path = if path.is_absolute() {
            path
        } else {
            env::current_dir()?.join(path)
        };
        Self::load_path(path)
    }

    fn load_path(path: PathBuf) -> crate::Result<Self> {
        let result =
            persistence::read(&path).and_then(|snapshot| Self::parse(path.clone(), snapshot));
        result.map_err(|error| crate::Error::from(error).at_path(&path))
    }

    fn parse(path: PathBuf, original: persistence::Snapshot) -> Result<Self, Error> {
        if original
            .text
            .as_ref()
            .is_some_and(|text| text.len() as u64 > persistence::LIMIT)
        {
            return Err(Error::TooLarge);
        }
        let parsed: Parsed = toml::from_str(original.text.as_deref().unwrap_or(""))?;
        let defaults = NamePrompts::default();
        let name_prompts = NamePrompts {
            tab: parsed.ui.prompt_new_tab_name.unwrap_or(defaults.tab),
            workspace: parsed
                .ui
                .prompt_new_workspace_name
                .unwrap_or(defaults.workspace),
        };
        let toast = parsed.ui.toast;
        // Herdr refuses a longer delay and keeps its default, so this does too.
        let delay = toast
            .delay_seconds
            .filter(|delay| *delay <= 3600)
            .unwrap_or(1);
        let theme_name = parsed.theme.name.as_deref().unwrap_or("catppuccin");
        let legacy_accent = parsed
            .ui
            .accent
            .as_deref()
            .filter(|accent| *accent != "cyan");
        // Resolve arbitrary config strings once on the loader, not per rendered
        // indicator. Appearance changes only select a fixed-size palette.
        let palettes =
            [false, true].map(|light| parsed.theme.resolve(theme_name, legacy_accent, light));
        Ok(Self {
            // keep-sorted start
            agent_sort: parsed.ui.agent_panel_sort,
            clipboard: toast.clipboard,
            copy_on_select: parsed.ui.copy_on_select.unwrap_or(true),
            hide_tab_bar_when_single_tab: parsed.ui.hide_tab_bar_when_single_tab,
            mouse_scroll_lines: parsed
                .ui
                .mouse_scroll_lines
                .unwrap_or(DEFAULT_MOUSE_SCROLL_LINES),
            name_prompts,
            original,
            palettes,
            pane_history: parsed.experimental.pane_history,
            path,
            right_click_passthrough: parsed.ui.right_click_passthrough_modifier,
            sidebar_collapsed_mode: parsed.ui.sidebar_collapsed_mode,
            sidebar_start_collapsed: parsed.ui.sidebar_start_collapsed,
            sound_enabled: parsed.ui.sound.enabled,
            tab_bar_position: parsed.ui.tab_bar_position,
            theme_name: theme_name.into(),
            toast_delay_seconds: delay,
            toast_delivery: toast.delivery.unwrap_or(if toast.enabled == Some(true) {
                ToastDelivery::Herdr
            } else {
                ToastDelivery::Off
            }),
            toast_position: toast.herdr.position,
            // keep-sorted end
        })
    }

    /// Background-only. Returns a fresh snapshot only after durable replacement.
    /// Never merges stale edits: even unrelated external changes require reload.
    pub(crate) fn save(&self, edit: Edit) -> crate::Result<Self> {
        let result = (|| -> Result<Self, Error> {
            let mut document = self
                .original
                .text
                .as_deref()
                .unwrap_or("")
                .parse::<DocumentMut>()?;
            let sidebar_edit = matches!(edit, Edit::SidebarToken { .. });
            match edit {
                Edit::Theme(name) => {
                    let name =
                        palette::canonical(&name).ok_or_else(|| Error::Theme(name.clone()))?;
                    set(&mut document, &["theme", "name"], name.into())?;
                    set(&mut document, &["theme", "auto_switch"], false.into())?;
                }
                Edit::Sound(enabled) => {
                    set(&mut document, &["ui", "sound", "enabled"], enabled.into())?
                }
                Edit::CopyOnSelect(enabled) => {
                    set(&mut document, &["ui", "copy_on_select"], enabled.into())?
                }
                Edit::TabBarPosition(position) => set(
                    &mut document,
                    &["ui", "tab_bar_position"],
                    match position {
                        TabBarPosition::Top => "top",
                        TabBarPosition::Bottom => "bottom",
                    }
                    .into(),
                )?,
                Edit::HideSingleTabBar(hide) => set(
                    &mut document,
                    &["ui", "hide_tab_bar_when_single_tab"],
                    hide.into(),
                )?,
                Edit::SidebarToken {
                    scope,
                    token,
                    shown,
                } => sidebar::edit(&mut document, scope, &token, shown)?,
                Edit::PaneHistory(enabled) => set(
                    &mut document,
                    &["experimental", "pane_history"],
                    enabled.into(),
                )?,
                Edit::Toasts(delivery) => {
                    set(
                        &mut document,
                        &["ui", "toast", "delivery"],
                        match delivery {
                            ToastDelivery::Off => "off",
                            ToastDelivery::Herdr => "herdr",
                            ToastDelivery::Terminal => "terminal",
                            ToastDelivery::System => "system",
                        }
                        .into(),
                    )?;
                    let mut comments = String::new();
                    if let Some(table) = document["ui"]["toast"].as_table_like_mut() {
                        if let Some(key) = table.key("enabled")
                            && let Some(prefix) =
                                key.leaf_decor().prefix().and_then(|raw| raw.as_str())
                            && prefix.contains('#')
                        {
                            comments.push_str(prefix);
                        }
                        if let Some(Item::Value(value)) = table.remove("enabled")
                            && let Some(suffix) =
                                value.decor().suffix().and_then(|raw| raw.as_str())
                            && suffix.contains('#')
                        {
                            comments.push_str(suffix);
                            comments.push('\n');
                        }
                    }
                    // The deleted legacy key has no place to keep its decor;
                    // retain its comments at EOF rather than discarding them.
                    if !comments.is_empty() {
                        document.set_trailing(format!(
                            "{}\n{comments}",
                            document.trailing().as_str().unwrap_or("")
                        ));
                    }
                }
            }
            let text = document.to_string();
            // Validate before performing any writes, including directory creation.
            // The GUI falls back to default rows on a layout it cannot read, so
            // a sidebar edit must leave one both it and Herdr accept. Other
            // edits never touch the layout and must not fail on it.
            if sidebar_edit {
                crate::config::SidebarLayout::from_daemon_config(&text.parse::<toml::Table>()?)?;
            }
            let mut snapshot = self.original.clone();
            snapshot.text = Some(text.clone());
            let mut next = Self::parse(self.path.clone(), snapshot)?;
            next.original = persistence::save(&self.path, &self.original, &text)?;
            Ok(next)
        })();
        result.map_err(|error| crate::Error::from(error).at_path(&self.path))
    }

    /// Pure: no filesystem access; safe to use prepared settings on the UI thread.
    pub(crate) fn theme(&self, light: bool) -> crate::Result<crate::config::Theme> {
        Ok(self.colors(light).theme())
    }

    /// Pure preview of a theme edit, retaining custom colors and legacy accent.
    /// Like saving a named theme, this disables automatic light/dark switching.
    pub(crate) fn preview_theme(
        &self,
        name: &str,
        light: bool,
    ) -> crate::Result<crate::config::Theme> {
        let name = palette::canonical(name).ok_or_else(|| Error::Theme(name.into()))?;
        let mut document = self
            .original
            .text
            .as_deref()
            .unwrap_or("")
            .parse::<DocumentMut>()
            .map_err(Error::from)?;
        set(&mut document, &["theme", "name"], name.into())?;
        set(&mut document, &["theme", "auto_switch"], false.into())?;
        let mut snapshot = self.original.clone();
        snapshot.text = Some(document.to_string());
        Self::parse(self.path.clone(), snapshot)?.theme(light)
    }

    fn colors(&self, light: bool) -> &palette::Palette {
        &self.palettes[usize::from(light)]
    }

    pub(crate) fn status_color(&self, status: AgentStatus, light: bool) -> u32 {
        self.colors(light).status(status)
    }
}

fn set(document: &mut DocumentMut, keys: &[&'static str], mut value: Value) -> Result<(), Error> {
    let mut item = document.as_item_mut();
    for key in &keys[..keys.len() - 1] {
        let table = item.as_table_like_mut().ok_or(Error::Table(key))?;
        if !table.contains_key(key) {
            table.insert(key, Item::Table(toml_edit::Table::new()));
        }
        item = table.get_mut(key).ok_or(Error::Table(key))?;
    }
    let key = keys[keys.len() - 1];
    let table = item.as_table_like_mut().ok_or(Error::Table(key))?;
    if let Some(previous) = table.get(key).and_then(Item::as_value) {
        *value.decor_mut() = previous.decor().clone();
    }
    table.insert(key, Item::Value(value));
    Ok(())
}
