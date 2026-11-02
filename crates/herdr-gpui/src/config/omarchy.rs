//! Omarchy's desktop theme. Omarchy renders the theme it applies into one
//! directory, a Ghostty `ghostty.conf` among the files, and swaps that
//! directory whole on every switch, so watching its files follows the desktop.
use super::{Config, Theme, parse_hex};
use crate::{Error, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

// i18n: data
pub(crate) const NAME: &str = "Omarchy";

/// Where Omarchy keeps the applied theme under the home directory: current
/// releases write state, older ones linked it from config. Omarchy names both
/// from `$HOME` alone, ignoring the XDG variables.
const DIRECTORIES: [&str; 2] = [
    ".local/state/omarchy/current/theme",
    ".config/omarchy/current/theme",
];

/// The applied theme's directory, if Omarchy has applied one.
fn directory(home: &Path) -> Option<PathBuf> {
    DIRECTORIES
        .iter()
        .map(|relative| home.join(relative))
        .find(|directory| directory.join("ghostty.conf").is_file())
}

pub(super) fn installed(home: &Path) -> bool {
    directory(home).is_some()
}

/// The files [`load`] reads. Without a theme applied yet, the current
/// release's paths, so applying one reloads.
pub(super) fn files(home: &Path) -> [PathBuf; 2] {
    let directory = directory(home).unwrap_or_else(|| home.join(DIRECTORIES[0]));
    [
        directory.join("ghostty.conf"),
        directory.join("colors.toml"),
    ]
}

/// Terminal colors from the rendered `ghostty.conf`, whose template Omarchy
/// keeps in step with its tokens; the chrome's accent and selected rows from
/// the `accent` and `selection` tokens in `colors.toml` when they parse.
pub(super) fn load(home: &Path) -> Result<Theme> {
    let [ghostty, colors] = files(home);
    let text =
        fs::read_to_string(&ghostty).map_err(|error| Error::from(error).at_path(&ghostty))?;
    let mut theme = Theme::parse_ghostty(&text).map_err(|error| error.at_path(&ghostty))?;
    // Omarchy owns this file and its token set grows, so anything missing or
    // unreadable keeps the colors derived from the terminal palette.
    let tokens = fs::read_to_string(&colors)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .unwrap_or_default();
    let token = |key: &str| tokens.get(key)?.as_str().and_then(parse_hex);
    if let Some(accent) = token("accent") {
        theme.accent = Some(accent);
    }
    if let Some(selection) = token("selection") {
        theme.active = selection;
    }
    Ok(theme)
}

impl Config {
    /// Omarchy's theme stands in for the managed default until the user's
    /// own overrides name a theme, so an Omarchy desktop needs no setup.
    pub(super) fn follow_omarchy(&mut self, overrides: &str, home: &Path) {
        let chosen = overrides
            .parse::<toml::Table>()
            .is_ok_and(|table| table.contains_key("theme"));
        if !chosen && installed(home) {
            self.theme = NAME.into();
        }
    }
}
