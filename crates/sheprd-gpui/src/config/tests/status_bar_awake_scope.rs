use super::*;
use crate::config::{
    preferences::Preference,
    status_bar::{AwakeScope, Edit},
};

#[test]
fn keep_awake_scope_defaults_to_the_display_and_parses() -> anyhow::Result<()> {
    assert_eq!(
        Config::parse("")?.status_bar.keep_awake_scope,
        AwakeScope::Display
    );
    let config = Config::parse("[status_bar]\nkeep_awake_scope = 'system'\n")?;
    assert_eq!(config.status_bar.keep_awake_scope, AwakeScope::System);
    assert!(Config::parse("[status_bar]\nkeep_awake_scope = 'screen'\n").is_err());
    Ok(())
}

#[test]
fn keep_awake_scope_edit_round_trips_through_the_overrides_file() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("local.toml");
    for scope in [AwakeScope::System, AwakeScope::Display, AwakeScope::System] {
        Config::save_preference_path(Preference::StatusBar(Edit::KeepAwakeScope(scope)), &path)?;
        let config = Config::parse(&fs::read_to_string(&path)?)?;
        assert_eq!(config.status_bar.keep_awake_scope, scope);
    }
    Ok(())
}
