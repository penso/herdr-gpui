//! Projects the terminal theme onto gpui-kit's global theme, so every kit
//! component (dialogs, menus, inputs, sidebar) follows the terminal colors and
//! UI font instead of the kit's bundled palettes.

use crate::config::{Config, Theme, corners};
use gpui_kit::{
    App, Global,
    component::theme::{Theme as KitTheme, ThemeConfig, ThemeMode},
};
use std::rc::Rc;

/// The inputs of the last projection. Several windows render with the same
/// config, so an unchanged key keeps repeated syncs to one comparison.
#[derive(Clone, PartialEq)]
struct Applied {
    theme: Theme,
    ui_family: String,
    ui_size: f32,
    mono_family: String,
    mono_size: f32,
}

impl Global for Applied {}

/// Applies `theme` and the config's fonts to the kit theme when they differ
/// from the last call. Cheap enough for render: a no-op is one comparison.
pub(crate) fn sync(theme: &Theme, config: &Config, cx: &mut App) {
    let applied = Applied {
        theme: theme.clone(),
        ui_family: config.ui.family.clone(),
        ui_size: config.ui.size,
        mono_family: config.terminal.family.clone(),
        mono_size: config.terminal.size,
    };
    if cx.try_global::<Applied>() == Some(&applied) {
        return;
    }
    let kit = project(&applied);
    let mode = kit.mode;
    let kit = Rc::new(kit);
    let global = KitTheme::global_mut(cx);
    global.light_theme = kit.clone();
    global.dark_theme = kit;
    // `change` re-applies the active config and rebuilds the Base projection
    // that owns scrollbars and resize handles.
    KitTheme::change(mode, None, cx);
    cx.set_global(applied);
    cx.refresh_windows();
}

fn hex(color: u32) -> String {
    format!("#{:06x}", color & 0x00ff_ffff)
}

/// `color` at `alpha` (0-255), as the kit's `#rrggbbaa` notation.
fn hex_alpha(color: u32, alpha: u8) -> String {
    format!("#{:06x}{alpha:02x}", color & 0x00ff_ffff)
}

fn luminance(color: u32) -> f32 {
    let channel = |shift: u32| ((color >> shift) & 255) as f32 / 255.;
    0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
}

fn project(applied: &Applied) -> ThemeConfig {
    let theme = &applied.theme;
    let mode = if luminance(theme.background) < 0.5 {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    let ansi = |index: usize| hex(theme.palette[index]);
    let primary = theme.primary();
    let colors: serde_json::Map<String, serde_json::Value> = [
        ("background", hex(theme.background)),
        ("foreground", hex(theme.foreground)),
        ("border", hex(theme.active)),
        ("input.border", hex(theme.active)),
        ("caret", hex(theme.cursor)),
        ("ring", hex(primary)),
        ("selection.background", hex_alpha(primary, 0x55)),
        ("muted.background", hex(theme.surface)),
        ("muted.foreground", hex(theme.subtext())),
        ("accent.background", hex(theme.active)),
        ("accent.foreground", hex(theme.foreground)),
        ("primary.background", hex(primary)),
        ("primary.foreground", hex(theme.text_on(primary))),
        ("secondary.background", hex(theme.surface)),
        ("secondary.hover.background", hex(theme.active)),
        ("secondary.foreground", hex(theme.foreground)),
        ("popover.background", hex(theme.surface)),
        ("popover.foreground", hex(theme.foreground)),
        ("list.active.background", hex(theme.primary_wash())),
        ("list.active.border", hex(primary)),
        ("list.hover.background", hex(theme.active)),
        ("sidebar.background", hex(theme.surface)),
        ("sidebar.foreground", hex(theme.foreground)),
        ("sidebar.border", hex(theme.active)),
        ("sidebar.accent.background", hex(theme.active)),
        ("sidebar.accent.foreground", hex(theme.foreground)),
        ("sidebar.primary.background", hex(theme.primary_wash())),
        ("sidebar.primary.foreground", hex(theme.foreground)),
        ("title_bar.background", hex(theme.background)),
        ("title_bar.border", hex(theme.active)),
        ("status_bar.background", hex(theme.surface)),
        ("status_bar.border", hex(theme.active)),
        ("tab_bar.background", hex(theme.surface)),
        ("tab.background", hex(theme.surface)),
        ("tab.foreground", hex(theme.subtext())),
        ("tab.active.background", hex(theme.primary_wash())),
        ("tab.active.foreground", hex(theme.foreground)),
        ("overlay", hex_alpha(theme.background, 0xb0)),
        ("window.border", hex(theme.active)),
        ("link", ansi(4)),
        ("danger.background", ansi(1)),
        ("danger.foreground", hex(theme.text_on(theme.palette[1]))),
        ("success.background", ansi(2)),
        ("success.foreground", hex(theme.text_on(theme.palette[2]))),
        ("warning.background", ansi(3)),
        ("warning.foreground", hex(theme.text_on(theme.palette[3]))),
        ("info.background", ansi(4)),
        ("info.foreground", hex(theme.text_on(theme.palette[4]))),
        ("base.red", ansi(1)),
        ("base.green", ansi(2)),
        ("base.yellow", ansi(3)),
        ("base.blue", ansi(4)),
        ("base.magenta", ansi(5)),
        ("base.cyan", ansi(6)),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_owned(), value.into()))
    .collect();
    let config = serde_json::json!({
        "name": "Herdr",
        "mode": if mode.is_dark() { "dark" } else { "light" },
        "font.family": applied.ui_family,
        "font.size": applied.ui_size,
        "mono_font.family": applied.mono_family,
        "mono_font.size": applied.mono_size,
        "radius": corners::CONTROL as usize,
        "radius.lg": corners::PANEL as usize,
        "colors": colors,
    });
    // Every value above is a well-formed color or number, so the schema
    // accepts it; a future schema change degrades to the kit defaults rather
    // than failing startup.
    serde_json::from_value(config).unwrap_or_else(|error| {
        tracing::warn!(%error, "gpui-kit rejected the projected theme");
        ThemeConfig {
            mode,
            ..ThemeConfig::default()
        }
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn every_builtin_theme_projects_without_falling_back() {
        for name in ["Default", "Nord", "Dracula"] {
            let theme = Theme::builtin(name).unwrap();
            let applied = Applied {
                theme: theme.clone(),
                ui_family: "Menlo".into(),
                ui_size: 13.,
                mono_family: "Menlo".into(),
                mono_size: 13.,
            };
            let config = project(&applied);
            assert_eq!(config.name.as_ref(), "Herdr", "{name}");
            assert_eq!(config.font_size, Some(13.));
            assert_eq!(
                config.colors.background.as_deref(),
                Some(hex(theme.background).as_str())
            );
            assert!(config.mode.is_dark(), "{name} has a dark background");
        }
    }
}
