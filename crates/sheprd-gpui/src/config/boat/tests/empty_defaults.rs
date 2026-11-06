use super::*;
use crate::boat::{DEFAULT_API_URL, Settings};
use crate::cloud::ProviderError;

#[test]
fn uncommented_boat_example_loads_with_account_defaults() {
    let example = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/config-gpui.example.toml"
    ));
    let text = example
        .lines()
        .skip_while(|line| *line != "# [boat]")
        .take_while(|line| line.starts_with("# "))
        .map(|line| line.strip_prefix("# ").unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    let config = Config::parse(&text).unwrap();
    let settings = config.boat.settings_with(|_| None).unwrap().unwrap();
    let defaults = Settings::resolve(
        &BoatConfig {
            api_url: Some(DEFAULT_API_URL.into()),
            ..BoatConfig::default()
        },
        |_| None,
    )
    .unwrap()
    .unwrap();
    assert_eq!(settings, defaults);
}

#[test]
fn nonempty_invalid_optional_values_remain_invalid() {
    for (field, value) in [
        ("organization", " "),
        ("organization", "team#other"),
        ("machine_type", " "),
        ("machine_type", "enormous"),
        ("environment", " "),
        ("environment", "base#other"),
    ] {
        let config: BoatConfig = toml::from_str(&format!(
            "api_url = {DEFAULT_API_URL:?}\n{field} = {value:?}\n"
        ))
        .unwrap();
        assert!(matches!(
            config.settings_with(|_| None),
            Err(sheprd_remote::Error::Provider(ProviderError::Boat(
                crate::boat::Error::Field(_)
            )))
        ));
    }
}
