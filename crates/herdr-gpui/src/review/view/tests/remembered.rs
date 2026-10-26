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
