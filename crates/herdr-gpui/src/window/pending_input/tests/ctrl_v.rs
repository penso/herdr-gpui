//! On Windows plain Ctrl+V is the app's paste, so while input waits for the
//! frame it is held as a paste is and reads the clipboard when delivered.
use super::*;

fn ctrl_v() -> gpui::KeyDownEvent {
    gpui::KeyDownEvent {
        keystroke: gpui::Keystroke::parse("ctrl-v").unwrap(),
        is_held: false,
        prefer_character_input: false,
    }
}

#[gpui::test]
fn a_ctrl_v_held_for_the_frame_pastes_the_clipboard_once_it_returns(cx: &mut TestAppContext) {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update_in(cx, |view, window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("copied before".into()));
        sized_by_another(&peer, view, cx);
        view.key_down(&ctrl_v(), window, cx);
        assert_eq!(view.pending_input.len(), 1, "Ctrl+V waits for the frame");
        cx.write_to_clipboard(ClipboardItem::new_string("copied after".into()));
        frame_returns(view, cx);
        assert!(view.local_error.is_none());
    });
    cx.run_until_parked();
    assert_eq!(next_pane_input(&mut peer), typed(paste("copied after")));
    sentinel_is_next(&mut peer);
}
