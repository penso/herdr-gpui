//! The orchestrator view: the issues, pull requests, and agent runs of every
//! repository the window's workspaces hold, as one list the activity bar's
//! Pull requests and Issues sections draw. Each repository keeps its own
//! worker and database (`repo`); an item's page and actions go to its own.
//! The view talks to its host only through [`Event`]s and setters.
//!
//! It never blocks: the [`Service`] worker does the I/O, and the host pushes
//! the theme, live agent statuses, and the GitHub account from its tick.

mod conversation;
mod detail;
mod dispatch;
mod hosts;
mod inbox;
mod list;
mod listing;
mod look;
mod markdown;
pub(crate) mod pane;
mod pills;
mod pull;
mod repo;
mod rows;
mod status;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

pub(crate) use look::Look;
pub(crate) use markdown::without_html;
use pane::{PANE_DEFAULT, PaneDrag, Resize};
pub(crate) use repo::RepoSpec;
pub(crate) use rows::{LiveAgent, Tab as List, key_label};

use super::{Notice, Provider, Request, Service, Snapshot, SyncState};
use crate::search_input::{self, SearchInput};
use gpui::{prelude::*, *};
use repo::Repo;
pub(crate) use rows::Tab;
use rows::{Filters, Runs, Sort};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

/// What the view asks its host to do.
#[derive(Clone, Debug)]
pub(crate) enum Event {
    LoadAgents {
        endpoint: Option<String>,
    },
    /// Show the pane a run's agent is in, on the endpoint the host listed it
    /// from.
    OpenRun {
        endpoint: usize,
        pane_id: String,
    },
    /// Show a run's Herdr workspace on the host it is on: `None` for this
    /// machine, else an SSH destination.
    OpenWorkspace {
        host: Option<String>,
        session: Option<String>,
        workspace_id: String,
    },
    /// Open a web address, such as an issue's page, in a browser tab.
    OpenUrl(String),
    /// Sign in to GitHub, which the view needs to list a repository.
    SignIn,
    /// Start `request` on the host `endpoint`, which the window sets it up
    /// for, then hands back through [`OrchestratorView::dispatch_elsewhere`].
    Dispatch {
        request: Box<super::DispatchRequest>,
        endpoint: String,
    },
}

impl EventEmitter<Event> for OrchestratorView {}

/// How long Refresh spins before the workers' own sync states take over.
const REFRESH_GRACE: Duration = Duration::from_millis(1500);

/// The detail page of one item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Detail {
    /// The item's canonical key.
    pub(crate) key: String,
    pub(crate) tab: DetailTab,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DetailTab {
    Conversation,
    Checks,
    #[default]
    Description,
    Agent,
    Details,
}

impl DetailTab {
    /// The pages an item has: a pull request also has its conversation and
    /// checks, first.
    pub(crate) fn tabs(pull_request: bool) -> &'static [Self] {
        if pull_request {
            &[
                Self::Conversation,
                Self::Checks,
                Self::Description,
                Self::Agent,
                Self::Details,
            ]
        } else {
            &[Self::Description, Self::Agent, Self::Details]
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Conversation => "Conversation",
            Self::Checks => "Checks",
            Self::Description => "Description",
            Self::Agent => "Agent",
            Self::Details => "Details",
        }
    }
}

