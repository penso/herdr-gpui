use super::*;
use anyhow::Context as _;
use std::env;
use std::sync::atomic::{AtomicU64, Ordering};

mod accent;
mod activity_bar;
mod agent_skill;
mod agents_header;
mod agents_placement;
mod backdrop;
mod bitmap_fonts;
mod bold_color;
mod change_counts;
mod cjk_fonts;
mod cloud_setup;
mod code;
mod code_navigation;
mod code_start;
mod default_fonts;
mod device_footer;
mod device_icons;
mod device_theme;
mod discovery;
mod edit;
mod favorite_themes;
mod features;
mod finished_notifications;
mod focus_follows_mouse;
mod font_aliases;
mod font_weight;
mod fonts;
mod github;
mod github_accounts;
mod icon_font;
mod keybindings;
mod language;
mod line_height;
mod loading;
mod lock_access;
mod missing_fonts;
mod notification_settings;
mod omarchy_theme;
mod preferences;
mod px0;
mod remote;
mod repository_icons;
mod sidebar_scope;
mod sidebar_search;
mod sidebar_settings;
mod sidebar_style;
mod sound_volume;
mod ssh_hosts;
mod status_bar;
mod status_bar_awake_scope;
mod status_bar_gpu;
mod status_icons;
mod system_themes;
mod tab_layout;
mod terminal_padding;
mod theme_files;
mod themes;
mod update_channel;
mod usage_inline;
mod worktrees;
mod zen;

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
