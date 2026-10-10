//! The inbox's list: its column heads and virtualized rows for issues, pull
//! requests, and runs, each a click to select and a double-click to open.

use super::{
    OrchestratorView,
    look::{BLUE, CYAN, GREEN, Look, MAGENTA, RED, YELLOW, age, agent_icon},
    rows::{Group, ItemRow, RunLine, Tab},
};
use crate::{
    fonts::StyledFont,
    orchestrator::{Owner, Provider, SyncState},
};
use gpui::{prelude::*, *};

/// Row heights at the usual text size; [`Look::row`] grows them with it.
const ISSUE_ROW: f32 = 30.;
const PULL_REQUEST_ROW: f32 = 40.;
const RUN_ROW: f32 = 34.;

impl OrchestratorView {
    pub(super) fn render_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let look = &self.look;
        let (count, head): (usize, &[(&str, Option<f32>)]) = match self.tab {
            Tab::Issues => (
                self.issue_rows.len(),
                &[
                    ("AGE", Some(32.)),
                    ("SRC", Some(22.)),
                    ("RUN", Some(30.)),
                    ("TITLE", None),
                    ("AUTHOR", Some(110.)),
                    ("\u{1f4ac}", Some(34.)),
                    ("STATE", Some(86.)),
                ],
            ),
            Tab::PullRequests => (
                self.pull_request_rows.len(),
                &[
                    ("PR", Some(44.)),
                    ("RUN", Some(30.)),
                    ("TITLE \u{00b7} AUTHOR \u{00b7} BRANCH", None),
                    ("DIFF", Some(44.)),
                    ("ACTIVITY", Some(70.)),
                    ("STATE", Some(60.)),
                    ("", Some(28.)),
                ],
            ),
            Tab::Runs => (self.run_lines.len(), &[]),
        };
        let list = if count == 0 {
            self.render_empty(cx).into_any_element()
        } else {
            uniform_list(
                "orchestrator-rows",
                count,
                cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                    range.map(|index| this.render_row(index, cx)).collect()
                }),
            )
            .track_scroll(&self.scroll)
            .flex_1()
            .min_h_0()
            .into_any_element()
        };
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .when(!head.is_empty(), |el| el.child(column_head(look, head)))
            .child(list)
            .into_any_element()
    }

    fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let look = &self.look;
        let what = match self.tab {
            Tab::Issues => "open issues",
            Tab::PullRequests => "pull requests",
            Tab::Runs => "runs",
        };
        let loading = self.snapshot.error.is_none()
            && (self.snapshot.repo.is_none()
                || self
                    .snapshot
                    .sources
                    .iter()
                    .any(|status| matches!(status.state, SyncState::Syncing)));
        let text = if loading && self.query.trim().is_empty() {
            "Loading\u{2026}".to_owned()
        } else if self.query.trim().is_empty() {
            format!("No {what} yet")
        } else {
            format!("No {what} match \u{201c}{}\u{201d}", self.query.trim())
        };
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .child(look.icon("icons/search.svg", 22., look.theme.muted))
            .child(text)
            .when(!self.query.trim().is_empty(), |el| {
                el.child(
                    look.button("orchestrator-clear", "Clear filter", false)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.search.update(cx, |search, cx| search.clear(cx));
                        })),
                )
            })
    }

    fn render_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        match self.tab {
            Tab::Issues => self.render_issue_row(index, cx),
            Tab::PullRequests => self.render_pull_request_row(index, cx),
            Tab::Runs => self.render_run_line(index, cx),
        }
    }

    /// A row's shell: selection, click to select, double-click to open.
    fn row_shell(
        &self,
        id: (&'static str, usize),
        key: String,
        height: f32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.look.theme;
        let selected = self.selected.as_deref() == Some(key.as_str());
        let hover = theme.surface;
        div()
            .id(id)
            .w_full()
            .h(px(height))
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .cursor_pointer()
            .hover(move |style| style.bg(rgb(hover)))
            .when(selected, |el| el.bg(rgb(theme.primary_wash())))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    this.selected = Some(key.clone());
                    if event.click_count == 2 {
                        this.open_detail(key.clone(), cx);
                    }
                    cx.notify();
                }),
            )
    }

    fn render_issue_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.issue_rows.get(index) else {
            return div().h(px(self.look.row(ISSUE_ROW))).into_any_element();
        };
        let look = &self.look;
        let theme = &look.theme;
        let item = &self.snapshot.items[row.item];
        let key = item.key.canonical();
        let now = chrono::Utc::now();
        let expander = row.children.map(|open| {
            let key = key.clone();
            div()
                .id(("orchestrator-expand", index))
                .child(look.icon(
                    if open {
                        "icons/chevron-down.svg"
                    } else {
                        "icons/chevron-right.svg"
                    },
                    10.,
                    theme.muted,
                ))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_children(key.clone(), cx);
                    }),
                )
        });
        self.row_shell(
            ("orchestrator-issue", index),
            key,
            self.look.row(ISSUE_ROW),
            cx,
        )
        .child(
            div()
                .flex_none()
                .w(px(32.))
                .text_size(look.small())
                .text_color(rgb(theme.muted))
                .child(age(item.updated_at, now)),
        )
        .child(
            div()
                .flex_none()
                .w(px(22.))
                .child(source_mark(look, item.key.source.provider)),
        )
        .child(self.run_mark(row))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap_2()
                .pl(px(f32::from(row.depth) * 18.))
                .child(div().flex_none().w(px(10.)).children(expander))
                .child(
                    look.mono(item.identifier.clone())
                        .flex_none()
                        .text_color(rgb(theme.muted)),
                )
                .when_some(item.priority, |el, priority| {
                    el.child(look.badge(format!("P{priority}"), look.priority_hue(priority)))
                })
                .child(div().min_w_0().truncate().child(item.title.clone()))
                .children(
                    item.labels
                        .iter()
                        .take(2)
                        .map(|label| look.chip(label.clone())),
                )
                .when(!item.blocked_by.is_empty(), |el| {
                    el.child(look.icon("icons/lock.svg", 11., look.hue(YELLOW)))
                }),
        )
        .child(
            div()
                .flex_none()
                .w(px(110.))
                .truncate()
                .text_size(look.small())
                .text_color(rgb(theme.subtext()))
                .child(item.author.clone().unwrap_or_default()),
        )
        .child(
            div()
                .flex_none()
                .w(px(34.))
                .text_size(look.small())
                .text_color(rgb(theme.muted))
                .child(
                    item.activity
                        .and_then(|activity| activity.comments)
                        .filter(|count| *count > 0)
                        .map(|count| count.to_string())
                        .unwrap_or_default(),
                ),
        )
        .child(
            div()
                .flex_none()
                .w(px(86.))
                .flex()
                .child(look.badge(item.state.replace('_', " "), look.state_hue(&item.state))),
        )
        .into_any_element()
    }

    fn render_pull_request_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.pull_request_rows.get(index) else {
            return div()
                .h(px(self.look.row(PULL_REQUEST_ROW)))
                .into_any_element();
        };
        let look = &self.look;
        let theme = &look.theme;
        let item = &self.snapshot.items[row.item];
        let Some(pr) = &item.pull_request else {
            return div()
                .h(px(self.look.row(PULL_REQUEST_ROW)))
                .into_any_element();
        };
        let fork = pr
            .head_repository
            .as_deref()
            .is_some_and(|head| head != item.key.source.repository);
        let activity = item.activity.unwrap_or_default();
        self.row_shell(
            ("orchestrator-pr", index),
            item.key.canonical(),
            self.look.row(PULL_REQUEST_ROW),
            cx,
        )
        .child(
            look.mono(item.identifier.clone())
                .flex_none()
                .w(px(44.))
                .text_color(rgb(theme.muted)),
        )
        .child(self.run_mark(row))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(div().truncate().child(item.title.clone()))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_size(look.tiny())
                        .text_color(rgb(theme.muted))
                        .child(
                            div()
                                .flex_none()
                                .text_color(rgb(theme.subtext()))
                                .child(item.author.clone().unwrap_or_default()),
                        )
                        .child("\u{00b7}")
                        .child(look.icon("icons/git-branch.svg", 10., theme.muted))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_font(&look.mono)
                                .child(pr.head_ref.clone()),
                        )
                        .when(fork, |el| el.child(look.badge("fork", CYAN))),
                ),
        )
        .child(diff_squares(look, pr.additions, pr.deletions))
        .child(
            div()
                .flex_none()
                .w(px(70.))
                .text_size(look.tiny())
                .text_color(rgb(theme.muted))
                .child(format!(
                    "\u{1f4ac}{} \u{2387}{}",
                    activity.comments.unwrap_or(0),
                    activity.commits.unwrap_or(0)
                )),
        )
        .child(
            div()
                .flex_none()
                .w(px(60.))
                .flex()
                .child(look.badge(item.state.clone(), look.state_hue(&item.state))),
        )
        .child(
            div()
                .flex_none()
                .w(px(28.))
                .text_size(look.small())
                .text_color(rgb(theme.muted))
                .child(age(item.updated_at, chrono::Utc::now())),
        )
        .into_any_element()
    }

    fn render_run_line(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let look = &self.look;
        let theme = &look.theme;
        let Some(line) = self.run_lines.get(index) else {
            return div().h(px(self.look.row(RUN_ROW))).into_any_element();
        };
        let row = match line {
            RunLine::Group { group, count } => {
                let hue = match group {
                    Group::Attention => look.hue(YELLOW),
                    Group::Working => look.hue(GREEN),
                    Group::Idle => look.hue(BLUE),
                    Group::Finished => look.hue(CYAN),
                };
                return div()
                    .w_full()
                    .h(px(self.look.row(RUN_ROW)))
                    .flex()
                    .items_end()
                    .gap_2()
                    .px_4()
                    .pb_1()
                    .child(look.dot(hue, 7.))
                    .child(look.label(group.label()))
                    .child(look.muted(count.to_string()))
                    .into_any_element();
            }
            RunLine::Run(row) => *row,
        };
        let run = &self.snapshot.runs[row.run];
        let item = self.item(&run.item_key);
        let color = look.status_hue(row.status);
        let host = run
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.host.clone())
            .unwrap_or_else(|| "local".into());
        let run_index = row.run;
        self.row_shell(
            ("orchestrator-run", index),
            run.id.clone(),
            self.look.row(RUN_ROW),
            cx,
        )
        .child(look.icon(agent_icon(&run.agent), 15., theme.foreground))
        .child(
            look.mono(item.map_or_else(
                || super::rows::key_label(&run.item_key),
                |item| item.identifier.clone(),
            ))
            .flex_none()
            .w(px(80.))
            .text_color(rgb(theme.muted)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .when(item.is_none(), |el| el.text_color(rgb(theme.muted)))
                .child(item.map_or_else(
                    || "Not listed (closed or filtered)".to_owned(),
                    |item| item.title.clone(),
                )),
        )
        .child(div().flex_none().w(px(100.)).flex().child(look.chip(host)))
        .child(
            look.mono(
                run.workspace
                    .as_ref()
                    .map(|workspace| workspace.branch.clone())
                    .unwrap_or_default(),
            )
            .flex_none()
            .w(px(220.))
            .truncate()
            .text_color(rgb(theme.subtext())),
        )
        .child(
            div()
                .flex_none()
                .w(px(100.))
                .flex()
                .items_center()
                .gap_1()
                .child(look.dot(color, 7.))
                .child(
                    div()
                        .text_size(look.small())
                        .text_color(rgb(color))
                        .child(row.status.label()),
                ),
        )
        .child(
            div()
                .flex_none()
                .w(px(36.))
                .text_size(look.small())
                .text_color(rgb(theme.muted))
                .child(age(Some(run.started_at), chrono::Utc::now())),
        )
        .child(div().flex_none().w(px(100.)).flex().child(look.badge(
            run.owner.as_str(),
            if run.owner == Owner::HerdrGpui {
                BLUE
            } else {
                CYAN
            },
        )))
        .child(
            look.icon_button(("orchestrator-run-open", index), "icons/arrow-right.svg")
                .debug_selector(move || format!("orchestrator-run-open-{index}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.open_run(run_index, cx);
                    }),
                ),
        )
        .into_any_element()
    }

    /// The agent and status of an item's newest run.
    fn run_mark(&self, row: &ItemRow) -> Div {
        let look = &self.look;
        div()
            .flex_none()
            .w(px(30.))
            .flex()
            .items_center()
            .gap_1()
            .when_some(row.run, |el, run| {
                let agent = &self.snapshot.runs[run.run].agent;
                el.child(look.icon(agent_icon(agent), 13., look.theme.foreground))
                    .child(look.dot(look.status_hue(run.status), 7.))
            })
    }
}

