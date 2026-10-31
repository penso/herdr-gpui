//! Moving a pane to another device. The move picker lists every connected
//! device with the pane's repository open, or every device for session-only
//! moves; choosing one reviews the move
//! there, the review waits for Enter, and the move then runs on its own
//! thread (see `crate::teleport::carry`). Closing the menu abandons a review
//! but never a move under way, which reports as a flash.

use super::Target;
use crate::direction::Logical;
use crate::i18n::fl;
use crate::{
    HerdrWindow,
    menu::Page,
    progress::{self, Progress},
    teleport::{
        Follow, HostRepositories, Place, Step,
        carry::{self, Carried, Choice, Landing, Origin, Review},
        host_for,
    },
    window::Flash,
};
pub(super) use availability::{Availability, PaneState};
use gpui::{prelude::*, *};
use herdr_client::protocol::{AgentStatus, ClientShellSnapshot};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

mod availability;

/// A place on another device the menu's pane can move to: a new worktree,
/// or one of the device's spaces of the pane's repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Device {
    pub(super) endpoint_id: String,
    pub(super) label: String,
    pub(super) choice: Choice,
    /// What the picker row calls the place on that device.
    pub(super) place: String,
}

/// The repository holding the menu's pane, as the GUI snapshot shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PaneRepository {
    key: String,
    label: String,
}

impl PaneRepository {
    pub(super) fn of(snapshot: &ClientShellSnapshot, workspace_id: &str) -> Option<Self> {
        let worktree = snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_id == workspace_id)?
            .worktree
            .as_ref()?;
        Some(Self {
            key: worktree.key.clone(),
            label: worktree.label.clone(),
        })
    }
}

/// Whether the agent in pane `id` is mid-task. Its session file is still
/// being written, so it does not move until it settles.
pub(super) fn busy(snapshot: &ClientShellSnapshot, id: &str) -> bool {
    snapshot
        .agents
        .iter()
        .any(|agent| agent.pane_id == id && agent.agent_status == AgentStatus::Working)
}

enum Event {
    Reviewed(carry::Result<Review>),
    Step(Step),
    Moved(carry::Result<Carried>),
}

enum Stage {
    Reviewing,
    Ready(Review),
    Moving(Option<Step>),
    Failed(String),
}

pub(crate) struct Carry {
    origin: Origin,
    destination: String,
    stage: Stage,
    cancelled: Arc<AtomicBool>,
    events: mpsc::Receiver<Event>,
}

impl Drop for Carry {
    fn drop(&mut self) {
        // A move is never abandoned halfway: it would strand a worktree.
        if !self.moving() {
            self.cancelled.store(true, Ordering::Release);
        }
    }
}

impl Carry {
    /// How far a running move is, for the taskbar; `None` while the dialog
    /// waits on the user or after it failed.
    #[cfg(any(windows, test))]
    pub(crate) fn progress(&self) -> Option<Progress> {
        match &self.stage {
            Stage::Reviewing => Some(Progress::Busy),
            Stage::Moving(step) => Some(Progress::Working(step.map_or(0., Step::progress))),
            Stage::Ready(_) | Stage::Failed(_) => None,
        }
    }

    /// Starts reviewing a move of `origin`'s pane to `destination`.
    fn review(origin: Origin, destination: HostRepositories, choice: Choice) -> Self {
        let label = destination.place.label.clone();
        let source = origin.clone();
        let (cancelled, events) = spawn(move |sender, cancelled| {
            let reviewed = carry::review(&source, &destination, &choice, cancelled);
            let _ = sender.send(Event::Reviewed(reviewed));
        });
        Self {
            origin,
            destination: label,
            stage: Stage::Reviewing,
            cancelled,
            events,
        }
    }

    pub(crate) fn moving(&self) -> bool {
        matches!(self.stage, Stage::Moving(_))
    }

    fn start(&mut self) {
        let Stage::Ready(review) = &self.stage else {
            return;
        };
        let (origin, review) = (self.origin.clone(), review.clone());
        let (cancelled, events) = spawn(move |sender, cancelled| {
            let report = |step| {
                let _ = sender.send(Event::Step(step));
            };
            let moved = carry::run(&origin, &review, report, cancelled);
            let _ = sender.send(Event::Moved(moved));
        });
        self.cancelled = cancelled;
        self.events = events;
        self.stage = Stage::Moving(None);
    }

