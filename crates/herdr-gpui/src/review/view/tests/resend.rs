use super::*;

fn copied(cx: &mut gpui::VisualTestContext) -> String {
    cx.update(|_, cx| cx.read_from_clipboard())
        .and_then(|item| item.text())
        .unwrap()
}

fn send(view: &Entity<HerdrWindow>, cx: &mut gpui::VisualTestContext) -> String {
    cx.update(|_, cx| view.update(cx, |view, cx| view.send_review(the(view), cx)));
    copied(cx)
}

/// Sent notes stay listed and marked; Send then delivers only new or edited
/// ones, everything again once all were sent, and Clear sent ends the round.
#[gpui::test]
fn edited_notes_are_sent_again_and_sent_ones_stay_until_cleared(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, None);
    cx.update(|window, cx| view.update(cx, |view, cx| view.seed_review(changes(), window, cx)));
    note(&view, cx, line(3), "Implement this");
    note(&view, cx, RowId::Header(0), "Add a test");
    let first = send(&view, cx);
    assert!(first.contains("Note: Implement this") && first.contains("Note: Add a test"));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("review-clear-sent").is_some());

    // Escape leaves the note as it was.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.edit_review_note(the(view), 0, window, cx);
            let review = view.reviews.values().next().unwrap();
            assert_eq!(review.input.read(cx).text(), "Implement this");
            view.cancel_review_note(the(view), window, cx);
        });
    });
    // Saving an edit makes the note new again.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.edit_review_note(the(view), 0, window, cx);
            let input = view.reviews.values().next().unwrap().input.clone();
            input.update(cx, |input, cx| {
                input.set_text_selected("Implement it fully", cx)
            });
            view.add_review_note(the(view), window, cx);
            let review = view.reviews.values().next().unwrap();
            assert_eq!(review.editing, None);
            assert_eq!(review.notes[0].comment, "Implement it fully");
            assert!(!review.notes[0].sent && review.notes[1].sent);
        });
    });
    let edited = send(&view, cx);
    assert!(edited.contains("\n1. On `src/lib.rs:2`"), "{edited}");
    assert!(edited.contains("Note: Implement it fully"));
    assert!(
        !edited.contains("Add a test"),
        "only the edited note: {edited}"
    );

    // Everything was sent: Send resends it all.
    let again = send(&view, cx);
    assert!(again.contains("Implement it fully") && again.contains("Add a test"));

    cx.update(|_, cx| view.update(cx, |view, cx| view.clear_sent_review_notes(the(view), cx)));
    view.read_with(cx, |view, _| {
        let review = view.reviews.values().next().unwrap();
        assert!(review.notes.is_empty() && review.marks.is_empty());
    });
}
