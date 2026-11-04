use super::*;

#[test]
fn the_activity_bar_is_on_unless_turned_off() -> anyhow::Result<()> {
    for config in [Config::default(), Config::parse(DEFAULT_CONFIG)?] {
        assert!(config.sidebar_style.activity_bar);
    }
    let off = Config::parse("[sidebar]\nactivity_bar = false")?;
    assert!(!off.sidebar_style.activity_bar);
    assert!(Config::parse("[sidebar]\nactivity_bar = \"off\"").is_err());
    Ok(())
}

#[test]
fn turning_the_activity_bar_off_writes_the_sidebar_key() -> anyhow::Result<()> {
    use crate::config::preferences::Preference;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("local.toml");
    fs::write(&path, "[sidebar]\nfamily = 'Inter' # font\n")?;
    for shown in [false, true] {
        Config::save_preference_path(Preference::SidebarActivityBar(shown), &path)?;
        let text = fs::read_to_string(&path)?;
        assert!(text.contains("family = 'Inter' # font"), "{text}");
        assert_eq!(Config::parse(&text)?.sidebar_style.activity_bar, shown);
    }
    Ok(())
}

#[test]
fn automations_are_on_unless_turned_off_and_write_their_key() -> anyhow::Result<()> {
    use crate::config::preferences::Preference;
    for config in [Config::default(), Config::parse(DEFAULT_CONFIG)?] {
        assert!(config.sidebar_style.automations);
    }
    assert!(
        !Config::parse("[sidebar]\nautomations = false")?
            .sidebar_style
            .automations
    );
    assert!(Config::parse("[sidebar]\nautomations = \"off\"").is_err());
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("local.toml");
    fs::write(&path, "[sidebar]\nactivity_bar = true\n")?;
    for shown in [false, true] {
        Config::save_preference_path(Preference::SidebarAutomations(shown), &path)?;
        let text = fs::read_to_string(&path)?;
        let config = Config::parse(&text)?;
        assert_eq!(config.sidebar_style.automations, shown);
        assert!(config.sidebar_style.activity_bar, "{text}");
    }
    Ok(())
}
