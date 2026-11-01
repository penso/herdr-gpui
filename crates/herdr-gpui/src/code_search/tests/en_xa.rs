//! The `en-XA` leak test (`crate::i18n::leak_tests`) for code search: Go to
//! File and Go to Symbol over a checkout.
use super::{Mode, checkout, search, window};
use crate::i18n::leak_tests::{assert_translated, draw, uncut, under_en_xa};

#[test]
fn code_search_shows_only_messages_under_en_xa() {
    under_en_xa("code search en-XA leak", |cx, recorder| {
        let dir = checkout();
        let (view, cx) = window(cx, Some(dir.path()));
        draw(cx);
        recorder.take();
        for mode in [Mode::Files, Mode::Symbols] {
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.dismiss_menu(window, cx);
                    view.open_code_search(mode, window, cx);
                })
            });
            cx.run_until_parked();
            draw(cx);
            // The header names the checkout as its index does, which differs
            // by platform (a canonical path, forward slashes on Windows).
            let root = search(&view, cx, |code| {
                code.index.as_ref().unwrap().root().display().to_string()
            });
            // Paths and the names the checkout defines are its own.
            let data = [
                root,
                "src/lib.rs".to_owned(),
                "alpha".to_owned(),
                "Beta".to_owned(),
                "src/lib.rs:1".to_owned(),
                "src/lib.rs:3".to_owned(),
            ];
            assert_translated(&format!("{mode:?}"), &uncut(recorder.take()), &data);
        }
    });
}
