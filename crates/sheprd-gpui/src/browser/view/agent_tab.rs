//! Tabs an agent asks for, from this machine's control socket or a remote
//! host's request, and the workspace switch that shows them.

use super::{scope, store};
use crate::{
    NavigationTarget, SheprdWindow,
    browser::{Location, Store, TabId},
};
use gpui::*;

impl SheprdWindow {
    /// Opens `location` in a tab of `workspace` on endpoint `index` for the
    /// agent in pane `origin`, as a control request or a remote host's
    /// request asks. `None` when too many tabs are open.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open_agent_tab(
        &mut self,
        index: usize,
        workspace: &str,
        origin: Option<&str>,
        location: &Location,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<TabId> {
        let scope = scope(&self.endpoints[index]);
        // An agent showing the same page again gets its tab back, reloaded,
        // rather than another tab for every revision.
        let before =
            store(cx).and_then(|store| store.opened_before(&scope, workspace, origin, location));
        let id = match before {
            Some(id) => {
                #[cfg(any(target_os = "macos", windows))]
                self.browser.pages.reload(id);
                id
            }
            None => {
                let opened = Store::update(cx, |store| {
                    store.open(
                        scope,
                        workspace,
                        Some(location.clone()),
                        origin.map(str::to_owned),
                    )
                });
                opened?
            }
        };
        if focus {
            self.show_agent_tab(index, workspace, id, window, cx);
        }
        cx.notify();
        Some(id)
    }

    /// Goes to `workspace` of endpoint `index` and shows tab `id` there.
    pub(crate) fn show_agent_tab(
        &mut self,
        index: usize,
        workspace: &str,
        id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.go_to_workspace(index, workspace, cx);
        // Recorded against the workspace, so the tab shows once the
        // navigation lands even if it is still in flight.
        self.show_browser_tab(id, window, cx);
    }

    /// Reloads this window's pages for `tabs`, as an agent asks after
    /// editing a page it showed.
    pub(crate) fn reload_browser_tabs(&mut self, tabs: &[TabId]) {
        #[cfg(any(target_os = "macos", windows))]
        for id in tabs {
            self.browser.pages.reload(*id);
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        let _ = tabs;
    }

    /// Whether the window shows `workspace` of endpoint `index` now.
    pub(crate) fn shows_workspace(&self, index: usize, workspace: &str) -> bool {
        index == self.selected_endpoint
            && self
                .live
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.focused_workspace_id.as_deref())
                == Some(workspace)
    }

    /// Switches to `workspace` of endpoint `index`, unless the window shows
    /// it already. The switch lands with a later snapshot.
    pub(crate) fn go_to_workspace(
        &mut self,
        index: usize,
        workspace: &str,
        cx: &mut Context<Self>,
    ) {
        if self.shows_workspace(index, workspace) {
            return;
        }
        if index != self.selected_endpoint {
            let endpoint = self.endpoints[index].id.clone();
            self.navigate_endpoint(&endpoint, NavigationTarget::Workspace(workspace), cx);
        } else {
            self.navigate(NavigationTarget::Workspace(workspace), cx);
        }
    }
}
