//! The pane menu's moves: to a new tab, a new workspace, or another tab
//! through the move picker. Each move runs the `herdr` CLI on a background
//! worker (see `crate::pane_move`); the menu stays open saying so until
//! Herdr answers.

use super::{Action, PaneMenu, Target};
use crate::direction::Logical;
use crate::i18n::fl;
use crate::{
    HerdrWindow,
    menu::Page,
    pane_move::{self, Choice, Destination},
};
use gpui::{prelude::*, *};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Tells one pane move's result from another's, so a late answer never
/// settles a menu reopened since.
static NEXT_MOVE: AtomicU64 = AtomicU64::new(0);

impl Target {
    /// The move picker's rows: a new tab here and a new workspace first, then
    /// every other place the pane can go.
    pub(super) fn destinations(&self) -> Vec<Choice> {
        [
            Choice {
                destination: Destination::NewTab {
                    workspace_id: self.workspace.clone(),
                },
                workspace: None,
                place: fl!("pane-menu-new-tab"),
            },
            Choice {
                destination: Destination::NewWorkspace,
                workspace: None,
                place: fl!("pane-menu-new-workspace"),
            },
        ]
        .into_iter()
        .chain(self.choices.iter().cloned())
        .collect()
    }
}

impl PaneMenu {
    /// How many rows the current page lists.
    pub(super) fn rows(&self, page: Option<Page>) -> usize {
        if page == Some(Page::MovePane) {
            self.target.destinations().len() + self.target.devices.len()
        } else {
            self.actions().len()
        }
    }
}

/// A picker row's text. A long workspace name gives way first, so the tab
/// that tells two rows of one workspace apart stays readable.
fn destination_label(index: usize, workspace: Option<String>, place: String) -> Div {
    // The label fills its row, so the tab's cap below is a share of the row:
    // sized to its text, a short workspace name would squeeze the tab.
    let row = div().row().flex_1().min_w(px(0.)).overflow_hidden();
    let place = div()
        .debug_selector(move || format!("pane-move-place-{index}"))
        .min_w(px(0.))
        .truncate()
        .child(place);
    let Some(workspace) = workspace else {
        return row.child(place);
    };
    row.child(
        div()
            .min_w(px(0.))
            .flex_shrink_1()
            .truncate()
            .child(workspace),
    )
    .child(div().flex_none().child(format!(
        "\u{a0}{}\u{a0}",
        crate::direction::glyph("\u{203a}")
    )))
    .child(place.flex_none().max_w(relative(0.6)))
}

/// Why panes on `target` cannot move, if they cannot: host scripts need a
/// POSIX client (see `herdr_client::run_script`) and a host the `herdr` CLI
/// can address.
pub(super) fn refusal(target: &herdr_client::ConnectTarget) -> Option<pane_move::Error> {
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return Some(pane_move::Error::UnsupportedHost);
    }
    pane_move::host_for(target).err()
}

