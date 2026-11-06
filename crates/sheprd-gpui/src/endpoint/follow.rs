//! Following a focus asked for on a host the window is not showing.
//!
//! `herdr agent focus` and its kin move every client of the daemon, so the
//! connection this window keeps to a background host sees the move in its
//! snapshot. Herdr sends no event for it, and names no asker, so the snapshot
//! is all there is to go by.
use super::{Endpoint, SheprdWindow};
use gpui::{App, Context, WindowId};
use sheprd_client::{ConnectTarget, protocol::ClientShellSnapshot};
use std::time::{Duration, Instant};

/// How long after the window leaves a host that host's moves are still taken
/// for the window's own: a click just before leaving is answered after it.
const LEAVING: Duration = Duration::from_secs(2);

/// Whether going from `old` to `new` shows someone asking for a focus.
///
/// Only a move between two tabs that both still exist counts. Herdr keeps a
/// tab per client, and a background connection may not navigate, so nothing
/// but a public focus moves it there. A closed tab moves it too, and a new
/// tab or workspace takes focus as it opens, which a script or dispatch does
/// on a host nobody is watching; neither should take the window. The pane
/// within a tab is shared by the daemon's clients, so a move inside one may
/// be someone working in a terminal on that host, and is not followed.
fn focus_requested(old: &ClientShellSnapshot, new: &ClientShellSnapshot) -> bool {
    let (Some(from), Some(to)) = (old.focused_tab_id.as_deref(), new.focused_tab_id.as_deref())
    else {
        return false;
    };
    let lists =
        |snapshot: &ClientShellSnapshot, id: &str| snapshot.tabs.iter().any(|tab| tab.tab_id == id);
    old.boot_id == new.boot_id && from != to && lists(new, from) && lists(old, to)
}

impl Endpoint {
    /// Notes a focus asked for in the update `new` about to replace `live`.
    pub(super) fn observe_focus(&mut self, new: &crate::LiveState) {
        self.focus_requested |= self
            .live
            .latest_snapshot()
            .zip(new.latest_snapshot())
            .is_some_and(|(old, new)| focus_requested(old, new));
    }
}

impl SheprdWindow {
    /// Shows the host a focus was asked for on, as the window already does
    /// for the host it shows. The daemon has put this connection on the
    /// target, so selecting the host is all it takes. A focus the window
    /// passes up is dropped, not kept for later: a dialog stays open, and a
    /// host another window shows stays there.
    pub(crate) fn follow_focus(&mut self, own: WindowId, now: Instant, cx: &mut Context<Self>) {
        let selected = self.selected_endpoint;
        let mut followed = None;
        for (index, endpoint) in self.endpoints.iter_mut().enumerate() {
            if std::mem::take(&mut endpoint.focus_requested)
                && index != selected
                && endpoint
                    .left_at
                    .is_none_or(|left| now.duration_since(left) >= LEAVING)
            {
                followed = Some(index);
            }
        }
        let Some(index) = followed else {
            return;
        };
        let endpoint = &self.endpoints[index];
        if self.menu.page.is_some() || shown_elsewhere(&endpoint.connection.target, own, cx) {
            return;
        }
        let id = endpoint.id.clone();
        self.select_endpoint(&id, cx);
    }
}

/// Whether a window other than `own` shows `target`. Several windows each
/// see the same move: the first to follow it speaks for the rest.
fn shown_elsewhere(target: &ConnectTarget, own: WindowId, cx: &App) -> bool {
    cx.windows()
        .into_iter()
        .filter(|handle| handle.window_id() != own)
        .filter_map(|handle| handle.downcast::<SheprdWindow>())
        .filter_map(|handle| handle.read(cx).ok())
        .any(|view| view.endpoints[view.selected_endpoint].connection.target == *target)
}
