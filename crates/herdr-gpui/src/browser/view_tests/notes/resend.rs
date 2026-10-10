use super::*;

fn send(
    view: &Entity<HerdrWindow>,
    tab: &crate::browser::Tab,
    cx: &mut VisualTestContext,
) -> String {
    cx.update(|_, cx| view.update(cx, |view, cx| view.send_notes(tab, cx)));
    kept(cx).unwrap()
}

/// Sent notes stay listed; Send then delivers only new or edited ones, all
/// of them again once every one was sent, and Clear sent removes them.
#[gpui::test]
fn page_notes_are_kept_when_sent_and_edited_ones_go_again(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    let tab = noted_tab(&view, cx);
    assert!(send(&view, &tab, cx).contains("Note: Make it blue"));
    view.read_with(cx, |view, _| {
        let notes = &view.browser.annotations;
        assert_eq!(notes.queued(tab.id), 0);
        assert_eq!(notes.listed(tab.id), [("Make it blue".into(), true)]);
        assert!(notes.open(tab.id), "sent notes keep the panel open");
    });

    // A second note goes alone.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.page_posted(tab.id, PICK, window, cx);
            let input = view.browser.annotations.input.clone();
            input.update(cx, |input, cx| input.set_text_selected("Bigger", cx));
            view.add_note(tab.id, window, cx);
        });
    });
    let second = send(&view, &tab, cx);
    assert!(second.contains("1. On <button>") && second.contains("Note: Bigger"));
    assert!(!second.contains("Make it blue"), "{second}");

    // Editing the first makes it new again, and only it goes.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.edit_note(tab.id, 0, window, cx);
            let input = view.browser.annotations.input.clone();
            assert_eq!(input.read(cx).text(), "Make it blue");
            input.update(cx, |input, cx| input.set_text_selected("Make it red", cx));
            view.add_note(tab.id, window, cx);
        });
    });
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.browser.annotations.listed(tab.id),
            [("Make it red".into(), false), ("Bigger".into(), true)]
        );
    });
    let edited = send(&view, &tab, cx);
    assert!(edited.contains("Note: Make it red") && !edited.contains("Bigger"));

    // All sent: Send repeats them all; Clear sent empties the list.
    let again = send(&view, &tab, cx);
    assert!(again.contains("Make it red") && again.contains("Bigger"));
    cx.update(|_, cx| view.update(cx, |view, cx| view.clear_sent_notes(tab.id, cx)));
    assert!(view.read_with(cx, |view, _| {
        view.browser.annotations.listed(tab.id).is_empty()
    }));
}
