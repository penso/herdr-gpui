use super::*;
use gpui::VisualTestContext;

/// The modifier fields move and delete by word with.
const WORD: &str = if cfg!(target_os = "macos") {
    "alt"
} else {
    "ctrl"
};

fn field<'a>(
    text: &str,
    masked: bool,
    cx: &'a mut gpui::TestAppContext,
) -> (gpui::Entity<SearchInput>, &'a mut VisualTestContext) {
    let (input, cx) = cx.add_window_view(|_, cx| SearchInput::new(cx));
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.set_masked(masked, cx);
            input.edit.replace(None, text, false, None, Newlines::Strip);
            window.focus(&input.focus, cx);
        });
        window.draw(cx).clear(cx);
    });
    (input, cx)
}

fn edit(input: &gpui::Entity<SearchInput>, cx: &mut VisualTestContext) -> (String, Range<usize>) {
    input.read_with(cx, |input, _| {
        (input.edit.text.clone(), input.edit.selection())
    })
}

fn press(cx: &mut VisualTestContext, keys: &str) {
    let keys = keys.replace("word-", &format!("{WORD}-"));
    cx.simulate_keystrokes(&keys);
}

#[gpui::test]
fn word_backspace_and_delete_remove_a_word(cx: &mut gpui::TestAppContext) {
    let (input, cx) = field("git commit -m fix", false, cx);
    press(cx, "word-backspace");
    assert_eq!(edit(&input, cx), ("git commit -m ".into(), 14..14));
    press(cx, "word-backspace");
    assert_eq!(edit(&input, cx), ("git commit -".into(), 12..12));
    press(cx, "cmd-left word-delete");
    assert_eq!(edit(&input, cx), (" commit -".into(), 0..0));
}

#[gpui::test]
fn shift_does_not_stop_word_deletion(cx: &mut gpui::TestAppContext) {
    let (input, cx) = field("one two three", false, cx);
    press(cx, "word-shift-backspace");
    assert_eq!(edit(&input, cx), ("one two ".into(), 8..8));
    press(cx, "cmd-left word-shift-delete");
    assert_eq!(edit(&input, cx), (" two ".into(), 0..0));
}

#[gpui::test]
fn word_arrows_move_and_shift_extends(cx: &mut gpui::TestAppContext) {
    let (input, cx) = field("one two three", false, cx);
    press(cx, "word-left");
    assert_eq!(edit(&input, cx).1, 8..8);
    press(cx, "word-shift-left");
    assert_eq!(edit(&input, cx).1, 4..8);
    // With a selection, a plain word arrow starts from its edge.
    press(cx, "word-right");
    assert_eq!(edit(&input, cx).1, 13..13);
    press(cx, "word-shift-left word-backspace");
    assert_eq!(edit(&input, cx), ("one two ".into(), 8..8));
}

#[gpui::test]
fn a_masked_field_is_one_word(cx: &mut gpui::TestAppContext) {
    let (input, cx) = field("hunter two", true, cx);
    press(cx, "word-left");
    assert_eq!(edit(&input, cx).1, 0..0);
    press(cx, "word-right");
    assert_eq!(edit(&input, cx).1, 10..10);
    press(cx, "word-backspace");
    assert_eq!(edit(&input, cx), (String::new(), 0..0));
}
