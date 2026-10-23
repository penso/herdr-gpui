use super::*;

#[test]
fn the_status_bar_orders_providers_by_selection_and_usage() {
    let reading = |id: &str, used: Option<f64>| Reading {
        provider: provider(id),
        report: used.map(|used| report(provider(id), used)),
        error: None,
        access: None,
    };
    let mut entry = super::super::Entry {
        readings: vec![
            reading("codex", Some(12.)),
            reading("gemini", None),
            reading("copilot", Some(4.)),
            reading("claude", Some(32.)),
            reading("zed", Some(12.)),
        ],
        ..Default::default()
    };
    // An answer with neither windows nor balances has nothing to show.
    entry.readings.push(Reading {
        provider: provider("azureopenai"),
        report: Some(Report::new(
            provider("azureopenai"),
            Account::default(),
            vec![],
        )),
        error: None,
        access: None,
    });
    let ids = |chosen: Option<&str>| {
        entry
            .headline(chosen.map(provider))
            .iter()
            .map(|reading| reading.provider.id())
            .collect::<Vec<_>>()
    };
    // Ties keep the registry order.
    assert_eq!(ids(None), ["claude", "codex", "zed", "copilot"]);
    // A provider picked in the panel leads, then the closest to a limit.
    assert_eq!(ids(Some("copilot")), ["copilot", "claude", "codex", "zed"]);
    assert_eq!(ids(Some("codex")), ["codex", "claude", "zed", "copilot"]);
    // One with nothing to show on this host leaves the bar as it was.
    assert_eq!(ids(Some("gemini")), ["claude", "codex", "zed", "copilot"]);
}

#[test]
fn panel_tabs_leave_out_sign_ins_with_nothing_to_show() {
    let with = |id: &str| Reading {
        provider: provider(id),
        report: Some(report(provider(id), 5.)),
        error: None,
        access: None,
    };
    let without = |id: &str| Reading {
        provider: provider(id),
        report: None,
        error: Some(usage::Error::NoPlan.to_string()),
        access: None,
    };
    let entry = super::super::Entry {
        readings: vec![with("codex"), without("gemini"), without("cursor")],
        ..Default::default()
    };
    let tabs = |config: &UsageConfig| {
        entry
            .tabs(config)
            .iter()
            .map(|reading| reading.provider.id())
            .collect::<Vec<_>>()
    };
    assert_eq!(tabs(&UsageConfig::default()), ["codex"]);
    // Asked for by the config: shown, so its panel can say what to set up.
    let asked: UsageConfig = toml::from_str("show_providers = [\"cursor\"]").unwrap();
    assert_eq!(tabs(&asked), ["codex", "cursor"]);
}
