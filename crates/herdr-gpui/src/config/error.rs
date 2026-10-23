//! Why the user's settings or a theme could not be loaded, validated, or edited.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("HERDR_GITHUB_OAUTH_CLIENT_ID must be UTF-8")]
    ClientIdEncoding,
    /// A `[devices.<id>]` theme that does not parse, named by its device.
    #[error("devices.{device}.theme: {source}")]
    DeviceTheme {
        device: String,
        #[source]
        source: Box<crate::Error>,
    },
    #[error("{0}.fallback families must not be empty")]
    EmptyFontFallback(&'static str),
    #[error("{0}.family must not be empty")]
    EmptyFontFamily(&'static str),
    #[error("theme must not be empty")]
    EmptyTheme,
    #[error(
        "{0} must be 1..256 ASCII letters, digits, '.', '_' or '-' (public client ID, not a secret)"
    )]
    InvalidClientId(&'static str),
    #[error("code must be a TOML table")]
    InvalidCodeTable,
    #[error("devices.{0:?} is not a saved device ID")]
    InvalidDeviceId(String),
    #[error("devices and each devices.<id> must be tables")]
    InvalidDevicesTable,
    #[error("{0}.size must be finite and between 8 and 48 logical pixels")]
    InvalidFontSize(&'static str),
    #[error("{0}.weight must be a weight name such as \"medium\" or a number from 100 to 900")]
    InvalidFontWeight(&'static str),
    #[error("sidebar.hosts.{host:?} must be a #rgb or #rrggbb colour, not {value:?}")]
    InvalidHostColor { host: String, value: String },
    #[error("{0}.line_height must be a finite number between 1 and 2 times the font size")]
    InvalidLineHeight(&'static str),
    #[error("layout.sidebar_gap must be finite and between 0 and 64 logical pixels")]
    InvalidSidebarGap,
    #[error("sidebar.{key} must be finite and between 0 and {max} logical pixels")]
    InvalidSidebarMetric { key: &'static str, max: f32 },
    #[error("sound.volume must be between 0 and 100")]
    InvalidSoundVolume,
    #[error(
        "terminal.{0} must be one number or a pair of numbers, each finite and between 0 and 64 logical pixels"
    )]
    InvalidTerminalPadding(&'static str),
    #[error("a theme that follows the system must name both sides: light:NAME,dark:NAME")]
    InvalidThemePair,
    #[error("theme must be a name, absolute path, or ~/ path")]
    InvalidThemePath,
    #[error("usage must be a TOML table")]
    InvalidUsageTable,
    #[error("zen.font_size must be finite and between 8 and 48 logical pixels")]
    InvalidZenFontSize,
    #[error("zen.width must be 20 to 1000 columns, or a percentage from \"10%\" to \"100%\"")]
    InvalidZenWidth,
    #[error("{key}: a part of this key holds a value, not a table; unset that part first")]
    KeyNotTable { key: String },
    #[error("Cannot migrate {}: {} already contains different settings. Merge your old settings into the local file, then remove the old config; it will be regenerated.", original.display(), local.display())]
    MigrationConflict { original: PathBuf, local: PathBuf },
    #[error("XDG_CONFIG_HOME must be an absolute path")]
    RelativeRoot,
    #[error("line {line}: {key}: {source}")]
    ThemeLine {
        line: usize,
        key: String,
        #[source]
        source: ThemeParseError,
    },
    #[error("theme {name:?} not found in {directories:?}")]
    ThemeNotFound {
        name: String,
        directories: Vec<PathBuf>,
    },
    #[error("[devices] must list at most {0} devices")]
    TooManyDevices(usize),
    #[error("{0}.fallback must list at most 8 families")]
    TooManyFontFallbacks(&'static str),
    #[error("Herdr UI has no setting {}; config-gpui.toml lists every setting", .0.join(", "))]
    UnknownKeys(Vec<String>),
    #[error("Unknown sidebar layout {0:?}")]
    UnknownLayout(String),
    // keep-sorted end
}

/// Why one line of a terminal theme's colors did not parse.
#[derive(Debug, thiserror::Error)]
pub enum ThemeParseError {
    #[error("expected a six-digit RGB hex color (optionally prefixed by #)")]
    InvalidColor,
    #[error("invalid hex color")]
    InvalidHex(#[source] std::num::ParseIntError),
    #[error("expected index=color")]
    MissingPaletteColor,
    #[error("palette index must be between 0 and 255")]
    InvalidPaletteIndex(#[source] std::num::ParseIntError),
    #[error("palette index must be between 0 and 255")]
    PaletteIndexOutOfRange,
}
