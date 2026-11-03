use super::DialogInput;
use gpui::Keystroke;

mod branch_format;
mod graphemes;
mod word_keys;

#[test]
fn utf16_replacement_and_composition_use_relative_selection() {
    let mut input = DialogInput::new("a\u{1f600}z".into());
    assert_eq!(input.range_from_utf16(1..3), 1..5);
    assert_eq!(input.to_utf16(1..5), 1..3);
    input.replace(Some(1..3), "\u{65e5}\u{1f600}", true, Some(1..3));
    assert_eq!(input.text, "a\u{65e5}\u{1f600}z");
    assert_eq!(input.selection, 4..8);
    assert_eq!(input.marked, Some(1..8));
    input.replace(None, "\u{672c}", true, Some(1..1));
    assert_eq!(input.text, "a\u{672c}z");
    assert_eq!(input.selection, 4..4);
    input.replace(None, "\u{65e5}\u{672c}", false, None);
    assert_eq!(input.text, "a\u{65e5}\u{672c}z");
    assert_eq!(input.selection, 7..7);
    assert!(input.marked.is_none());
    assert_eq!(
        input.range_from_utf16(999..1000),
        input.text.len()..input.text.len()
    );
}

#[gpui::test]
fn editing_uses_graphemes_selection_clipboard_and_bounded_single_line_text(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(|cx| {
        let mut input =
            DialogInput::new("e\u{301}\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}".into());
        input.key(&Keystroke::parse("end").unwrap_or_default(), cx);
        input.key(&Keystroke::parse("backspace").unwrap_or_default(), cx);
        assert_eq!(input.text, "e\u{301}");
        input.key(&Keystroke::parse("shift-left").unwrap_or_default(), cx);
        assert_eq!(input.selection, 0..3);
        input.key(&Keystroke::parse("cmd-x").unwrap_or_default(), cx);
        assert_eq!(input.text, "");
        input.key(&Keystroke::parse("cmd-v").unwrap_or_default(), cx);
        assert_eq!(input.text, "e\u{301}");
        input.replace(None, "\n\r\tX", false, None);
        assert_eq!(input.text, "e\u{301}X");
        input.replace(None, &"x".repeat(20000), false, None);
        assert_eq!(input.text.len(), 4100);
    });
}

#[gpui::test]
fn cmd_backspace_and_cmd_delete_reach_the_line_ends(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let mut input = DialogInput::new("one two".into());
        input.key(&Keystroke::parse("cmd-left").unwrap_or_default(), cx);
        for _ in 0..3 {
            input.key(&Keystroke::parse("right").unwrap_or_default(), cx);
        }
        input.key(&Keystroke::parse("cmd-delete").unwrap_or_default(), cx);
        assert_eq!(input.text, "one");
        input.key(&Keystroke::parse("left").unwrap_or_default(), cx);
        input.key(&Keystroke::parse("cmd-backspace").unwrap_or_default(), cx);
        assert_eq!(input.text, "e");
        assert_eq!(input.selection, 0..0);
    });
}
