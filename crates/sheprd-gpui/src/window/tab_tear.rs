//! Carrying a tab out of the window to open it in a new one, as a web
//! browser does. Herdr lists a workspace's tabs in every client and cannot
//! move one to another workspace, so the tab stays where it is: the new
//! window is another client of the same daemon that goes to the tab's
//! workspace and hides that workspace's other tabs, while this one moves on
//! to a neighbour and hides the tab. Tabs opened later show in both. When
//! the new window closes, the tab is listed again in the one it left.

use super::{SheprdWindow, tab_drag::TabDrag};
use crate::{
    NavigationTarget,
    app::Placement,
    browser::{Pick, Scope, Store},
    window_state::WindowState,
};
use gpui::{Bounds, Context, Subscription, WeakEntity, Window};
use sheprd_client::ConnectTarget;

/// A tab carried out of a window, and where the window that shows it opens.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TornTab {
    /// The daemon the new window connects to, as New Window's does.
    pub(crate) target: ConnectTarget,
    /// The host the tab belongs to, which the new window shows.
    pub(super) endpoint: String,
    /// What that host's endpoint was connected to, naming the session it
    /// showed, which the new window's may not.
    pub(super) session: ConnectTarget,
    pub(crate) pick: Pick,
    /// The workspace the tab belongs to, as the window it left keys it.
    pub(crate) workspace: (Scope, String),
    /// The workspace's other tabs when it was torn off, which the new
    /// window hides.
    pub(crate) others: Vec<Pick>,
    /// The window the tab left, which lists it again once the new one
    /// closes.
    pub(crate) from: WeakEntity<SheprdWindow>,
    pub(crate) placement: Placement,
}

/// The tabs torn off in a test, which opens no windows.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct TornOff(pub(crate) Vec<TornTab>);

#[cfg(test)]
impl gpui::Global for TornOff {}

impl SheprdWindow {
    /// Opens the tab `drag` carried out of the window in a new window,
    /// placed as if this one had been carried along with it.
    pub(super) fn tear_off_tab(
        &mut self,
        drag: &TabDrag,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.browser_key() else {
            return;
        };
        let frame = window.bounds();
        let bounds = Bounds::new(frame.origin + (drag.pointer - drag.origin), frame.size);
        let torn = TornTab {
            target: self.endpoints[0].connection.target.clone(),
            endpoint: self.endpoints[self.selected_endpoint].id.clone(),
            session: self.endpoints[self.selected_endpoint]
                .connection
                .target
                .clone(),
            pick: drag.pick.clone(),
            others: self
                .open_workspace_tabs(cx)
                .into_iter()
                .filter(|pick| *pick != drag.pick)
                .collect(),
            workspace: workspace.clone(),
            from: cx.entity().downgrade(),
            placement: WindowState::beside(bounds, window, cx),
        };
        self.leave_tab(&drag.pick, cx);
        // A tab the group could not move on from stays listed while shown.
        self.browser
            .hidden
            .hide(workspace.clone(), [drag.pick.clone()]);
        #[cfg(test)]
        cx.default_global::<TornOff>().0.push(torn);
        #[cfg(not(test))]
        {
            let (from, pick) = (torn.from.clone(), torn.pick.clone());
            crate::app::open_additional_window_then(
                torn.target.clone(),
                Some(torn.placement),
                move |view, window, cx| view.arrive(torn, window, cx),
                move |cx| {
                    // A window already closed has nothing to list it in.
                    let _ = from.update(cx, |this, cx| this.untear(&workspace, &pick, cx));
                },
                cx,
            );
        }
    }

    /// Lists `pick` of `workspace` again when the window it was carried to
    /// could not open, since no window closing would give it back.
    pub(super) fn untear(
        &mut self,
        workspace: &(Scope, String),
        pick: &Pick,
        cx: &mut Context<Self>,
    ) {
        self.browser.hidden.show(workspace, pick);
        cx.notify();
    }

    /// Lists again, once a window closes, the tabs this one hid when they
    /// were carried off to it.
    pub(crate) fn give_back_torn_tabs(cx: &mut Context<Self>) -> Subscription {
        let this = cx.entity().downgrade();
        cx.on_window_closed(move |cx, closed| {
            let _ = this.update(cx, |this, cx| {
                if this.browser.hidden.release(closed) {
                    cx.notify();
                }
            });
        })
    }

    /// Goes to the tab `torn` names, in a window just opened for it. A page
    /// is chosen against its workspace at once, to show once the window
    /// gets there. The window lists only this machine's daemon until its
    /// saved hosts are read, so the way there waits for the tab's host.
    pub(crate) fn arrive(&mut self, torn: TornTab, window: &mut Window, cx: &mut Context<Self>) {
        // The workspace's saved split would show the tabs this window
        // hides, so it is dropped before the page is shown in it.
        self.browser.saved.remove(&torn.workspace);
        if let Pick::Page(id) = torn.pick {
            self.show_browser_tab(id, window, cx);
        }
        let holder = window.window_handle().window_id();
        let _ = torn.from.update(cx, |from, _| {
            from.browser
                .hidden
                .hold(&torn.workspace, &torn.pick, holder);
        });
        self.arrival = Some(torn);
        self.land_arrival(false, cx);
    }

    /// Goes where the waiting torn-off tab is, once its host is listed:
    /// the session it showed there, then the tab, or a page's workspace,
    /// with the workspace's other tabs hidden and unsplit.
    /// The navigation itself waits for the surface. After the saved hosts
    /// are `listed`, a host still missing is gone, and the tab with it.
    pub(crate) fn land_arrival(&mut self, listed: bool, cx: &mut Context<Self>) {
        let Some(torn) = self.arrival.take() else {
            return;
        };
        if !self
            .endpoints
            .iter()
            .any(|endpoint| endpoint.id == torn.endpoint && endpoint.enabled)
        {
            if !listed {
                self.arrival = Some(torn);
            }
            return;
        }
        self.browser
            .hidden
            .hide(torn.workspace.clone(), torn.others.iter().cloned());
        if let Some(session) = remote_session(&torn.session) {
            self.select_device_session(&torn.endpoint, session, cx);
        }
        let target = match &torn.pick {
            Pick::Herdr(tab) => NavigationTarget::Tab(tab.clone()),
            Pick::Page(id) => {
                let Some(tab) = cx.try_global::<Store>().and_then(|store| store.get(*id)) else {
                    return;
                };
                NavigationTarget::Workspace(tab.workspace_id.clone())
            }
        };
        self.navigate_endpoint(&torn.endpoint, target.as_deref(), cx);
    }
}

/// The session a remote host's endpoint is attached to, which another
/// window of the same host may not be.
pub(super) fn remote_session(target: &ConnectTarget) -> Option<&str> {
    match target {
        ConnectTarget::Ssh { session, .. } | ConnectTarget::Wsl { session, .. } => Some(session),
        _ => None,
    }
}
