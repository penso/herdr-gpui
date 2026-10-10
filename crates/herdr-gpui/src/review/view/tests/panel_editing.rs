use super::*;
use crate::review::view::panels::Panel;

#[gpui::test]
fn hiding_notes_cancels_an_edit_without_changing_the_note(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, None);
    cx.update(|window, cx| view.update(cx, |view, cx| view.seed_review(changes(), window, cx)));
    note(&view, cx, line(3), "Keep the original.");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let id = the(view);
            view.edit_review_note(id, 0, window, cx);
            let review = view.reviews.get(&id).unwrap();
            assert_eq!(review.editing, Some(0));
            review.input.clone().update(cx, |input, cx| {
                input.set_text_selected("Unsaved edit.", cx);
            });
        });
        crate::sidebar::layout_tests::full_draw(window, cx).clear(cx);
    });
    let toggle = cx.debug_bounds("review-toggle-notes").unwrap();
    cx.simulate_click(toggle.center(), gpui::Modifiers::default());
    view.read_with(cx, |view, _| {
        let review = view.reviews.get(&the(view)).unwrap();
        assert!(!review.shows(Panel::Notes));
        assert!(review.editing.is_none() && review.draft.is_none());
        assert_eq!(review.notes[0].comment, "Keep the original.");
    });
    cx.simulate_click(toggle.center(), gpui::Modifiers::default());
    view.read_with(cx, |view, _| {
        let review = view.reviews.get(&the(view)).unwrap();
        assert!(review.shows(Panel::Notes));
        assert!(review.editing.is_none());
    });
}