pub(crate) struct OrchestratorView {
    /// Every repository listed, and the one the open item, the selection,
    /// and the dialogs belong to.
    repos: Vec<Repo>,
    /// Non-project folders already checked; retried on Refresh or after removal.
    skipped_repos: HashSet<String>,
    active: usize,
    /// What a view without a repository reads.
    blank: Snapshot,
    /// Only these backends and projects (by repository key); none picked
    /// lists them all.
    backends: HashSet<Provider>,
    projects: HashSet<String>,
    /// Counts changes to what is listed, so the window redraws the bar's
    /// counts only when they may have moved.
    revision: u64,
    /// When Refresh was last pressed: the button spins from then until the
    /// workers report their syncs, which they start a moment later.
    refresh_asked: Option<Instant>,
    look: Look,
    live: Arc<Vec<LiveAgent>>,
    /// Herdr workspaces' branches on connected hosts, for finding work.
    live_branches: Arc<Vec<super::LiveBranch>>,
    tab: Tab,
    sort: Sort,
    sort_open: bool,
    filters: Filters,
    mine: bool,
    /// Whether the Issues filters' column is unfolded.
    filters_open: bool,
    /// Only issues with this label.
    label: Option<String>,
    collapsed: HashSet<String>,
    search: Entity<SearchInput>,
    query: String,
    /// The selected row's item key, or run id on the Runs tab.
    selected: Option<String>,
    detail: Option<Detail>,
    pane_width: f32,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    /// The shown list's lines: headings and rows.
    lines: Vec<rows::Line>,
    /// Every repository listing each shown item, by its key: one item, such
    /// as a GitHub issue two checkouts read, is one row.
    holders: HashMap<String, Vec<usize>>,
    /// Whether the shown list mixes backends, so rows mark their source.
    mixed_sources: bool,
    /// The open item's description, parsed once per change of its text.
    description: markdown::Markdown,
    dialog: Option<dispatch::Dialog>,
    confirm: Option<dispatch::Confirm>,
    /// The newest action's outcome, until dismissed or replaced.
    notice: Option<Notice>,
    /// What to type to the open item's newest run.
    message: Entity<SearchInput>,
    /// The open pull request's conversation, comment, and merge.
    pr: crate::pr_actions::Actions,
    comment: Entity<SearchInput>,
    /// Conversation rows, the list drawing them, and the threads unfolded.
    conversation: conversation::Rows,
    /// Conversation authors' avatars by URL.
    avatars: conversation::Avatars,
    merge_open: bool,
    /// Hosts a dispatch could go to, while the dialog is open.
    hosts: Arc<Vec<crate::dispatch::Candidate>>,
    _subscriptions: Vec<Subscription>,
}

