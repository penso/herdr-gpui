use super::*;
use core::prelude::v1::test;

#[gpui::test]
fn detected_services_stay_listed_when_disabled(cx: &mut TestAppContext) {
    let (view, cx) = fixture(cx);
    install(cx, &view, cached(&["codex", "claude"]));
    draw(cx);
    assert!(cx.debug_bounds("settings-usage-provider-claude").is_some());
    assert!(cx.debug_bounds("settings-usage-provider-codex").is_some());
    assert!(cx.debug_bounds("settings-usage-provider-cursor").is_none());
    assert!(cx.debug_bounds("settings-usage-providers-empty").is_none());
    view.update(cx, |view, cx| {
        view.config.usage.hide_providers = vec!["claude".into(), "cursor".into()];
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("settings-usage-provider-claude").is_some());
    assert!(cx.debug_bounds("settings-usage-provider-cursor").is_some());
}

#[gpui::test]
fn source_notifications_replace_empty_state_without_reopening_settings(cx: &mut TestAppContext) {
    let (view, cx) = fixture(cx);
    let edits = recording(cx, &view);
    view.update(cx, |view, cx| {
        view.config.usage.show_providers = vec!["cursor".into()];
        cx.notify();
    });
    install(
        cx,
        &view,
        vec![usage::Reading {
            provider: usage::providers()
                .find(|provider| provider.id() == "cursor")
                .unwrap(),
            report: None,
            error: Some("Not signed in".into()),
            access: None,
        }],
    );
    draw(cx);
    assert!(cx.debug_bounds("settings-usage-providers-empty").is_some());
    assert!(cx.debug_bounds("settings-usage-provider-cursor").is_none());
    assert_eq!(click(cx, &edits, "settings-usage-providers-all"), None);
    assert_eq!(click(cx, &edits, "settings-usage-providers-none"), None);
    install(cx, &view, cached(&["claude"]));
    draw(cx);
    assert!(cx.debug_bounds("settings-usage-providers-empty").is_none());
    assert!(cx.debug_bounds("settings-usage-provider-claude").is_some());
    install(cx, &view, Vec::new());
    draw(cx);
    assert!(cx.debug_bounds("settings-usage-providers-empty").is_some());
    assert!(cx.debug_bounds("settings-usage-provider-claude").is_none());
}

#[gpui::test]
fn detected_controls_are_idle_while_usage_is_off(cx: &mut TestAppContext) {
    let (view, cx) = fixture(cx);
    let edits = recording(cx, &view);
    install(cx, &view, cached(&["claude"]));
    view.update(cx, |view, cx| {
        view.config.usage.show = false;
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("settings-usage-provider-claude").is_some());
    for selector in [
        "settings-usage-provider-claude",
        "settings-usage-providers-none",
    ] {
        assert_eq!(click(cx, &edits, selector), None, "{selector}");
    }
}
