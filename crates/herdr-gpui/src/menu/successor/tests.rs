use super::pick;
use crate::sidebar::layout_tests::{fixture_window, snapshot};
use std::sync::Arc;

fn closing(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| (*id).to_owned()).collect()
}

#[test]
fn picks_the_row_above() {
    let rows = ["main", "a", "b", "c"];
    assert_eq!(pick(&rows, "b", &closing(&["b"])), Some("a"));
    assert_eq!(pick(&rows, "a", &closing(&["a"])), Some("main"));
}

#[test]
fn picks_the_row_below_at_the_top() {
    let rows = ["main", "a", "b"];
    assert_eq!(pick(&rows, "main", &closing(&["main"])), Some("a"));
}

#[test]
fn skips_rows_closing_with_it() {
    let rows = ["other", "main", "a", "b", "last"];
    let group = closing(&["main", "a", "b"]);
    assert_eq!(pick(&rows, "a", &group), Some("other"));
    assert_eq!(pick(&rows[1..], "a", &group), Some("last"));
}

#[test]
fn nothing_when_unlisted_or_alone() {
    assert_eq!(pick(&["a", "b"], "hidden", &closing(&["hidden"])), None);
    assert_eq!(pick(&["a"], "a", &closing(&["a"])), None);
    assert_eq!(pick(&["a", "b"], "a", &closing(&["a", "b"])), None);
}

/// Removing the focused worktree lands on the row above it, not on the
/// main checkout the daemon picks, once the workspace leaves the snapshot.
#[gpui::test]
fn a_removed_worktree_hands_focus_to_the_row_above(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update_in(cx, |view, _, cx| {
        let mut fixture = snapshot(7);
        fixture.focused_workspace_id = Some("w5".into());
        let boot = fixture.boot_id.clone();
        view.live.snapshot = Some(Arc::new(fixture));

        // Closing an unfocused workspace leaves focus where it is.
        view.expect_successor(&boot, &closing(&["w4"]));
        assert!(view.successor.is_none());

        view.expect_successor(&boot, &closing(&["w5"]));
        assert_eq!(view.successor.as_ref().unwrap().target, "w4");
        // Still removing, and still focused: keep waiting.
        view.follow_successor(cx);
        assert!(view.successor.is_some());

        // The daemon drops it and jumps to the main checkout.
        let landed = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        landed.workspaces.retain(|w| w.workspace_id != "w5");
        landed.focused_workspace_id = Some("w3".into());
        view.follow_successor(cx);
        // Without a connection the navigation cannot be queued yet, so the
        // successor is kept for the next poll.
        assert_eq!(view.successor.as_ref().unwrap().target, "w4");

        // Once focus is on the successor there is nothing left to do.
        let focused = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        focused.focused_workspace_id = Some("w4".into());
        view.follow_successor(cx);
        assert!(view.successor.is_none());
    });
}

/// Focus moved by the user while the close is in flight, or a snapshot
/// from another boot, drops the successor.
#[gpui::test]
fn a_successor_never_overrides_the_user(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update_in(cx, |view, _, cx| {
        let mut fixture = snapshot(7);
        fixture.focused_workspace_id = Some("w5".into());
        let boot = fixture.boot_id.clone();
        view.live.snapshot = Some(Arc::new(fixture));

        view.expect_successor(&boot, &closing(&["w5"]));
        let moved = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        moved.focused_workspace_id = Some("w1".into());
        view.follow_successor(cx);
        assert!(view.successor.is_none());

        let back = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        back.focused_workspace_id = Some("w5".into());
        view.expect_successor(&boot, &closing(&["w5"]));
        let rebooted = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        rebooted.boot_id = "another-boot".into();
        view.follow_successor(cx);
        assert!(view.successor.is_none());
    });
}
