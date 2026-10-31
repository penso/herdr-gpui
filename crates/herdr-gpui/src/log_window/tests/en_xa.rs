//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the log window:
//! its controls and status are messages; the records it lists are not.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, under_en_xa};

#[test]
fn the_log_window_shows_only_translated_text_under_en_xa() {
    under_en_xa("log window en-XA leak", |cx, recorder| {
        let (_view, cx) = cx.add_window_view(LogWindow::new);
        draw(cx);
        assert_translated("log window", &recorder.take(), &[]);
    });
}
