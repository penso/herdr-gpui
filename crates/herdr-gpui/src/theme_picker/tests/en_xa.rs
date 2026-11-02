//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the color scheme
//! picker: its frame and status are messages; theme names are data.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, under_en_xa};

#[test]
fn the_theme_picker_shows_only_translated_text_under_en_xa() {
    under_en_xa("theme picker en-XA leak", |cx, recorder| {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        cx.simulate_resize(size(px(800.), px(600.)));
        draw(cx);
        recorder.take();
        cx.update(|window, cx| view.update(cx, |view, cx| view.open_theme_picker(window, cx)));
        draw(cx);
        // Every theme there is may be listed: their names are data.
        let names = view.read_with(cx, |view, _| {
            view.menu.themes.as_ref().unwrap().names.clone()
        });
        assert_translated("theme picker", &recorder.take(), &names);
    });
}
