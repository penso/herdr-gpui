//! The orchestrator's view, owned by the window: one, made once there is a
//! repository to list while the activity bar is on, so the bar counts pull
//! requests and issues before either section is shown (see `section`). It
//! lists every distinct repository the workspaces of every connected host
//! hold. The window's tick hands it those repositories, the theme, live
//! agents, and each host's GitHub account, and acts on its events. The view
//! itself knows nothing of its host.

use super::{
    Event, GitLabRead, LiveAgent, LiveBranch, Look, OrchestratorView, RepoSpec, Request, Timing,
};
use crate::{
    HerdrWindow, NavigationTarget,
    browser::{OrchestratorRepo, WebUrl},
};
use gpui::{prelude::*, *};
use herdr_client::ConnectTarget;
use std::sync::Arc;

/// Events waiting for the tick; a view cannot emit more than a few per frame.
const MAX_EVENTS: usize = 16;

/// A view the window made, by the order it was made in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct ViewId(u64);

#[cfg(test)]
impl ViewId {
    pub(crate) fn test(id: u64) -> Self {
        Self(id)
    }
}

/// The most repositories one view lists; workspaces beyond are left out.
const MAX_REPOS: usize = 32;

/// A view and its event subscription.
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
    ) -> (Option<crate::github::Tokens>, Option<String>) {
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
            Some(profile) => (Some(self.tokens_over(profile)), Some(profile.login.clone())),
            None => (None, None),
        }
    }

    /// The sections' view, made if the window has none yet.
    pub(super) fn sections_view(&mut self, cx: &mut Context<Self>) -> ViewId {
        if let Some(id) = self
            .activity
            .section_view
            .filter(|id| self.orchestrators.contains_key(id))
        {
            return id;
        }
        self.next_orchestrator += 1;
        let id = ViewId(self.next_orchestrator);
        let look = self.look();
        let view = cx.new(|cx| OrchestratorView::new(look, cx));
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
        id
    }

    /// Every distinct repository the workspaces of the connected hosts that
    /// can run the orchestrator's scripts hold, in sidebar order. Worktrees
    /// of one repository are one: they share its database.
    pub(crate) fn repo_specs(&self) -> Vec<RepoSpec> {
        let mut specs: Vec<RepoSpec> = Vec::new();
        // A headless window never reads the user's saved GitLab tokens.
        let gitlab_store = if self.reads_credentials() {
            self.config.gitlab.store()
        } else {
            herdr_credentials::Store::default()
        };
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            let target = &endpoint.connection.target;
            let state = if index == self.selected_endpoint {
                &self.live
            } else {
                &endpoint.live
            };
            let (true, true, Some(snapshot)) = (
                endpoint.enabled && state.status.is_connected(),
                scripts_run_on(target),
                state.snapshot.as_deref(),
            ) else {
                continue;
            };
            let (token, _) = self.github_account(target);
            for workspace in &snapshot.workspaces {
                let Ok(repo) = OrchestratorRepo::new(workspace.new_workspace_cwd.clone()) else {
                    continue;
                };
                let key = repo_key(
                    &endpoint.id,
                    workspace.worktree.as_ref().map(|tree| tree.key.as_str()),
                    &repo.checkout,
                );
                if specs.iter().any(|spec| spec.key == key) {
                    continue;
                }
                if specs.len() == MAX_REPOS {
                    return specs;
                }
                specs.push(RepoSpec {
                    key,
                    host: endpoint.label.clone(),
                    request: Request {
                        target: target.clone(),
                        checkout: repo.checkout,
                        workspace_id: workspace.workspace_id.clone(),
                        token: token.clone(),
                        data_root: None,
                        gitlab_store,
                        timing: Timing::default(),
                    },
                });
            }
        }
        specs
    }

    /// The key [`Self::repo_specs`] gives workspace `workspace` on the shown
    /// host, so a section can pick that project.
    pub(crate) fn workspace_repo_key(&self, workspace: &str) -> Option<String> {
        let endpoint = self.endpoints.get(self.selected_endpoint)?;
        let found = self
            .live
            .snapshot
            .as_deref()?
            .workspaces
            .iter()
            .find(|shown| shown.workspace_id == workspace)?;
        let repo = OrchestratorRepo::new(found.new_workspace_cwd.clone()).ok()?;
        Some(repo_key(
            &endpoint.id,
            found.worktree.as_ref().map(|tree| tree.key.as_str()),
            &repo.checkout,
        ))
    }

    pub(super) fn orchestrator_view(&self, id: ViewId) -> Option<&Entity<OrchestratorView>> {
        self.orchestrators
            .get(&id)
            .map(|orchestrator| &orchestrator.view)
    }

    /// The GitLab projects this window's views read, for the account menu.
    pub(crate) fn orchestrator_gitlab_hosts(&self, cx: &App) -> Vec<GitLabRead> {
        self.orchestrators
            .values()
            .flat_map(|orchestrator| {
                orchestrator
                    .view
                    .read(cx)
                    .gitlab_hosts()
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// [`Self::orchestrator_gitlab_hosts`] over every main window, since a
    /// saved token serves them all. A window handling its own input is left
    /// out; never call it while a window's root view is being updated
    /// without its window, as a spawned task's update is, since GPUI panics
    /// reading that view.
    pub(crate) fn all_orchestrator_gitlab_hosts(cx: &App) -> Vec<GitLabRead> {
        cx.windows()
            .into_iter()
            .filter_map(|handle| handle.downcast::<HerdrWindow>())
            .filter_map(|handle| handle.read(cx).ok())
            .flat_map(|window| window.orchestrator_gitlab_hosts(cx))
            .collect()
    }

    /// One of this window's views has a GitLab source waiting for a token,
    /// which the title bar's account button marks.
    pub(crate) fn orchestrator_gitlab_needs_sign_in(&self, cx: &App) -> bool {
        self.orchestrators
            .values()
            .any(|orchestrator| orchestrator.view.read(cx).gitlab_needs_sign_in())
    }

    /// A saved sign-in changed, such as a GitLab token pasted in the account
    /// menu: every view in every main window syncs again, so a new token
    /// shows without pressing Refresh. Each window keeps its own views and
    /// workers. Like [`Self::all_orchestrator_gitlab_hosts`], it runs
    /// outside any window's update.
    pub(crate) fn refresh_all_orchestrators(cx: &mut App) {
        let windows: Vec<_> = cx
            .windows()
            .into_iter()
            .filter_map(|handle| handle.downcast::<HerdrWindow>())
            .collect();
        for handle in windows {
            let _ = handle.update(cx, |window, _, cx| {
                for orchestrator in window.orchestrators.values() {
                    orchestrator
                        .view
                        .update(cx, |view, cx| view.credentials_changed(cx));
                }
            });
        }
    }

    /// Every view, by id.
    pub(crate) fn orchestrator_views(&self) -> Vec<(ViewId, Entity<OrchestratorView>)> {
        let mut views: Vec<_> = self
            .orchestrators
            .iter()
            .map(|(id, orchestrator)| (*id, orchestrator.view.clone()))
            .collect();
        views.sort_unstable_by_key(|(id, _)| *id);
        views
    }

    /// Runs every window tick: views of closed workspaces go, the shown
    /// section gets its view, and every view gets its worker's news and the
    /// window's current look, agents, and account.
    pub(crate) fn poll_orchestrators(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (id, event) in std::mem::take(&mut self.orchestrator_events) {
            self.orchestrator_event(id, event, window, cx);
        }
        self.sync_activity_section(cx);
        self.prepare_activity_sections(cx);
        let views = self.orchestrator_views();
        // The bar's counts read what the views polled last tick.
        if self.activity.observe_sources(&views, cx) {
            cx.notify();
        }
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
            .filter_map(|(_, view)| {
                let workspace = view.read(cx).workspace_id()?.to_owned();
                Some((view.clone(), workspace))
            })
            .collect();
        self.orchestrator_sampling = !wanting.is_empty();
        for (view, workspace) in wanting {
            let hosts = if view.read(cx).target() == Some(&shown) {
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
        let specs = self.repo_specs();
        for (_, view) in views {
            let accounts: Vec<_> = view
                .read(cx)
                .targets()
                .into_iter()
                .map(|target| {
                    let (token, login) = self.github_account(&target);
                    (target, token, login)
                })
                .collect();
            view.update(cx, |view, cx| {
                view.set_repos(specs.clone(), cx);
                view.poll(cx);
                view.set_look(look.clone(), cx);
                view.set_live(live.clone(), branches.clone(), cx);
                for (target, token, login) in accounts {
                    view.set_github(&target, token, login, cx);
                }
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
        id: ViewId,
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
                        None => view.read(cx).target().cloned(),
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
            Event::OpenRun {
                endpoint, pane_id, ..
            } => {
                let Some(endpoint) = self.endpoints.get(endpoint).map(|e| e.id.clone()) else {
                    self.tell_orchestrator(id, super::Error::WorkspaceClosed, cx);
                    return;
                };
                if self.navigate_endpoint(&endpoint, NavigationTarget::Pane(&pane_id), cx) {
                    self.leave_activity_section(id, cx);
                } else {
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
                        // Only a page tab shows behind the section; the system
                        // browser, or a page that did not open, leaves it be.
                        let opened =
                            self.open_workspace_page(&endpoint, &workspace, url, window, cx);
                        if opened == crate::browser::PageOpened::Tab {
                            self.leave_activity_section(id, cx);
                        }
                    }
                }
                Err(_) => self.tell_orchestrator(id, super::Error::NotWebAddress, cx),
            },
            Event::SignIn(provider) => {
                if self.open_menu(window, cx) {
                    if provider == super::Provider::Gitlab {
                        self.open_gitlab_page(window, cx);
                    } else {
                        self.menu.page = Some(crate::menu::Page::GitHub);
                    }
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
    /// hands it back to view `id` to run.
    fn dispatch_elsewhere(
        &mut self,
        id: ViewId,
        mut request: super::DispatchRequest,
        endpoint: &str,
        cx: &mut Context<Self>,
    ) {
        let shown = &self.endpoints[self.selected_endpoint].connection.target;
        if self
            .orchestrator_view(id)
            .is_some_and(|view| view.read(cx).target() != Some(shown))
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
        view: ViewId,
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
        if self.navigate_endpoint(&id, NavigationTarget::Workspace(workspace_id), cx) {
            self.leave_activity_section(view, cx);
        } else {
            self.tell_orchestrator(view, super::Error::MenuOpen, cx);
        }
    }

    /// Shows why a view's request did not happen in that view's own banner:
    /// the window's flash would draw over terminals the section hides.
    fn tell_orchestrator(&self, id: ViewId, error: super::Error, cx: &mut Context<Self>) {
        if let Some(view) = self.orchestrator_view(id).cloned() {
            view.update(cx, |view, cx| view.report(error, cx));
        }
    }
}

/// Whether the orchestrator's scripts run on `target`: the local session or
/// an SSH host, not a bare socket or WSL.
pub(super) fn scripts_run_on(target: &ConnectTarget) -> bool {
    !matches!(target, ConnectTarget::Socket(_) | ConnectTarget::Wsl { .. })
}

/// How the view tells repositories apart: the endpoint, then the Git common
/// directory its worktrees share, else the folder itself.
fn repo_key(endpoint: &str, common: Option<&str>, checkout: &str) -> String {
    format!("{endpoint}\u{1f}{}", common.unwrap_or(checkout))
}
