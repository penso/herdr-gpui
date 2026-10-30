//! The activity bar's Overview: one page across the window's body for what
//! needs the user on every host this window connects to. Agents waiting on
//! the user and failed orchestrator runs lead, in a card of their own; then
//! finished ones ready for review, working agents by host, and idle ones.
//! Rows can be pinned, snoozed until a time, or settled as finished
//! (`marks`); agents that closed wait under Closed to be started again
//! (`closed`, `reopen`); a column beside the list shows each host's load
//! and ports, and folds away.
//!
//! Everything it shows is already in the window: the endpoints' snapshots,
//! the orchestrator views' runs, the pull request prefetch, and the system
//! load and port monitors. Drawing reads that prepared state only.

mod closed;
mod hosts;
mod marks;
mod model;
mod presets;
mod reopen;
mod view;

#[cfg(test)]
mod tests;

pub(crate) use marks::Marks;
pub(crate) use model::{Key, Open, Row, Sections, State};

use crate::{HerdrWindow, orchestrator::RunState, sidebar::ActivityPanel};
use gpui::{prelude::*, *};
use std::{
    collections::{HashMap, HashSet},
    time::SystemTime,
};

/// Below this window width the host column starts folded, leaving the
/// list room for its titles.
const NARROW: f32 = 1000.;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid saved overview marks")]
    InvalidMarks,
    #[error("Cannot read saved overview marks: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    ReopenRequest(#[source] std::sync::Arc<crate::Error>),
    #[error("Unexpected daemon response while opening the agent's tab")]
    ReopenResponse,
}

/// The page's own state in a window.
pub(crate) struct Page {
    /// When each agent entered its state.
    since: model::Since,
    /// Hosts folded to one line in the host column, by endpoint id.
    folded: HashSet<String>,
    /// Whether the host column was folded to a strip of load dots, or None
    /// to fold it only while the window is narrow.
    hosts_closed: Option<bool>,
    /// The row whose snooze menu is open.
    menu: Option<Key>,
    snoozed_open: bool,
    settled_open: bool,
    closed: closed::Closed,
    closed_open: bool,
    reopen: Option<reopen::Job>,
    scroll: ScrollHandle,
    /// What the page last drew from: the minute its elapsed times were
    /// counted in, the marks' revision, how many rows needed the user and
    /// worked, and the closed agents' revision, so the tick redraws only
    /// when one of them moves.
    drawn: (u64, u64, usize, usize, u64),
    /// When the latest row needing the user entered its state, on the last
    /// tick, so the bar's count can tell new from already seen.
    needing_since: Option<SystemTime>,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            since: Default::default(),
            folded: Default::default(),
            hosts_closed: None,
            menu: None,
            snoozed_open: false,
            settled_open: false,
            closed: Default::default(),
            closed_open: false,
            reopen: None,
            scroll: ScrollHandle::new(),
            drawn: (0, 0, 0, 0, 0),
            needing_since: None,
        }
    }
}

