use super::*;

fn key(name: &str) -> Keystroke {
    Keystroke::parse(name).unwrap_or_default()
}

#[gpui::test]
fn a_letter_typed_before_an_accent_leaves_the_caret_after_both(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let mut input = DialogInput::new("\u{301}".into());
        input.key(&key("home"), cx);
        input.replace(None, "e", false, None);
        assert_eq!(input.text, "e\u{301}");
        assert_eq!(input.selection, 3..3);
        input.key(&key("delete"), cx);
        assert_eq!(input.text, "e\u{301}");
        input.key(&key("backspace"), cx);
        assert_eq!(input.text, "");
    });
}
