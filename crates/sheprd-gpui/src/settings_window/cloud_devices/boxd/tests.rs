#![allow(clippy::unwrap_used)]
use super::{
    super::super::{Section, SettingsWindow},
    Account, CloudProvider, Key, Overview,
};
use crate::config::BoxdConfig;
use gpui::{Entity, TestAppContext, VisualTestContext, px, size};

fn open(cx: &mut TestAppContext) -> (Entity<SettingsWindow>, &mut VisualTestContext) {
    let main = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| main.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = SettingsWindow::new(weak, cx);
        window.focus(&view.focus, cx);
        view
    });
    cx.simulate_resize(size(px(960.), px(900.)));
    // An unconfigured account reads nothing from disk or the network.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.boxd = BoxdConfig::default();
            view.select_section(Section::CloudDevices, window, cx);
            view.cloud_tab = CloudProvider::Boxd;
            cx.notify();
        })
    });
    (view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

#[gpui::test]
fn an_unconfigured_account_offers_boxd_and_cannot_create(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    draw(cx);
    assert!(cx.debug_bounds("cloud-boxd-card").is_some());
    view.read_with(cx, |view, cx| {
        let card = view.boxd_card.as_ref().unwrap();
        assert!(card.job.is_none(), "nothing to check without an account");
        assert_eq!(card.values(cx).api_url, crate::boxd::DEFAULT_API_URL);
        assert!(matches!(
            &card.account,
            Account::Known(Overview {
                key: Key::Missing,
                ..
            })
        ));
    });
    let create = cx.debug_bounds("cloud-boxd-create").unwrap().center();
    cx.simulate_click(create, Default::default());
    view.read_with(cx, |view, cx| {
        let main = view.source.upgrade().unwrap();
        assert!(main.read(cx).cloud_jobs.is_empty());
    });
}

/// Failures from the account check and from a removal show in the chosen
/// language, not as the error's English text.
#[test]
fn failures_show_in_the_chosen_language() {
    use super::Update;
    use crate::i18n::Localize;
    use crate::i18n::leak_tests::{assert_translated, draw, under_en_xa};
    use sheprd_remote::boxd::Error;
    under_en_xa("boxd failures en-XA", |cx, recorder| {
        let (view, cx) = open(cx);
        for (update, error) in [
            (
                Update::Overview(Err(Error::Authentication)),
                Error::Authentication,
            ),
            (Update::Done(Err(Error::Account)), Error::Account),
        ] {
            cx.update(|_, cx| view.update(cx, |view, cx| view.apply_boxd(update, cx)));
            view.read_with(cx, |view, _| {
                let message = view.boxd_card.as_ref().unwrap().message.clone();
                assert_eq!(message, Some(error.localize()));
                assert_ne!(message, Some(error.to_string()));
            });
            recorder.take();
            cx.update(|_, cx| view.update(cx, |_, cx| cx.notify()));
            draw(cx);
            assert_translated(
                "Settings > Cloud devices > boxd failure",
                &recorder.take(),
                &[crate::boxd::DEFAULT_API_URL.into()],
            );
        }
    });
}
