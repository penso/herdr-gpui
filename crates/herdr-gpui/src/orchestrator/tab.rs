//! The orchestrator in a tab: opening it for the focused workspace's
//! repository, restoring it after a restart, feeding it the theme, live
//! agents and GitHub account from the window's tick, and acting on its
//! events. The view itself is an entity that knows nothing of tabs.

use super::{Event, LiveAgent, LiveBranch, Look, OrchestratorView, Request, Timing};
use crate::{
    HerdrWindow, NavigationTarget,
    browser::{Location, OrchestratorRepo, Slot, Store, Tab, TabId, WebUrl},
    window::Flash,
};
use gpui::{prelude::*, *};
use herdr_client::ConnectTarget;
use std::sync::Arc;

/// Events waiting for the tick; a view cannot emit more than a few per frame.
const MAX_EVENTS: usize = 16;

/// An open orchestrator tab's view and its event subscription.
pub(crate) struct Orchestrator {
    pub(super) view: Entity<OrchestratorView>,
    pub(super) _events: Subscription,
}

#[cfg(test)]
impl Orchestrator {
    pub(crate) fn view(&self) -> &Entity<OrchestratorView> {
        &self.view
    }
}

/// The live agents and workspace branches last handed to the views, and the
/// snapshots they came from, so they are rebuilt only when a snapshot changes.
#[derive(Default)]
pub(crate) struct LiveCache {
    from: Vec<usize>,
    agents: Arc<Vec<LiveAgent>>,
    branches: Arc<Vec<LiveBranch>>,
}