    /// Applies finished work. Returns a finished move's result once.
    fn poll(&mut self) -> (bool, Option<carry::Result<Carried>>) {
        let mut changed = false;
        loop {
            let event = match self.events.try_recv() {
                Ok(event) => event,
                Err(mpsc::TryRecvError::Empty) => return (changed, None),
                // The worker never started, or ended without its answer: a
                // review or move left waiting would never finish.
                Err(mpsc::TryRecvError::Disconnected) => {
                    let stopped = crate::teleport::Error::WorkerStopped;
                    return match self.stage {
                        Stage::Reviewing => {
                            self.stage = Stage::Failed(stopped.to_string());
                            (true, None)
                        }
                        Stage::Moving(_) => {
                            self.stage = Stage::Failed(stopped.to_string());
                            (true, Some(Err(stopped)))
                        }
                        Stage::Ready(_) | Stage::Failed(_) => (changed, None),
                    };
                }
            };
            changed = true;
            match event {
                Event::Reviewed(Ok(review)) => self.stage = Stage::Ready(review),
                Event::Reviewed(Err(error)) => self.stage = Stage::Failed(error.to_string()),
                Event::Step(step) => self.stage = Stage::Moving(Some(step)),
                Event::Moved(result) => {
                    if let Err(error) = &result {
                        self.stage = Stage::Failed(error.to_string());
                    }
                    return (true, Some(result));
                }
            }
        }
    }
}

/// Runs `work` on a dedicated thread with a fresh cancellation flag.
fn spawn(
    work: impl FnOnce(&mpsc::Sender<Event>, &AtomicBool) + Send + 'static,
) -> (Arc<AtomicBool>, mpsc::Receiver<Event>) {
    let (sender, receiver) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    let spawned = std::thread::Builder::new()
        .name("herdr-pane-carry".into())
        .spawn(move || work(&sender, &flag));
    if let Err(error) = spawned {
        tracing::warn!(%error, "could not start the pane move worker");
    }
    (cancelled, receiver)
}

/// Why panes cannot move to another device right now, if they cannot.
#[derive(Debug, thiserror::Error)]
pub enum Refusal {
    #[error("The agent is working. Move it once it is idle.")]
    Busy,
    #[error("Another move to a device is still running.")]
    Underway,
}

