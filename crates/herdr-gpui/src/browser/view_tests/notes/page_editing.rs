use super::*;

#[gpui::test]
fn edits_on_two_pages_keep_their_own_text(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    let first = noted_tab(&view, cx);
    let second = noted_tab(&view, cx);
    assert_ne!(first.id, second.id);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            for (tab, text) in [(&first, "First page edit."), (&second, "Second page edit.")] {
                view.edit_note(tab.id, 0, window, cx);
                view.browser
                    .annotations
                    .input(tab.id, cx)
                    .update(cx, |input, cx| input.set_text_selected(text, cx));
            }
            // Returning to A and saving must not read or clear B's field.
            view.add_note(first.id, window, cx);
            view.add_note(second.id, window, cx);
            assert_eq!(
                view.browser.annotations.listed(first.id),
                [("First page edit.".into(), false)]
            );
            assert_eq!(
                view.browser.annotations.listed(second.id),
                [("Second page edit.".into(), false)]
            );
        });
    });
}
