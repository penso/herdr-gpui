use super::*;
use crate::config::preferences::Preference;

#[test]
fn tab_layouts_load_by_name_and_refuse_unknown_ones() -> anyhow::Result<()> {
    assert_eq!(Config::default().tab_layout, TabLayoutMode::Classic);
    assert_eq!(Config::parse("")?.tab_layout, TabLayoutMode::Classic);
    assert_eq!(
        Config::parse(DEFAULT_CONFIG)?.tab_layout,
        TabLayoutMode::Classic
    );
    for &mode in TabLayoutMode::ALL {
        let text = format!("tab_layout = '{mode}'");
        assert_eq!(Config::parse(&text)?.tab_layout, mode);
        assert_eq!(TabLayoutMode::try_from(mode.name())?, mode);
    }
    assert!(Config::parse("tab_layout = 'tiles'").is_err());
    assert!(matches!(
        TabLayoutMode::try_from("tiles"),
        Err(crate::config::tab_layout::Error::Unknown(name)) if name == "tiles"
    ));
    Ok(())
}

#[test]
fn tab_layout_names_and_labels_are_unique() {
    let mut names: Vec<_> = TabLayoutMode::ALL.iter().map(|mode| mode.name()).collect();
    let mut labels: Vec<_> = TabLayoutMode::ALL.iter().map(|mode| mode.label()).collect();
    names.sort_unstable();
    names.dedup();
    labels.sort_unstable();
    labels.dedup();
    assert_eq!(names.len(), TabLayoutMode::ALL.len());
    assert_eq!(labels.len(), TabLayoutMode::ALL.len());
}

#[test]
fn the_tab_layout_saves_in_place() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("local.toml");
    fs::write(&path, "# personal\nlayout = 'orca' # mode\n")?;
    Config::save_preference_path(Preference::TabLayout(TabLayoutMode::Pills), &path)?;
    let text = fs::read_to_string(&path)?;
    assert!(text.contains("# personal") && text.contains("# mode"));
    let config = Config::parse(&text)?;
    assert_eq!(config.tab_layout, TabLayoutMode::Pills);
    assert_eq!(config.layout.mode, LayoutMode::Orca);
    Ok(())
}
