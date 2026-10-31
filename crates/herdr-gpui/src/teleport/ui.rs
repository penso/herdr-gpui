//! The Teleport dialog: choose a destination, review, and follow the move.
//!
//! Each phase runs on its own named thread, never the UI thread, and reports
//! through a channel the window drains on its tick. Dismissing the dialog
//! cancels discovery and review; a move in progress keeps going and reports
//! its outcome as a flash, because stopping halfway would strand work.

use super::text;
use crate::teleport::{
    error::{Error, Step},
    job::{
        self, Action, Candidate, GitHubAccess, HostRepositories, Outcome, Place, Review, Source,
    },
    marks::{Destination as MarkDestination, Mark},
    remote::MatchReason,
};
use crate::{
    HerdrWindow, NavigationTarget,
    i18n::fl,
    menu::Page,
    progress::{self, Progress},
    window::Flash,
};
use gpui::{prelude::*, *};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

enum Event {
    Reviewed(Result<Box<(Candidate, Review)>, Error>),
    Step(Step),
    Moved(Result<Outcome, Error>),
}

enum Stage {
    Choosing,
    Reviewing(Place),
    Ready(Candidate, Box<Review>),
    Moving(Candidate, Option<Step>),
    Failed(String),
}

/// How long the window keeps steering to a teleported workspace.
const FOLLOW_FOR: Duration = Duration::from_secs(20);
const FOLLOW_RETRY: Duration = Duration::from_millis(750);

/// The workspace a finished teleport should end up focused on, or the pane
/// a moved pane landed in.
pub(crate) struct Follow {
    endpoint_id: String,
    workspace_id: String,
    pane_id: Option<String>,
    until: Instant,
    next: Option<Instant>,
}

impl Follow {
    pub(crate) fn new(endpoint_id: String, workspace_id: String) -> Self {
        Self {
            endpoint_id,
            workspace_id,
            pane_id: None,
            until: Instant::now() + FOLLOW_FOR,
            next: None,
        }
    }

    /// Follow to `pane_id` in `workspace_id`, which need not be its active tab.
    pub(crate) fn pane(endpoint_id: String, workspace_id: String, pane_id: String) -> Self {
        Self {
            pane_id: Some(pane_id),
            ..Self::new(endpoint_id, workspace_id)
        }
    }
}

pub(crate) struct Teleport {
    source: Source,
    label: String,
    /// Every host the worktree could go to, listed without probing any.
    hosts: Vec<HostRepositories>,
    stage: Stage,
    /// The highlighted host. Nothing is chosen for the user: a move to the
    /// wrong machine is costly, so the list starts without a selection.
    selected: Option<usize>,
    cancelled: Arc<AtomicBool>,
    events: mpsc::Receiver<Event>,
}

impl Drop for Teleport {
    fn drop(&mut self) {
        // A move is never abandoned halfway; see the module comment.
        if !self.moving() {
            self.cancelled.store(true, Ordering::Release);
        }
    }
}

/// Run `work` on a dedicated thread with a fresh cancellation flag.
fn spawn(
    work: impl FnOnce(&mpsc::Sender<Event>, &AtomicBool) + Send + 'static,
) -> (Arc<AtomicBool>, mpsc::Receiver<Event>) {
    let (sender, receiver) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    let spawned = std::thread::Builder::new()
        .name("herdr-teleport".into())
        .spawn(move || work(&sender, &flag));
    if let Err(error) = spawned {
        tracing::warn!(%error, "could not start the teleport worker");
    }
    (cancelled, receiver)
}

impl Teleport {
    /// How far a running teleport is, for the taskbar; `None` while the
    /// dialog waits on the user or after it failed.
    #[cfg(any(windows, test))]
    pub(crate) fn progress(&self) -> Option<Progress> {
        match &self.stage {
            Stage::Reviewing(_) => Some(Progress::Busy),
            Stage::Moving(_, step) => Some(Progress::Working(step.map_or(0., Step::progress))),
            Stage::Choosing | Stage::Ready(..) | Stage::Failed(_) => None,
        }
    }