pub(super) fn source_mark(look: &Look, provider: Provider) -> AnyElement {
    match provider {
        Provider::Github => look
            .icon("icons/github.svg", 13., look.theme.subtext())
            .into_any_element(),
        Provider::Beads => look.badge("bd", MAGENTA).into_any_element(),
        Provider::Gitlab => look.badge("gl", RED).into_any_element(),
    }
}

fn column_head(look: &Look, cells: &[(&str, Option<f32>)]) -> Div {
    let theme = &look.theme;
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap_2()
        .h(px(24.))
        .px_4()
        .border_b_1()
        .border_color(rgb(theme.active))
        .text_size(look.tiny())
        .text_color(rgb(theme.muted))
        .children(cells.iter().map(|(text, width)| {
            match width {
                Some(width) => div()
                    .flex_none()
                    .w(px(*width))
                    .whitespace_nowrap()
                    .child(SharedString::from(text.to_string())),
                None => div().flex_1().child(SharedString::from(text.to_string())),
            }
        }))
}

/// Five squares, green for added lines and red for removed, in proportion.
pub(super) fn diff_squares(look: &Look, additions: Option<u64>, deletions: Option<u64>) -> Div {
    let (added, removed) = (additions.unwrap_or(0), deletions.unwrap_or(0));
    let total = added + removed;
    let green = (added * 5 + total / 2)
        .checked_div(total)
        .map_or(0, |green| green.min(5));
    let known = additions.is_some() || deletions.is_some();
    div()
        .flex_none()
        .w(px(44.))
        .flex()
        .items_center()
        .gap(px(1.))
        .children((0..5).map(move |index| {
            let color = if !known {
                look.theme.active
            } else if index < green {
                look.hue(GREEN)
            } else {
                look.hue(RED)
            };
            div().size(px(7.)).rounded(px(1.)).bg(rgb(color))
        }))
}
