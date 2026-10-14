use super::*;
use core::prelude::v1::test;

fn saved_config() -> Config {
    let mut config = Config::default();
    let settings = config
        .usage
        .providers
        .entry("openrouter".into())
        .or_default();
    *settings = std::mem::take(settings).with("api_key", "test-only-key");
    config
        .usage
        .providers
        .insert("unknown".into(), Default::default());
    config.usage.hide_providers = vec!["openrouter".into(), "claude".into(), "unknown".into()];
    config
}

#[gpui::test]
fn fresh_saved_and_hide_only_services_are_selectable_without_reports(cx: &mut TestAppContext) {
    let (view, cx) = fixture(cx);
    let edits = recording(cx, &view);
    view.update(cx, |view, cx| {
        view.config = saved_config();
        cx.notify();
    });
    draw(cx);
    for id in [
        "settings-usage-provider-openrouter",
        "settings-usage-provider-claude",
    ] {
        assert!(cx.debug_bounds(id).is_some());
    }
    assert!(cx.debug_bounds("settings-usage-provider-unknown").is_none());
    assert!(cx.debug_bounds("settings-usage-providers-empty").is_none());
    assert_eq!(
        click(cx, &edits, "settings-usage-provider-openrouter"),
        Some(Preference::UsageProviders(ProviderEdit::Enable(
            "openrouter"
        ))),
    );
}

#[gpui::test]
fn loaded_preferences_replace_visible_services_without_source_notifications(
    cx: &mut TestAppContext,
) {
    let (view, cx) = fixture(cx);
    recording(cx, &view);
    draw(cx);
    assert!(
        cx.debug_bounds("settings-usage-provider-openrouter")
            .is_none()
    );
    view.update(cx, |view, cx| {
        let mut loaded = skill_load().unwrap();
        loaded.config = saved_config();
        view.apply_loaded(Ok(loaded), cx);
    });
    draw(cx);
    assert!(
        cx.debug_bounds("settings-usage-provider-openrouter")
            .is_some()
    );
    assert!(cx.debug_bounds("settings-usage-provider-claude").is_some());
    view.update(cx, |view, cx| view.reload_with(skill_load, cx));
    cx.run_until_parked();
    draw(cx);
    assert!(
        cx.debug_bounds("settings-usage-provider-openrouter")
            .is_none()
    );
    assert!(cx.debug_bounds("settings-usage-provider-claude").is_none());
}

#[gpui::test]
fn source_config_changes_refresh_app_level_evidence(cx: &mut TestAppContext) {
    let (view, cx) = fixture(cx);
    recording(cx, &view);
    let source = view.read_with(cx, |view, _| view.source.upgrade().unwrap());
    for present in [true, false] {
        source.update(cx, |source, cx| {
            source.config.usage = if present {
                saved_config().usage
            } else {
                Default::default()
            };
            cx.notify();
        });
        cx.run_until_parked();
        draw(cx);
        assert_eq!(
            cx.debug_bounds("settings-usage-provider-openrouter")
                .is_some(),
            present
        );
        assert_eq!(
            cx.debug_bounds("settings-usage-provider-claude").is_some(),
            present
        );
        assert!(cx.debug_bounds("settings-usage-provider-unknown").is_none());
    }
}
