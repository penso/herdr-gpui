//! The Overview page as drawn: a summary line, then the list beside the
//! host column. A row shows its state, agent, title, checkout, host, pull
//! request, and how long it has been so; hovering it offers Snooze, Settle,
//! Pin, and Open, or on a closed agent Reopen and Forget. With nothing to
//! list, the page says what it is for.

use super::{Marks, Row, Sections, State, presets};
use crate::{
    HerdrWindow, config::corners, devices_overview::Look, fonts::StyledFont, icons::AgentIcon,
    pull_request::Outcome,
};
use chrono::{DateTime, Local};
use gpui::{prelude::*, *};
use herdr_client::protocol::AgentStatus;
use std::time::SystemTime;

const ROW_HEIGHT: f32 = 44.;
const BRANCH_WIDTH: f32 = 170.;
const HOST_WIDTH: f32 = 110.;
const PR_WIDTH: f32 = 64.;
const SINCE_WIDTH: f32 = 40.;
/// The least a title keeps when the branch and host columns give way.
const TITLE_MIN: f32 = 140.;
/// The least the branch and host columns shrink to.
const COLUMN_MIN: f32 = 56.;

/// Where a row is drawn, which decides its actions.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shelf {
    Active,
    Snoozed(SystemTime),
    Settled,
    /// With the closed agent's id and when it closed.
    Closed(u64, SystemTime),
}

/// What every row of one frame draws with.
struct Frame<'a> {
    look: Look<'a>,
    now: SystemTime,
    local: DateTime<Local>,
}

