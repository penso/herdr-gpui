//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the file tree tab:
//! its header and messages are messages; file names, the folders' paths,
//! and the change letters are data.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, under_en_xa};
use gpui::{px, size};

#[test]
fn a_file_tree_shows_only_translated_text_under_en_xa() {
    let dir = checkout();
    under_en_xa("file tree en-XA leak", |cx, recorder| {
        let (view, cx) = window(cx);
        cx.simulate_resize(size(px(1600.), px(900.)));
        draw(cx);
        recorder.take();
        open(&view, cx, dir.path());
        draw(cx);
        let data: Vec<String> = [
            "docs/", "notes/", "src/", "new.txt", "gone.rs", "main.rs", "M", "D", "U",
        ]
        .map(str::to_owned)
        .to_vec();
        assert_translated("file tree", &recorder.take(), &data);
    });
}
