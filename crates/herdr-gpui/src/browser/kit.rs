//! The explainer kit: stylesheet, script, and the user's theme, served to
//! local pages under `/.herdr/`, so an agent's explainer or mockup matches
//! the window it sits in. Files whose names start with a dot are never read
//! from a page's folder, so these paths cannot hide one of the page's own.
use crate::config::{Theme, mix};

const KIT_CSS: &str = include_str!("kit/kit.css");
const KIT_JS: &str = include_str!("kit/kit.js");

/// The theme's colors the kit uses, compared to tell when the served theme
/// must change without formatting it every frame.
pub(crate) type ThemeKey = [u32; 7];

pub(crate) fn theme_key(theme: &Theme) -> ThemeKey {
    [
        theme.background,
        theme.foreground,
        theme.surface,
        theme.active,
        theme.muted,
        theme.primary(),
        theme.palette[1] ^ theme.palette[2].rotate_left(8) ^ theme.palette[3].rotate_left(16),
    ]
}

fn luminance(color: u32) -> f32 {
    let channel = |shift: u32| ((color >> shift) & 255) as f32 / 255.;
    0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
}

/// The kit's color tokens for `theme`. The selector outranks the kit's own
/// light and dark defaults, which come after it in the cascade.
pub(crate) fn theme_css(theme: &Theme) -> String {
    let light = luminance(theme.background) > 0.5;
    let raised = if light {
        mix(theme.background, 0xffffff, 70)
    } else {
        theme.active
    };
    let tokens = [
        ("bg", theme.background),
        ("fg", theme.foreground),
        ("muted", theme.subtext()),
        ("surface", theme.surface),
        ("raised", raised),
        ("border", mix(theme.background, theme.foreground, 14)),
        ("accent", theme.ink(theme.primary())),
        ("ok", theme.ink(theme.palette[2])),
        ("warn", theme.ink(theme.palette[3])),
        ("bad", theme.ink(theme.palette[1])),
    ];
    // i18n: data
    let mut css = String::from("/* The user's Herdr UI theme, for kit.css. */\n");
    css.push_str(":root:not(#hk-untheme) {\n");
    css.push_str(if light {
        "  color-scheme: light;\n"
    } else {
        "  color-scheme: dark;\n"
    });
    for (name, color) in tokens {
        css.push_str(&format!("  --hk-{name}: #{:06x};\n", color & 0xff_ffff));
    }
    css.push_str("}\n");
    css
}

/// The kit file a local page asked for, with its content type, or `None`
/// for any other path, which the page's folder answers.
pub(crate) fn asset(path: &str, theme: &str) -> Option<(Vec<u8>, &'static str)> {
    Some(match path {
        "/.herdr/kit.css" => (KIT_CSS.as_bytes().to_vec(), "text/css; charset=utf-8"),
        "/.herdr/kit.js" => (KIT_JS.as_bytes().to_vec(), "text/javascript; charset=utf-8"),
        "/.herdr/theme.css" => (theme.as_bytes().to_vec(), "text/css; charset=utf-8"),
        _ => return None,
    })
}

#[cfg(test)]
mod tests;
