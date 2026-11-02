use super::*;
use crate::config::preferences::Preference;

#[test]
fn new_terminals_stay_herdr_1_until_chosen() -> anyhow::Result<()> {
    assert!(!Config::default().herdr_2_terminals);
    assert!(!Config::parse("")?.herdr_2_terminals);
    assert!(!Config::parse(DEFAULT_CONFIG)?.herdr_2_terminals);
    assert!(Config::parse("herdr_2_terminals = true")?.herdr_2_terminals);
    assert!(Config::parse("herdr_2_terminals = 'yes'").is_err());
    Ok(())
}

#[test]
fn choosing_herdr_2_saves_in_place() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("local.toml");
    fs::write(&path, "# personal\nlayout = 'orca' # mode\n")?;
    Config::save_preference_path(Preference::Herdr2Terminals(true), &path)?;
    let text = fs::read_to_string(&path)?;
    assert!(text.contains("# personal") && text.contains("# mode"));
    assert!(Config::parse(&text)?.herdr_2_terminals);
    Ok(())
}
