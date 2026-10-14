use super::super::{Host, Message, Usage};
use super::*;
use std::collections::HashSet;

#[test]
fn detected_providers_include_hidden_usage_and_access_but_not_empty_or_setup_entries() {
    let reading = |id: &str, report, access| Reading {
        provider: provider(id),
        report,
        error: None,
        access,
    };
    let entry = super::super::Entry {
        readings: vec![
            reading(
                "codex",
                Some(Report::new(provider("codex"), Account::default(), vec![])),
                None,
            ),
            reading("claude", Some(report(provider("claude"), 10.)), None),
            reading("cursor", None, None),
            reading("gemini", None, Some(super::super::Access::Needed)),
            reading("zed", None, Some(super::super::Access::Denied)),
        ],
        ..Default::default()
    };
    let config: UsageConfig =
        toml::from_str("show_providers = [\"cursor\"]\nhide_providers = [\"claude\"]").unwrap();
    assert_eq!(
        entry
            .detected()
            .map(|provider| provider.id())
            .collect::<Vec<_>>(),
        ["claude", "gemini", "zed"]
    );
    assert_eq!(
        entry
            .tabs(&config)
            .iter()
            .map(|reading| reading.provider.id())
            .collect::<Vec<_>>(),
        ["cursor", "gemini", "zed"]
    );
}

#[test]
fn visibility_changes_filter_headline_and_tabs_without_refreshing() {
    let claude = provider("claude");
    let codex = provider("codex");
    let entry = super::super::Entry {
        readings: vec![
            Reading {
                provider: codex,
                report: Some(report(codex, 10.)),
                error: None,
                access: None,
            },
            Reading {
                provider: claude,
                report: Some(report(claude, 90.)),
                error: None,
                access: None,
            },
        ],
        ..Default::default()
    };
    let mut config = UsageConfig::default();
    assert_eq!(entry.headline(&config, 2, Some(claude))[0].provider, claude);
    config.hide_providers.push(claude.id().into());
    config.show_providers.push(claude.id().into());
    assert_eq!(
        entry
            .headline(&config, 2, Some(claude))
            .iter()
            .map(|reading| reading.provider)
            .collect::<Vec<_>>(),
        [codex]
    );
    assert_eq!(
        entry
            .tabs(&config)
            .iter()
            .map(|reading| reading.provider)
            .collect::<Vec<_>>(),
        [codex]
    );
    assert_eq!(entry.detected().collect::<Vec<_>>(), [codex, claude]);
    config.hide_providers.clear();
    assert_eq!(entry.headline(&config, 2, Some(claude))[0].provider, claude);
    assert_eq!(entry.tabs(&config).len(), 2);
}
#[test]
fn hidden_readings_survive_refresh_and_reenable_from_cache_while_signed_out_readings_drop() {
    let now = Instant::now();
    let (claude, codex, gemini) = (provider("claude"), provider("codex"), provider("gemini"));
    let mut usage = Usage::default();
    let mut config = UsageConfig::default();
    begin(&mut usage, &Host::Local, now);
    for provider in [claude, codex, gemini] {
        usage.apply(
            Message::Reading(Host::Local, provider, Ok(report(provider, 20.))),
            &config,
            now,
        );
    }
    usage.apply(Message::Done(Host::Local, Ok(())), &config, now);

    config.hide_providers.push(claude.id().into());
    begin(&mut usage, &Host::Local, now);
    usage.apply(
        Message::Reading(Host::Local, codex, Ok(report(codex, 5.))),
        &config,
        now,
    );
    usage.apply(Message::Done(Host::Local, Ok(())), &config, now);
    let entry = usage.current().unwrap();
    assert_eq!(entry.detected().collect::<Vec<_>>(), [codex, claude]);
    assert_eq!(entry.headline(&config, 2, Some(claude))[0].provider, codex);
    assert_eq!(entry.tabs(&config).len(), 1);

    // Re-enabling an inactive window exposes its cached report without
    // starting a worker, even though the config revision makes it due.
    config.hide_providers.clear();
    usage.poll(Some(Host::Local), &config, &HashSet::new(), 1, false, now);
    assert!(usage.worker.is_none());
    assert!(!usage.busy());
    let entry = usage.current().unwrap();
    assert_eq!(entry.headline(&config, 2, Some(claude))[0].provider, claude);
    assert_eq!(entry.tabs(&config).len(), 2);
    assert_eq!(
        entry
            .readings
            .iter()
            .find(|reading| reading.provider == claude)
            .unwrap()
            .report,
        Some(report(claude, 20.))
    );

    // Once enabled, a refresh that cannot find that sign-in removes it.
    begin(&mut usage, &Host::Local, now);
    usage.apply(
        Message::Reading(Host::Local, codex, Ok(report(codex, 5.))),
        &config,
        now,
    );
    usage.apply(Message::Done(Host::Local, Ok(())), &config, now);
    assert_eq!(
        usage.current().unwrap().detected().collect::<Vec<_>>(),
        [codex]
    );
}
