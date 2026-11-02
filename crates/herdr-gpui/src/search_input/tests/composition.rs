#![allow(clippy::unwrap_used)]

//! Composing CJK text through the platform input API, as Pinyin, Zhuyin,
//! and Japanese input methods do, and where the candidate window opens.

use super::*;
use gpui::VisualTestContext;

/// A field narrower than the text typed into it, so it scrolls.
struct Narrow(gpui::Entity<SearchInput>);

impl Render for Narrow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(120.)).child(self.0.clone())
    }
}

fn field(cx: &mut gpui::TestAppContext) -> (gpui::Entity<SearchInput>, &mut VisualTestContext) {
    let (narrow, cx) = cx.add_window_view(|_, cx| Narrow(cx.new(SearchInput::new)));
    let input = narrow.read_with(cx, |narrow, _| narrow.0.clone());
    cx.update(|window, cx| {
        window.focus(&input.focus_handle(cx), cx);
        window.draw(cx).clear(cx);
    });
    (input, cx)
}

/// Draws the window, then asks for the candidate window's place over
/// UTF-16 `range`, with the field's painted bounds.
fn candidate_bounds(
    input: &gpui::Entity<SearchInput>,
    range: Range<usize>,
    cx: &mut VisualTestContext,
) -> (Bounds<Pixels>, Bounds<Pixels>) {
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        input.update(cx, |input, cx| {
            let place = input
                .bounds_for_range(range, Bounds::default(), window, cx)
                .unwrap();
            (place, input.bounds.unwrap())
        })
    })
}

fn compose(
    input: &gpui::Entity<SearchInput>,
    text: &str,
    selection: Range<usize>,
    cx: &mut VisualTestContext,
) {
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(None, text, Some(selection), window, cx);
        })
    });
}

#[gpui::test]
fn pinyin_composes_over_its_letters_and_commits_the_characters(cx: &mut gpui::TestAppContext) {
    let (input, cx) = field(cx);
    compose(&input, "zhong'wen", 9..9, cx);
    let (place, bounds) = candidate_bounds(&input, 0..9, cx);
    assert_eq!(place.left(), bounds.left());
    assert!(place.right() > place.left());
    assert_eq!(
        (place.top(), place.bottom()),
        (bounds.top(), bounds.bottom())
    );

    // The candidate list replaces the letters with the characters chosen.
    compose(&input, "中文", 2..2, cx);
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            assert_eq!(input.marked_text_range(window, cx), Some(0..2));
            input.replace_text_in_range(None, "中文", window, cx);
            assert_eq!(input.text(), "中文");
            assert!(!input.is_composing());
            assert_eq!(
                input.selected_text_range(false, window, cx).unwrap().range,
                2..2
            );
        })
    });
}

#[gpui::test]
fn the_candidate_window_follows_the_clause_being_converted(cx: &mut gpui::TestAppContext) {
    // Japanese converts one clause at a time: the selection inside the
    // composition is the clause, and the candidates open under it.
    let (input, cx) = field(cx);
    compose(&input, "日本語を", 0..3, cx);
    let (whole, _) = candidate_bounds(&input, 0..4, cx);
    let (first, _) = candidate_bounds(&input, 0..3, cx);
    let (last, _) = candidate_bounds(&input, 3..4, cx);
    assert_eq!(first.left(), whole.left());
    assert!(last.left() >= first.right());
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            let selection = input.selected_text_range(false, window, cx).unwrap();
            assert_eq!(selection.range, 0..3);
        })
    });
}

#[gpui::test]
fn composing_past_the_edge_keeps_the_candidates_on_the_field(cx: &mut gpui::TestAppContext) {
    // Zhuyin after a long query: the field scrolls so the composition stays
    // in view, and the candidate window opens over it rather than past the
    // field's right edge where the unscrolled text would end.
    let (input, cx) = field(cx);
    let typed = "a".repeat(80);
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_text_in_range(None, &typed, window, cx)
        })
    });
    compose(&input, "ㄓㄨㄥ", 3..3, cx);
    let (place, bounds) = candidate_bounds(&input, 80..83, cx);
    assert!(place.left() > bounds.left(), "{place:?} in {bounds:?}");
    assert!(place.right() <= bounds.right(), "{place:?} in {bounds:?}");
    assert!(place.right() > place.left());
}

#[gpui::test]
fn deletion_and_arrows_step_over_whole_grapheme_clusters(cx: &mut gpui::TestAppContext) {
    // A Hangul syllable spelled as conjoining jamo, and an accented letter.
    let (input, cx) = field(cx);
    let syllable = "\u{1112}\u{1161}\u{11ab}";
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_text_in_range(None, &format!("e\u{301}{syllable}"), window, cx)
        })
    });
    cx.simulate_keystrokes("left");
    let cursor = input.read_with(cx, |input, _| input.edit.cursor);
    assert_eq!(cursor, "e\u{301}".len());
    cx.simulate_keystrokes("backspace");
    assert_eq!(
        input.read_with(cx, |input, _| input.text().to_owned()),
        syllable
    );
    cx.simulate_keystrokes("delete");
    assert_eq!(input.read_with(cx, |input, _| input.text().to_owned()), "");
}

#[gpui::test]
fn a_letter_typed_before_an_accent_leaves_the_caret_after_both(cx: &mut gpui::TestAppContext) {
    let (input, cx) = field(cx);
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_text_in_range(None, "\u{301}", window, cx)
        })
    });
    cx.simulate_keystrokes("home");
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_text_in_range(None, "e", window, cx)
        })
    });
    let (text, cursor) =
        input.read_with(cx, |input, _| (input.text().to_owned(), input.edit.cursor));
    assert_eq!((text.as_str(), cursor), ("e\u{301}", 3));
    // Delete at the end removes nothing; Backspace removes the whole letter.
    cx.simulate_keystrokes("delete");
    assert_eq!(
        input.read_with(cx, |input, _| input.text().to_owned()),
        "e\u{301}"
    );
    cx.simulate_keystrokes("backspace");
    assert_eq!(input.read_with(cx, |input, _| input.text().to_owned()), "");
}
