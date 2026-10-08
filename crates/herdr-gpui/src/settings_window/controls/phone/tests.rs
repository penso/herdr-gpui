#![allow(clippy::unwrap_used)]
use super::*;
use core::prelude::v1::test;

fn configured() -> crate::notifications::phone::PhoneConfig {
    toml::from_str("enabled = true\n[ntfy]\ntopic = 'herdr-test'").unwrap()
}

fn open(cx: &mut TestAppContext) -> (Entity<SettingsWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(super::super::tests::skill_fixture);
    cx.simulate_resize(size(px(960.), px(2200.)));
    view.update(cx, |view, cx| {
        view.section = Section::Notifications;
        cx.notify();
    });
    (view, cx)
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let point = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(point, Default::default());
    cx.run_until_parked();
}

#[gpui::test]
fn test_send_is_disabled_until_a_service_is_configured(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    click(cx, "settings-phone-test");
    view.read_with(cx, |view, _| assert!(view.controls.phone_test.is_none()));
}

#[gpui::test]
fn test_send_runs_off_the_ui_thread_and_reports_typed_results(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |view, cx| {
        view.config.phone = configured();
        view.controls.phone_send = Some(|service, message| {
            assert!(matches!(service, Service::Ntfy { .. }));
            assert_eq!(message, &Message::test());
            Err(crate::Error::PhoneRejected(401))
        });
        cx.notify();
    });
    click(cx, "settings-phone-test");
    view.read_with(cx, |view, _| {
        assert!(matches!(
            view.controls.phone_test,
            Some(PhoneTest::Failed(crate::Error::PhoneRejected(401)))
        ));
    });
    view.update(cx, |view, _| view.controls.phone_send = Some(|_, _| Ok(())));
    click(cx, "settings-phone-test");
    view.read_with(cx, |view, _| {
        assert!(matches!(view.controls.phone_test, Some(PhoneTest::Sent)));
    });
}
