//! One item's page, in the pane beside a section's list: a header like a
//! security alert's (crumbs, stepping, tags, title, tabs), the status box
//! (`status`) over its conversation or description, its runs, and its raw
//! details.

use super::{
    Detail, DetailTab, Event, OrchestratorView,
    dispatch::Confirm,
    list::source_mark,
    look::{BLUE, CYAN, age, agent_icon},
    rows::Tab,
    rows::{RunRow, Runs},
};
use crate::{
    config::{corners, mix},
    fonts::StyledFont,
    orchestrator::{Access, Item, Owner},
    release_notes,
};
use gpui::{prelude::*, *};

impl OrchestratorView {
    pub(super) fn render_detail(
        &mut self,
        detail: Detail,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(item) = self.item(&detail.key).cloned() else {
            self.detail = None;
            return div().into_any_element();
        };
        let runs = Runs::new(&self.snapshot().runs, &self.snapshot().sessions, &self.live);
        let item_runs = runs.of_item(&detail.key);
        let body = match detail.tab {
            // The status box leads the item's first page: the conversation
            // for a pull request, the description for an issue.
            DetailTab::Description => div()
                .id("orchestrator-description-page")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p_4()
                .flex()
                .flex_col()
                .gap_4()
                .when(item.pull_request.is_none(), |el| {
                    el.child(self.render_status_box(&item, &runs, &item_runs, cx))
                })
                .child(self.render_meta(&item))
                .child(self.render_description(&item))
                .into_any_element(),
            DetailTab::Conversation => div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex_none()
                        .px_4()
                        .pt_4()
                        .child(self.render_status_box(&item, &runs, &item_runs, cx)),
                )
                .child(self.render_conversation(cx))
                .into_any_element(),
            DetailTab::Agent => self.render_runs(&runs, &item_runs, cx),
            DetailTab::Details => self.render_details(&item, cx),
            DetailTab::Checks => self.render_checks(&item),
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.render_detail_header(&item, detail.tab, item_runs.len(), cx))
            .child(body)
            .into_any_element()
    }

    fn render_detail_header(
        &self,
        item: &Item,
        active: DetailTab,
        runs: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let look = &self.look;
        let theme = &look.theme;
        let (list, list_icon, list_tab) = match item.pull_request {
            Some(_) => ("Pull requests", "icons/git-branch.svg", Tab::PullRequests),
            None => ("Issues", "icons/note.svg", Tab::Issues),
        };
        let repo = self
            .snapshot()
            .repo
            .as_ref()
            .and_then(|info| info.remote.as_ref())
            .map(|remote| remote.source.repository.clone())
            .unwrap_or_else(|| item.key.source.repository.clone());
        let pull_request = item.pull_request.is_some();
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap_2()
            .px_4()
            .pt_2()
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        self.breadcrumbs(&repo, list, list_icon, list_tab, item, cx)
                            .flex_1(),
                    )
                    .child(self.stepper(item, cx)),
            )
            .child(
                div()
                    .px_1()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(self.render_tags(item))
                    .child(
                        div()
                            .debug_selector(|| "orchestrator-detail-title".into())
                            .text_size(px(look.ui.size + 4.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(item.title.clone()),
                    ),
            )
            .child(div().px_1().pt_1().flex().gap_4().children(
                DetailTab::tabs(pull_request).iter().copied().map(|tab| {
                    let on = tab == active;
                    let label = match tab {
                        DetailTab::Agent if runs > 0 => format!("{}  {runs}", tab.label()),
                        _ => tab.label().to_owned(),
                    };
                    div()
                        .id(SharedString::from(format!(
                            "orchestrator-detail-{}",
                            tab.label()
                        )))
                        .pb_2()
                        .border_b_2()
                        .border_color(rgb(if on {
                            theme.primary()
                        } else {
                            theme.background
                        }))
                        .cursor_pointer()
                        .text_color(rgb(if on { theme.foreground } else { theme.muted }))
                        .child(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(detail) = &mut this.detail {
                                detail.tab = tab;
                            }
                            this.merge_open = false;
                            this.follow_pull_request();
                            cx.notify();
                        }))
                }),
            ))
    }

    /// Where the item sits in the list, stepping to its neighbors, the
    /// item on the web, and close.
    fn stepper(&self, item: &Item, cx: &mut Context<Self>) -> Div {
        let look = &self.look;
        let keys = self.keys();
        let key = item.key.canonical();
        let at = keys.iter().position(|listed| *listed == key);
        let url = item.url.clone();
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .text_size(look.small())
            .text_color(rgb(look.theme.muted))
            .children(at.map(|at| div().pr_1().child(format!("{} of {}", at + 1, keys.len()))))
            .child(
                look.icon_button(
                    "orchestrator-detail-previous",
                    "icons/chevron-up.svg",
                    "Previous",
                )
                .on_click(cx.listener(|this, _, _, cx| this.step_detail(-1, cx))),
            )
            .child(
                look.icon_button("orchestrator-detail-next", "icons/chevron-down.svg", "Next")
                    .debug_selector(|| "orchestrator-detail-next".into())
                    .on_click(cx.listener(|this, _, _, cx| this.step_detail(1, cx))),
            )
            .when_some(url, |el, url| {
                el.child(
                    look.icon_button(
                        "orchestrator-detail-web",
                        "icons/external.svg",
                        "Open in browser",
                    )
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(Event::OpenUrl(url.clone()));
                    })),
                )
            })
            .child(
                look.icon_button(
                    "orchestrator-detail-close",
                    "icons/close.svg",
                    "Close details",
                )
                .debug_selector(|| "orchestrator-detail-close".into())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.detail = None;
                    this.merge_open = false;
                    cx.notify();
                })),
            )
    }

    /// Opens the item `by` rows away from the open one in the shown list.
    pub(super) fn step_detail(&mut self, by: isize, cx: &mut Context<Self>) {
        let keys = self.keys();
        let Some(at) = self
            .detail
            .as_ref()
            .and_then(|detail| keys.iter().position(|key| *key == detail.key))
        else {
            return;
        };
        let next = at.saturating_add_signed(by).min(keys.len() - 1);
        if next == at {
            return;
        }
        let key = keys[next].clone();
        self.selected = Some(key.clone());
        self.scroll
            .scroll_to_item(self.line_of(&key), ScrollStrategy::Center);
        self.open_detail(key, cx);
    }

    /// The item's state, priority, branches, and parent, as tags.
    fn render_tags(&self, item: &Item) -> Div {
        let look = &self.look;
        let theme = &look.theme;
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(look.tag(
                item.state.replace('_', " "),
                look.hue(look.state_hue(&item.state)),
            ))
            .when_some(item.priority, |el, priority| {
                el.child(look.tag(
                    format!("P{priority}"),
                    look.hue(look.priority_hue(priority)),
                ))
            })
            .when_some(item.pull_request.as_ref(), |el, pr| {
                el.child(look.icon("icons/git-branch.svg", 11., theme.muted))
                    .child(
                        look.mono(pr.head_ref.clone())
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(theme.subtext())),
                    )
                    .child(look.icon("icons/arrow-right.svg", 11., theme.muted))
                    .child(
                        look.mono(pr.base_ref.clone())
                            .text_color(rgb(theme.subtext())),
                    )
            })
            .when_some(item.parent_id.clone(), |el, parent| {
                el.child(look.muted(format!("child of {parent}")))
            })
    }

    /// Back, then where the page sits: the repository, the list it came
    /// from, and the item itself. Every step but the last goes back there.
    fn breadcrumbs(
        &self,
        repo: &str,
        list: &'static str,
        list_icon: &'static str,
        list_tab: Tab,
        item: &Item,
        cx: &mut Context<Self>,
    ) -> Div {
        let look = &self.look;
        let theme = &look.theme;
        let size = look.ui.size;
        let separator = || look.icon("icons/chevron-right.svg", size * 0.7, theme.muted);
        let step = |id: &'static str, icon: &'static str, text: String| {
            let hover = theme.active;
            div()
                .id(id)
                .flex_none()
                .flex()
                .items_center()
                .gap_1()
                .px_1()
                .rounded(px(corners::SMALL))
                .cursor_pointer()
                .hover(move |style| style.bg(rgb(hover)))
                .text_color(rgb(theme.subtext()))
                .child(look.icon(icon, size * 0.9, theme.subtext()))
                .child(text)
        };
        div()
            .flex()
            .items_center()
            .gap_1()
            .min_w_0()
            .text_size(look.size())
            .child(
                look.icon_button("orchestrator-back", "icons/arrow-left.svg", "Back")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.detail = None;
                        cx.notify();
                    })),
            )
            .child(
                step(
                    "orchestrator-crumb-repo",
                    "icons/github.svg",
                    repo.to_owned(),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.detail = None;
                    this.select_tab(Tab::Issues, cx);
                })),
            )
            .child(separator())
            .child(
                step("orchestrator-crumb-list", list_icon, list.to_owned()).on_click(cx.listener(
                    move |this, _, _, cx| {
                        this.detail = None;
                        this.select_tab(list_tab, cx);
                    },
                )),
            )
            .child(separator())
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_1()
                    .text_color(rgb(theme.foreground))
                    .child(source_mark(look, item.key.source.provider))
                    .child(div().min_w_0().truncate().child(item.identifier.clone())),
            )
    }

    fn render_description(&self, item: &Item) -> AnyElement {
        let look = &self.look;
        let body = item.description.as_deref().unwrap_or("");
        let content = if body.trim().is_empty() {
            look.muted("No description.").into_any_element()
        } else {
            let lines = self.description.lines(body);
            release_notes::render("orchestrator-description", &lines, &look.theme, &look.mono)
                .into_any_element()
        };
        div()
            .id("orchestrator-description")
            .min_w_0()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Description"))
            .child(content)
            .into_any_element()
    }

    fn render_runs(
        &self,
        runs: &Runs<'_>,
        item_runs: &[RunRow],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let look = &self.look;
        let theme = &look.theme;
        let mut column = div()
            .id("orchestrator-runs")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_4()
            .flex()
            .flex_col()
            .gap_3();
        if item_runs.is_empty() {
            column = column.child(look.muted("No agent has worked on this yet."));
        }
        for (position, row) in item_runs.iter().enumerate() {
            if position == 1 {
                column = column.child(look.label("EARLIER RUNS"));
            }
            column = column.child(self.run_card(runs, *row, position == 0, cx));
        }
        column
            .child(
                div()
                    .text_size(look.small())
                    .text_color(rgb(theme.muted))
                    .child("Status comes from each host's Herdr snapshot. Runs started by agent-launcher are shown here; it alone updates them."),
            )
            .into_any_element()
    }

    fn run_card(&self, runs: &Runs<'_>, row: RunRow, current: bool, cx: &mut Context<Self>) -> Div {
        let look = &self.look;
        let theme = &look.theme;
        let run = &runs.runs[row.run];
        let color = look.status_hue(row.status);
        let live = runs.live(row.run);
        let host = run
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.host.clone())
            .unwrap_or_else(|| "local".into());
        let branch = run
            .workspace
            .as_ref()
            .map(|workspace| workspace.branch.clone())
            .unwrap_or_default();
        let line = live
            .and_then(|agent| agent.title.clone())
            .or_else(|| run.message.clone());
        let index = row.run;
        div()
            .flex()
            .rounded(px(corners::PANEL))
            .border_1()
            .border_color(rgb(if current {
                mix(theme.active, color, 40)
            } else {
                theme.active
            }))
            .overflow_hidden()
            .child(div().flex_none().w(px(3.)).bg(rgb(color)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(look.icon(agent_icon(&run.agent), 16., theme.foreground))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(run.agent.clone()),
                            )
                            .child(look.chip(host))
                            .child(look.badge(
                                run.owner.as_str(),
                                if run.owner == Owner::HerdrGpui {
                                    BLUE
                                } else {
                                    CYAN
                                },
                            ))
                            .child(div().flex_1())
                            .child(look.dot(color, 8.))
                            .child(div().text_color(rgb(color)).child(row.status.label()))
                            .child(look.muted(age(run.started(), chrono::Utc::now()))),
                    )
                    .when(!branch.is_empty(), |el| {
                        el.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(look.icon("icons/git-branch.svg", 11., theme.muted))
                                .child(look.mono(branch).text_color(rgb(theme.subtext()))),
                        )
                    })
                    .when_some(line, |el, line| {
                        el.child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded(px(corners::SMALL))
                                .bg(rgb(theme.surface))
                                .text_font(&look.mono)
                                .text_size(look.small())
                                .text_color(rgb(theme.subtext()))
                                .truncate()
                                .child(line),
                        )
                    })
                    .child(self.run_controls(run, index, current, live.is_some(), cx)),
            )
    }

    fn render_details(&self, item: &Item, cx: &mut Context<Self>) -> AnyElement {
        let look = &self.look;
        let theme = &look.theme;
        let mut rows: Vec<(&'static str, String)> = vec![
            ("Key", item.key.canonical()),
            ("Identifier", item.identifier.clone()),
            ("State", item.state.clone()),
        ];
        if let Some(url) = &item.url {
            rows.push(("URL", url.clone()));
        }
        if let Some(pr) = &item.pull_request {
            rows.push(("Base", format!("{} @ {}", pr.base_ref, pr.base_sha)));
            rows.push(("Head", format!("{} @ {}", pr.head_ref, pr.head_sha)));
            if let Some(head) = &pr.head_repository {
                rows.push(("Head repository", head.clone()));
            }
        }
        if let Some(activity) = item.activity {
            let count =
                |value: Option<u64>| value.map_or_else(|| "\u{2014}".to_owned(), |v| v.to_string());
            rows.push(("Comments", count(activity.comments)));
            rows.push(("Review comments", count(activity.review_comments)));
            rows.push(("Commits", count(activity.commits)));
        }
        let time = |value: Option<chrono::DateTime<chrono::Utc>>| {
            value.map_or_else(|| "\u{2014}".to_owned(), |value| value.to_rfc3339())
        };
        let deletable = item.key.source.provider == crate::orchestrator::Provider::Beads
            && self.snapshot().access == Some(Access::ReadWrite);
        let (source, id) = (item.key.source.clone(), item.key.native_id.clone());
        rows.push(("Created", time(item.created_at)));
        rows.push(("Updated", time(item.updated_at)));
        div()
            .id("orchestrator-details")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_4()
            .flex()
            .flex_col()
            .gap_1()
            .children(rows.into_iter().map(|(key, value)| {
                div()
                    .flex()
                    .gap_3()
                    .child(
                        div()
                            .flex_none()
                            .w(px(140.))
                            .text_color(rgb(theme.muted))
                            .child(key),
                    )
                    .child(look.mono(value).min_w_0())
            }))
            .when(deletable, |el| {
                el.child(div().h_4()).child(
                    div().flex().child(
                        look.button("orchestrator-delete-bead", "Delete bead\u{2026}", false)
                            .text_color(rgb(look.hue(super::look::RED)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.ask(
                                    Confirm::DeleteBead {
                                        source: source.clone(),
                                        id: id.clone(),
                                    },
                                    cx,
                                );
                            })),
                    ),
                )
            })
            .into_any_element()
    }

    /// A run card's buttons: open its pane while Herdr shows it; for the
    /// newest run herdr-gpui owns, a message field, Send, and Stop; Remove
    /// for any run herdr-gpui owns that has a worktree or failed.
    fn run_controls(
        &self,
        run: &crate::orchestrator::Run,
        index: usize,
        current: bool,
        live: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let look = &self.look;
        let theme = &look.theme;
        let own = run.owner == Owner::HerdrGpui;
        let writable = self.snapshot().access == Some(Access::ReadWrite);
        let branch = run
            .workspace
            .as_ref()
            .map(|workspace| workspace.branch.clone())
            .unwrap_or_default();
        let (stop_run, remove_run, agent) = (run.id.clone(), run.id.clone(), run.agent.clone());
        // A failed run with no worktree is still forgotten through Remove.
        let removable =
            run.workspace.is_some() || run.state == crate::orchestrator::RunState::Failed;
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .when(live, |el| {
                el.child(
                    look.button(("orchestrator-card-open", index), "Open pane", true)
                        .on_click(cx.listener(move |this, _, _, cx| this.open_run(index, cx))),
                )
            })
            .when(own && writable && current && live, |el| {
                el.child(
                    div()
                        .flex_1()
                        .min_w(px(160.))
                        .px_2()
                        .py(px(4.))
                        .rounded(px(corners::CONTROL))
                        .border_1()
                        .border_color(rgb(theme.active))
                        .child(self.message.clone()),
                )
                .child(
                    look.button(("orchestrator-card-send", index), "Send", false)
                        .on_click(cx.listener(|this, _, _, cx| this.send_message(cx))),
                )
                .child(
                    look.button(("orchestrator-card-stop", index), "Stop", false)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.ask(
                                Confirm::Stop {
                                    run: stop_run.clone(),
                                    agent: agent.clone(),
                                },
                                cx,
                            );
                        })),
                )
            })
            .when(own && writable && removable, |el| {
                el.child(
                    look.icon_button(
                        ("orchestrator-card-remove", index),
                        "icons/trash.svg",
                        "Remove",
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.ask(
                            Confirm::Remove {
                                run: remove_run.clone(),
                                branch: branch.clone(),
                            },
                            cx,
                        );
                    })),
                )
            })
    }
}
