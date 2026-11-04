use super::*;
use crate::config::{AgentsPlacement, preferences::Preference};

#[test]
fn agents_placement_is_named_and_defaults_to_the_panel() -> anyhow::Result<()> {
    for config in [
        Config::default(),
        Config::parse("")?,
        Config::parse(DEFAULT_CONFIG)?,
    ] {
        assert_eq!(config.agents_placement, AgentsPlacement::Panel);
    }
    for placement in [AgentsPlacement::Panel, AgentsPlacement::Nested] {
        let text = format!("agents_placement = \"{}\"", placement.as_str());
        assert_eq!(Config::parse(&text)?.agents_placement, placement);
    }
    for bad in ["\"Nested\"", "\"inline\"", "true"] {
        assert!(Config::parse(&format!("agents_placement = {bad}")).is_err());
    }
    Ok(())
}

#[test]
fn agents_placement_saves_in_place_and_keeps_other_settings() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config.toml");
    let original = "theme = 'Nord' # keep\nagents_placement = \"panel\" # mine\n";
    fs::write(&path, original)?;
    Config::save_preference_path(Preference::AgentsPlacement(AgentsPlacement::Nested), &path)?;
    let text = fs::read_to_string(&path)?;
    assert!(text.contains("theme = 'Nord' # keep"));
    assert!(text.contains("agents_placement = \"nested\" # mine"));
    assert_eq!(
        Config::parse(&text)?.agents_placement,
        AgentsPlacement::Nested
    );
    Config::save_preference_path(Preference::AgentsPlacement(AgentsPlacement::Panel), &path)?;
    assert_eq!(fs::read_to_string(&path)?, original);
    Ok(())
}
