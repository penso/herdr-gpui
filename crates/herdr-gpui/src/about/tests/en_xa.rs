//! The `en-XA` leak test for the About box: everything but names, the
//! version, and the license draws a message.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, uncut, under_en_xa};

#[test]
fn the_about_box_draws_only_messages() {
    under_en_xa("about en-XA leak", |cx, recorder| {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        draw(cx);
        recorder.take();
        cx.update(|window, cx| view.update(cx, |view, cx| view.open_about(window, cx)));
        cx.update(|window, _| window.refresh());
        draw(cx);
        let names = [
            "Herdr",
            "herdr.dev",
            "GitHub",
            AUTHOR,
            LICENSE,
            COPYRIGHT_YEAR,
            env!("HERDR_BUILD_BRANCH"),
        ]
        .map(str::to_owned);
        assert_translated("About", &uncut(recorder.take()), &names);
    });
}