impl HerdrWindow {
    /// The page across the window's body.
    pub(crate) fn render_overview(&self, cx: &mut Context<Self>) -> AnyElement {
        let frame = Frame {
            look: self.overview_look(cx),
            now: SystemTime::now(),
            local: Local::now(),
        };
        let sections = self.overview_sections(cx);
        let theme = frame.look.theme;
        let list = if sections.is_empty() && self.overview.closed.is_empty() {
            overview_empty(&frame.look)
        } else {
            self.overview_list(&frame, &sections, cx)
        };
        div()
            .id("overview")
            .debug_selector(|| "overview".into())
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.foreground))
            .text_font(frame.look.ui)
            .text_size(px(frame.look.ui.size))
            .child(self.overview_summary(&frame.look, &sections))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .id("overview-list")
                            .debug_selector(|| "overview-list".into())
                            .flex_1()
                            .min_w_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.overview.scroll)
                            .child(list),
                    )
                    .child(self.render_overview_hosts(&frame.look, cx)),
            )
            .into_any_element()
    }

    fn overview_summary(&self, look: &Look, sections: &Sections) -> Div {
        let theme = look.theme;
        let stat = |count: usize, label: &'static str, status: AgentStatus| {
            // A zero says nothing the missing stat does not.
            (count > 0).then(|| {
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(look.dot(status))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(count.to_string()),
                    )
                    .child(div().text_color(rgb(theme.subtext())).child(label))
            })
        };
        div()
            .debug_selector(|| "overview-summary".into())
            .flex_none()
            .px_5()
            .py_3()
            .flex()
            .items_center()
            .gap_5()
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .text_size(px(look.ui.size + 3.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Overview"),
            )
            .children(stat(sections.needing(), "need you", AgentStatus::Blocked))
            .children(stat(
                sections.working.len(),
                "working",
                AgentStatus::Working,
            ))
            .children(stat(sections.ready.len(), "ready", AgentStatus::Done))
    }

    fn overview_list(&self, frame: &Frame, sections: &Sections, cx: &mut Context<Self>) -> Div {
        let look = &frame.look;
        let theme = look.theme;
        let row = |row: &Row, shelf: Shelf, cx: &mut Context<Self>| {
            self.overview_row(frame, row, shelf, cx)
        };
        let mut list = div().px_4().pb_4().flex().flex_col();
        if !sections.pinned.is_empty() {
            list = list.child(heading(look, "Pinned", Some(sections.pinned.len())));
            for pinned in &sections.pinned {
                list = list.child(row(pinned, Shelf::Active, cx));
            }
        }
        let blocked = look.status(AgentStatus::Blocked);
        let mut needs = div()
            .debug_selector(|| "overview-needs".into())
            .mt_3()
            .p_1()
            .flex()
            .flex_col()
            .rounded(px(corners::PANEL))
            .border_1()
            .border_color(rgb(crate::config::mix(theme.background, blocked, 35)))
            .bg(rgb(crate::config::mix(theme.background, blocked, 6)))
            .child(
                div()
                    .px_2()
                    .pt_1()
                    .flex()
                    .items_center()
                    .gap_2()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(look.dot(AgentStatus::Blocked))
                    .child("Needs you")
                    .child(
                        div()
                            .text_color(rgb(theme.muted))
                            .child(sections.needs.len().to_string()),
                    ),
            );
        for waiting in &sections.needs {
            needs = needs.child(row(waiting, Shelf::Active, cx));
        }
        // Only what is there shows: no card when nothing needs the user, no
        // heading over an empty group.
        if !sections.needs.is_empty() {
            list = list.child(needs);
        }
        // Finished work waits on the user too; working agents do not.
        if !sections.ready.is_empty() {
            list = list.child(heading(
                look,
                "Ready for review",
                Some(sections.ready.len()),
            ));
        }
        for ready in &sections.ready {
            list = list.child(row(ready, Shelf::Active, cx));
        }
        if !sections.working.is_empty() {
            list = list.child(heading(look, "Working", Some(sections.working.len())));
        }
        let mut host: Option<&str> = None;
        for working in &sections.working {
            // Rows arrive in sidebar order, so each host's run together.
            if host != Some(working.endpoint.as_str()) {
                host = Some(working.endpoint.as_str());
                list = list.child(
                    div()
                        .pt_2()
                        .px_2()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_size(look.small())
                        .text_color(rgb(theme.subtext()))
                        .child(
                            svg()
                                .path("icons/computer.svg")
                                .size(px(12.))
                                .text_color(rgb(theme.subtext())),
                        )
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(working.host.clone()),
                        ),
                );
            }
            list = list.child(row(working, Shelf::Active, cx));
        }
        if !sections.idle.is_empty() {
            list = list.child(heading(look, "Idle", Some(sections.idle.len())));
            for idle in &sections.idle {
                list = list.child(row(idle, Shelf::Active, cx));
            }
        }
        // The shelves of rows held out of the way, each only when it holds
        // some: an empty one is a click that shows nothing.
        let closed = self.overview.closed.len();
        if sections.snoozed.is_empty() && sections.settled.is_empty() && closed == 0 {
            return list;
        }
        list = list.child(div().mt_3().h(px(1.)).bg(rgb(theme.active)));
        if !sections.snoozed.is_empty() {
            list =
                list.child(self.overview_shelf(look, ShelfId::Snoozed, sections.snoozed.len(), cx));
        }
        if self.overview.snoozed_open {
            for (snoozed, until) in &sections.snoozed {
                list = list.child(row(snoozed, Shelf::Snoozed(*until), cx));
            }
        }
        if !sections.settled.is_empty() {
            list =
                list.child(self.overview_shelf(look, ShelfId::Settled, sections.settled.len(), cx));
        }
        if self.overview.settled_open {
            for settled in &sections.settled {
                list = list.child(row(settled, Shelf::Settled, cx));
            }
        }
        if closed > 0 {
            list = list.child(self.overview_shelf(look, ShelfId::Closed, closed, cx));
        }
        if self.overview.closed_open {
            for agent in self.overview.closed.agents() {
                list = list.child(row(&agent.row, Shelf::Closed(agent.id, agent.closed), cx));
            }
        }
        list
    }

    fn overview_shelf(
        &self,
        look: &Look,
        shelf: ShelfId,
        count: usize,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = look.theme;
        let (id, label, open) = match shelf {
            ShelfId::Snoozed => ("overview-snoozed", "Snoozed", self.overview.snoozed_open),
            ShelfId::Settled => ("overview-settled", "Settled", self.overview.settled_open),
            ShelfId::Closed => ("overview-closed", "Closed", self.overview.closed_open),
        };
        div()
            .id(id)
            .debug_selector(move || id.into())
            .mt_2()
            .h(px(28.))
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .rounded(px(corners::CONTROL))
            .hover(|style| style.bg(rgb(theme.active)))
            .text_size(look.small())
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(theme.muted))
            .child(
                svg()
                    .path(if open {
                        "icons/chevron-down.svg"
                    } else {
                        "icons/chevron-right.svg"
                    })
                    .size(px(11.))
                    .text_color(rgb(theme.muted)),
            )
            .child(label)
            .child(count.to_string())
            .on_click(cx.listener(move |this, _, _, cx| {
                let open = match shelf {
                    ShelfId::Snoozed => &mut this.overview.snoozed_open,
                    ShelfId::Settled => &mut this.overview.settled_open,
                    ShelfId::Closed => &mut this.overview.closed_open,
                };
                *open = !*open;
                cx.notify();
            }))
    }

    fn overview_row(
        &self,
        frame: &Frame,
        row: &Row,
        shelf: Shelf,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let look = &frame.look;
        let theme = look.theme;
        // A closed agent may share its key with others, closed or running.
        let selector = match shelf {
            Shelf::Closed(id, _) => format!("overview-closed-{id}"),
            _ => format!("overview-row-{}", row.key.selector()),
        };
        let group = SharedString::from(selector.clone());
        let woke = Marks::of(cx)
            .and_then(|marks| marks.get(&row.key))
            .is_some_and(|mark| mark.woke && shelf == Shelf::Active);
        let pinned = Marks::of(cx)
            .and_then(|marks| marks.get(&row.key))
            .is_some_and(|mark| mark.pinned);
        let menu_open = self.overview.menu.as_ref() == Some(&row.key);
        let glyph = match row.state {
            State::Failed => svg()
                .path("icons/error.svg")
                .size(px(12.))
                .flex_none()
                .text_color(rgb(look.status(AgentStatus::Blocked)))
                .into_any_element(),
            state => look.dot(state.status()).into_any_element(),
        };
        let icon = if row.identity.is_none() && matches!(row.open, super::Open::Run(_)) {
            "icons/fan-out.svg"
        } else {
            AgentIcon::from_identity(row.identity.as_deref()).path()
        };
        let second = match shelf {
            Shelf::Snoozed(until) => format!("Wakes {}", presets::wakes(until, &frame.local)),
            _ if row.workspace.is_empty() => row.detail.clone(),
            _ => format!("{} · {}", row.workspace, row.detail),
        };
        let actions = self.overview_actions(row, shelf, pinned, &group, menu_open, look, cx);
        let clicked = row.clone();
        let title_selector = match shelf {
            Shelf::Closed(id, _) => format!("overview-closed-title-{id}"),
            _ => format!("overview-title-{}", row.key.selector()),
        };
        let since = match shelf {
            Shelf::Closed(_, closed) => Some(closed),
            _ => row.since,
        };
        div()
            .id(SharedString::from(selector.clone()))
            .debug_selector(move || selector.clone())
            .group(group)
            .relative()
            .h(px(ROW_HEIGHT))
            .flex_none()
            .px_2()
            .flex()
            .items_center()
            .gap_3()
            .rounded(px(corners::CONTROL))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(theme.active)))
            .when(shelf != Shelf::Active, |row| row.opacity(0.7))
            .child(div().w(px(12.)).flex().justify_center().child(glyph))
            .child(crate::icons::icon(icon, 15., theme.foreground))
            .child(
                div()
                    .debug_selector(move || title_selector.clone())
                    .flex_1()
                    .min_w(px(TITLE_MIN))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(row.title.clone()),
                            )
                            .when(woke, |title| title.child(tag(look, "Woke"))),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(look.small())
                            .text_color(rgb(theme.muted))
                            .child(second),
                    ),
            )
            .child(
                look.mono(row.branch.clone().unwrap_or_default())
                    .w(px(BRANCH_WIDTH))
                    .min_w(px(COLUMN_MIN))
                    .flex_shrink_1()
                    .truncate(),
            )
            .child(
                div()
                    .w(px(HOST_WIDTH))
                    .min_w(px(COLUMN_MIN))
                    .flex_shrink_1()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(look.small())
                    .text_color(rgb(theme.muted))
                    .child(
                        svg()
                            .path("icons/computer.svg")
                            .size(px(11.))
                            .flex_none()
                            .text_color(rgb(theme.muted)),
                    )
                    .child(div().min_w_0().truncate().child(row.host.clone())),
            )
            .child(
                div()
                    .w(px(PR_WIDTH))
                    .flex_none()
                    .when_some(row.pr, |cell, pr| cell.child(pr_chip(look, pr))),
            )
            .child(
                div()
                    .w(px(SINCE_WIDTH))
                    .flex_none()
                    .flex()
                    .justify_end()
                    .child(
                        look.mono(
                            since
                                .map(|since| presets::elapsed(since, frame.now))
                                .unwrap_or_default(),
                        ),
                    ),
            )
            .child(actions)
            .on_click(cx.listener(move |this, _, window, cx| {
                if let Shelf::Closed(id, _) = shelf {
                    this.open_closed_agent(id, window, cx);
                } else {
                    this.open_overview_row(&clicked, window, cx);
                }
            }))
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn overview_actions(
        &self,
        row: &Row,
        shelf: Shelf,
        pinned: bool,
        group: &SharedString,
        menu_open: bool,
        look: &Look,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = look.theme;
        let key = match shelf {
            Shelf::Closed(id, _) => id.to_string(),
            _ => row.key.selector(),
        };
        let action = |name: &'static str, icon: &'static str, label: &'static str| {
            div()
                .id(SharedString::from(format!("overview-{name}-{key}")))
                .debug_selector({
                    let id = format!("overview-{name}-{key}");
                    move || id.clone()
                })
                .h(px(22.))
                .px(px(6.))
                .flex()
                .items_center()
                .gap_1()
                .rounded(px(corners::SMALL))
                .border_1()
                .border_color(rgb(theme.active))
                .bg(rgb(theme.background))
                .cursor_pointer()
                .text_size(look.small())
                .child(
                    svg()
                        .path(icon)
                        .size(px(11.))
                        .flex_none()
                        .text_color(rgb(theme.subtext())),
                )
                .child(label)
        };
        let (snooze, settle, pin, restore, open) = (
            row.key.clone(),
            (row.key.clone(), row.state),
            row.key.clone(),
            row.key.clone(),
            row.clone(),
        );
        let mut actions = div()
            .absolute()
            .right(px(SINCE_WIDTH + 12.))
            .top(px(6.))
            .p(px(4.))
            .flex()
            .gap_1()
            .rounded(px(corners::CONTROL))
            .bg(rgb(theme.active))
            .when(!menu_open, |actions| {
                actions
                    .opacity(0.)
                    .group_hover(group.clone(), |style| style.opacity(1.))
            });
        if let Shelf::Closed(id, _) = shelf {
            let reopenable = self
                .overview
                .closed
                .get(id)
                .and_then(super::reopen::kind)
                .is_some();
            if reopenable {
                actions = actions.child(action("reopen", "icons/play.svg", "Reopen").on_click(
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.show_activity_panel(
                            crate::sidebar::ActivityPanel::Workspaces,
                            window,
                            cx,
                        );
                        this.activity.return_to = Some(crate::sidebar::ActivityPanel::Overview);
                        this.reopen_closed_agent(id, cx);
                        window.focus(&this.focus, cx);
                    }),
                ));
            }
            return actions.child(
                action("forget", "icons/x.svg", "Forget").on_click(cx.listener(
                    move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.overview.closed.forget(id);
                        cx.notify();
                    },
                )),
            );
        }
        if shelf == Shelf::Active {
            actions = actions
                .child(
                    action("snooze", "icons/coffee.svg", "Snooze").on_click(cx.listener(
                        move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.overview.menu = (this.overview.menu.as_ref() != Some(&snooze))
                                .then(|| snooze.clone());
                            cx.notify();
                        },
                    )),
                )
                .child(
                    action("settle", "icons/check.svg", "Settle").on_click(cx.listener(
                        move |this, _, _, cx| {
                            cx.stop_propagation();
                            let (key, state) = &settle;
                            this.mark_overview_row(cx, |marks| marks.settle(key, *state));
                        },
                    )),
                )
                .child(
                    action(
                        "pin",
                        "icons/lock.svg",
                        if pinned { "Unpin" } else { "Pin" },
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.mark_overview_row(cx, |marks| marks.toggle_pin(&pin));
                    })),
                );
        } else {
            let label = if shelf == Shelf::Settled {
                "Unsettle"
            } else {
                "Wake now"
            };
            actions = actions.child(action("restore", "icons/refresh.svg", label).on_click(
                cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.mark_overview_row(cx, |marks| marks.release(&restore));
                }),
            ));
        }
        actions = actions.child(action("open", "icons/arrow-right.svg", "Open").on_click(
            cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.open_overview_row(&open, window, cx);
            }),
        ));
        if menu_open {
            actions = actions.child(
                deferred(
                    div()
                        .absolute()
                        .top(px(30.))
                        .right_0()
                        .child(crate::motion::pop_in(
                            self.snooze_menu(row, look, cx),
                            "overview-snooze-menu",
                        )),
                )
                .with_priority(1),
            );
        }
        actions
    }

    fn snooze_menu(&self, row: &Row, look: &Look, cx: &mut Context<Self>) -> Div {
        let theme = look.theme;
        let mut menu = div()
            .debug_selector(|| "overview-snooze-menu".into())
            .occlude()
            .w(px(230.))
            .p_1()
            .flex()
            .flex_col()
            .rounded(px(corners::PANEL))
            .border_1()
            .border_color(rgb(theme.active))
            .bg(rgb(theme.surface))
            .shadow_lg()
            .child(
                div()
                    .px_2()
                    .py_1()
                    .text_size(look.small())
                    .text_color(rgb(theme.muted))
                    .child("Snooze until"),
            );
        for (index, preset) in presets::presets(&Local::now()).into_iter().enumerate() {
            let (key, state, until) = (row.key.clone(), row.state, preset.until);
            menu = menu.child(
                div()
                    .id(("overview-snooze-preset", index))
                    .debug_selector(move || format!("overview-snooze-preset-{index}"))
                    .h(px(26.))
                    .px_2()
                    .flex()
                    .items_center()
                    .rounded(px(corners::CONTROL))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(theme.active)))
                    .child(div().flex_1().child(preset.label))
                    .child(look.mono(preset.when))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.mark_overview_row(cx, |marks| marks.snooze(&key, state, until));
                    })),
            );
        }
        menu
    }
}

