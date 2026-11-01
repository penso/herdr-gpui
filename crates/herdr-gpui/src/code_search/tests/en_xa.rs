//! The `en-XA` leak test (`crate::i18n::leak_tests`) for code search: Go to
//! File and Go to Symbol over a checkout, and the picker without one.
use super::{Mode, checkout, window};
use crate::i18n::leak_tests::{assert_translated, draw, uncut, under_en_xa};

#[test]
fn code_search_shows_only_messages_under_en_xa() {
    under_en_xa("code search en-XA leak", |cx, recorder| {
        let dir = checkout();
        // The index names the checkout by its canonical path.
        let root = dir.path().canonicalize().unwrap().display().to_string();
        let (view, cx) = window(cx, Some(dir.path()));
        draw(cx);
        recorder.take();
        // Paths and the names the checkout defines are its own.
        let data = [
            root.clone(),
            dir.path().display().to_string(),
            "src/lib.rs".to_owned(),
            "alpha".to_owned(),
            "Beta".to_owned(),
            "src/lib.rs:1".to_owned(),
            "src/lib.rs:3".to_owned(),
        ];
        for mode in [Mode::Files, Mode::Symbols] {
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.dismiss_menu(window, cx);
                    view.open_code_search(mode, window, cx);
                })
            });
            cx.run_until_parked();
            draw(cx);
            assert_translated(&format!("{mode:?}"), &uncut(recorder.take()), &data);
        }
    });
}
