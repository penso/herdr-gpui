//! GUI-only fonts and themes; no daemon settings are read or changed.
use serde::Deserialize;
use std::{
    env, fs,
    io::{ErrorKind, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const DEFAULT_CONFIG: &str = include_str!("../config-gpui.example.toml");

#[derive(Clone, Debug)]
pub struct Config {
    pub theme: String,
    pub sidebar: FontConfig,
    pub tabs: FontConfig,
    pub terminal: FontConfig,
    pub ui: FontConfig,
}

#[derive(Clone, Debug)]
pub struct FontConfig {
    pub family: String,
    pub size: f32,
}

impl FontConfig {
    pub fn line_height(&self) -> f32 {
        self.size * 20.0 / 14.0
    }
}

impl Default for Config {
    fn default() -> Self {
        let font = |family: &str, size| FontConfig {
            family: family.into(),
            size,
        };
        Self {
            theme: "Default".into(),
            sidebar: font("Menlo", 12.0),
            tabs: font(".SystemUIFont", 14.0),
            terminal: font("Menlo", 14.0),
            ui: font(".SystemUIFont", 12.0),
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Settings {
    theme: Option<String>,
    sidebar: FontSettings,
    tabs: FontSettings,
    terminal: FontSettings,
    ui: FontSettings,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct FontSettings {
    family: Option<String>,
    size: Option<f32>,
}

fn home() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set".into())
}

fn config_root() -> Result<PathBuf, String> {
    match env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        Some(value) => {
            let path = PathBuf::from(value);
            if !path.is_absolute() {
                return Err("XDG_CONFIG_HOME must be an absolute path".into());
            }
            Ok(path)
        }
        None => Ok(home()?.join(".config")),
    }
}

fn theme_directories() -> Result<Vec<PathBuf>, String> {
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
    pub fn path() -> Result<PathBuf, String> {
        Ok(config_root()?.join("herdr/config-gpui.toml"))
    }

    pub fn load() -> Result<Self, String> {
        Self::load_path(&Self::path()?)
    }

    fn load_path(path: &Path) -> Result<Self, String> {
        let result = (|| {
            match fs::read_to_string(path) {
                Ok(text) => return Self::parse(&text),
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
            {
                Ok(mut file) => file
                    .write_all(DEFAULT_CONFIG.as_bytes())
                    .map_err(|error| error.to_string())?,
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.to_string()),
            }
            Self::parse(&fs::read_to_string(path).map_err(|error| error.to_string())?)
        })();
        result.map_err(|error| format!("{}: {error}", path.display()))
    }

    fn parse(text: &str) -> Result<Self, String> {
        let settings: Settings = toml::from_str(text).map_err(|error| error.to_string())?;
        let mut config = Self::default();
        if let Some(theme) = settings.theme {
            if theme.trim().is_empty() {
                return Err("theme must not be empty".into());
            }
            config.theme = theme;
        }
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
            if font.family.trim().is_empty() {
                return Err(format!("{name}.family must not be empty"));
            }
            if !font.size.is_finite() || !(8.0..=48.0).contains(&font.size) {
                return Err(format!(
                    "{name}.size must be finite and between 8 and 48 logical pixels"
                ));
            }
        }
        Ok(config)
    }

    /// Discover names without parsing every theme. On failure, callers can use
    /// `Theme::BUILTIN_NAMES`, which remain loadable without any directories.
    pub fn available_themes(&self) -> Result<Vec<String>, String> {
        self.available_themes_in(&theme_directories()?)
    }

    fn available_themes_in(&self, directories: &[PathBuf]) -> Result<Vec<String>, String> {
        let mut names: Vec<String> = Theme::BUILTIN_NAMES
            .iter()
            .map(|name| (*name).into())
            .collect();
        for directory in directories {
            let entries = match fs::read_dir(directory) {
                Ok(entries) => entries,
                Err(error) if error.kind() == ErrorKind::NotFound => continue,
                Err(error) => return Err(format!("{}: {error}", directory.display())),
            };
            for entry in entries {
                let entry = entry.map_err(|error| format!("{}: {error}", directory.display()))?;
                // Follow symlinks just as the named theme loader does.
                let metadata = match fs::metadata(entry.path()) {
                    Ok(metadata) => metadata,
                    Err(error) if error.kind() == ErrorKind::NotFound => continue,
                    Err(error) => return Err(format!("{}: {error}", entry.path().display())),
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
    pub fn save_theme(&self, name: &str) -> Result<(), String> {
        self.save_theme_path(name, &Self::path()?)
    }

    fn save_theme_path(&self, name: &str, path: &Path) -> Result<(), String> {
        let selected = Self {
            theme: name.into(),
            ..self.clone()
        };
        selected.theme()?;
        let result = (|| -> Result<(), String> {
            let text = match fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) if error.kind() == ErrorKind::NotFound => DEFAULT_CONFIG.into(),
                Err(error) => return Err(error.to_string()),
            };
            let mut document = text
                .parse::<toml_edit::DocumentMut>()
                .map_err(|error| error.to_string())?;
            let mut value = toml_edit::Value::from(name);
            if let Some(previous) = document.get("theme").and_then(toml_edit::Item::as_value) {
                *value.decor_mut() = previous.decor().clone();
            }
            document["theme"] = toml_edit::Item::Value(value);
            let parent = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
            let (temporary, mut file) = loop {
                let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
                let temporary =
                    parent.join(format!(".config-gpui-{}-{id}.tmp", std::process::id()));
                match fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&temporary)
                {
                    Ok(file) => break (temporary, file),
                    Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error.to_string()),
                }
            };
            let write_result = (|| {
                file.write_all(document.to_string().as_bytes())?;
                file.sync_all()?;
                drop(file);
                fs::rename(&temporary, path)
            })();
            if let Err(error) = write_result {
                fs::remove_file(&temporary).map_err(|cleanup| {
                    format!("{error}; removing {}: {cleanup}", temporary.display())
                })?;
                return Err(error.to_string());
            }
            Ok(())
        })();
        result.map_err(|error| format!("{}: {error}", path.display()))
    }

    pub fn theme(&self) -> Result<Theme, String> {
        self.theme_with_directories(theme_directories)
    }

    fn theme_with_directories(
        &self,
        directories: impl FnOnce() -> Result<Vec<PathBuf>, String>,
    ) -> Result<Theme, String> {
        let name = self.theme.trim();
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
                return Err("theme must be a name, absolute path, or ~/ path".into());
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
                    Err(error) => return Err(format!("{}: {error}", candidate.display())),
                }
            }
            found.ok_or_else(|| format!("theme {name:?} not found in {directories:?}"))?
        };
        let text =
            fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        Theme::parse_ghostty(&text).map_err(|error| format!("{}: {error}", path.display()))
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
    pub palette: [u32; 256],
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
            palette,
        }
    }
}

