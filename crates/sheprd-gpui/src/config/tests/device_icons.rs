use super::*;

#[test]
fn the_device_icons_show_unless_turned_off() -> anyhow::Result<()> {
    assert!(Config::default().show_device_icons);
    assert!(Config::parse("")?.show_device_icons);
    assert!(Config::parse(DEFAULT_CONFIG)?.show_device_icons);
    assert!(!Config::parse("show_device_icons = false")?.show_device_icons);
    Ok(())
}

#[test]
fn only_layouts_whose_rows_draw_icons_mark_their_devices() {
    for mode in LayoutMode::ALL {
        let drawn = !matches!(mode, LayoutMode::Orca | LayoutMode::Minimal);
        assert_eq!(mode.device_icons(), drawn, "{mode}");
    }
}
