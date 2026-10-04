use super::*;

#[test]
fn every_palette_status_overrides_and_auto_switch() -> anyhow::Result<()> {
    assert_eq!(THEME_NAMES.len(), 18);
    for name in THEME_NAMES {
        let settings = parsed(&format!("[theme]\nname = '{name}'"))?;
        let theme = settings.theme(false)?;
        assert_eq!(
            theme.palette[255],
            crate::config::Theme::default().palette[255]
        );
        assert_eq!(theme.cursor, theme.foreground);
        for status in [
            AgentStatus::Working,
            AgentStatus::Blocked,
            AgentStatus::Done,
            AgentStatus::Idle,
            AgentStatus::Unknown,
        ] {
            assert!(settings.status_color(status, false) <= 0xffffff);
        }
    }
    let settings = parsed("")?;
    assert_eq!(settings.theme(false)?.surface, 0x181825);
    assert_eq!(settings.theme(false)?.foreground, 0xcdd6f4);
    assert_eq!(settings.status_color(AgentStatus::Working, false), 0xf9e2af);
    assert_eq!(settings.status_color(AgentStatus::Blocked, false), 0xf38ba8);
    assert_eq!(settings.status_color(AgentStatus::Done, false), 0x94e2d5);
    assert_eq!(settings.status_color(AgentStatus::Idle, false), 0xa6e3a1);
    assert_eq!(settings.status_color(AgentStatus::Unknown, false), 0x6c7086);
    let settings = parsed(
        "[theme]\nname = 'nord'\nauto_switch = true\ndark_name = 'gruvbox'\nlight_name = 'latte'\n[theme.custom]\nred = '#123'\naccent = 'rgb(1, 2, 3)'\n[theme.custom.light]\nred = '#abcdef'\nsidebar_bg = '#fefefe'",
    )?;
    assert_eq!(settings.theme(false)?.surface, 0x282828);
    assert_eq!(settings.theme(true)?.surface, 0xeff1f5);
    // `sidebar_bg` colors the sidebar alone, as in Herdr, never the window.
    assert_eq!(settings.theme(true)?.background, 0xe6e9ef);
    assert_eq!(settings.theme(true)?.sidebar, Some(0xfefefe));
    assert_eq!(settings.theme(true)?.sidebar_background(), 0xfefefe);
    assert_eq!(settings.theme(false)?.sidebar, None);
    assert_eq!(settings.theme(false)?.sidebar_background(), 0x282828);
    assert_eq!(settings.theme(false)?.palette[5], 0x010203);
    assert_eq!(settings.status_color(AgentStatus::Blocked, false), 0x112233);
    assert_eq!(settings.status_color(AgentStatus::Blocked, true), 0xabcdef);
    assert_eq!(
        parsed("[theme]\nname = 'one-dark'\nauto_switch = true")?
            .theme(true)?
            .surface,
        0xfafafa
    );
    assert_eq!(
        parsed("[theme]\nname = 'unknown'")?.theme(false)?,
        parsed("")?.theme(false)?
    );
    // Invalid Unicode hex must never slice a code point or panic.
    assert_eq!(
        parsed("[theme.custom]\nred = '#\u{e9}\u{e9}\u{e9}'")?
            .status_color(AgentStatus::Blocked, false),
        0x008080
    );
    Ok(())
}

/// Herdr paints `sidebar_bg` on the sidebar and nowhere else, whether set for
/// both modes or per mode; built-in themes leave it unset.
#[test]
fn sidebar_background_colors_only_the_sidebar() -> anyhow::Result<()> {
    let plain = parsed("[theme]\nname = 'catppuccin'")?.theme(false)?;
    assert_eq!(plain.sidebar, None);
    assert_eq!(plain.sidebar_background(), plain.surface);
    for light in [false, true] {
        let base = parsed("[theme]\nname = 'catppuccin'\nauto_switch = true")?.theme(light)?;
        let common = parsed(
            "[theme]\nname = 'catppuccin'\nauto_switch = true\n[theme.custom]\nsidebar_bg = '#0d0e0f'",
        )?
        .theme(light)?;
        assert_eq!(common.sidebar, Some(0x0d0e0f), "light {light}");
        assert_eq!(
            crate::config::Theme {
                sidebar: None,
                ..common
            },
            base,
            "light {light}: nothing but the sidebar changes"
        );
        let per_mode = parsed(
            "[theme]\nname = 'catppuccin'\nauto_switch = true\n[theme.custom]\nsidebar_bg = '#0d0e0f'\n[theme.custom.light]\nsidebar_bg = '#f0f1f2'\n[theme.custom.dark]\nsidebar_bg = '#101112'",
        )?
        .theme(light)?;
        assert_eq!(
            per_mode.sidebar,
            Some(if light { 0xf0f1f2 } else { 0x101112 })
        );
    }
    Ok(())
}
