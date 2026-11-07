use super::*;
use crate::config::preferences::Preference;

#[test]
fn tabs_keep_their_numbers_until_chosen() -> anyhow::Result<()> {
    assert!(!Config::default().tab_labels.agent_title);
    assert!(!Config::parse("")?.tab_labels.agent_title);
    assert!(!Config::parse(DEFAULT_CONFIG)?.tab_labels.agent_title);
    assert!(
        Config::parse("[tab_labels]\nagent_title = true")?
            .tab_labels
            .agent_title
    );
    assert!(Config::parse("[tab_labels]\nagent_title = 'yes'").is_err());
    Ok(())
}

#[test]
fn an_unknown_tab_labels_key_is_reported() -> anyhow::Result<()> {
    let config = Config::parse("[tab_labels]\nagent_titles = true\n")?;
    assert_eq!(config.unknown_keys, ["tab_labels.agent_titles"]);
    assert!(!config.tab_labels.agent_title);
    Ok(())
}

#[test]
fn choosing_agent_titles_saves_in_place() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("local.toml");
    fs::write(&path, "# personal\nlayout = 'orca' # mode\n")?;
    Config::save_preference_path(Preference::TabAgentTitle(true), &path)?;
    let text = fs::read_to_string(&path)?;
    assert!(text.contains("# personal") && text.contains("# mode"));
    assert!(Config::parse(&text)?.tab_labels.agent_title);
    Ok(())
}
