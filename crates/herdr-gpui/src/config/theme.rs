//! Terminal and chrome colors: built-in palettes, Ghostty theme files, and
//! where they are found.
use super::{Config, config_root, home};
use crate::{Error, Result, contrast::Contrast, error::ThemeParseError};
use std::{
    env, fs,
    io::ErrorKind,
    path::{Component, Path, PathBuf},
};

const FOLLOW_HERDR: &str = "Follow Herdr";

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
    /// Discover names without parsing every theme. On failure, callers can use
    /// `Theme::BUILTIN_NAMES`, which remain loadable without any directories.
    pub fn available_themes(&self) -> Result<Vec<String>> {
        self.available_themes_in(&theme_directories()?)
    }

    pub(super) fn available_themes_in(&self, directories: &[PathBuf]) -> Result<Vec<String>> {
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

    pub fn theme(&self) -> Result<Theme> {
        self.theme_with_directories(theme_directories)
            .map(|theme| theme.with_contrast(self.contrast))
    }

    pub(super) fn theme_with_directories(
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

    pub(crate) fn builtin(name: &str) -> Option<Self> {
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

    pub(super) fn parse_ghostty(text: &str) -> Result<Self> {
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
