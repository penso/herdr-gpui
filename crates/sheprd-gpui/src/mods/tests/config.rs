use super::*;

#[test]
fn mods_run_by_default() {
    let config = Config::default();
    assert!(config.enabled);
    assert!(config.disabled.is_empty());
    config.validate().unwrap();
}

#[test]
fn disabled_names_are_bounded() {
    let many = Config {
        disabled: (0..65).map(|n| format!("m{n}")).collect(),
        ..Config::default()
    };
    assert!(matches!(
        many.validate(),
        Err(crate::Error::Mods(Error::TooManyDisabled(64)))
    ));
    let control = Config {
        disabled: vec!["bad\u{7}".into()],
        ..Config::default()
    };
    assert!(matches!(
        control.validate(),
        Err(crate::Error::Mods(Error::DisabledName(_)))
    ));
}

#[test]
fn the_table_parses_from_toml() {
    let config: Config = toml::from_str("disabled = [\"agents\"]\n").unwrap();
    assert_eq!(config.disabled, ["agents"]);
    assert!(config.enabled);
    assert!(toml::from_str::<Config>("enable = false\n").is_err());
}