impl OrchestratorView {
    pub(crate) fn new(look: Look, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| {
            let mut input = SearchInput::new(cx);
            input.set_placeholder("Filter: title, id, author, label, body\u{2026}", cx);
            input.set_frameless(cx);
            input.set_appearance(look.ui.clone(), look.theme.clone(), cx);
            input
        });
        let subscription = cx.subscribe(&search, |this, search, _: &search_input::Changed, cx| {
            this.query = search.read(cx).text().to_owned();
            this.refresh_rows();
            cx.notify();
        });
        let message = cx.new(|cx| {
            let mut input = SearchInput::new(cx);
            input.set_placeholder("Send a message to the agent\u{2026}", cx);
            input.set_appearance(look.ui.clone(), look.theme.clone(), cx);
            input
        });
        let comment = cx.new(|cx| {
            let mut input = SearchInput::new(cx);
            input.set_placeholder("Leave a comment\u{2026}", cx);
            input.set_appearance(look.ui.clone(), look.theme.clone(), cx);
            input
        });
        Self {
            repos: Vec::new(),
            skipped_repos: HashSet::new(),
            active: 0,
            blank: Snapshot::default(),
            backends: HashSet::new(),
            projects: HashSet::new(),
            revision: 0,
            refresh_asked: None,
            look,
            live: Arc::default(),
            live_branches: Arc::default(),
            tab: Tab::default(),
            sort: Sort::default(),
            sort_open: false,
            filters: Filters::default(),
            mine: false,
            filters_open: false,
            label: None,
            collapsed: HashSet::new(),
            search,
            query: String::new(),
            selected: None,
            detail: None,
            pane_width: PANE_DEFAULT,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            lines: Vec::new(),
            mixed_sources: false,
            holders: HashMap::new(),
            description: markdown::Markdown::default(),
            dialog: None,
            confirm: None,
            notice: None,
            message,
            pr: crate::pr_actions::Actions::default(),
            comment,
            conversation: conversation::Rows::default(),
            avatars: conversation::Avatars::default(),
            merge_open: false,
            hosts: Arc::default(),
            _subscriptions: vec![subscription],
        }
    }

    /// A view of one repository, for tests that drive its state directly.
    #[cfg(test)]
    pub(crate) fn with_repo(request: Request, look: Look, cx: &mut Context<Self>) -> Self {
        let mut view = Self::new(look, cx);
        view.repos.push(Repo::start(RepoSpec {
            key: "test".into(),
            host: "local".into(),
            request,
        }));
        view
    }

    /// The repositories listed, by key, for tests.
    #[cfg(test)]
    pub(crate) fn repo_keys(&self) -> Vec<String> {
        self.repos.iter().map(|repo| repo.key.clone()).collect()
    }

    /// The projects picked, for tests.
    #[cfg(test)]
    pub(crate) fn picked_projects(&self) -> Vec<String> {
        self.projects.iter().cloned().collect()
    }

    /// Whether detection found the checkout `key` names outside Git, for
    /// tests.
    #[cfg(test)]
    pub(crate) fn outside_git(&self, key: &str) -> bool {
        self.repos
            .iter()
            .any(|repo| repo.key == key && repo.outside_git())
    }

    /// The shown repository's last worker snapshot, for tests.
    #[cfg(test)]
    pub(crate) fn repo_snapshot_for_tests(&self) -> Snapshot {
        self.repos[self.active].snapshot.clone()
    }

    #[cfg(test)]
    pub(crate) fn repo_mut(&mut self) -> &mut Repo {
        &mut self.repos[self.active]
    }

    pub(crate) fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    /// Takes the worker's newest snapshot; called from the host's tick.
    pub(crate) fn poll(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        let mut read = false;
        for repo in &mut self.repos {
            let Some(service) = &repo.service else {
                continue;
            };
            if let Some(notice) = service.notices().pop() {
                self.notice = Some(notice);
                changed = true;
            }
            if let Some(snapshot) = service.poll() {
                repo.recorded_runs = snapshot.runs.clone();
                repo.snapshot = snapshot;
                read = true;
            }
        }
        if read {
            self.discard_non_projects();
            self.reconcile_agents();
            self.refresh_rows();
            self.follow_pull_request();
            changed = true;
        }
        if self.pr.poll() {
            // A comment or merge landed: read the pull request and list again.
            if self.pr.take_settled() {
                self.refresh();
            }
            self.fetch_avatars(cx);
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }

    /// Asks the worker for the open pull request's status.
    fn reload_pull_request(&self) {
        let number = self
            .detail
            .as_ref()
            .and_then(|detail| self.item(&detail.key))
            .and_then(|item| item.pull_request.as_ref())
            .map(|pr| pr.number);
        if let (Some(number), Some(service)) = (number, self.service()) {
            service.load_pull_request(number);
        }
    }

    /// Whether the dispatch dialog is open, so hosts are sampled and pushed.
    pub(crate) fn wants_hosts(&self) -> bool {
        self.dialog.is_some()
    }

    /// The host the active repository is on.
    pub(crate) fn target(&self) -> Option<&herdr_client::ConnectTarget> {
        Some(&self.request()?.target)
    }

    /// The Herdr workspace the active repository was found in.
    pub(crate) fn workspace_id(&self) -> Option<&str> {
        Some(&self.request()?.workspace_id)
    }

    /// The hosts the listed repositories are on, once each.
    pub(crate) fn targets(&self) -> Vec<herdr_client::ConnectTarget> {
        let mut targets: Vec<herdr_client::ConnectTarget> = Vec::new();
        for repo in &self.repos {
            if !targets.contains(&repo.request.target) {
                targets.push(repo.request.target.clone());
            }
        }
        targets
    }

    fn repo(&self) -> Option<&Repo> {
        self.repos.get(self.active)
    }

    /// The active repository's snapshot, or an empty one.
    fn snapshot(&self) -> &Snapshot {
        self.repo().map_or(&self.blank, |repo| &repo.snapshot)
    }

    fn service(&self) -> Option<&Service> {
        self.repo()?.service.as_ref()
    }

    fn request(&self) -> Option<&Request> {
        self.repo().map(|repo| &repo.request)
    }

    /// Changes whenever what the views list may have.
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn set_hosts(
        &mut self,
        hosts: Vec<crate::dispatch::Candidate>,
        cx: &mut Context<Self>,
    ) {
        if *self.hosts == hosts {
            return;
        }
        self.hosts = Arc::new(hosts);
        cx.notify();
    }

    /// The banner's text, for tests outside the view.
    #[cfg(test)]
    pub(crate) fn notice_text(&self) -> Option<String> {
        let notice = self.notice.as_ref()?;
        Some(match &notice.outcome {
            Ok(text) => (*text).to_owned(),
            Err(error) => error.to_string(),
        })
    }

    /// Shows `error` in the banner, for a request the host could not carry out.
    pub(crate) fn report(&mut self, error: super::Error, cx: &mut Context<Self>) {
        self.notice = Some(Notice {
            outcome: Err(Arc::new(error)),
        });
        cx.notify();
    }

    /// Starts a dispatch the window set up for another host.
    pub(crate) fn dispatch_elsewhere(&mut self, request: super::DispatchRequest) {
        self.act(super::Action::Dispatch(Box::new(request)));
    }

    pub(crate) fn set_look(&mut self, look: Look, cx: &mut Context<Self>) {
        if look.theme == self.look.theme
            && look.ui.family == self.look.ui.family
            && look.ui.size == self.look.ui.size
            && look.mono.family == self.look.mono.family
            && look.mono.size == self.look.mono.size
        {
            return;
        }
        for input in [&self.search, &self.message, &self.comment] {
            input.update(cx, |input, cx| {
                input.set_appearance(look.ui.clone(), look.theme.clone(), cx);
            });
        }
        self.look = look;
        self.conversation.remeasure();
        cx.notify();
    }

    pub(crate) fn set_live(
        &mut self,
        live: Arc<Vec<LiveAgent>>,
        branches: Arc<Vec<super::LiveBranch>>,
        cx: &mut Context<Self>,
    ) {
        if live == self.live && branches == self.live_branches {
            return;
        }
        self.live = live;
        self.live_branches = branches;
        self.refresh_rows();
        cx.notify();
    }

    /// The account GitHub is read as on `target`; a change syncs that host's
    /// repositories again at once and reads the open pull request again.
    pub(crate) fn set_github(
        &mut self,
        target: &herdr_client::ConnectTarget,
        token: Option<crate::github::Tokens>,
        login: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        for repo in self
            .repos
            .iter_mut()
            .filter(|repo| repo.request.target == *target)
        {
            repo.login = login.clone();
            let same = match (&token, &repo.request.token) {
                (Some(new), Some(old)) => new.same(old),
                (None, None) => true,
                _ => false,
            };
            if same {
                continue;
            }
            repo.request.token = token.clone();
            if let Some(service) = &repo.service {
                service.refresh(token.clone());
            }
            changed = true;
        }
        if !changed {
            return;
        }
        // What the old account read goes, and the open pull request is read
        // again as the new one: a lookup made signed out was dropped.
        self.pr.track(None);
        self.reload_pull_request();
        self.follow_pull_request();
        cx.notify();
    }

    /// Syncs again, and reads the open pull request's status and
    /// conversation again, which a sync alone leaves as they were.
    /// Refresh as the user asks for it, which waits for a sync under way.
    fn ask_refresh(&mut self) {
        if self.refreshing() {
            return;
        }
        self.refresh_asked = Some(Instant::now());
        self.refresh();
    }

    fn refresh(&mut self) {
        self.skipped_repos.clear();
        for repo in &self.repos {
            if let Some(service) = &repo.service {
                service.refresh(repo.request.token.clone());
            }
        }
        self.reload_pull_request();
        self.reload_conversation();
    }

    /// Whether a sync is under way, or Refresh was pressed so recently that
    /// the workers may not have reported starting one yet.
    pub(crate) fn refreshing(&self) -> bool {
        self.refresh_asked
            .is_some_and(|asked| asked.elapsed() < REFRESH_GRACE)
            || self.repos.iter().any(|repo| {
                repo.snapshot
                    .sources
                    .iter()
                    .any(|source| matches!(source.state, SyncState::Syncing))
            })
    }

    /// The live agents last handed in, which change run statuses.
    pub(crate) fn live(&self) -> &Arc<Vec<LiveAgent>> {
        &self.live
    }

    /// The list shown.
    #[cfg(test)]
    pub(crate) fn list(&self) -> Tab {
        self.tab
    }

    /// Lists `tab`'s items, as clicking its header would; the activity
    /// bar's sections pick the list they show.
    pub(crate) fn show_list(&mut self, tab: Tab, cx: &mut Context<Self>) {
        if self.tab != tab {
            self.select_tab(tab, cx);
        }
    }

    fn select_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        self.selected = None;
        // The open item belongs to the list left behind.
        self.detail = None;
        self.merge_open = false;
        self.sort_open = false;
        self.refresh_rows();
        self.follow_pull_request();
        cx.notify();
    }

    fn move_selection(&mut self, by: isize, cx: &mut Context<Self>) {
        let keys = self.keys();
        if keys.is_empty() {
            return;
        }
        let at = self
            .selected
            .as_ref()
            .and_then(|selected| keys.iter().position(|key| key == selected))
            .unwrap_or(0);
        let next = at.saturating_add_signed(by).min(keys.len() - 1);
        self.selected = Some(keys[next].clone());
        self.activate(&keys[next]);
        let line = self.line_of(&keys[next]);
        self.scroll.scroll_to_item(line, ScrollStrategy::Center);
        cx.notify();
    }

    fn open_detail(&mut self, key: String, cx: &mut Context<Self>) {
        self.activate(&key);
        let Some(item) = self.item(&key) else {
            return;
        };
        let pull_request = item.pull_request.is_some();
        self.detail = Some(Detail {
            key,
            tab: if pull_request {
                DetailTab::Conversation
            } else {
                DetailTab::Description
            },
        });
        self.merge_open = false;
        if pull_request {
            self.reload_pull_request();
        }
        self.follow_pull_request();
        cx.notify();
    }

    fn toggle_children(&mut self, key: String, cx: &mut Context<Self>) {
        if !self.collapsed.remove(&key) {
            self.collapsed.insert(key);
        }
        self.refresh_rows();
        cx.notify();
    }

    /// Shows run `run`: its agent's pane while Herdr shows the agent, else
    /// the workspace it worked in, else says why there is nothing to show.
    fn open_run(&mut self, run: usize, cx: &mut Context<Self>) {
        let runs = Runs::new(&self.snapshot().runs, &self.snapshot().sessions, &self.live);
        if let Some(agent) = runs.live(run) {
            cx.emit(Event::OpenRun {
                endpoint: agent.endpoint,
                pane_id: agent.pane_id.clone(),
            });
            return;
        }
        let workspace = self.snapshot().runs.get(run).and_then(|run| {
            run.workspace.as_ref().filter(|workspace| {
                workspace.backend == super::Backend::Herdr && !workspace.id.is_empty()
            })
        });
        match workspace {
            Some(workspace) => cx.emit(Event::OpenWorkspace {
                host: workspace.host.clone(),
                session: runs
                    .session(run)
                    .and_then(|session| session.session.clone())
                    .or_else(|| {
                        let run = self.snapshot().runs.get(run)?;
                        self.live_branches
                            .iter()
                            .find(|branch| {
                                super::naming::workspace_run_id(
                                    &branch.host,
                                    &branch.session,
                                    &branch.workspace_id,
                                ) == run.id
                            })
                            .and_then(|branch| branch.session.clone())
                    }),
                workspace_id: workspace.id.clone(),
            }),
            None => {
                self.notice = Some(Notice {
                    outcome: Err(Arc::new(super::Error::NothingToOpen)),
                });
                cx.notify();
            }
        }
    }

    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if (self.dialog.is_some() || self.confirm.is_some())
            && self.dialog_key(&event.keystroke.key, window, cx)
        {
            cx.stop_propagation();
            return;
        }
        if self.dialog.is_some() {
            return;
        }
        if self.comment.read(cx).focus.is_focused(window) {
            match event.keystroke.key.as_str() {
                "enter" => self.post_comment(cx),
                "escape" => window.focus(&self.focus, cx),
                _ => return,
            }
            cx.stop_propagation();
            return;
        }
        if self.message.read(cx).focus.is_focused(window) {
            match event.keystroke.key.as_str() {
                "enter" => self.send_message(cx),
                "escape" => window.focus(&self.focus, cx),
                _ => return,
            }
            cx.stop_propagation();
            return;
        }
        if self.search.read(cx).focus.is_focused(window) {
            if event.keystroke.key == "escape" {
                window.focus(&self.focus, cx);
                cx.stop_propagation();
            }
            return;
        }
        let modifiers = &event.keystroke.modifiers;
        if modifiers.platform || modifiers.control || modifiers.alt {
            return;
        }
        let handled = match (event.keystroke.key.as_str(), &self.detail) {
            ("escape", Some(_)) => {
                self.detail = None;
                cx.notify();
                true
            }
            ("escape", None) if self.sort_open => {
                self.sort_open = false;
                cx.notify();
                true
            }
            ("/", None) => {
                window.focus(&self.search.read(cx).focus.clone(), cx);
                true
            }
            ("up" | "k", None) => {
                self.move_selection(-1, cx);
                true
            }
            ("down" | "j", None) => {
                self.move_selection(1, cx);
                true
            }
            ("enter", None) => {
                if let Some(key) = self.selected.clone() {
                    self.open_detail(key, cx);
                }
                true
            }
            ("r", _) => {
                self.ask_refresh();
                true
            }
            ("1" | "2" | "3" | "4" | "5", Some(detail)) => {
                let index = event.keystroke.key.parse::<usize>().unwrap_or(1) - 1;
                let pull_request = self
                    .item(&detail.key)
                    .is_some_and(|item| item.pull_request.is_some());
                let tabs = DetailTab::tabs(pull_request);
                let mut detail = detail.clone();
                detail.tab = tabs[index.min(tabs.len() - 1)];
                self.detail = Some(detail);
                self.follow_pull_request();
                cx.notify();
                true
            }
            _ => false,
        };
        if handled {
            cx.stop_propagation();
        }
    }
}

impl Focusable for OrchestratorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for OrchestratorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = self.render_inbox(window, cx);
        let notice = self.render_notice(cx);
        let overlay = self.render_overlay(cx);
        let look = &self.look;
        div()
            .id("orchestrator")
            .role(Role::Group)
            .aria_label("Orchestrator")
            .relative()
            .key_context("Orchestrator")
            .track_focus(&self.focus)
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .bg(rgb(look.theme.background))
            .text_color(rgb(look.theme.foreground))
            .text_size(look.size())
            .on_key_down(cx.listener(Self::on_key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if !this.search.read(cx).focus.is_focused(window) {
                        window.focus(&this.focus, cx);
                    }
                }),
            )
            .children(notice)
            .child(body)
            .children(overlay)
    }
}