impl HerdrWindow {
    /// Every row the page could list: each reachable host's agents, in
    /// sidebar order, then the open orchestrators' failed runs.
    pub(crate) fn overview_rows(&self, cx: &App) -> Vec<Row> {
        let mut rows = Vec::new();
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            let live = if index == self.selected_endpoint {
                &self.live
            } else {
                &endpoint.live
            };
            // Each device's answers are its own: another host may have the
            // same checkout path and branch.
            let view = self.endpoint_pr_view(index);
            let pr = |key: &str, branch: &str| {
                view.as_ref()
                    .and_then(|view| self.menu.pr_cache.peek(view, key, branch))
                    .map(|pr| model::Pr {
                        number: pr.number,
                        checks: pr.checks_outcome(),
                    })
            };
            rows.extend(model::agent_rows(
                endpoint,
                live,
                &self.overview.since,
                pr,
                crate::notification_mutes::Mutes::any(cx),
            ));
        }
        let mut runs = HashSet::new();
        for (tab, view) in self.orchestrator_views() {
            for (target, items, listed) in view.read(cx).listed() {
                let host = self
                    .endpoints
                    .iter()
                    .find(|endpoint| endpoint.connection.target == *target)
                    .map(|endpoint| (endpoint.label.clone(), endpoint.id.clone()))
                    .unwrap_or_default();
                for run in listed {
                    if run.state != RunState::Failed || !runs.insert(run.id.clone()) {
                        continue;
                    }
                    let item = items
                        .iter()
                        .find(|item| item.key.canonical() == run.item_key);
                    let title = item.map_or_else(
                        || crate::orchestrator::key_label(&run.item_key),
                        |item| format!("{} {}", item.identifier, item.title),
                    );
                    rows.push(Row {
                        key: Key::Run { id: run.id.clone() },
                        state: State::Failed,
                        title: format!("Run failed: {title}"),
                        detail: run
                            .message
                            .as_deref()
                            .map(|message| crate::notifications::safe_text(message, 200))
                            .unwrap_or_else(|| run.agent.clone()),
                        identity: None,
                        workspace: run
                            .workspace
                            .as_ref()
                            .and_then(|workspace| workspace.path.as_deref())
                            .and_then(|path| path.file_name())
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                        branch: run
                            .workspace
                            .as_ref()
                            .map(|workspace| workspace.branch.clone()),
                        host: host.0.clone(),
                        endpoint: host.1.clone(),
                        since: Some(run.updated_at.into()),
                        pr: None,
                        open: Open::Run(tab),
                        muted: false,
                    });
                }
            }
        }
        rows
    }

    pub(crate) fn overview_sections(&self, cx: &App) -> Sections {
        Sections::of(self.overview_rows(cx), Marks::of(cx), SystemTime::now())
    }

    /// Runs every window tick: notes when agents changed state or closed,
    /// wakes snoozes whose time came, advances a reopen, and redraws the
    /// page or the bar's count when what they show moved.
    pub(crate) fn poll_overview(&mut self, cx: &mut Context<Self>) {
        let now = SystemTime::now();
        let mut live: HashMap<&str, Vec<&str>> = HashMap::new();
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            let state = if index == self.selected_endpoint {
                &self.live
            } else {
                &endpoint.live
            };
            let Some(snapshot) = state
                .snapshot
                .as_deref()
                .filter(|_| state.status.is_connected())
            else {
                continue;
            };
            self.overview
                .since
                .observe(&endpoint.id, &snapshot.agents, now);
            live.insert(
                &endpoint.id,
                snapshot
                    .agents
                    .iter()
                    .map(|agent| agent.pane_id.as_str())
                    .collect(),
            );
        }
        self.overview.since.retain(&live);
        let rows = self.overview_rows(cx);
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            let state = if index == self.selected_endpoint {
                &self.live
            } else {
                &endpoint.live
            };
            if let Some(snapshot) = state
                .snapshot
                .as_deref()
                .filter(|_| endpoint.enabled && state.status.is_connected())
            {
                self.overview
                    .closed
                    .observe(&endpoint.id, snapshot, &rows, now);
            }
        }
        let endpoints = &self.endpoints;
        self.overview
            .closed
            .retain_hosts(|id| endpoints.iter().any(|endpoint| endpoint.id == id));
        self.poll_overview_reopen(cx);
        Marks::update(cx, |marks| {
            marks.wake(rows.iter().map(|row| (&row.key, row.state)), now)
        });
        let revision = Marks::of(cx).map_or(0, Marks::revision);
        let sections = Sections::of(rows, Marks::of(cx), now);
        let (needing, working) = (sections.needing(), sections.working.len());
        let minute = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs() / 60);
        let shown = self.activity_section() == Some(ActivityPanel::Overview);
        let drawn = (
            if shown { minute } else { 0 },
            revision,
            needing,
            working,
            self.overview.closed.revision(),
        );
        let needing_since = sections.needing_since();
        if drawn != self.overview.drawn || needing_since != self.overview.needing_since {
            self.overview.drawn = drawn;
            self.overview.needing_since = needing_since;
            cx.notify();
        }
    }

    /// How many rows needed the user on the last tick, for the bar's count.
    pub(crate) fn overview_needing(&self) -> usize {
        self.overview.drawn.2
    }

    /// When the latest row [`Self::overview_needing`] counts entered its
    /// state, on the last tick.
    pub(crate) fn overview_needing_since(&self) -> Option<SystemTime> {
        self.overview.needing_since
    }

    /// How many agents were working on the last tick, which animates the
    /// bar's Overview icon.
    pub(crate) fn overview_working(&self) -> usize {
        self.overview.drawn.3
    }

    /// Whether the page is shown, so every host's load is sampled for its
    /// host column.
    pub(crate) fn overview_shown(&self) -> bool {
        self.activity_section() == Some(ActivityPanel::Overview) && !self.overview_hosts_closed()
    }

    /// Whether the host column shows as a strip: as the user left it, else
    /// while the window is too narrow for it beside the list.
    pub(crate) fn overview_hosts_closed(&self) -> bool {
        self.overview
            .hosts_closed
            .unwrap_or(self.viewport_width < NARROW)
    }

    /// Shows what a row stands for: an agent's pane among the terminals, or
    /// a run's item in the Issues section.
    fn open_overview_row(&mut self, row: &Row, window: &mut Window, cx: &mut Context<Self>) {
        self.overview.menu = None;
        Marks::update(cx, |marks| marks.seen(&row.key));
        match &row.open {
            Open::Pane {
                endpoint,
                pane,
                tab,
                workspace,
            } => {
                // Read before navigating, which may switch devices.
                let on_screen = self.endpoints[self.selected_endpoint].id == *endpoint
                    && self
                        .browser_key()
                        .is_some_and(|(_, shown)| shown == *workspace);
                self.show_activity_panel(ActivityPanel::Workspaces, window, cx);
                self.activity.return_to = Some(ActivityPanel::Overview);
                self.navigate_endpoint(endpoint, crate::NavigationTarget::Pane(pane), cx);
                // An agent in the tab already focused moves nothing, so show
                // it here, as the Devices overview does.
                if on_screen {
                    self.terminal_focus_moved(tab, false, cx);
                }
                window.focus(&self.focus, cx);
            }
            Open::Run(_) => self.show_activity_panel(ActivityPanel::Issues, window, cx),
        }
        cx.notify();
    }

    /// Shows the workspace a closed agent ran in.
    fn open_closed_agent(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.overview.menu = None;
        let Some(agent) = self.overview.closed.get(id).cloned() else {
            return;
        };
        let Open::Pane {
            endpoint,
            workspace,
            ..
        } = &agent.row.open
        else {
            return;
        };
        let current = self.endpoints.iter().enumerate().any(|(index, host)| {
            let live = if index == self.selected_endpoint {
                &self.live
            } else {
                &host.live
            };
            host.id == *endpoint
                && host.enabled
                && live.status.is_connected()
                && live
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.boot_id == agent.boot)
        });
        if !current {
            self.show_flash(
                crate::window::Flash::warning(
                    "The agent's original session is no longer connected",
                ),
                cx,
            );
            return;
        }
        self.show_activity_panel(ActivityPanel::Workspaces, window, cx);
        self.activity.return_to = Some(ActivityPanel::Overview);
        self.navigate_endpoint(endpoint, crate::NavigationTarget::Workspace(workspace), cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Whether a closed agent is being started again, which holds the
    /// connection's script slot.
    pub(crate) fn overview_reopening(&self) -> bool {
        self.overview.reopen.is_some()
    }

    fn mark_overview_row(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Marks) -> bool) {
        self.overview.menu = None;
        Marks::update(cx, f);
        cx.notify();
    }
}
