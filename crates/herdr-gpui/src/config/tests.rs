use super::*;
use anyhow::Context as _;
use std::sync::atomic::{AtomicU64, Ordering};

mod backdrop;
mod bitmap_fonts;
mod bold_color;
mod code;
mod code_navigation;
mod code_start;
mod default_fonts;
mod device_footer;
mod device_icons;
mod device_theme;
mod discovery;
mod edit;
mod finished_notifications;
mod focus_follows_mouse;
mod font_aliases;
mod font_weight;
mod fonts;
mod github;
mod icon_font;
mod keybindings;
mod line_height;
mod loading;
mod missing_fonts;
mod notification_settings;
mod preferences;
mod repository_icons;
mod sidebar_scope;
mod sidebar_search;
mod sidebar_settings;
mod sidebar_style;
mod sound_volume;
mod status_bar;
mod system_themes;
mod terminal_padding;
mod theme_files;
mod themes;
mod update_channel;
mod usage_inline;
mod usage_provider_comments;
mod usage_providers;

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
