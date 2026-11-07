use super::*;

#[test]
fn the_repository_icons_show_unless_turned_off() -> anyhow::Result<()> {
    assert!(Config::default().show_repository_icons);
    assert!(Config::parse("")?.show_repository_icons);
    assert!(Config::parse(DEFAULT_CONFIG)?.show_repository_icons);
    assert!(!Config::parse("show_repository_icons = false")?.show_repository_icons);
    Ok(())
}
