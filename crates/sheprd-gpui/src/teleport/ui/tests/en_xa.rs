//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the Teleport
//! dialog at each stage: everything but host, repository, and program names
//! is a message.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, under_en_xa};
use core::prelude::v1::test;

#[test]
fn every_teleport_stage_shows_only_translated_text_under_en_xa() {
    under_en_xa("Teleport en-XA leak", |cx, recorder| {
        let data: Vec<String> = DATA.iter().map(|&name| name.to_owned()).collect();
        for (name, teleport) in stages() {
            let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
            cx.simulate_resize(size(px(1200.), px(900.)));
            draw(cx);
            recorder.take();
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.menu.page = Some(Page::Teleport);
                    view.teleport = Some(teleport);
                    window.focus(&view.menu.focus, cx);
                    cx.notify();
                })
            });
            draw(cx);
            assert_translated(&format!("Teleport, {name}"), &recorder.take(), &data);
        }
    });
}