    /// Opens on the host list at once: which repository a host has, if any,
    /// is only looked up for the host chosen.
    pub(crate) fn start(source: Source, label: String, hosts: Vec<HostRepositories>) -> Self {
        let (_, events) = mpsc::channel();
        Self {
            source,
            label,
            hosts,
            stage: Stage::Choosing,
            selected: None,
            cancelled: Arc::new(AtomicBool::new(false)),
            events,
        }
    }

    #[cfg(test)]
    pub(crate) fn reviewing(&self) -> bool {
        matches!(self.stage, Stage::Reviewing(_))
    }

    /// Turn a review into a move that has reached `Step::Fetch`, without
    /// running either.
    #[cfg(test)]
    pub(crate) fn fetching(&mut self) {
        let Stage::Reviewing(place) = &self.stage else {
            return;
        };
        let candidate = Candidate {
            place: place.clone(),
            destination: job::Destination::Arrive(crate::teleport::provision::Arrival::Clone {
                path: "/tmp/repo".into(),
            }),
            origin: None,
        };
        self.cancelled.store(true, Ordering::Release);
        self.stage = Stage::Moving(candidate, Some(Step::Fetch));
    }

    pub(crate) fn moving(&self) -> bool {
        matches!(self.stage, Stage::Moving(..))
    }

    fn restart(&mut self, work: impl FnOnce(&mpsc::Sender<Event>, &AtomicBool) + Send + 'static) {
        self.cancelled.store(true, Ordering::Release);
        let (cancelled, events) = spawn(work);
        self.cancelled = cancelled;
        self.events = events;
    }

    /// Whether `endpoint_id` is among the hosts the work could move to.
    pub(crate) fn offers(&self, endpoint_id: &str) -> bool {
        self.hosts
            .iter()
            .any(|host| host.place.endpoint_id == endpoint_id)
    }

    /// Choose the host `endpoint_id` and start its review at once.
    pub(crate) fn review_host(&mut self, endpoint_id: &str) {
        self.selected = self
            .hosts
            .iter()
            .position(|host| host.place.endpoint_id == endpoint_id);
        self.review();
    }

    fn review(&mut self) {
        let Stage::Choosing = &self.stage else {
            return;
        };
        let Some(host) = self
            .selected
            .and_then(|index| self.hosts.get(index))
            .cloned()
        else {
            return;
        };
        let source = self.source.clone();
        let place = host.place.clone();
        self.restart(move |sender, cancelled| {
            let reviewed = job::resolve(&source, &host, cancelled).and_then(|candidate| {
                let review = job::review(&source, &candidate, cancelled)?;
                Ok(Box::new((candidate, review)))
            });
            let _ = sender.send(Event::Reviewed(reviewed));
        });
        self.stage = Stage::Reviewing(place);
    }

    fn teleport(&mut self) {
        let Stage::Ready(candidate, review) = &self.stage else {
            return;
        };
        let (candidate, review) = (candidate.clone(), review.clone());
        let source = self.source.clone();
        let target = candidate.clone();
        self.restart(move |sender, cancelled| {
            let result = job::run(
                &source,
                &target,
                &review,
                |step| {
                    let _ = sender.send(Event::Step(step));
                },
                cancelled,
            );
            let _ = sender.send(Event::Moved(result));
        });
        self.stage = Stage::Moving(candidate, None);
    }

    /// Apply finished work. Returns a completed move's outcome once.
    fn poll(&mut self) -> (bool, Option<Result<Outcome, String>>) {
        let mut changed = false;
        while let Ok(event) = self.events.try_recv() {
            changed = true;
            match event {
                Event::Reviewed(Ok(reviewed)) => {
                    let (candidate, review) = *reviewed;
                    if let Stage::Reviewing(_) = &self.stage {
                        self.stage = Stage::Ready(candidate, Box::new(review));
                    }
                }
                Event::Step(step) => {
                    if let Stage::Moving(_, current) = &mut self.stage {
                        *current = Some(step);
                    }
                }
                Event::Moved(result) => {
                    let result = result.map_err(|error| error.to_string());
                    if let Err(error) = &result {
                        self.stage = Stage::Failed(error.clone());
                    }
                    return (true, Some(result));
                }
                Event::Reviewed(Err(error)) => {
                    self.stage = Stage::Failed(error.to_string());
                }
            }
        }
        (changed, None)
    }

