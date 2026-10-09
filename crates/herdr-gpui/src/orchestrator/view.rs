//! The orchestrator view: one repository's issues, pull requests, and agent
//! runs, as a GPUI entity of its own. A tab hosts it today; because it owns
//! its state and talks to its host only through [`Event`]s and setters, a
//! separate window could host the same entity.
//!
//! It never blocks: the [`Service`] worker does the I/O, and the host pushes
//! the theme, live agent statuses, and the GitHub account from its tick.

mod conversation;
mod detail;
mod dispatch;
mod hosts;
mod inbox;
mod list;
mod look;
mod markdown;
mod preview;
mod pull;
mod rows;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

pub(crate) use look::Look;
pub(crate) use rows::LiveAgent;

use super::{Item, Notice, Request, Service, Snapshot};
use crate::search_input::{self, SearchInput};
use gpui::{prelude::*, *};
use rows::{Filters, ItemRow, RunLine, Runs, Sort, Tab};
use secrecy::SecretString;
use std::{collections::HashSet, sync::Arc};

/// What the view asks its host to do.
#[derive(Clone, Debug)]
pub(crate) enum Event {
    /// Show the pane a run's agent is in, on the endpoint the host listed it
    /// from.
    OpenRun { endpoint: usize, pane_id: String },
    /// Show a run's Herdr workspace on the host it is on: `None` for this
    /// machine, else an SSH destination.
    OpenWorkspace {
        host: Option<String>,
        workspace_id: String,
    },
    /// Open a web address, such as an issue's page, in a browser tab.
    OpenUrl(String),
    /// Sign in to GitHub, which the view needs to list a repository.
    SignIn,
    /// Move the view into a window of its own.
    OpenWindow,
    /// Start `request` on the host `endpoint`, which the window sets it up
    /// for, then hands back through [`OrchestratorView::dispatch_elsewhere`].
    Dispatch {
        request: Box<super::DispatchRequest>,
        endpoint: String,
    },
}

impl EventEmitter<Event> for OrchestratorView {}

/// The preview's width bounds, in pixels.
const PREVIEW_MIN: f32 = 280.;
const PREVIEW_DEFAULT: f32 = 380.;
const PREVIEW_MAX_SHARE: f32 = 0.6;

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

/// The preview's drag handle, as a drag payload.
#[derive(Clone, Copy)]
pub(crate) struct PreviewDrag;

impl Render for PreviewDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

pub(crate) struct OrchestratorView {
    service: Option<Service>,
    request: Request,
    snapshot: Snapshot,
    look: Look,
    live: Arc<Vec<LiveAgent>>,
    /// The signed-in GitHub login, for "Mine".
    login: Option<String>,
    tab: Tab,
    sort: Sort,
    sort_open: bool,
    filters: Filters,
    mine: bool,
    active_runs: bool,
    collapsed: HashSet<String>,
    search: Entity<SearchInput>,
    query: String,
    /// The selected row's item key, or run id on the Runs tab.
    selected: Option<String>,
    detail: Option<Detail>,
    preview_open: bool,
    preview_width: f32,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    issue_rows: Vec<ItemRow>,
    pull_request_rows: Vec<ItemRow>,
    run_lines: Vec<RunLine>,
    /// The open item's description, parsed once per change of its text.
    description: markdown::Markdown,
    /// The previewed item's description, likewise.
    preview_markdown: markdown::Markdown,
    dialog: Option<dispatch::Dialog>,
    confirm: Option<dispatch::Confirm>,
    /// The newest action's outcome, until dismissed or replaced.
    notice: Option<Notice>,
    /// What to type to the open item's newest run.
    message: Entity<SearchInput>,
    /// The open pull request's conversation, comment, and merge.
    pr: crate::pr_actions::Actions,
    comment: Entity<SearchInput>,
    /// Conversation bodies, parsed once per text.
    bodies: conversation::Bodies,
    /// Conversation authors' avatars by URL.
    avatars: conversation::Avatars,
    /// Resolved threads unfolded, by author, path, and time.
    shown_threads: HashSet<String>,
    merge_open: bool,
    /// Hosts a dispatch could go to, while the dialog is open.
    hosts: Arc<Vec<crate::dispatch::Candidate>>,
    /// Hosted by a window of its own rather than a tab.
    detached: bool,
    _subscriptions: Vec<Subscription>,
}

