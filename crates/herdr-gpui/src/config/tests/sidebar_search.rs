use super::*;

#[test]
fn the_sidebar_search_shows_unless_turned_off() -> anyhow::Result<()> {
    assert!(Config::default().show_sidebar_search);
    assert!(Config::parse("")?.show_sidebar_search);
    assert!(Config::parse(DEFAULT_CONFIG)?.show_sidebar_search);
    assert!(!Config::parse("show_sidebar_search = false")?.show_sidebar_search);
    Ok(())
}
