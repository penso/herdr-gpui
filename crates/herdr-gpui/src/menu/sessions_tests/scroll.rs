//! The keys walk into an unfolded device's sessions, which open as one block:
//! the session they reach is scrolled into view, not merely the block.
use super::*;

#[gpui::test]
fn the_keys_scroll_to_a_session_below_the_list(cx: &mut TestAppContext) {
    let entries: Vec<LocalSession> = (0..12)
        .map(|index| session(&format!("session-{index}"), SessionState::Running))
        .collect();
    let (view, cx) = open_list(cx, entries, development("session-0"));
    // The shortcut starts on the current session, the first; walk to the last.
    for _ in 0..11 {
        cx.simulate_keystrokes("down");
        draw(cx);
    }
    let selected = view.read_with(cx, |view, _| view.menu.selected);
    assert!(selected.is_some());
    let panel = bounds(cx, "menu-panel");
    let row = bounds(cx, "sessions-row-11");
    assert!(
        row.top() >= panel.top() && row.bottom() <= panel.bottom(),
        "{row:?} is outside {panel:?}"
    );
}
