use crate::core_terminals::keyboard_free;
use gpui::TestAppContext;

#[gpui::test]
fn a_waiting_pane_takes_the_keyboard_only_while_nothing_newer_holds_it(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let window = cx.focus_handle();
        let picker = cx.focus_handle();
        assert!(keyboard_free(None, &window));
        assert!(keyboard_free(Some(&window), &window));
        assert!(!keyboard_free(Some(&picker), &window));
    });
}