/// The shelves under the list, which fold.
#[derive(Clone, Copy)]
enum ShelfId {
    Snoozed,
    Settled,
    Closed,
}

/// What the page is for, while it has nothing to list.
fn overview_empty(look: &Look) -> Div {
    let theme = look.theme;
    div()
        .debug_selector(|| "overview-empty".into())
        .size_full()
        .min_h(px(320.))
        .px_6()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .child(
            svg()
                .path("icons/computer.svg")
                .size(px(22.))
                .text_color(rgb(theme.muted)),
        )
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .child("No agents to look at"),
        )
        .child(
            div()
                .max_w(px(460.))
                .text_center()
                .text_color(rgb(theme.muted))
                .child(
                    "The Overview gathers the agents on every host this window connects \
                     to: those waiting on you and failed runs first, then finished work \
                     ready for review, working agents, and idle ones. Snooze or settle a \
                     row to put it aside; an agent you close waits under Closed to be \
                     reopened.",
                ),
        )
}

fn heading(look: &Look, label: &'static str, count: Option<usize>) -> Div {
    let theme = look.theme;
    div()
        .pt_4()
        .pb_1()
        .px_2()
        .flex()
        .items_center()
        .gap_2()
        .text_size(look.small())
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(theme.subtext()))
        .child(label)
        .when_some(count, |heading, count| {
            heading.child(div().text_color(rgb(theme.muted)).child(count.to_string()))
        })
}