impl Theme {
    pub const BUILTIN_NAMES: &'static [&'static str] = &[
        "Default",
        "Nord",
        "Dracula",
        "Catppuccin Mocha",
        "Catppuccin Latte",
    ];

    fn derive_chrome(&mut self) {
        let blend = |percent: u32| {
            let channel = |shift: u32| {
                let bg = (self.background >> shift) & 255;
                let fg = (self.foreground >> shift) & 255;
                (bg * (100 - percent) + fg * percent) / 100
            };
            (channel(16) << 16) | (channel(8) << 8) | channel(0)
        };
        self.surface = blend(5);
        self.active = blend(12);
        self.muted = blend(55);
    }

    fn builtin(name: &str) -> Option<Self> {
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

    fn parse_ghostty(text: &str) -> Result<Self, String> {
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
            let error = |message| format!("line {}: {key}: {message}", index + 1);
            let color = |value: &str| -> Result<u32, String> {
                let hex = value.strip_prefix('#').unwrap_or(value);
                if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return Err(error(
                        "expected a six-digit RGB hex color (optionally prefixed by #)",
                    ));
                }
                u32::from_str_radix(hex, 16).map_err(|_| error("invalid hex color"))
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
                        .ok_or_else(|| error("expected index=color"))?;
                    let index = index
                        .trim()
                        .parse::<usize>()
                        .ok()
                        .filter(|index| *index < 256)
                        .ok_or_else(|| error("palette index must be between 0 and 255"))?;
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
    use super::*;

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
    fn discovers_sorted_names_and_loads_in_precedence_order()
    -> Result<(), Box<dyn std::error::Error>> {
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
            Theme::builtin("Nord").ok_or("missing builtin")?
        );
        Ok(())
    }

    #[test]
    fn discovery_includes_explicit_selection_and_reports_errors()
    -> Result<(), Box<dyn std::error::Error>> {
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
                    .theme_with_directories(|| Err("unavailable directories".into()))
                    .is_ok()
            );
        }
        Ok(())
    }

    #[test]
    fn saves_only_theme_and_preserves_latest_settings_and_comments()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDirectory::new()?;
        let path = temp.0.join("config.toml");
        let config = Config::default();
        // These on-disk settings differ from the in-memory snapshot, including
        // a setting this version does not understand.
        let original = "# heading\ntheme = 'Default' # selection\nfuture = true\n\n[tabs] # fonts\nsize = 19 # keep\n";
        fs::write(&path, original)?;
        config.save_theme_path("Nord", &path)?;
        assert_eq!(
            fs::read_to_string(&path)?,
            original.replace("'Default'", "\"Nord\"")
        );
        assert_eq!(config.theme, "Default");
        assert_eq!(fs::read_dir(&temp.0)?.count(), 1);

        fs::write(&path, "# no theme\n[tabs]\nsize = 19\n")?;
        config.save_theme_path("Dracula", &path)?;
        let saved = fs::read_to_string(&path)?;
        let parsed = Config::parse(&saved)?;
        assert_eq!(parsed.theme, "Dracula");
        assert_eq!(parsed.tabs.size, 19.0);
        assert!(saved.contains("# no theme"));
        Ok(())
    }

    #[test]
    fn save_validates_theme_and_toml_before_writing() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDirectory::new()?;
        let path = temp.0.join("config.toml");
        let config = Config::default();
        let custom = temp.0.join("custom");
        fs::write(&custom, "background=invalid")?;
        let custom_name = custom.to_str().ok_or("non-UTF8 temporary path")?;
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
        assert_eq!(Config::load_path(&new_path)?.theme()?.background, 0x112233);
        assert_eq!(
            fs::read_dir(new_path.parent().ok_or("missing parent")?)?.count(),
            1
        );
        Ok(())
    }

    #[test]
    fn defaults_and_partial_settings() -> Result<(), String> {
        let config = Config::parse(DEFAULT_CONFIG)?;
        assert_eq!(config.theme()?, Theme::default());
        assert_eq!(config.sidebar.size, 12.0);
        assert_eq!(config.tabs.family, ".SystemUIFont");
        assert_eq!(config.terminal.line_height(), 20.0);
        assert_eq!(config.ui.size, 12.0);
        let config = Config::parse("[tabs]\nsize = 18\n[terminal]\nfamily = 'Monaco'")?;
        assert_eq!(config.tabs.family, ".SystemUIFont");
        assert_eq!(config.tabs.size, 18.0);
        assert_eq!(config.terminal.size, 14.0);
        assert_eq!(config.terminal.family, "Monaco");
        assert_eq!(Config::parse("")?.theme, "Default");
        Ok(())
    }

    #[test]
    fn rejects_invalid_settings() {
        for text in [
            "unknown = 1",
            "[sidebar]\nunknown = 1",
            "[unknown]",
            "theme = ''",
            "[ui]\nfamily = '  '",
            "[tabs]\nsize = 7.9",
            "[terminal]\nsize = 48.1",
            "[sidebar]\nsize = nan",
            "[sidebar]\nsize = inf",
            "[sidebar]\nsize = -inf",
            "[tabs]\nsize = '14'",
            "[tabs]\nfamily = 14",
        ] {
            assert!(Config::parse(text).is_err(), "accepted {text:?}");
        }
        assert!(Config::parse("[tabs]\nsize = 8\n[ui]\nsize = 48").is_ok());
    }

    #[test]
    fn default_palette_and_builtins() -> Result<(), String> {
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
    fn ghostty_colors_and_ignored_settings() -> Result<(), String> {
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
                matches!(result, Err(ref error) if error.starts_with("line 2:")),
                "{result:?}"
            );
        }
    }

    #[test]
    fn creates_config_without_overwriting_and_loads_absolute_theme() -> Result<(), String> {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let directory =
            env::temp_dir().join(format!("herdr-config-{}-{unique}", std::process::id()));
        let path = directory.join("config-gpui.toml");
        let result = (|| {
            Config::load_path(&path)?;
            assert_eq!(
                fs::read_to_string(&path).map_err(|e| e.to_string())?,
                DEFAULT_CONFIG
            );
            fs::write(&path, "theme = 'Nord'").map_err(|e| e.to_string())?;
            assert_eq!(Config::load_path(&path)?.theme, "Nord");
            assert_eq!(
                fs::read_to_string(&path).map_err(|e| e.to_string())?,
                "theme = 'Nord'"
            );
            let theme_path = directory.join("custom-theme");
            fs::write(&theme_path, "background=112233").map_err(|e| e.to_string())?;
            let config = Config {
                theme: theme_path.to_string_lossy().into_owned(),
                ..Config::default()
            };
            assert_eq!(config.theme()?.background, 0x112233);
            Ok(())
        })();
        fs::remove_dir_all(directory).map_err(|e| e.to_string())?;
        result
    }
}
