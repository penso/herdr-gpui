#![allow(clippy::unwrap_used)]
use super::{
    super::super::{Section, SettingsWindow},
    Account, CloudProvider, Overview, Token,
};
use crate::config::SpritesConfig;
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
            view.config.sprites = SpritesConfig::default();
            view.select_section(Section::CloudDevices, window, cx);
            view.cloud_tab = CloudProvider::Sprites;
            cx.notify();
        })
    });
    (view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

#[gpui::test]
fn an_unconfigured_account_offers_fly_and_cannot_create(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    draw(cx);
    assert!(cx.debug_bounds("cloud-sprites-card").is_some());
    view.read_with(cx, |view, cx| {
        let card = view.sprites_card.as_ref().unwrap();
        assert!(card.job.is_none(), "nothing to check without an account");
        assert_eq!(card.values(cx).api_url, crate::sprites::DEFAULT_API_URL);
        assert!(matches!(
            &card.account,
            Account::Known(Overview {
                token: Token::Missing,
                ..
            })
        ));
    });
    let create = cx.debug_bounds("cloud-sprites-create").unwrap().center();
    cx.simulate_click(create, Default::default());
    view.read_with(cx, |view, cx| {
        let main = view.source.upgrade().unwrap();
        assert!(main.read(cx).cloud_jobs.is_empty());
    });
}
