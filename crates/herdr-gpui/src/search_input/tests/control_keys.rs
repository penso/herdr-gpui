//! macOS's Control editing keys in a field.

#![cfg(target_os = "macos")]

use super::*;
use gpui::VisualTestContext;

fn field<'a>(
    text: &str,
    cx: &'a mut gpui::TestAppContext,
) -> (gpui::Entity<SearchInput>, &'a mut VisualTestContext) {
    let (input, cx) = cx.add_window_view(|_, cx| SearchInput::new(cx));
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.edit.replace(None, text, false, None);
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

#[gpui::test]
fn ctrl_a_and_ctrl_e_reach_the_line_ends(cx: &mut gpui::TestAppContext) {
    let (input, cx) = field("one two", cx);
    cx.simulate_keystrokes("ctrl-a");
    assert_eq!(edit(&input, cx), ("one two".into(), 0..0));
    cx.simulate_keystrokes("ctrl-f ctrl-f");
    assert_eq!(edit(&input, cx), ("one two".into(), 2..2));
    cx.simulate_keystrokes("ctrl-b");
    assert_eq!(edit(&input, cx), ("one two".into(), 1..1));
    cx.simulate_keystrokes("ctrl-e");
    assert_eq!(edit(&input, cx), ("one two".into(), 7..7));
}

#[gpui::test]
fn ctrl_k_cuts_to_the_end_and_ctrl_y_puts_it_back(cx: &mut gpui::TestAppContext) {
    let (input, cx) = field("git commit -m fix", cx);
    cx.simulate_keystrokes("ctrl-a ctrl-f ctrl-f ctrl-f ctrl-k");
    assert_eq!(edit(&input, cx), ("git".into(), 3..3));
    cx.simulate_keystrokes("ctrl-a ctrl-y");
    assert_eq!(edit(&input, cx), (" commit -m fixgit".into(), 14..14));
    // Ctrl-K at the end keeps what it cut before.
    cx.simulate_keystrokes("ctrl-e ctrl-k ctrl-y");
    assert_eq!(
        edit(&input, cx),
        (" commit -m fixgit commit -m fix".into(), 31..31)
    );
}

#[gpui::test]
fn ctrl_h_and_ctrl_d_delete_a_character(cx: &mut gpui::TestAppContext) {
    let (input, cx) = field("a\u{e9}b", cx);
    cx.simulate_keystrokes("ctrl-h");
    assert_eq!(edit(&input, cx), ("a\u{e9}".into(), 3..3));
    cx.simulate_keystrokes("ctrl-a ctrl-d");
    assert_eq!(edit(&input, cx), ("\u{e9}".into(), 0..0));
}

#[gpui::test]
fn a_masked_secret_is_not_kept_to_put_back(cx: &mut gpui::TestAppContext) {
    let (input, cx) = field("hunter2", cx);
    input.update(cx, |input, cx| input.set_masked(true, cx));
    cx.simulate_keystrokes("ctrl-a ctrl-k ctrl-y");
    assert_eq!(edit(&input, cx), (String::new(), 0..0));
}

#[gpui::test]
fn ctrl_k_cuts_a_selection_and_ctrl_y_puts_it_all_back(cx: &mut gpui::TestAppContext) {
    let (input, cx) = field("abcdef", cx);
    input.update(cx, |input, _| {
        input.edit.select_to(1, false);
        input.edit.select_to(3, true);
    });
    cx.simulate_keystrokes("ctrl-k");
    assert_eq!(edit(&input, cx), ("adef".into(), 1..1));
    cx.simulate_keystrokes("ctrl-y");
    assert_eq!(edit(&input, cx), ("abcdef".into(), 3..3));
}

#[gpui::test]
fn ctrl_y_before_any_cut_keeps_the_selection(cx: &mut gpui::TestAppContext) {
    let (input, cx) = field("keep", cx);
    input.update(cx, |input, _| {
        input.edit.select_to(0, false);
        input.edit.select_to(4, true);
    });
    cx.simulate_keystrokes("ctrl-y");
    assert_eq!(edit(&input, cx), ("keep".into(), 0..4));
}
