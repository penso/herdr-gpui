//! The workspace focus moves to after the focused one is closed or its
//! worktree removed. Herdr picks by its own list order, and after a worktree
//! removal jumps to the repository's main checkout; the window instead lands
//! on the sidebar row above the closed one, or the one below at the top.

use crate::{HerdrWindow, NavigationTarget};
use gpui::Context;
use std::time::{Duration, Instant};

/// How long a close may take to leave the snapshot: a worktree removal waits
/// on Git and on its terminals exiting before the daemon drops the workspace.
const FOLLOW_FOR: Duration = Duration::from_secs(30);

pub(crate) struct Successor {
    /// Same fence as the menu target: another connection's snapshot is not
    /// the one this close was sent to.
    endpoint: (u64, u64),
    boot_id: String,
    closing: Vec<String>,
    target: String,
    until: Instant,
}

/// The row above `focused` that is not closing, else the first such row below
/// it. `None` when `focused` is not listed or nothing else would remain.
fn pick<'a>(rows: &[&'a str], focused: &str, closing: &[String]) -> Option<&'a str> {
    let at = rows.iter().position(|row| *row == focused)?;
    let open = |row: &&&str| !closing.iter().any(|id| id == **row);
    rows[..at]
        .iter()
        .rev()
        .find(open)
        .or_else(|| rows[at + 1..].iter().find(open))
        .copied()
}

impl HerdrWindow {
    /// Remember where focus goes once `closing` leaves the snapshot. Only a
    /// close of the focused workspace moves focus; closing another leaves it
    /// where it is, so nothing is recorded then.
    pub(super) fn expect_successor(&mut self, boot_id: &str, closing: &[String]) {
        self.successor = None;
        let Some(focused) = self
            .live
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.focused_workspace_id.as_deref())
            .filter(|focused| closing.iter().any(|id| id == focused))
        else {
            return;
        };
        let selected = self.selected_endpoint;
        let rows: Vec<&str> = self
            .sidebar_workspaces()
            .into_iter()
            .filter(|(host, _)| *host == selected)
            .map(|(_, id)| id)
            .collect();
        let Some(target) = pick(&rows, focused, closing) else {
            return;
        };
        self.successor = Some(Successor {
            endpoint: (
                self.selection_epoch,
                self.endpoints[self.selected_endpoint].generation,
            ),
            boot_id: boot_id.to_owned(),
            closing: closing.to_vec(),
            target: target.to_owned(),
            until: Instant::now() + FOLLOW_FOR,
        });
    }

    /// Focus the recorded successor once every closed workspace has left the
    /// snapshot. Gives up when the connection changes, the user moves focus
    /// themselves first, or the close never lands.
    pub(crate) fn follow_successor(&mut self, cx: &mut Context<Self>) {
        let Some(successor) = &self.successor else {
            return;
        };
        let current = successor.endpoint
            == (
                self.selection_epoch,
                self.endpoints[self.selected_endpoint].generation,
            )
            && Instant::now() < successor.until;
        let Some(snapshot) = self
            .live
            .snapshot
            .as_ref()
            .filter(|snapshot| current && snapshot.boot_id == successor.boot_id)
        else {
            self.successor = None;
            return;
        };
        let listed = |id: &str| snapshot.workspaces.iter().any(|w| w.workspace_id == id);
        let focused = snapshot.focused_workspace_id.as_deref();
        if successor.closing.iter().any(|id| listed(id)) {
            // Still closing. Focus leaving the closing workspaces now is the
            // user's choice, which the successor must not override.
            if !focused.is_some_and(|focused| successor.closing.iter().any(|id| id == focused)) {
                self.successor = None;
            }
            return;
        }
        if focused == Some(successor.target.as_str()) || !listed(&successor.target) {
            self.successor = None;
            return;
        }
        let target = successor.target.clone();
        if self.navigate(NavigationTarget::Workspace(&target), cx) {
            self.successor = None;
        }
    }

    /// A refused removal leaves the workspace in place, so nothing follows it.
    pub(super) fn drop_successor(&mut self) {
        self.successor = None;
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

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
}
