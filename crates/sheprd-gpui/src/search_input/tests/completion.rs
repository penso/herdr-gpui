#![allow(clippy::unwrap_used)]

use super::*;
use gpui::{Modifiers, VisualTestContext};

fn field(cx: &mut gpui::TestAppContext) -> (gpui::Entity<SearchInput>, &mut VisualTestContext) {
    let (input, cx) = cx.add_window_view(|_, cx| SearchInput::new(cx));
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.set_text_selected("sam", cx);
            // As after typing: the caret ends the text, nothing selected.
            input.edit.select_to(3, false);
            input.set_completion(Some("ba.tail1.ts.net".into()), cx);
            window.focus(&input.focus, cx);
        });
        window.draw(cx).clear(cx);
    });
    (input, cx)
}

fn shown(input: &gpui::Entity<SearchInput>, cx: &mut VisualTestContext) -> Option<String> {
    input.read_with(cx, |input, _| input.shown_completion().map(str::to_owned))
}

#[gpui::test]
fn the_completion_shows_only_with_the_caret_at_the_end(cx: &mut gpui::TestAppContext) {
    let (input, cx) = field(cx);
    assert_eq!(shown(&input, cx).as_deref(), Some("ba.tail1.ts.net"));

    // A caret inside the text, or a selection, hides it.
    cx.update(|_, cx| input.update(cx, |input, _| input.edit.select_to(1, false)));
    assert_eq!(shown(&input, cx), None);
    cx.update(|_, cx| {
        input.update(cx, |input, _| {
            input.edit.select_to(0, false);
            input.edit.select_to(3, true);
        })
    });
    assert_eq!(shown(&input, cx), None);

    // Composition hides it too.
    cx.update(|_, cx| {
        input.update(cx, |input, _| {
            input.edit.select_to(3, false);
            input.edit.marked = Some(2..3);
        })
    });
    assert_eq!(shown(&input, cx), None);
    cx.update(|_, cx| input.update(cx, |input, _| input.edit.marked = None));
    assert_eq!(shown(&input, cx).as_deref(), Some("ba.tail1.ts.net"));

    // A secret never shows one.
    cx.update(|_, cx| input.update(cx, |input, cx| input.set_masked(true, cx)));
    assert_eq!(shown(&input, cx), None);

    // Clearing it removes it.
    cx.update(|_, cx| {
        input.update(cx, |input, cx| {
            input.set_masked(false, cx);
            input.set_completion(None, cx);
        })
    });
    assert_eq!(shown(&input, cx), None);
}

#[gpui::test]
fn clicking_on_the_completion_puts_the_caret_after_the_typed_text(cx: &mut gpui::TestAppContext) {
    let (input, cx) = field(cx);
    cx.update(|_, cx| input.update(cx, |input, _| input.edit.select_to(0, false)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    // Far right of the field, over the dimmed completion.
    let bounds = input.read_with(cx, |input, _| input.bounds).unwrap();
    let point = point(bounds.right() - px(2.), bounds.center().y);
    cx.simulate_click(point, Modifiers::default());
    input.read_with(cx, |input, _| {
        assert_eq!(input.text(), "sam", "clicking does not accept it");
        assert_eq!(input.edit.selection(), 3..3);
    });
}
