use super::*;
use core::prelude::v1::test;
use gpui::{InputEvent, MouseButton, MouseDownEvent, MouseUpEvent};

// Unlike simulate_click, dispatch_event leaves the deterministic executor parked.
// This holds the real preference worker completion until the test releases it.
fn click_before_completion(cx: &mut VisualTestContext, selector: &'static str) {
    draw(cx);
    let position = cx.debug_bounds(selector).unwrap().center();
    cx.update(|window, cx| {
        window.dispatch_event(
            MouseDownEvent {
                position,
                modifiers: Default::default(),
                button: MouseButton::Left,
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseUpEvent {
                position,
                modifiers: Default::default(),
                button: MouseButton::Left,
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
    });
}

#[gpui::test]
fn provider_controls_do_not_edit_during_real_save_or_reload(cx: &mut TestAppContext) {
    let (view, cx) = fixture(cx);
    let edits = recording(cx, &view);
    install(cx, &view, cached(&["claude", "codex"]));
    for reload in [false, true] {
        view.update(cx, |view, cx| {
            view.config.usage.hide_providers = vec!["codex".into()];
            cx.notify();
        });
        if reload {
            view.update(cx, |view, cx| view.reload_with(skill_load, cx));
        } else {
            click_before_completion(cx, "settings-usage-provider-claude");
        }
        view.update(cx, |view, cx| {
            let mut button = view.bulk_choice(
                "test-busy-provider-choice",
                "Turn all on",
                ProviderEdit::EnableMany(view.usage_providers),
                true,
                cx,
            );
            assert_eq!(button.style().opacity, Some(0.5));
        });
        for selector in [
            "settings-usage-provider-claude",
            "settings-usage-provider-codex",
            "settings-usage-providers-all",
            "settings-usage-providers-none",
        ] {
            click_before_completion(cx, selector);
            view.read_with(cx, |view, _| {
                assert!(view.busy());
                assert_eq!(view.loading, reload);
                assert_eq!(view.saving, !reload);
                assert!(
                    !view.config.usage.hidden(
                        usage::providers()
                            .find(|provider| provider.id() == "claude")
                            .unwrap()
                    )
                );
                assert_eq!(view.config.usage.hide_providers, ["codex"]);
            });
        }
        assert!(edits.try_recv().is_err());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(!view.busy());
            assert!(view.config.usage.hide_providers.is_empty());
        });
        if !reload {
            assert_eq!(
                edits.try_recv().unwrap(),
                Preference::UsageProviders(ProviderEdit::Disable("claude"))
            );
        }
        assert!(edits.try_recv().is_err());
        assert_eq!(
            click(cx, &edits, "settings-usage-provider-codex"),
            Some(Preference::UsageProviders(ProviderEdit::Disable("codex"))),
        );
        assert_eq!(
            click(cx, &edits, "settings-usage-providers-none"),
            Some(Preference::UsageProviders(ProviderEdit::DisableMany(
                view.read_with(cx, |view, _| view.usage_providers)
            ))),
        );
    }
}