impl HerdrWindow {
    /// Opens the focused workspace's orchestrator tab, or brings back the one
    /// already open.
    pub(crate) fn open_orchestrator(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_, workspace)) = self.browser_key() else {
            self.show_flash(Flash::warning("Open a workspace first"), cx);
            return;
        };
        let checkout = self.focused_checkout();
        self.open_orchestrator_in(workspace, checkout, window, cx);
    }

    /// Opens workspace `workspace`'s orchestrator tab, listing the repository
    /// of `checkout`. Another workspace than the focused one is switched to,
    /// and its tab shows once it is.
    pub(crate) fn open_orchestrator_in(
        &mut self,
        workspace: String,
        checkout: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = &self.endpoints[self.selected_endpoint].connection.target;
        if matches!(target, ConnectTarget::Socket(_) | ConnectTarget::Wsl { .. }) {
            self.show_flash(
                Flash::warning("The orchestrator needs a local or SSH host"),
                cx,
            );
            return;
        }
        let scope = crate::browser::scope(&self.endpoints[self.selected_endpoint]);
        let Some(repo) = checkout.and_then(|path| OrchestratorRepo::new(path).ok()) else {
            self.show_flash(Flash::warning("This workspace has no folder to read"), cx);
            return;
        };
        let existing = cx.try_global::<Store>().and_then(|store| {
            store
                .in_workspace(&scope, &workspace)
                .find(|tab| matches!(&tab.location, Some(Location::Orchestrator { .. })))
                .map(|tab| tab.id)
        });
        let opened = existing.or_else(|| {
            Store::update(cx, |store| {
                store.open(
                    scope,
                    &workspace,
                    Some(Location::Orchestrator { repo }),
                    None,
                )
            })
        });
        let Some(id) = opened else {
            self.show_flash(Flash::warning("Too many tabs are open"), cx);
            return;
        };
        self.dismiss_menu(window, cx);
        if self
            .browser_key()
            .is_none_or(|(_, focused)| focused != workspace)
        {
            self.navigate(NavigationTarget::Workspace(&workspace), cx);
            return;
        }
        self.ensure_orchestrator(id, cx);
        self.show_browser_tab(id, window, cx);
        if let Some(orchestrator) = self.orchestrators.get(&id) {
            let focus = orchestrator.view.read(cx).focus_handle().clone();
            window.focus(&focus, cx);
        }
    }

    /// Whether the focused workspace's repository can be listed: a host
    /// scripts can run on, with a workspace open.
    pub(crate) fn can_open_orchestrator(&self) -> bool {
        self.orchestrator_host() && self.browser_key().is_some()
    }

    /// Whether the shown host can run the orchestrator's scripts.
    pub(crate) fn orchestrator_host(&self) -> bool {
        !matches!(
            self.endpoints[self.selected_endpoint].connection.target,
            ConnectTarget::Socket(_) | ConnectTarget::Wsl { .. }
        )
    }

    /// The folder new tabs of workspace `id` start in.
    pub(crate) fn workspace_folder(&self, id: &str) -> Option<String> {
        self.live
            .snapshot
            .as_deref()?
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_id == id)
            .map(|workspace| workspace.new_workspace_cwd.clone())
            .filter(|path| !path.is_empty())
    }

    /// A folder of the focused workspace: its focused pane's, else the one
    /// new tabs start in.
    fn focused_checkout(&self) -> Option<String> {
        let snapshot = self.live.snapshot.as_deref()?;
        let workspace = snapshot.focused_workspace_id.as_deref()?;
        let pane = snapshot
            .panes
            .iter()
            .find(|pane| pane.workspace_id == workspace && pane.focused)
            .and_then(|pane| pane.foreground_cwd.clone().or_else(|| pane.cwd.clone()));
        pane.or_else(|| {
            snapshot
                .workspaces
                .iter()
                .find(|found| found.workspace_id == workspace)
                .map(|found| found.new_workspace_cwd.clone())
        })
        .filter(|path| !path.is_empty())
    }

    /// The orchestrator's look: the UI font's family at the terminal font's
    /// size, so Cmd-+ and Cmd-- scale it with the terminals.
    fn look(&self) -> Look {
        let mut ui = self.config.ui.clone();
        ui.size = self.config.terminal.size;
        Look {
            theme: self.theme.clone(),
            ui,
            mono: self.config.terminal.clone(),
        }
    }

    /// The GitHub account a view on `target` reads and writes with: the SSH
    /// host's own account when it has one, as the GitHub page picks it for
    /// that host, else the main account. Never the host shown now, so a
    /// detached view keeps its account when another host is selected.
    pub(super) fn github_account(
        &self,
        target: &ConnectTarget,
    ) -> (Option<Arc<secrecy::SecretString>>, Option<String>) {
        let host = self
            .endpoints
            .iter()
            .filter(|endpoint| matches!(endpoint.connection.target, ConnectTarget::Ssh { .. }))
            .find(|endpoint| &endpoint.connection.target == target)
            .and_then(|endpoint| self.menu.github_hosts.get(&endpoint.id));
        let profile =
            host.and_then(|auth| auth.profile.as_ref())
                .or(self.menu.github.profile.as_ref());
        match profile {
            Some(profile) => (Some(profile.token.clone()), Some(profile.login.clone())),
            None => (None, None),
        }
    }

    /// Makes `id`'s view from its tab, if the window has none yet.
    fn ensure_orchestrator(&mut self, id: TabId, cx: &mut Context<Self>) {
        if self.orchestrators.contains_key(&id) {
            return;
        }
        let Some(Location::Orchestrator { repo }) = cx
            .try_global::<Store>()
            .and_then(|store| store.get(id))
            .and_then(|tab| tab.location.clone())
        else {
            return;
        };
        let Some((_, workspace_id)) = self.browser_key() else {
            return;
        };
        let target = self.endpoints[self.selected_endpoint]
            .connection
            .target
            .clone();
        let (token, login) = self.github_account(&target);
        let request = Request {
            target,
            checkout: repo.checkout,
            workspace_id,
            token,
            data_root: None,
            timing: Timing::default(),
        };
        let look = self.look();
        let view = cx.new(|cx| {
            let mut view = OrchestratorView::new(request, look, cx);
            view.set_github(None, login, cx);
            view
        });
        // Handled from the tick, which has the window acting on them needs.
        let events = cx.subscribe(&view, move |this, _, event: &Event, cx| {
            if this.orchestrator_events.len() < MAX_EVENTS {
                this.orchestrator_events.push((id, event.clone()));
                cx.notify();
            }
        });
        self.orchestrators.insert(
            id,
            Orchestrator {
                view,
                _events: events,
            },
        );
    }

    /// Runs every window tick: views for restored tabs are made, closed
    /// tabs' views go, and every view gets its worker's news and the
    /// window's current look, agents, and account.
    pub(crate) fn poll_orchestrators(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (id, event) in std::mem::take(&mut self.orchestrator_events) {
            self.orchestrator_event(id, event, window, cx);
        }
        let Some(store) = cx.try_global::<Store>() else {
            self.orchestrators.clear();
            return;
        };
        self.orchestrators.retain(|id, _| store.get(*id).is_some());
        if let Some((scope, workspace)) = self.browser_key() {
            let missing: Vec<TabId> = store
                .in_workspace(&scope, &workspace)
                .filter(|tab| {
                    matches!(tab.location, Some(Location::Orchestrator { .. }))
                        && !self.orchestrators.contains_key(&tab.id)
                })
                .map(|tab| tab.id)
                .collect();
            for id in missing {
                self.ensure_orchestrator(id, cx);
            }
        }
        self.forget_closed_orchestrator_windows(cx);
        let views = self.orchestrator_views();
        if views.is_empty() {
            self.orchestrator_sampling = false;
            return;
        }
        // Hosts for open dispatch dialogs, ranked as smart dispatch ranks them.
        // Only a view of the shown host's repository can be ranked against it.
        let shown = self.endpoints[self.selected_endpoint]
            .connection
            .target
            .clone();
        let wanting: Vec<(Entity<OrchestratorView>, String)> = views
            .iter()
            .filter(|(_, view)| view.read(cx).wants_hosts())
            .map(|(_, view)| (view.clone(), view.read(cx).workspace_id().to_owned()))
            .collect();
        self.orchestrator_sampling = !wanting.is_empty();
        for (view, workspace) in wanting {
            let hosts = if view.read(cx).target() == &shown {
                let label = self
                    .orchestrator_repository(&workspace)
                    .map(|(_, label)| label)
                    .unwrap_or_default();
                self.dispatch_candidates(&label, cx)
            } else {
                Vec::new()
            };
            view.update(cx, |view, cx| view.set_hosts(hosts, cx));
        }
        let (live, branches) = self.live_agents();
        let look = self.look();
        for (_, view) in views {
            let (token, login) = self.github_account(view.read(cx).target());
            view.update(cx, |view, cx| {
                view.poll(cx);
                view.set_look(look.clone(), cx);
                view.set_live(live.clone(), branches.clone(), cx);
                view.set_github(token, login, cx);
            });
        }
    }

    /// Every agent and workspace branch on a connected host, rebuilt only
    /// when a snapshot changed.
    fn live_agents(&mut self) -> (Arc<Vec<LiveAgent>>, Arc<Vec<LiveBranch>>) {
        let snapshots: Vec<_> = (0..self.endpoints.len())
            .map(|index| {
                let live = if index == self.selected_endpoint {
                    &self.live
                } else {
                    &self.endpoints[index].live
                };
                live.snapshot
                    .as_ref()
                    .filter(|_| live.status.is_connected())
                    .cloned()
            })
            .collect();
        let from: Vec<usize> = snapshots
            .iter()
            .map(|snapshot| snapshot.as_ref().map_or(0, |s| Arc::as_ptr(s) as usize))
            .collect();
        if from == self.orchestrator_live.from {
            return (
                self.orchestrator_live.agents.clone(),
                self.orchestrator_live.branches.clone(),
            );
        }
        let place_of =
            |index: usize| super::actions::place(&self.endpoints[index].connection.target);
        let branches: Vec<LiveBranch> = snapshots
            .iter()
            .enumerate()
            .filter_map(|(index, snapshot)| Some((index, snapshot.as_ref()?)))
            .flat_map(|(index, snapshot)| {
                snapshot.workspaces.iter().filter_map(move |workspace| {
                    Some(LiveBranch {
                        host: place_of(index).0,
                        session: place_of(index).1,
                        workspace_id: workspace.workspace_id.clone(),
                        branch: workspace.branch.clone()?,
                    })
                })
            })
            .collect();
        let agents: Vec<LiveAgent> = snapshots
            .iter()
            .enumerate()
            .filter_map(|(index, snapshot)| Some((index, snapshot.as_ref()?)))
            .flat_map(|(index, snapshot)| {
                let (host, session) = place_of(index);
                snapshot.agents.iter().map(move |agent| LiveAgent {
                    endpoint: index,
                    host: host.clone(),
                    session: session.clone(),
                    workspace_id: agent.workspace_id.clone(),
                    pane_id: agent.pane_id.clone(),
                    status: agent.agent_status,
                    title: agent
                        .title
                        .as_deref()
                        .or(agent.terminal_title_stripped.as_deref())
                        .map(|title| crate::notifications::safe_text(title, 160))
                        .filter(|title| !title.trim().is_empty()),
                })
            })
            .collect();
        self.orchestrator_live = LiveCache {
            from,
            agents: Arc::new(agents),
            branches: Arc::new(branches),
        };
        (
            self.orchestrator_live.agents.clone(),
            self.orchestrator_live.branches.clone(),
        )
    }

    fn orchestrator_event(
        &mut self,
        id: TabId,
        event: Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            Event::LoadAgents { endpoint } => {
                if let Some(view) = self.orchestrator_view(id).cloned() {
                    let target = match &endpoint {
                        Some(endpoint) => self
                            .endpoints
                            .iter()
                            .find(|e| &e.id == endpoint)
                            .map(|e| e.connection.target.clone()),
                        None => Some(view.read(cx).target().clone()),
                    };
                    if let Some(target) = target {
                        view.update(cx, |view, cx| view.load_agents(endpoint, target, cx));
                    } else {
                        self.tell_orchestrator(id, super::Error::HostCannotTake, cx);
                    }
                }
            }
            Event::Dispatch { request, endpoint } => {
                self.dispatch_elsewhere(id, *request, &endpoint, cx)
            }
            Event::OpenWindow => self.detach_orchestrator(id, cx),
            Event::OpenRun {
                endpoint, pane_id, ..
            } => {
                let Some(endpoint) = self.endpoints.get(endpoint).map(|e| e.id.clone()) else {
                    self.tell_orchestrator(id, super::Error::WorkspaceClosed, cx);
                    return;
                };
                if !self.navigate_endpoint(&endpoint, NavigationTarget::Pane(&pane_id), cx) {
                    self.tell_orchestrator(id, super::Error::MenuOpen, cx);
                }
            }
            Event::OpenWorkspace {
                host,
                session,
                workspace_id,
            } => {
                self.open_run_workspace(id, host.as_deref(), session.as_deref(), &workspace_id, cx)
            }
            Event::OpenUrl(url) => match WebUrl::try_from(url.as_str()) {
                Ok(url) => {
                    if let Some((_, workspace)) = self.browser_key() {
                        let endpoint = self.endpoints[self.selected_endpoint].id.clone();
                        self.open_workspace_page(&endpoint, &workspace, url, window, cx);
                    }
                }
                Err(_) => self.tell_orchestrator(id, super::Error::NotWebAddress, cx),
            },
            Event::SignIn => {
                if self.open_menu(window, cx) {
                    self.menu.page = Some(crate::menu::Page::GitHub);
                }
                cx.notify();
            }
        }
    }

    /// The repository of workspace `id` on the shown host, as smart dispatch
    /// keys it: its Git common directory and its name.
    fn orchestrator_repository(&self, id: &str) -> Option<(String, String)> {
        let worktree = self
            .live
            .snapshot
            .as_deref()?
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_id == id)?
            .worktree
            .as_ref()?;
        Some((worktree.key.clone(), worktree.label.clone()))
    }

    /// Sets `request` up for host `endpoint` as smart dispatch does, then
    /// hands it back to tab `id`'s view to run.
    fn dispatch_elsewhere(
        &mut self,
        id: TabId,
        mut request: super::DispatchRequest,
        endpoint: &str,
        cx: &mut Context<Self>,
    ) {
        let shown = &self.endpoints[self.selected_endpoint].connection.target;
        if self
            .orchestrator_view(id)
            .is_some_and(|view| view.read(cx).target() != shown)
        {
            self.tell_orchestrator(id, super::Error::HostNotShown, cx);
            return;
        }
        let set_up = self
            .orchestrator_repository(&request.workspace_id)
            .and_then(|(key, label)| {
                let origin = self.dispatch_origin(&request.workspace_id, &key, &label)?;
                let destination = self.dispatch_destination(endpoint)?;
                let target = self
                    .endpoints
                    .iter()
                    .find(|e| e.id == endpoint)?
                    .connection
                    .target
                    .clone();
                Some((
                    label,
                    super::actions::Elsewhere {
                        origin,
                        destination,
                        target,
                    },
                ))
            });
        let Some((label, elsewhere)) = set_up else {
            self.tell_orchestrator(id, super::Error::HostCannotTake, cx);
            return;
        };
        crate::dispatch::History::update(cx, |history| history.record(&label, endpoint));
        request.elsewhere = Some(elsewhere);
        if let Some(view) = self.orchestrator_view(id).cloned() {
            view.update(cx, |view, _| view.dispatch_elsewhere(request));
        }
    }

    /// Shows workspace `workspace_id` on the connected host `host` names:
    /// the shown host when it fits, else the first that does.
    fn open_run_workspace(
        &mut self,
        view: TabId,
        host: Option<&str>,
        session: Option<&str>,
        workspace_id: &str,
        cx: &mut Context<Self>,
    ) {
        let fits = |target: &ConnectTarget| {
            let place = super::actions::place(target);
            place.0.as_deref() == host && place.1.as_deref() == session
        };
        let has = |index: usize| {
            let live = if index == self.selected_endpoint {
                &self.live
            } else {
                &self.endpoints[index].live
            };
            live.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.workspace_id == workspace_id)
            })
        };
        let found = std::iter::once(self.selected_endpoint)
            .chain(0..self.endpoints.len())
            .find(|&index| fits(&self.endpoints[index].connection.target) && has(index));
        let Some(index) = found else {
            self.tell_orchestrator(view, super::Error::WorkspaceClosed, cx);
            return;
        };
        let id = self.endpoints[index].id.clone();
        if !self.navigate_endpoint(&id, NavigationTarget::Workspace(workspace_id), cx) {
            self.tell_orchestrator(view, super::Error::MenuOpen, cx);
        }
    }

    /// Shows why a view's request did not happen in that view's own banner:
    /// the window's flash draws over terminals, behind the view's tab.
    fn tell_orchestrator(&self, id: TabId, error: super::Error, cx: &mut Context<Self>) {
        if let Some(view) = self.orchestrator_view(id).cloned() {
            view.update(cx, |view, cx| view.report(error, cx));
        }
    }

    /// An orchestrator tab, drawn in `slot` where a page would be.
    pub(crate) fn render_orchestrator_tab(&self, slot: Slot, tab: &Tab, gap: f32) -> AnyElement {
        let body = match self.orchestrators.get(&tab.id) {
            Some(orchestrator) => orchestrator.view.clone().into_any_element(),
            None => div().into_any_element(),
        };
        div()
            .id(SharedString::from(slot.selector("orchestrator-tab")))
            .size_full()
            .min_w_0()
            .pl(px(gap))
            .child(body)
            .into_any_element()
    }
}
