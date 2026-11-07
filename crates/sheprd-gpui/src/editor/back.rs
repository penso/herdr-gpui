//! Coming back from the editor. A review is a tab of its own, so the editor
//! cannot take its place the way a terminal program takes a pane; instead
//! the review is remembered, and when the editor's pane closes the review
//! is shown again. Quitting the editor is then the way back to the diff.

use super::SheprdWindow;
use crate::{NavigationTarget, browser::TabId};
use gpui::{Context, Window};
use sheprd_client::protocol::ClientShellSnapshot;

/// Returns remembered at most; the oldest is forgotten first.
const MAX_RETURNS: usize = 16;

/// A page to show again once an editor pane closes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Return {
    pub(super) endpoint_id: String,
    pub(super) boot: String,
    pub(super) pane_id: String,
    pub(super) page: TabId,
    /// The terminal tab holding the pane, once a snapshot has shown it: the
    /// answer to a split can arrive first, and a pane not yet listed has
    /// not closed.
    pub(super) seen_in: Option<String>,
    /// Whether that tab was the one in use at the last snapshot with the
    /// pane open. Quitting a tab's last pane empties the tab, so the snapshot
    /// after it cannot tell that from the user having left.
    pub(super) in_use: bool,
    /// Whether this window last navigated away from that tab. Snapshots are
    /// coalesced, so the one showing the user gone can be replaced by the
    /// one showing the pane closed, and only the window saw them leave.
    pub(super) left: bool,
}

/// Records a navigation of `endpoint` to terminal tab `to`, or to somewhere
/// else with None: each return has left its tab unless that is where it went.
pub(super) fn navigated(returns: &mut [Return], endpoint: &str, to: Option<&str>) {
    for entry in returns.iter_mut().filter(|e| e.endpoint_id == endpoint) {
        entry.left = entry.seen_in.as_deref() != to;
    }
}

/// Follows `returns` through `snapshot` of `endpoint`, forgetting those of
/// an older boot, and answers with the page to show: the last whose pane
/// closed while its terminal tab was still the one in use, or took that tab
/// with it. A pane closed from elsewhere pulls nobody away from what they
/// moved on to.
pub(super) fn settle(
    returns: &mut Vec<Return>,
    endpoint: &str,
    snapshot: &ClientShellSnapshot,
) -> Option<TabId> {
    let focused_tab = snapshot.focused_pane_id.as_deref().and_then(|id| {
        let pane = snapshot.panes.iter().find(|pane| pane.pane_id == id)?;
        Some(pane.tab_id.as_str())
    });
    let mut page = None;
    returns.retain_mut(|entry| {
        if entry.endpoint_id != endpoint {
            return true;
        }
        if entry.boot != snapshot.boot_id {
            return false;
        }
        let open = snapshot
            .panes
            .iter()
            .find(|pane| pane.pane_id == entry.pane_id);
        if let Some(pane) = open {
            entry.in_use = focused_tab == Some(pane.tab_id.as_str());
            entry.seen_in = Some(pane.tab_id.clone());
            return true;
        }
        let Some(tab) = entry.seen_in.as_deref() else {
            return true;
        };
        let emptied = !snapshot.panes.iter().any(|pane| pane.tab_id == tab);
        if focused_tab == Some(tab) || (entry.in_use && !entry.left && emptied) {
            page = Some(entry.page);
        }
        false
    });
    page
}

impl SheprdWindow {
    /// Remembers that `pane`'s editor came from `page`. A later file from
    /// nowhere in particular, reusing the pane, forgets the page.
    pub(crate) fn note_editor_return(
        &mut self,
        endpoint_id: &str,
        boot: &str,
        pane: &str,
        page: Option<TabId>,
    ) {
        self.editor_returns.retain(|entry| {
            entry.pane_id != pane || entry.endpoint_id != endpoint_id || entry.boot != boot
        });
        let Some(page) = page else {
            return;
        };
        if self.editor_returns.len() == MAX_RETURNS {
            self.editor_returns.remove(0);
        }
        self.editor_returns.push(Return {
            endpoint_id: endpoint_id.to_owned(),
            boot: boot.to_owned(),
            pane_id: pane.to_owned(),
            page,
            seen_in: None,
            in_use: false,
            left: false,
        });
    }

    /// Notes a navigation about to go to `target`, which may leave the
    /// terminal tab of an editor pane.
    pub(crate) fn note_editor_navigation(&mut self, target: NavigationTarget<&str>) {
        if self.editor_returns.is_empty() {
            return;
        }
        let snapshot = self.live.snapshot.as_deref();
        let to = match target {
            NavigationTarget::Tab(tab) => Some(tab),
            NavigationTarget::Pane(id) => snapshot.and_then(|snapshot| {
                let pane = snapshot.panes.iter().find(|pane| pane.pane_id == id)?;
                Some(pane.tab_id.as_str())
            }),
            NavigationTarget::Workspace(_) => None,
        };
        let endpoint = &self.endpoints[self.selected_endpoint].id;
        navigated(&mut self.editor_returns, endpoint, to);
    }

    /// Notes a page taking the window's attention from the terminals.
    pub(crate) fn note_editor_page_shown(&mut self) {
        let endpoint = &self.endpoints[self.selected_endpoint].id;
        for entry in self.editor_returns.iter_mut() {
            entry.left |= entry.endpoint_id == *endpoint;
        }
    }

    /// Runs every window tick: shows the review whose editor just quit. A
    /// dropped connection says nothing about the pane, so it waits.
    pub(crate) fn poll_editor_returns(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor_returns.is_empty() || !self.live.status.is_connected() {
            return;
        }
        let Some(snapshot) = self.live.snapshot.clone() else {
            return;
        };
        let endpoint = &self.endpoints[self.selected_endpoint].id;
        let Some(page) = settle(&mut self.editor_returns, endpoint, &snapshot) else {
            return;
        };
        self.show_browser_tab(page, window, cx);
        if let Some(review) = self.reviews.get(&page) {
            window.focus(review.focus(), cx);
        }
        cx.notify();
    }
}
