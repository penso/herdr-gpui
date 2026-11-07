//! Files marked viewed come back in the next review of the same checkout.
use super::files::many_files;
use super::window;
use gpui::{Modifiers, px};

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

#[gpui::test]
fn files_marked_viewed_come_back_in_the_next_review(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, None);
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    cx.update(|window, cx| view.update(cx, |view, cx| view.seed_review(many_files(), window, cx)));
    draw(cx);
    let viewed = cx.debug_bounds("review-viewed-0").unwrap();
    cx.simulate_click(viewed.center(), Modifiers::default());
    draw(cx);

    // A later review of the checkout, as after a restart, starts with the
    // file still marked and folded, and the others as they were.
    let next = cx.update(|window, cx| {
        view.update(cx, |view, cx| view.seed_review(many_files(), window, cx))
    });
    view.read_with(cx, |view, _| {
        let review = &view.reviews[&next];
        assert!(review.is_viewed(0));
        assert!(review.loaded().unwrap().diff.files[0].folded);
        assert!(!review.is_viewed(1));
    });
}

#[gpui::test]
fn two_reviews_of_one_checkout_keep_each_others_marks(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, None);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let first = view.seed_review(many_files(), window, cx);
            let second = view.seed_review(many_files(), window, cx);
            view.toggle_review_viewed(first, 0, cx);
            view.toggle_review_viewed(second, 1, cx);
            let next = view.seed_review(many_files(), window, cx);
            let review = &view.reviews[&next];
            assert!(review.is_viewed(0), "the first review's mark is kept");
            assert!(review.is_viewed(1));
        });
    });
}

#[gpui::test]
fn a_mark_on_a_changed_file_is_forgotten_and_a_left_out_one_kept(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, None);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let id = view.seed_review(many_files(), window, cx);
            view.toggle_review_viewed(id, 0, cx);
            view.toggle_review_viewed(id, 1, cx);
            // README.md changes again, and src/a.rs is left out, as a
            // committed file is when only uncommitted changes show.
            let mut next = many_files();
            next.diff.files[0].added = Some(9);
            next.diff.files.remove(1);
            view.review_loaded(id, 1, Ok(next), cx);
            // A later review whose files are as they were marked.
            let next = view.seed_review(many_files(), window, cx);
            let review = &view.reviews[&next];
            assert!(!review.is_viewed(0), "the changed file's mark is gone");
            assert!(review.is_viewed(1), "the left out file's mark is kept");
        });
    });
}