impl HerdrWindow {
    /// The places on other devices `target`'s pane can move to: on every
    /// other enabled, connected, scriptable host showing a workspace of the
    /// same repository, a new worktree and then each such workspace. Their
    /// remotes are compared in the review. None while this host cannot be
    /// scripted. Non-repository panes list every device for a session-only move.
    pub(super) fn carry_devices(&self, repository: Option<&PaneRepository>) -> Vec<Device> {
        let scriptable = cfg!(any(target_os = "linux", target_os = "macos"))
            && host_for(&self.endpoints[self.selected_endpoint].connection.target).is_ok();
        if !scriptable {
            return Vec::new();
        }
        let mut devices = Vec::new();
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            if index == self.selected_endpoint
                || !endpoint.enabled
                || !endpoint.live.status.is_connected()
                || host_for(&endpoint.connection.target).is_err()
            {
                continue;
            }
            let device = |choice, place| Device {
                endpoint_id: endpoint.id.clone(),
                label: endpoint.label.clone(),
                choice,
                place,
            };
            let Some(repository) = repository else {
                devices.push(device(
                    Choice::SessionOnly,
                    fl!("pane-menu-carry-session-only"),
                ));
                continue;
            };
            let Some(snapshot) = endpoint.live.snapshot.as_deref() else {
                continue;
            };
            let spaces: Vec<_> = snapshot
                .workspaces
                .iter()
                .filter(|workspace| {
                    workspace
                        .worktree
                        .as_ref()
                        .is_some_and(|tree| tree.label == repository.label)
                })
                .collect();
            if spaces.is_empty() {
                continue;
            }
            devices.push(device(
                Choice::NewWorktree,
                fl!("pane-menu-carry-new-worktree"),
            ));
            devices.extend(spaces.into_iter().map(|space| {
                device(
                    Choice::Space {
                        workspace_id: space.workspace_id.clone(),
                    },
                    space.label.clone(),
                )
            }));
        }
        devices
    }

    /// Starts reviewing a move of the menu's pane to `device`.
    pub(super) fn review_carry(&mut self, device: &Device, cx: &mut Context<Self>) {
        let started = (|| {
            if self.pane_carry.as_ref().is_some_and(Carry::moving) {
                return Err(crate::Error::from(Refusal::Underway));
            }
            let target = self.validate_pane_target()?;
            let snapshot = self
                .live
                .snapshot
                .as_ref()
                .ok_or(crate::Error::NotConnected)?;
            if busy(snapshot, &target.pane) {
                return Err(Refusal::Busy.into());
            }
            let origin = self.carry_origin(target)?;
            let now = PaneState::of(snapshot, &target.pane);
            if device.choice == Choice::SessionOnly
                && let Some(pane) = &mut self.menu.pane
                && pane
                    .carry_availability
                    .as_ref()
                    .is_some_and(|availability| availability.stale(&now))
            {
                pane.carry_availability = Some(Availability::check(origin.clone(), now));
            }
            if device.choice == Choice::SessionOnly
                && let Some(reason) = self
                    .menu
                    .pane
                    .as_ref()
                    .and_then(|pane| pane.carry_availability.as_ref())
                    .and_then(Availability::reason)
            {
                if let Some(pane) = &mut self.menu.pane {
                    pane.error = Some(reason);
                }
                cx.notify();
                return Ok(None);
            }
            let destination = self
                .dispatch_destination(&device.endpoint_id)
                .ok_or(crate::Error::StaleConnection)?;
            Ok::<_, crate::Error>(Some(Carry::review(
                origin,
                destination,
                device.choice.clone(),
            )))
        })();
        match started {
            Ok(Some(carry)) => {
                self.pane_carry = Some(carry);
                self.menu.page = Some(Page::CarryPane);
                cx.notify();
            }
            Ok(None) => {}
            Err(error) => self.pane_error(error, cx),
        }
    }

    pub(super) fn carry_origin(&self, target: &Target) -> crate::Result<Origin> {
        let endpoint = &self.endpoints[self.selected_endpoint];
        Ok(Origin {
            place: Place {
                endpoint_id: endpoint.id.clone(),
                label: endpoint.label.clone(),
                host: host_for(&endpoint.connection.target)?,
            },
            workspace_id: target.workspace.clone(),
            pane_id: target.pane.clone(),
        })
    }

    /// Keeps the review current and follows a finished move to its pane.
    pub(crate) fn poll_carry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(availability) = self
            .menu
            .pane
            .as_mut()
            .and_then(|pane| pane.carry_availability.as_mut())
            && availability.poll()
        {
            cx.notify();
        }
        let open = self.menu.page == Some(Page::CarryPane);
        let Some(carry) = &mut self.pane_carry else {
            return;
        };
        if !open && !carry.moving() {
            self.pane_carry = None;
            return;
        }
        let (changed, finished) = carry.poll();
        let Some(result) = finished else {
            if changed {
                cx.notify();
            }
            return;
        };
        let destination = carry.destination.clone();
        let carried = match result {
            Ok(carried) => carried,
            Err(error) if !open => {
                self.pane_carry = None;
                self.show_flash(
                    Flash::warning(fl!("pane-menu-carry-failed", error = error.to_string())),
                    cx,
                );
                return;
            }
            // The menu shows the failure.
            Err(_) => return cx.notify(),
        };
        self.pane_carry = None;
        if open {
            self.dismiss_menu(window, cx);
        }
        for warning in &carried.warnings {
            tracing::warn!(%warning, "pane move");
        }
        let flash = match carried.warnings.first() {
            None => Flash::success(fl!("pane-menu-carry-moved", destination = destination)),
            Some(first) => Flash::warning(fl!(
                "pane-menu-carry-moved-warnings",
                destination = destination,
                count = carried.warnings.len(),
                first = first.to_string(),
            )),
        };
        self.show_flash(flash, cx);
        self.teleport_follow = Some(Follow::pane(
            carried.endpoint_id,
            carried.workspace_id,
            carried.pane_id,
        ));
    }

    pub(crate) fn carry_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        window.prevent_default();
        match event.keystroke.key.as_str() {
            "escape" => self.dismiss_menu(window, cx),
            "enter" => self.submit_carry(window, cx),
            _ => {}
        }
    }

    fn submit_carry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The agent may have started working while the review waited.
        let working = self.pane_carry.as_ref().is_some_and(|carry| {
            let origin = &carry.origin;
            matches!(carry.stage, Stage::Ready(_))
                && self
                    .endpoints
                    .iter()
                    .position(|endpoint| endpoint.id == origin.place.endpoint_id)
                    .and_then(|index| self.endpoint_snapshot(index))
                    .is_some_and(|snapshot| busy(snapshot, &origin.pane_id))
        });
        let Some(carry) = &mut self.pane_carry else {
            return;
        };
        match carry.stage {
            Stage::Ready(_) if working => carry.stage = Stage::Failed(Refusal::Busy.to_string()),
            Stage::Ready(_) => carry.start(),
            Stage::Failed(_) => return self.dismiss_menu(window, cx),
            Stage::Reviewing | Stage::Moving(_) => return,
        }
        cx.notify();
    }

    pub(crate) fn render_carry(&self, cx: &mut Context<Self>) -> Div {
        let Some(carry) = &self.pane_carry else {
            return div();
        };
        let theme = &self.theme;
        let muted = rgb(theme.muted);
        let subtext = rgb(theme.subtext());
        let danger = crate::menu::danger(theme);
        let bar = |progress| {
            progress::bar(
                "pane-carry-progress",
                progress,
                crate::menu::accent(theme).into(),
                rgb(theme.active).into(),
            )
        };
        let line = |text: String| div().min_w_0().child(text);
        let mut body = div().flex().flex_col().gap(px(6.)).p(px(8.));
        body = body.child(div().text_color(muted).truncate().child(fl!(
            "pane-menu-carry-title",
            destination = carry.destination.to_string()
        )));
        let (primary, armed) = match &carry.stage {
            Stage::Reviewing => {
                body = body
                    .child(bar(Progress::Busy))
                    .child(line(fl!("pane-menu-carry-reviewing")).text_color(muted));
                (fl!("pane-menu-carry-move"), false)
            }
            Stage::Ready(review) => {
                match review {
                    Review::SessionOnly(review) => {
                        body = body
                            .child(line(fl!(
                                "pane-menu-carry-session-cwd",
                                directory = review.cwd.to_string()
                            )))
                            .child(
                                line(review.describe()).debug_selector(|| "pane-carry-runs".into()),
                            )
                            .child(line(fl!("pane-menu-carry-session-note")).text_color(subtext))
                            .child(line(fl!("pane-menu-carry-closes")).text_color(subtext));
                    }
                    Review::Repository(review) => {
                        let state = &review.state;
                        let branch = match (&review.landing, &state.branch) {
                            (
                                Landing::Worktree {
                                    branch: Some(branch),
                                },
                                _,
                            ) => {
                                fl!("pane-menu-carry-branch", branch = branch.as_str())
                            }
                            (Landing::Worktree { branch: None }, Some(branch)) => {
                                fl!("pane-menu-carry-branch-in-use", branch = branch.as_str())
                            }
                            (Landing::Worktree { branch: None }, None) => {
                                fl!("pane-menu-carry-branch-commit")
                            }
                            (
                                Landing::Space {
                                    checkout,
                                    fast_forward: true,
                                    ..
                                },
                                _,
                            ) => {
                                fl!(
                                    "pane-menu-carry-tab-fast-forward",
                                    checkout = checkout.as_str()
                                )
                            }
                            (Landing::Space { checkout, .. }, _) => {
                                fl!("pane-menu-carry-tab", checkout = checkout.as_str())
                            }
                        };
                        body = body
                            .child(line(fl!(
                                "pane-menu-carry-into",
                                repository = review.repository.label.as_str()
                            )))
                            .child(line(branch))
                            .child(line(fl!(
                                "pane-menu-carry-state",
                                unpushed = state.unpushed,
                                changed = state.changed,
                                untracked = state.untracked,
                            )))
                            .child(
                                line(review.describe())
                                    .debug_selector(|| "pane-carry-runs".into())
                                    .when(
                                        matches!(review.action, carry::Action::Missing(_)),
                                        |row| row.text_color(danger),
                                    ),
                            )
                            .child(line(fl!("pane-menu-carry-copied")).text_color(subtext));
                    }
                }
                (fl!("pane-menu-carry-move"), true)
            }
            Stage::Moving(step) => {
                body = body
                    .child(bar(Progress::Working(step.map_or(0., Step::progress))))
                    .child(
                        line(step.map_or_else(
                            || fl!("pane-menu-carry-starting"),
                            |step| step.label().to_owned(),
                        ))
                        .debug_selector(|| "pane-carry-step".into())
                        .text_color(muted),
                    )
                    .child(line(fl!("pane-menu-carry-keeps-going")).text_color(subtext));
                (fl!("pane-menu-carry-moving"), false)
            }
            Stage::Failed(error) => {
                body = body.child(
                    line(error.clone())
                        .debug_selector(|| "pane-carry-error".into())
                        .text_color(danger),
                );
                (fl!("pane-menu-carry-close"), true)
            }
        };
        body.child(
            div().row().justify_end().pt(px(4.)).child(
                div()
                    .id("pane-carry-submit")
                    .debug_selector(|| "pane-carry-submit".into())
                    .px(px(12.))
                    .py(px(4.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .border_1()
                    .border_color(rgb(if armed {
                        theme.foreground
                    } else {
                        theme.active
                    }))
                    .text_color(if armed { rgb(theme.foreground) } else { muted })
                    .child(primary)
                    .when(!armed, |button| {
                        button.opacity(0.4).cursor(CursorStyle::OperationNotAllowed)
                    })
                    .when(armed, |button| {
                        button
                            .cursor_pointer()
                            .bg(rgb(theme.active))
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.submit_carry(window, cx);
                            }))
                    }),
            ),
        )
    }
}

#[cfg(test)]
mod tests;