    fn destination_label(&self) -> Option<&str> {
        match &self.stage {
            Stage::Reviewing(place) => Some(&place.label),
            Stage::Ready(c, _) | Stage::Moving(c, _) => Some(&c.place.label),
            _ => None,
        }
    }
}

impl HerdrWindow {
    pub(crate) fn poll_teleport(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.teleport_marks.poll() {
            cx.notify();
        }
        self.follow_teleport(cx);
        let open = self.menu.page == Some(Page::Teleport);
        let Some(teleport) = &mut self.teleport else {
            return;
        };
        // Closing the dialog abandons discovery and review, but not a move.
        if !open && !teleport.moving() {
            self.teleport = None;
            return;
        }
        let (changed, finished) = teleport.poll();
        let Some(result) = finished else {
            if changed {
                cx.notify();
            }
            return;
        };
        let destination = teleport.destination_label().unwrap_or_default().to_owned();
        let source = teleport.source.clone();
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) if !open => {
                self.teleport = None;
                self.show_flash(
                    Flash::warning(fl!("teleport-failed", error = error.to_string())),
                    cx,
                );
                return;
            }
            Err(_) => return cx.notify(),
        };
        self.teleport = None;
        if open {
            self.dismiss_menu(window, cx);
        }
        for warning in &outcome.warnings {
            tracing::warn!(%warning, "teleport");
        }
        // The checkout left behind is marked; the one the work came back to,
        // if any, no longer is.
        self.teleport_marks
            .remove(&outcome.endpoint_id, &outcome.repo_key, &outcome.branch);
        self.teleport_marks.add(Mark {
            endpoint: source.place.endpoint_id.clone(),
            repo_key: source.repo_key.clone(),
            branch: outcome.branch.clone(),
            destination: MarkDestination {
                endpoint: outcome.endpoint_id.clone(),
                label: destination.clone(),
                repo_key: outcome.repo_key.clone(),
                workspace_id: outcome.workspace_id.clone(),
            },
        });
        let flash = match outcome.warnings.first() {
            None => Flash::success(fl!("teleport-done", host = destination.as_str())),
            Some(first) => Flash::warning(fl!(
                "teleport-done-warnings",
                host = destination.as_str(),
                count = outcome.warnings.len(),
                first = first.as_str()
            )),
        };
        self.show_flash(flash, cx);
        self.teleport_follow = Some(Follow::new(outcome.endpoint_id, outcome.workspace_id));
        self.follow_teleport(cx);
    }

    /// Keep steering to the teleported workspace until it is focused: the
    /// host may still be connecting, its snapshot may not list the workspace
    /// yet, or a menu may be open. Gives up after a short while, so it never
    /// fights the user for long.
    fn follow_teleport(&mut self, cx: &mut Context<Self>) {
        let Some(follow) = &mut self.teleport_follow else {
            return;
        };
        let now = Instant::now();
        if now >= follow.until {
            self.teleport_follow = None;
            return;
        }
        if self.menu.page.is_some() || follow.next.is_some_and(|next| now < next) {
            return;
        }
        follow.next = Some(now + FOLLOW_RETRY);
        let (endpoint, workspace) = (follow.endpoint_id.clone(), follow.workspace_id.clone());
        let pane = follow.pane_id.clone();
        let target = match &pane {
            Some(pane) => NavigationTarget::Pane(pane.as_str()),
            None => NavigationTarget::Workspace(workspace.as_str()),
        };
        if self.endpoints[self.selected_endpoint].id != endpoint {
            self.navigate_endpoint(&endpoint, target, cx);
            return;
        }
        let Some(snapshot) = &self.live.snapshot else {
            return;
        };
        let (focused, listed) = match &pane {
            Some(pane) => (
                snapshot.focused_pane_id.as_deref() == Some(pane.as_str()),
                snapshot.panes.iter().any(|p| p.pane_id == *pane),
            ),
            None => (
                snapshot.focused_workspace_id.as_deref() == Some(workspace.as_str()),
                snapshot
                    .workspaces
                    .iter()
                    .any(|w| w.workspace_id == workspace),
            ),
        };
        if focused {
            self.teleport_follow = None;
            return;
        }
        if listed && self.pending_navigation.is_none() {
            self.navigate(target, cx);
        }
    }

    pub(crate) fn teleport_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        window.prevent_default();
        let Some(teleport) = &mut self.teleport else {
            self.dismiss_menu(window, cx);
            return;
        };
        match event.keystroke.key.as_str() {
            "escape" => self.dismiss_menu(window, cx),
            "up" | "down" => {
                if let Stage::Choosing = &teleport.stage
                    && !teleport.hosts.is_empty()
                {
                    let count = teleport.hosts.len();
                    let up = event.keystroke.key == "up";
                    teleport.selected = Some(match teleport.selected {
                        None if up => count - 1,
                        None => 0,
                        Some(index) if up => (index + count - 1) % count,
                        Some(index) => (index + 1) % count,
                    });
                    cx.notify();
                }
            }
            "enter" => self.submit_teleport(window, cx),
            _ => {}
        }
    }

    fn submit_teleport(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(teleport) = &mut self.teleport else {
            return;
        };
        match teleport.stage {
            Stage::Choosing => teleport.review(),
            Stage::Ready(..) => teleport.teleport(),
            Stage::Failed(_) => {
                self.teleport = None;
                self.dismiss_menu(window, cx);
                return;
            }
            _ => return,
        }
        cx.notify();
    }

    pub(crate) fn render_teleport(&self, cx: &mut Context<Self>) -> Div {
        let Some(teleport) = &self.teleport else {
            return div();
        };
        let theme = &self.theme;
        let font = &self.config.ui;
        let muted = rgb(theme.muted);
        let subtext = rgb(theme.subtext());
        let danger = crate::menu::danger(theme);
        let bar = |progress| {
            progress::bar(
                "teleport-progress",
                progress,
                crate::menu::accent(theme).into(),
                rgb(theme.active).into(),
            )
        };
        let line = |text: String| div().min_w_0().child(text);
        let mut body = div().flex().flex_col().gap(px(8.)).px(px(16.)).py(px(12.));
        let (primary, armed) = match &teleport.stage {
            Stage::Choosing => {
                if teleport.hosts.is_empty() {
                    body = body.child(line(fl!("teleport-no-hosts")));
                }
                for (index, host) in teleport.hosts.iter().enumerate() {
                    let selected = teleport.selected == Some(index);
                    // A connected host's snapshot came with the list; the
                    // others are reached over SSH once chosen.
                    let connected = host.repositories.is_some();
                    let detail = if connected {
                        fl!("teleport-host-connected")
                    } else {
                        fl!("teleport-host-ssh")
                    };
                    body = body.child(
                        div()
                            .id(("teleport-host", index))
                            .debug_selector(move || format!("teleport-host-{index}"))
                            .px(px(10.))
                            .py(px(6.))
                            .rounded(px(crate::config::corners::CONTROL))
                            .cursor_pointer()
                            .when(selected, |row| row.bg(rgb(theme.active)))
                            .hover(|row| row.bg(rgb(theme.active)))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .truncate()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(host.place.label.clone()),
                                    )
                                    .child(div().text_color(muted).child(detail)),
                            )
                            // The same online dot as the Devices list and the
                            // sidebar's host headers.
                            .child(
                                div()
                                    .debug_selector(move || format!("teleport-host-dot-{index}"))
                                    .size(px(7.))
                                    .flex_none()
                                    .rounded_full()
                                    .bg(rgb(if connected {
                                        crate::menu::online(theme)
                                    } else {
                                        theme.muted
                                    })),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                if let Some(teleport) = &mut this.teleport {
                                    teleport.selected = Some(index);
                                }
                                this.submit_teleport(window, cx);
                            })),
                    );
                }
                (fl!("teleport-review"), teleport.selected.is_some())
            }
            Stage::Reviewing(place) => {
                body = body.child(bar(Progress::Busy)).child(
                    line(fl!("teleport-checking", host = place.label.as_str())).text_color(muted),
                );
                (fl!("teleport-title"), false)
            }
            Stage::Ready(candidate, review) => {
                let state = &review.state;
                body = body
                    .child(line(text::destination(candidate)))
                    .child(line(fl!(
                        "teleport-state",
                        branch = review.branch.as_str(),
                        unpushed = state.unpushed,
                        changed = state.changed,
                        untracked = state.untracked
                    )));
                match review.github {
                    GitHubAccess::Direct => {}
                    GitHubAccess::Token => {
                        body = body.child(line(fl!(
                            "teleport-github-token",
                            host = candidate.place.label.as_str()
                        )));
                    }
                    GitHubAccess::Unavailable => {
                        body = body.child(
                            line(fl!(
                                "teleport-github-unavailable",
                                host = candidate.place.label.as_str()
                            ))
                            .text_color(danger),
                        );
                    }
                }
                if review.reason == Some(MatchReason::Name) {
                    body = body.child(line(fl!("teleport-name-only")).text_color(danger));
                }
                for (index, tab) in review.tabs.iter().enumerate() {
                    let number = index + 1;
                    let title = match &tab.label {
                        None => fl!("teleport-tab", number = number),
                        Some(label) => {
                            fl!(
                                "teleport-tab-labeled",
                                number = number,
                                label = label.as_str()
                            )
                        }
                    };
                    body = body.child(
                        div()
                            .pt(px(4.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    );
                    for pane in &tab.panes {
                        let missing = matches!(pane.action, Action::Missing(_));
                        body = body.child(
                            div()
                                .pl(px(12.))
                                .truncate()
                                .when(missing, |row| row.text_color(danger))
                                .child(text::pane(&pane.work, &pane.action)),
                        );
                    }
                }
                body = body.child(line(fl!("teleport-left-behind")).text_color(subtext));
                (fl!("teleport-title"), true)
            }
            Stage::Moving(candidate, step) => {
                body = body
                    .child(line(fl!(
                        "teleport-moving-to",
                        host = candidate.place.label.as_str()
                    )))
                    .child(bar(Progress::Working(step.map_or(0., Step::progress))))
                    .child(
                        line(text::step(*step))
                            .debug_selector(|| "teleport-step".into())
                            .text_color(muted),
                    )
                    .child(line(fl!("teleport-keeps-moving")).text_color(subtext));
                (fl!("teleport-moving"), false)
            }
            Stage::Failed(error) => {
                body = body
                    .child(
                        line(error.clone())
                            .debug_selector(|| "teleport-error".into())
                            .text_color(danger),
                    )
                    .child(line(fl!("teleport-source-closes-later")).text_color(subtext));
                (fl!("teleport-close"), true)
            }
        };
        let button = |id: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .px(px(12.))
                .py(px(6.))
                .rounded(px(crate::config::corners::CONTROL))
                .border_1()
                .cursor_pointer()
        };
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(16.))
                    .py(px(12.))
                    .border_b_1()
                    .border_color(rgb(theme.active))
                    .child(
                        svg()
                            .path("icons/teleport.svg")
                            .size(px(16.))
                            .flex_none()
                            .text_color(muted),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(font.size * 1.35))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(fl!("teleport-title")),
                            )
                            .child(div().truncate().text_color(muted).child(fl!(
                                "teleport-subtitle",
                                worktree = teleport.label.as_str(),
                                host = teleport.source.place.label.as_str()
                            ))),
                    ),
            )
            .child(
                div()
                    .id("teleport-body")
                    .max_h(px(420.))
                    .overflow_y_scroll()
                    .child(body),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .px(px(16.))
                    .py(px(12.))
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .child(
                        button("teleport-cancel")
                            .border_color(rgb(theme.active))
                            .hover(|button| button.bg(rgb(theme.active)))
                            .child(if teleport.moving() {
                                fl!("teleport-hide")
                            } else {
                                fl!("teleport-cancel")
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.dismiss_menu(window, cx);
                            })),
                    )
                    .child(
                        button("teleport-submit")
                            .border_color(if armed {
                                rgb(theme.foreground)
                            } else {
                                rgb(theme.active)
                            })
                            .text_color(if armed { rgb(theme.foreground) } else { muted })
                            .child(primary)
                            .when(!armed, |button| {
                                button.opacity(0.4).cursor(CursorStyle::OperationNotAllowed)
                            })
                            .when(armed, |button| {
                                button.bg(rgb(theme.active)).on_click(cx.listener(
                                    |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.submit_teleport(window, cx);
                                    },
                                ))
                            }),
                    ),
            )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
