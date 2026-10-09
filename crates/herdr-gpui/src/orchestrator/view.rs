//! The orchestrator view: one repository's issues, pull requests, and agent
//! runs, as a GPUI entity of its own. A tab hosts it today; because it owns
//! its state and talks to its host only through [`Event`]s and setters, a
//! separate window could host the same entity.
//!
//! It never blocks: the [`Service`] worker does the I/O, and the host pushes
//! the theme, live agent statuses, and the GitHub account from its tick.

mod detail;
mod inbox;
mod list;
mod look;
mod preview;
mod rows;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

pub(crate) use look::Look;
pub(crate) use rows::LiveAgent;

use super::{Item, Request, Service, Snapshot};
use crate::search_input::{self, SearchInput};
use gpui::{prelude::*, *};
use rows::{Filters, ItemRow, RunLine, Runs, Sort, Tab};
use secrecy::SecretString;
use std::{collections::HashSet, sync::Arc};

/// What the view asks its host to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    /// Show the pane a run's agent is in, on the endpoint the host listed it
    /// from.
    OpenRun {
        endpoint: usize,
        workspace_id: String,
        pane_id: String,
    },
    /// Open a web address, such as an issue's page, in a browser tab.
    OpenUrl(String),
    /// Sign in to GitHub, which the view needs to list a repository.
    SignIn,
    /// Start an agent on the item with this canonical key.
    Dispatch { item: String },
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
    #[default]
    Description,
    Agent,
    Details,
}

impl DetailTab {
    pub(crate) const ALL: [Self; 3] = [Self::Description, Self::Agent, Self::Details];

    pub(crate) fn label(self) -> &'static str {
        match self {
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
    description: crate::release_notes::Prepared,
    _subscriptions: Vec<Subscription>,
}

impl OrchestratorView {
    pub(crate) fn new(request: Request, look: Look, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| {
            let mut input = SearchInput::new(cx);
            input.set_placeholder("Filter: title, id, author, label, body\u{2026}", cx);
            input.set_appearance(look.ui.clone(), look.theme.clone(), cx);
            input
        });
        let subscription = cx.subscribe(&search, |this, search, _: &search_input::Changed, cx| {
            this.query = search.read(cx).text().to_owned();
            this.refresh_rows();
            cx.notify();
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
            description: crate::release_notes::Prepared::default(),
            _subscriptions: vec![subscription],
        }
    }

    pub(crate) fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    /// Takes the worker's newest snapshot; called from the host's tick.
    pub(crate) fn poll(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = self.service.as_ref().and_then(Service::poll) else {
            return;
        };
        self.snapshot = snapshot;
        self.refresh_rows();
        cx.notify();
    }

    pub(crate) fn set_look(&mut self, look: Look, cx: &mut Context<Self>) {
        if look.theme == self.look.theme
            && look.ui.family == self.look.ui.family
            && look.ui.size == self.look.ui.size
            && look.mono.family == self.look.mono.family
        {
            return;
        }
        self.search.update(cx, |search, cx| {
            search.set_appearance(look.ui.clone(), look.theme.clone(), cx);
        });
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
        if self.item(&key).is_none() {
            return;
        }
        self.detail = Some(Detail {
            key,
            tab: if self.tab == Tab::Runs {
                DetailTab::Agent
            } else {
                DetailTab::Description
            },
        });
        cx.notify();
    }

    fn toggle_children(&mut self, key: String, cx: &mut Context<Self>) {
        if !self.collapsed.remove(&key) {
            self.collapsed.insert(key);
        }
        self.refresh_rows();
        cx.notify();
    }

    fn open_run(&mut self, run: usize, cx: &mut Context<Self>) {
        let runs = Runs::new(&self.snapshot.runs, &self.snapshot.sessions, &self.live);
        if let Some(agent) = runs.live(run) {
            cx.emit(Event::OpenRun {
                endpoint: agent.endpoint,
                workspace_id: agent.workspace_id.clone(),
                pane_id: agent.pane_id.clone(),
            });
        }
    }

    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
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
            ("1" | "2" | "3", Some(detail)) => {
                let index = event.keystroke.key.parse::<usize>().unwrap_or(1) - 1;
                let mut detail = detail.clone();
                detail.tab = DetailTab::ALL[index.min(DetailTab::ALL.len() - 1)];
                self.detail = Some(detail);
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
        let look = &self.look;
        div()
            .id("orchestrator")
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
            .child(body)
    }
}