impl OrchestratorView {
    pub(crate) fn new(request: Request, look: Look, cx: &mut Context<Self>) -> Self {
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
        let (service, error) = match Service::start(request.clone()) {
            Ok(service) => (Some(service), None),
            Err(error) => (None, Some(Arc::new(super::Error::Worker(error)))),
        };
        let snapshot = Snapshot {
            error,
            ..Snapshot::default()
        };
        Self {
            service,
            request,
            snapshot,
            look,
            live: Arc::default(),
            login: None,
            tab: Tab::default(),
            sort: Sort::default(),
            sort_open: false,
            filters: Filters::default(),
            mine: false,
            active_runs: false,
            collapsed: HashSet::new(),
            search,
            query: String::new(),
            selected: None,
            detail: None,
            preview_open: true,
            preview_width: PREVIEW_DEFAULT,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            issue_rows: Vec::new(),
            pull_request_rows: Vec::new(),
            run_lines: Vec::new(),
            description: markdown::Markdown::default(),
            preview_markdown: markdown::Markdown::default(),
            dialog: None,
            confirm: None,
            notice: None,
            message,
            pr: crate::pr_actions::Actions::default(),
            comment,
            bodies: conversation::Bodies::default(),
            avatars: conversation::Avatars::default(),
            shown_threads: HashSet::new(),
            merge_open: false,
            hosts: Arc::default(),
            detached: false,
            _subscriptions: vec![subscription],
        }
    }