impl HerdrWindow {
    /// Opens the focused pane's move picker, as its menu's "Move to…" row
    /// would, at the pane's top-left corner.
    pub(crate) fn move_focused_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_focused_pane_menu(window, cx);
        if self.menu.page == Some(Page::Pane) {
            if self.menu.pane.as_ref().is_some_and(|pane| !pane.movable) {
                // Say why this endpoint cannot move panes, e.g. a --dev session
                // rather than a custom socket.
                let error = refusal(&self.endpoints[self.selected_endpoint].connection.target)
                    .unwrap_or(pane_move::Error::UnsupportedHost);
                self.pane_error(error, cx);
                return;
            }
            self.activate_pane_menu(Action::MoveTo, window, cx);
        }
    }

    /// Shows the move picker in place of the menu's rows.
    pub(super) fn open_move_picker(&mut self, cx: &mut Context<Self>) {
        let availability = self
            .menu
            .pane
            .as_ref()
            .filter(|pane| {
                pane.target.repository.is_none()
                    && !pane.target.devices.is_empty()
                    && pane.carry_availability.is_none()
            })
            .and_then(|pane| {
                let seen =
                    super::carry::PaneState::of(self.live.snapshot.as_deref()?, &pane.target.pane);
                Some(super::carry::Availability::check(
                    self.carry_origin(&pane.target).ok()?,
                    seen,
                ))
            });
        if let Some(availability) = availability
            && let Some(pane) = &mut self.menu.pane
        {
            pane.carry_availability = Some(availability);
        }
        if let Some(pane) = &mut self.menu.pane {
            pane.selected = None;
            pane.error = None;
        }
        self.menu.page = Some(Page::MovePane);
        cx.notify();
    }

    /// Moves the menu's pane through the `herdr` CLI on a background thread.
    /// The menu stays open, saying so, until Herdr answers; a failure shows
    /// in the menu, or in the window's status once the menu has closed.
    pub(super) fn move_pane(
        &mut self,
        destination: Destination,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .menu
            .pane
            .as_ref()
            .is_none_or(|pane| pane.moving.is_some())
        {
            return;
        }
        let started = (|| {
            let target = self.validate_pane_target()?;
            let snapshot = self
                .live
                .snapshot
                .as_ref()
                .ok_or(crate::Error::NotConnected)?;
            if !destination.exists(snapshot) {
                return Err(match destination {
                    Destination::NewTab { .. } => crate::Error::StaleWorkspace,
                    Destination::Tab { .. } | Destination::NewWorkspace => crate::Error::StaleTab,
                });
            }
            let request = pane_move::Request::new(target.pane.clone(), destination)?;
            let host =
                pane_move::host_for(&self.endpoints[self.selected_endpoint].connection.target)?;
            Ok::<_, crate::Error>((request, host))
        })();
        let (request, host) = match started {
            Ok(started) => started,
            Err(error) => {
                self.pane_error(error, cx);
                return;
            }
        };
        let id = NEXT_MOVE.fetch_add(1, Ordering::Relaxed);
        if let Some(pane) = &mut self.menu.pane {
            pane.moving = Some(id);
            pane.error = None;
        }
        // The CLI is a child process, possibly over SSH: never on this thread.
        let job = cx
            .background_executor()
            .spawn(async move { request.run(&host, &AtomicBool::new(false)) });
        cx.spawn_in(window, async move |this, cx| {
            let result = job.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.finish_pane_move(id, result, window, cx)
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn finish_pane_move(
        &mut self,
        id: u64,
        result: pane_move::Result<()>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open = self
            .menu
            .pane
            .as_ref()
            .is_some_and(|pane| pane.moving == Some(id));
        match result {
            Ok(()) if open => self.dismiss_menu(window, cx),
            Ok(()) => {}
            Err(error) if open => {
                if let Some(pane) = &mut self.menu.pane {
                    pane.moving = None;
                }
                self.pane_error(error, cx);
            }
            Err(error) => {
                self.local_error = Some(error.to_string());
                cx.notify();
            }
        }
    }

    /// Enter on the move picker moves the pane to the selected row.
    pub(super) fn submit_move_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pane) = &self.menu.pane else {
            return;
        };
        let Some(index) = pane.selected else {
            return;
        };
        let mut here = pane.target.destinations();
        if index < here.len() {
            self.move_pane(here.swap_remove(index).destination, window, cx);
        } else if let Some(device) = pane.target.devices.get(index - here.len()).cloned() {
            self.review_carry(&device, cx);
        }
    }

    pub(super) fn render_pane_move_pending(&self) -> Div {
        div().flex().flex_col().child(
            div()
                .debug_selector(|| "pane-move-pending".into())
                .p(px(8.))
                .text_color(rgb(self.theme.muted))
                .child(fl!("pane-menu-moving")),
        )
    }

    /// Fills the menu's `panel` with the picker. The heading and rows are the
    /// panel's own children, which its scroll handle addresses by index, so
    /// a row the keyboard selects in a long list scrolls into view.
    pub(crate) fn render_move_picker(
        &self,
        panel: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let Some(pane) = &self.menu.pane else {
            return panel;
        };
        if pane.moving.is_some() {
            return panel.child(self.render_pane_move_pending());
        }
        let mut body = panel.track_scroll(&pane.scroll).flex().flex_col().child(
            div()
                .flex_none()
                .px(px(8.))
                .py(px(4.))
                .text_color(rgb(self.theme.muted))
                .child(fl!("pane-menu-move-heading")),
        );
        let row = |index: usize| {
            div()
                .id(("pane-move-destination", index))
                .flex_none()
                .debug_selector(move || format!("pane-move-{index}"))
                .min_h(px(self.config.ui.line_height() + 12.))
                .px(px(8.))
                .row()
                .items_center()
                .overflow_hidden()
                .cursor_pointer()
                .when(pane.selected == Some(index), |row| {
                    row.bg(rgb(self.theme.active))
                })
                .hover(|row| row.bg(rgb(self.theme.active)))
                .on_hover(cx.listener(move |this, hovered, _, cx| {
                    if *hovered && let Some(pane) = &mut this.menu.pane {
                        pane.selected = Some(index);
                        cx.notify();
                    }
                }))
        };
        let here = pane.target.destinations();
        let local = here.len();
        for (index, choice) in here.into_iter().enumerate() {
            let destination = choice.destination;
            body = body.child(
                row(index)
                    .child(destination_label(index, choice.workspace, choice.place))
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        // The picker replaces the menu under the pointer, and
                        // snapping it into the window can shift its rows, so
                        // a double-click's second click would otherwise pick
                        // whatever row it lands on.
                        if event.click_count() > 1 {
                            return;
                        }
                        this.move_pane(destination.clone(), window, cx)
                    })),
            );
        }
        // A working agent cannot leave yet; its rows say so when chosen.
        let busy = self
            .live
            .snapshot
            .as_deref()
            .is_some_and(|snapshot| super::carry::busy(snapshot, &pane.target.pane));
        for (offset, device) in pane.target.devices.iter().enumerate() {
            let index = local + offset;
            let chosen = device.clone();
            body = body.child(
                row(index)
                    .when(
                        busy || pane
                            .carry_availability
                            .as_ref()
                            .is_some_and(super::carry::Availability::muted),
                        |row| row.text_color(rgb(self.theme.muted)),
                    )
                    .child(destination_label(
                        index,
                        Some(device.label.clone()),
                        device.place.clone(),
                    ))
                    .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                        if event.click_count() > 1 {
                            return;
                        }
                        this.review_carry(&chosen, cx)
                    })),
            );
        }
        body.when_some(pane.error.clone(), |body, error| {
            body.child(div().flex_none().p(px(8.)).child(error))
        })
    }
}
