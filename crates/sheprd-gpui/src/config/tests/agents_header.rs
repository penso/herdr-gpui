use super::*;
use crate::config::preferences::Preference;

#[test]
fn the_agents_header_shows_unless_turned_off() -> anyhow::Result<()> {
    assert!(Config::default().show_agents_header);
    assert!(Config::parse("")?.show_agents_header);
    assert!(Config::parse(DEFAULT_CONFIG)?.show_agents_header);
    assert!(!Config::parse("show_agents_header = false")?.show_agents_header);
    assert!(Config::parse("show_agents_header = 0").is_err());
    Ok(())
}

#[test]
fn the_agents_header_saves_in_place() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("local.toml");
    fs::write(&path, "# personal\nlayout = 'orca' # mode\n")?;
    Config::save_preference_path(Preference::ShowAgentsHeader(false), &path)?;
    let text = fs::read_to_string(&path)?;
    assert!(text.contains("# personal") && text.contains("# mode"));
    assert!(!Config::parse(&text)?.show_agents_header);
    Ok(())
}