fn tag(look: &Look, text: &'static str) -> Div {
    let theme = look.theme;
    let color = look.status(AgentStatus::Done);
    div()
        .flex_none()
        .px(px(6.))
        .rounded_full()
        .bg(rgb(crate::config::mix(theme.background, color, 18)))
        .text_color(rgb(color))
        .text_size(look.small())
        .child(text)
}

fn pr_chip(look: &Look, pr: super::model::Pr) -> Div {
    let theme = look.theme;
    let color = match pr.checks {
        Some(Outcome::Failed) => Some(look.status(AgentStatus::Blocked)),
        Some(Outcome::Pending) => Some(look.status(AgentStatus::Working)),
        Some(Outcome::Passed) => Some(theme.ink(theme.palette[2])),
        Some(Outcome::Skipped) | None => None,
    };
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(4.))
        .px(px(5.))
        .rounded(px(corners::SMALL))
        .border_1()
        .border_color(rgb(theme.active))
        .child(
            svg()
                .path("icons/pull-request.svg")
                .size(px(11.))
                .flex_none()
                .text_color(rgb(theme.muted)),
        )
        .child(look.mono(format!("#{}", pr.number)))
        .when_some(color, |chip, color| {
            chip.child(div().flex_none().size(px(6.)).rounded_full().bg(rgb(color)))
        })
}
