//! Settings and app services: config, themes, fonts, shortcuts, keeping the
//! machine awake, sounds, updates, and integrations. Reading and changing
//! the settings files, listing themes and sounds, the agent skill, logs, and
//! diagnostics need no running app, so the command line does those itself.
use super::app::Switch;
use serde::{Deserialize, Serialize};

/// Listing installed fonts or integrations, or asking for the latest
/// release, can take a few seconds.
const LIST_WAIT_SECONDS: u64 = 15;
/// Saving a theme or a font waits for the file lock other saves may hold.
const SAVE_WAIT_SECONDS: u64 = 10;
/// The longest `caffeine on --for`: a day.
pub const MAX_CAFFEINE_SECONDS: u64 = 24 * 60 * 60;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum SettingsOp {
    /// Reread Herdr's config and this app's, as the shortcut does.
    ReloadConfig,
    /// Configured fonts the app's text system does not have.
    CheckConfig,
    /// Choose and save a color theme.
    Theme {
        name: String,
    },
    /// The font families the app can use.
    Fonts,
    /// Choose and save the font of every face; none goes back to the default.
    Font {
        #[serde(default)]
        name: Option<String>,
    },
    /// The window's shortcuts, and whether each of Herdr's custom commands
    /// can be reached from this client.
    Keybinds,
    /// Report, or change for `seconds` (none: until changed again).
    Caffeine {
        #[serde(default)]
        switch: Option<Switch>,
        #[serde(default)]
        seconds: Option<u64>,
    },
    Sound {
        sound: Sound,
    },
    Bell,
    /// Ask for the latest release; never downloads or installs.
    CheckUpdate,
    Update,
    /// The integrations Herdr offers the selected host.
    Integrations,
    /// The locale the windows show, and what chose it. Changing the
    /// `language` setting is a file edit the command line makes itself.
    Language,
}

impl SettingsOp {
    pub fn wait_seconds(&self) -> u64 {
        match self {
            Self::Fonts | Self::Integrations | Self::CheckUpdate => LIST_WAIT_SECONDS,
            Self::Theme { .. } | Self::Font { .. } => SAVE_WAIT_SECONDS,
            _ => 0,
        }
    }
}

/// The notification sounds Herdr names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sound {
    Done,
    Request,
}

/// The control socket names sounds in its own JSON form; Herdr's type keeps
/// the vendored wire form.
impl From<Sound> for herdr_client::protocol::SemanticNotificationSound {
    fn from(sound: Sound) -> Self {
        match sound {
            Sound::Done => Self::Done,
            Sound::Request => Self::Request,
        }
    }
}

impl Sound {
    pub const ALL: [Self; 2] = [Self::Done, Self::Request];

    pub fn name(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Request => "request",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingInfo {
    pub name: String,
    pub label: String,
    pub keys: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomInfo {
    pub label: String,
    pub keys: String,
    /// "runs", "shadowed", "taken", "needs_modifier", "no_prefix",
    /// "over_limit", or "unsupported".
    pub reach: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrationInfo {
    pub id: String,
    pub label: String,
    /// "not_installed", "current", "outdated", or "unavailable".
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reply {
    /// What only the running app can check about the settings: configured
    /// fonts its text system does not have.
    Config {
        missing_fonts: Vec<String>,
    },
    Fonts {
        fonts: Vec<String>,
    },
    Keybinds {
        bindings: Vec<BindingInfo>,
        custom: Vec<CustomInfo>,
    },
    /// "off", "pending", or "on".
    Caffeine {
        state: String,
        /// Seconds until a timed hold ends, when one was set.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seconds: Option<u64>,
    },
    /// `state` is "disabled", "idle", "checking", "current", "available",
    /// "downloading", "ready", "installing", "homebrew", "upgrading",
    /// "restart", "restarting", "cancelling", or "error".
    Update {
        state: String,
        latest: Option<String>,
        /// Why updates are disabled, or what failed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    Integrations {
        integrations: Vec<IntegrationInfo>,
    },
    Language(LanguageInfo),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageInfo {
    /// The `language` setting: "system" or a BCP 47 tag.
    pub configured: String,
    /// The pseudo-locale QA shows over the setting, when one is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pseudo: Option<String>,
    /// The BCP 47 tag of the locale the windows show.
    pub locale: String,
    /// "ltr" or "rtl".
    pub direction: String,
}

#[cfg(test)]
mod tests;
