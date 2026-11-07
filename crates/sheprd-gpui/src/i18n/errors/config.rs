//! Settings and themes: `error-config.ftl`.

use super::Localize;
use crate::{
    config::{Error, ThemeParseError, ssh_hosts, tab_layout},
    i18n::fl,
};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::DeviceTheme { device, source } => fl!(
                "error-config-device-theme",
                device = device.as_str(),
                error = source.localize()
            ),
            Self::EmptyFontFallback(table) => {
                fl!("error-config-empty-font-fallback", table = *table)
            }
            Self::EmptyFontFamily(table) => fl!("error-config-empty-font-family", table = *table),
            Self::EmptyTheme => fl!("error-config-empty-theme"),
            Self::InvalidChatWidth => fl!("error-config-invalid-chat-width"),
            Self::InvalidCodeTable => fl!("error-config-invalid-code-table"),
            Self::InvalidDeviceId(device) => fl!(
                "error-config-invalid-device-id",
                device = format!("{device:?}")
            ),
            Self::InvalidDevicesTable => fl!("error-config-invalid-devices-table"),
            Self::InvalidFavoriteThemes => fl!("error-config-invalid-favorite-themes"),
            Self::InvalidFontSize(table) => fl!("error-config-invalid-font-size", table = *table),
            Self::InvalidFontWeight(table) => {
                fl!("error-config-invalid-font-weight", table = *table)
            }
            Self::InvalidHostColor { host, value } => fl!(
                "error-config-invalid-host-color",
                host = format!("{host:?}"),
                value = format!("{value:?}")
            ),
            Self::InvalidLineHeight(table) => {
                fl!("error-config-invalid-line-height", table = *table)
            }
            Self::InvalidSidebarGap => fl!("error-config-invalid-sidebar-gap"),
            // `max` as Rust writes the `f32`, as the `Display` does.
            Self::InvalidSidebarMetric { key, max } => fl!(
                "error-config-invalid-sidebar-metric",
                key = *key,
                max = max.to_string()
            ),
            Self::InvalidSoundVolume => fl!("error-config-invalid-sound-volume"),
            Self::InvalidTerminalPadding(key) => {
                fl!("error-config-invalid-terminal-padding", key = *key)
            }
            Self::InvalidThemePair => fl!("error-config-invalid-theme-pair"),
            Self::InvalidThemePath => fl!("error-config-invalid-theme-path"),
            Self::InvalidUsageTable => fl!("error-config-invalid-usage-table"),
            Self::InvalidZenFontSize => fl!("error-config-invalid-zen-font-size"),
            Self::InvalidZenWidth => fl!("error-config-invalid-zen-width"),
            Self::KeyNotTable { key } => fl!("error-config-key-not-table", key = key.as_str()),
            Self::MigrationConflict { original, local } => fl!(
                "error-config-migration-conflict",
                original = original.display().to_string(),
                local = local.display().to_string()
            ),
            Self::PreferencesNotObject => fl!("error-config-preferences-not-object"),
            Self::ThemeLine { line, key, source } => fl!(
                "error-config-theme-line",
                line = *line,
                key = key.as_str(),
                error = source.localize()
            ),
            Self::ThemeNotFound { name, directories } => fl!(
                "error-config-theme-not-found",
                name = format!("{name:?}"),
                directories = format!("{directories:?}")
            ),
            Self::TooManyDevices(limit) => fl!("error-config-too-many-devices", limit = *limit),
            Self::TooManyFontFallbacks(table) => {
                fl!("error-config-too-many-font-fallbacks", table = *table)
            }
            Self::UnknownKeys(keys) => fl!("error-config-unknown-keys", keys = keys.join(", ")),
            Self::UnknownLayout(n) => fl!("error-config-unknown-layout", name = format!("{n:?}")),
            // keep-sorted end
        }
    }
}

impl Localize for ThemeParseError {
    fn localize(&self) -> String {
        match self {
            Self::InvalidColor => fl!("error-config-theme-parse-invalid-color"),
            Self::InvalidHex(_) => fl!("error-config-theme-parse-invalid-hex"),
            Self::MissingPaletteColor => fl!("error-config-theme-parse-missing-palette-color"),
            Self::InvalidPaletteIndex(_) | Self::PaletteIndexOutOfRange => {
                fl!("error-config-theme-parse-palette-index")
            }
        }
    }
}

impl Localize for ssh_hosts::Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::InvalidTable => fl!("error-config-ssh-invalid-table"),
            Self::InvalidTarget(target) => fl!(
                "error-config-ssh-invalid-target",
                target = format!("{target:?}")
            ),
            Self::TooMany(limit) => fl!("error-config-ssh-too-many", limit = *limit),
            // keep-sorted end
        }
    }
}

impl Localize for tab_layout::Error {
    fn localize(&self) -> String {
        match self {
            Self::Unknown(name) => {
                fl!(
                    "error-config-tab-layout-unknown",
                    name = format!("{name:?}")
                )
            }
        }
    }
}
