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
#[allow(clippy::unwrap_used)]
mod tests;
