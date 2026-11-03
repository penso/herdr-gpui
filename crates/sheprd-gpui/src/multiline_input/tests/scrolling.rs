use super::*;

#[gpui::test]
fn wheel_scrolling_survives_repaints_and_copy_but_edits_and_arrows_reveal_the_caret(
    cx: &mut TestAppContext,
) {
    let (input, cx) = field(cx);
    let prompt = (0..30)
        .map(|row| format!("row {row}\n"))
        .collect::<String>();
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(prompt.clone()));
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-v"
    } else {
        "ctrl-v"
    });
    draw(cx);
    draw(cx);
    let end = input.read_with(cx, |input, _| {
        assert_eq!(input.text(), prompt);
        assert_eq!(input.scroll, input.max_scroll());
        assert!(input.scroll > px(0.));
        input.scroll
    });
    assert_caret_visible(&input, cx);

    scroll_up(&input, cx);
    let scrolled = input.read_with(cx, |input, _| input.scroll);
    assert!(scrolled < end, "wheel must reveal earlier lines");
    for _ in 0..3 {
        draw(cx);
        assert_eq!(input.read_with(cx, |input, _| input.scroll), scrolled);
    }
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-c"
    } else {
        "ctrl-c"
    });
    draw(cx);
    assert_eq!(input.read_with(cx, |input, _| input.scroll), scrolled);

    cx.simulate_input("continued");
    draw(cx);
    assert_caret_visible(&input, cx);
    scroll_up(&input, cx);
    cx.simulate_keystrokes("left");
    draw(cx);
    assert_caret_visible(&input, cx);
    scroll_up(&input, cx);
    cx.simulate_keystrokes("up");
    draw(cx);
    assert_caret_visible(&input, cx);
}

#[gpui::test]
fn clicking_earlier_text_after_scrolling_keeps_the_new_caret_visible(cx: &mut TestAppContext) {
    let (input, cx) = field(cx);
    cx.simulate_input(&"a line of text\n".repeat(30));
    draw(cx);
    draw(cx);
    scroll_up(&input, cx);
    let (position, end) = input.read_with(cx, |input, _| {
        let bounds = input.bounds.unwrap();
        (bounds.origin + point(px(4.), px(4.)), input.edit.cursor)
    });
    cx.simulate_click(position, Modifiers::default());
    draw(cx);
    assert!(input.read_with(cx, |input, _| input.edit.cursor) < end);
    assert_caret_visible(&input, cx);
}
