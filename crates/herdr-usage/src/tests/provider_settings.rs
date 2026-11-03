use super::*;
use crate::settings::ProviderSettings;

#[test]
fn config_ignores_unknown_providers_and_settings() {
    let parse = |text: &str| {
        let mut config: UsageConfig = toml::from_str(text).unwrap();
        let unknown = config.retain_known();
        (config, unknown)
    };
    let (config, unknown) = parse("show_providers = [\"claude\"]\nhide_providers = [\"codex\"]");
    assert!(config.shown(provider("claude")));
    assert!(config.hidden(provider("codex")));
    assert!(config.show);
    assert!(unknown.is_empty());
    // Names a newer build may know are dropped and reported, not fatal.
    let (config, unknown) = parse(
        "show_providers = [\"nope\", \"claude\"]\nhide_providers = [\"later\"]\n\
         [providers.claude]\napi_key = \"x\"\n[providers.future]\ntoken = \"y\"",
    );
    assert!(config.shown(provider("claude")));
    assert_eq!(config.show_providers, ["claude"]);
    assert!(config.hide_providers.is_empty());
    assert!(!config.providers.contains_key("future"));
    assert!(
        config
            .settings(provider("claude"))
            .is_none_or(|settings| settings.get("api_key").is_none())
    );
    assert_eq!(
        unknown,
        [
            "usage.show_providers.nope",
            "usage.hide_providers.later",
            "usage.providers.claude.api_key",
            "usage.providers.future",
        ]
    );
}

#[test]
fn settings_come_from_this_machines_config() {
    let settings = ProviderSettings::default().with("api_key", "config-key");
    let mut exec = Exec::Local;
    let mut jar = CookieJar::default();
    let probe = Probe::new(
        &mut exec,
        provider("claude"),
        Some(&settings),
        &mut jar,
        Consent::Quiet,
    );
    assert_eq!(probe.text_setting("api_key").as_deref(), Some("config-key"));
    assert!(probe.setting("missing").is_none());
}