    pub(crate) fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    /// Takes the worker's newest snapshot; called from the host's tick.
    pub(crate) fn poll(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        if let Some(notice) = self
            .service
            .as_ref()
            .and_then(|service| service.notices().pop())
        {
            self.notice = Some(notice);
            changed = true;
        }
        if let Some(snapshot) = self.service.as_ref().and_then(Service::poll) {
            self.snapshot = snapshot;
            self.refresh_rows();
            self.follow_pull_request();
            changed = true;
        }
        if self.pr.poll() {
            // A comment or merge landed: read the pull request and list again.
            if self.pr.take_settled() {
                self.reload_pull_request();
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
        if let (Some(number), Some(service)) = (number, &self.service) {
            service.load_pull_request(number);
        }
    }

    /// Whether the dispatch dialog is open, so hosts are sampled and pushed.
    pub(crate) fn wants_hosts(&self) -> bool {
        self.dialog.is_some()
    }

    /// The host the repository is on.
    pub(crate) fn target(&self) -> &herdr_client::ConnectTarget {
        &self.request.target
    }

    pub(crate) fn theme(&self) -> &crate::config::Theme {
        &self.look.theme
    }

    pub(crate) fn set_detached(&mut self, detached: bool, cx: &mut Context<Self>) {
        self.detached = detached;
        cx.notify();
    }

    /// The Herdr workspace the view was opened from.
    pub(crate) fn workspace_id(&self) -> &str {
        &self.request.workspace_id
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
        cx.notify();
    }

    pub(crate) fn set_live(&mut self, live: Arc<Vec<LiveAgent>>, cx: &mut Context<Self>) {
        if live == self.live {
            return;
        }
        self.live = live;
        self.refresh_rows();
        cx.notify();
    }

    /// The account GitHub is read as; a change syncs again at once.
    pub(crate) fn set_github(
        &mut self,
        token: Option<Arc<SecretString>>,
        login: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let same = match (&token, &self.request.token) {
            (Some(new), Some(old)) => Arc::ptr_eq(new, old),
            (None, None) => true,
            _ => false,
        };
        self.login = login;
        if same {
            return;
        }
        self.request.token = token.clone();
        if let Some(service) = &self.service {
            service.refresh(token);
        }
        cx.notify();
    }

    fn refresh(&mut self) {
        if let Some(service) = &self.service {
            service.refresh(self.request.token.clone());
        }
    }

    fn refresh_rows(&mut self) {
        let items = &self.snapshot.items;
        let runs = Runs::new(&self.snapshot.runs, &self.snapshot.sessions, &self.live);
        self.issue_rows = rows::issue_rows(
            items,
            &runs,
            &self.query,
            self.filters,
            self.sort,
            &self.collapsed,
        );
        self.pull_request_rows = rows::pull_request_rows(
            items,
            &runs,
            &self.query,
            self.filters,
            self.login.as_deref(),
            self.mine,
            self.sort,
        );
        self.run_lines = rows::run_lines(&runs, items, &self.query, self.active_runs);
        if self.selected.is_none() || !self.selection_listed() {
            self.selected = self.keys().into_iter().next();
        }
    }

    /// The selectable keys of the current tab, in order.
    fn keys(&self) -> Vec<String> {
        let items = &self.snapshot.items;
        match self.tab {
            Tab::Issues => self
                .issue_rows
                .iter()
                .map(|row| items[row.item].key.canonical())
                .collect(),
            Tab::PullRequests => self
                .pull_request_rows
                .iter()
                .map(|row| items[row.item].key.canonical())
                .collect(),
            Tab::Runs => self
                .run_lines
                .iter()
                .filter_map(|line| match line {
                    RunLine::Run(row) => Some(self.snapshot.runs[row.run].id.clone()),
                    RunLine::Group { .. } => None,
                })
                .collect(),
        }
    }

    fn selection_listed(&self) -> bool {
        self.selected
            .as_ref()
            .is_some_and(|selected| self.keys().contains(selected))
    }

    fn item(&self, key: &str) -> Option<&Item> {
        self.snapshot
            .items
            .iter()
            .find(|item| item.key.canonical() == key)
    }

    fn select_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        self.selected = None;
        self.sort_open = false;
        self.refresh_rows();
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
        let line = self.line_of(&keys[next]);
        self.scroll.scroll_to_item(line, ScrollStrategy::Center);
        cx.notify();
    }

    /// The list line showing `key`, counting the Runs tab's headings.
    fn line_of(&self, key: &str) -> usize {
        let items = &self.snapshot.items;
        match self.tab {
            Tab::Issues => self
                .issue_rows
                .iter()
                .position(|row| items[row.item].key.canonical() == key),
            Tab::PullRequests => self
                .pull_request_rows
                .iter()
                .position(|row| items[row.item].key.canonical() == key),
            Tab::Runs => self.run_lines.iter().position(
                |line| matches!(line, RunLine::Run(row) if self.snapshot.runs[row.run].id == key),
            ),
        }
        .unwrap_or(0)
    }

    fn open_detail(&mut self, key: String, cx: &mut Context<Self>) {
        // A run opens the item it works on.
        let key = match self.tab {
            Tab::Runs => match self.snapshot.runs.iter().find(|run| run.id == key) {
                Some(run) => run.item_key.clone(),
                None => return,
            },
            _ => key,
        };
        let Some(item) = self.item(&key) else {
            return;
        };
        let pull_request = item.pull_request.is_some();
        self.detail = Some(Detail {
            key,
            tab: match (self.tab, pull_request) {
                (Tab::Runs, _) => DetailTab::Agent,
                (_, true) => DetailTab::Conversation,
                (_, false) => DetailTab::Description,
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
        let runs = Runs::new(&self.snapshot.runs, &self.snapshot.sessions, &self.live);
        if let Some(agent) = runs.live(run) {
            cx.emit(Event::OpenRun {
                endpoint: agent.endpoint,
                pane_id: agent.pane_id.clone(),
            });
            return;
        }
        let workspace = self.snapshot.runs.get(run).and_then(|run| {
            run.workspace
                .as_ref()
                .filter(|workspace| workspace.backend == super::Backend::Herdr)
        });
        match workspace {
            Some(workspace) => cx.emit(Event::OpenWorkspace {
                host: workspace.host.clone(),
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
            ("tab", None) => {
                let at = Tab::ALL
                    .iter()
                    .position(|tab| *tab == self.tab)
                    .unwrap_or(0);
                let next = if modifiers.shift {
                    at + Tab::ALL.len() - 1
                } else {
                    at + 1
                } % Tab::ALL.len();
                self.select_tab(Tab::ALL[next], cx);
                true
            }
            ("r", _) => {
                self.refresh();
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
        let body = match self.detail.clone() {
            Some(detail) => self.render_detail(detail, window, cx),
            None => self.render_inbox(window, cx),
        };
        let notice = self.render_notice(cx);
        let overlay = self.render_overlay(cx);
        let merge_menu = self.render_merge_menu(cx);
        let look = &self.look;
        div()
            .id("orchestrator")
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
            .children(merge_menu)
            .children(overlay)
    }
}
