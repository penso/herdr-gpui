//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the Review tab: its
//! header, the changed files, each kind of file the diff cannot show yet,
//! a note in the notes panel, Find in changes, and a branch review whose
//! base list could not be read.
use super::{files::many_files, the, window};
use crate::{
    i18n::leak_tests::{assert_translated, draw, uncut, under_en_xa},
    review::diff::{Body, RowId, Scope, Unreadable},
};
use gpui::{px, size};

const NOTE: &str = "Name this better";

/// What the checkout holds: its branch, the files' paths and lines, and the
/// note the user wrote.
fn data() -> Vec<String> {
    let mut data: Vec<String> = [
        "feature",
        "README.md",
        "src/",
        "tests/",
        "a.rs",
        "b.rs",
        "c.rs",
    ]
    .into_iter()
    .chain([
        "z.txt",
        "src/a.rs",
        "src/b.rs",
        "tests/c.rs",
        "# demo",
        "old",
    ])
    .chain(["new", NOTE])
    .map(str::to_owned)
    .collect();
    data.extend((0..80).map(|line| format!("line {line}")));
    data
}

#[test]
fn review_shows_only_messages_and_the_checkout_under_en_xa() {
    under_en_xa("Review en-XA leak", |cx, recorder| {
        let (view, cx) = window(cx, None);
        cx.simulate_resize(size(px(2000.), px(1400.)));
        draw(cx);
        recorder.take();
        let data = data();

        let mut loaded = many_files();
        let bodies = [
            Body::Binary,
            Body::Large,
            Body::Failed(Unreadable::Change),
            Body::Failed(Unreadable::TooLarge),
        ];
        for (file, body) in loaded.diff.files.iter_mut().skip(1).zip(bodies) {
            file.body = body;
        }
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.seed_review(loaded, window, cx);
            })
        });
        draw(cx);
        for placeholder in [
            "review-placeholder-1",
            "review-placeholder-2",
            "review-placeholder-3",
            "review-placeholder-4",
        ] {
            assert!(cx.debug_bounds(placeholder).is_some(), "{placeholder}");
        }
        assert_translated("Review changes", &uncut(recorder.take()), &data);

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let id = the(view);
                view.begin_review_note(id, RowId::Header(0), window, cx);
                let input = view.reviews[&id].input.clone();
                input.update(cx, |input, cx| input.set_text_selected(NOTE, cx));
                view.add_review_note(id, window, cx);
            })
        });
        draw(cx);
        assert_translated("notes panel", &uncut(recorder.take()), &data);

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_review_search(the(view), window, cx)
            })
        });
        draw(cx);
        assert_translated("Find in changes", &uncut(recorder.take()), &data);

        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.set_review_scope(the(view), Scope::Branch, cx)
            })
        });
        cx.run_until_parked();
        draw(cx);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.open_review_bases(the(view), window, cx))
        });
        cx.run_until_parked();
        draw(cx);
        assert!(cx.debug_bounds("review-base-menu").is_some());
        assert_translated(
            "a branch review without a base",
            &uncut(recorder.take()),
            &data,
        );
    });
}
