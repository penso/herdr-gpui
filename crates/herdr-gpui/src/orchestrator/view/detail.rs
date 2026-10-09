//! One item's page: its description (Markdown, drawn as text), its runs, and
//! its raw details, with the overview in a column beside the description.

use super::{
    Detail, DetailTab, Event, OrchestratorView,
    dispatch::Confirm,
    list::source_mark,
    look::{BLUE, CYAN, age, agent_icon},
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
        let runs = Runs::new(&self.snapshot.runs, &self.snapshot.sessions, &self.live);
        let item_runs = runs.of_item(&detail.key);
        let body = match detail.tab {
            DetailTab::Description => div()
                .flex_1()
                .min_h_0()
                .flex()
                .child(self.render_description(&item))
                .child(self.render_overview(&item, &runs, &item_runs, cx))
                .into_any_element(),
            DetailTab::Agent => self.render_runs(&runs, &item_runs, cx),
            DetailTab::Details => self.render_details(&item, cx),
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
        let back = match item.pull_request {
            Some(_) => "Pull requests",
            None => "Issues",
        };
        let key = item.key.canonical();
        let url = item.url.clone();
        let writable = self.snapshot.access == Some(Access::ReadWrite);
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap_2()
            .px_4()
            .pt_3()
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .id("orchestrator-back")
                    .flex()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .text_size(look.small())
                    .text_color(rgb(theme.muted))
                    .child(look.icon("icons/arrow-left.svg", 12., theme.muted))
                    .child(back)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.detail = None;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        look.mono(item.identifier.clone())
                            .flex_none()
                            .text_color(rgb(theme.muted)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(look.ui.size + 5.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(item.title.clone()),
                    )
                    .when(writable, |el| {
                        el.child(
                            look.button(
                                "orchestrator-detail-dispatch",
                                "Dispatch agent",
                                runs == 0,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.open_dispatch(key.clone(), window, cx);
                                },
                            )),
                        )
                    })
                    .when_some(url, |el, url| {
                        el.child(
                            look.icon_button("orchestrator-detail-web", "icons/external.svg")
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    cx.emit(Event::OpenUrl(url.clone()));
                                })),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(source_mark(look, item.key.source.provider))
                    .child(look.badge(item.state.replace('_', " "), look.state_hue(&item.state)))
                    .when_some(item.priority, |el, priority| {
                        el.child(look.badge(format!("P{priority}"), look.priority_hue(priority)))
                    })
                    .when_some(item.pull_request.as_ref(), |el, pr| {
                        el.child(
                            look.mono(pr.head_ref.clone())
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
                    }),
            )
            .child(
                div()
                    .flex()
                    .gap_4()
                    .children(DetailTab::ALL.into_iter().map(|tab| {
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
                            .pb(px(6.))
                            .cursor_pointer()
                            .when(on, |el| el.border_b_2().border_color(rgb(theme.primary())))
                            .text_color(rgb(if on { theme.foreground } else { theme.muted }))
                            .child(label)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(detail) = &mut this.detail {
                                    detail.tab = tab;
                                }
                                cx.notify();
                            }))
                    })),
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
            .flex_1()
            .min_w_0()
            .overflow_y_scroll()
            .p_4()
            .child(content)
            .into_any_element()
    }

    fn render_overview(
        &self,
        item: &Item,
        runs: &Runs<'_>,
        item_runs: &[RunRow],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let look = &self.look;
        let theme = &look.theme;
        let row = |key: &'static str, value: AnyElement| {
            div()
                .flex()
                .gap_2()
                .text_size(look.small())
                .child(
                    div()
                        .flex_none()
                        .w(px(84.))
                        .text_color(rgb(theme.muted))
                        .child(key),
                )
                .child(div().flex_1().min_w_0().flex().child(value))
        };
        let text = |value: String| div().min_w_0().truncate().child(value).into_any_element();
        let now = chrono::Utc::now();
        let source = match item.key.source.provider {
            crate::orchestrator::Provider::Github => {
                format!("GitHub \u{00b7} {}", item.key.source.repository)
            }
            crate::orchestrator::Provider::Beads => "Beads".to_owned(),
            crate::orchestrator::Provider::Gitlab => {
                format!("GitLab \u{00b7} {}", item.key.source.repository)
            }
        };
        div()
            .id("orchestrator-overview")
            .flex_none()
            .w(px(300.))
            .overflow_y_scroll()
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .border_l_1()
            .border_color(rgb(theme.active))
            .child(look.label("OVERVIEW"))
            .child(row("Source", text(source)))
            .child(row(
                "Status",
                look.badge(item.state.replace('_', " "), look.state_hue(&item.state))
                    .into_any_element(),
            ))
            .when_some(item.priority, |el, priority| {
                el.child(row(
                    "Priority",
                    look.badge(format!("P{priority}"), look.priority_hue(priority))
                        .into_any_element(),
                ))
            })
            .when_some(item.parent_id.clone(), |el, parent| {
                el.child(row("Parent", look.mono(parent).into_any_element()))
            })
            .when(!item.blocked_by.is_empty(), |el| {
                el.child(row(
                    "Blocked by",
                    look.mono(item.blocked_by.join(", ")).into_any_element(),
                ))
            })
            .when_some(item.author.clone(), |el, author| {
                el.child(row("Author", text(author)))
            })
            .child(row(
                "Updated",
                text(format!("{} ago", age(item.updated_at, now))),
            ))
            .when(!item.labels.is_empty(), |el| {
                el.child(row(
                    "Labels",
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_1()
                        .children(item.labels.iter().map(|label| look.chip(label.clone())))
                        .into_any_element(),
                ))
            })
            .when_some(item_runs.first().copied(), |el, latest| {
                el.child(div().h_2())
                    .child(look.label("LATEST RUN"))
                    .child(self.run_card_small(runs, latest, cx))
            })
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
                            .child(look.muted(age(Some(run.started_at), chrono::Utc::now()))),
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
            && self.snapshot.access == Some(Access::ReadWrite);
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
    /// for any run herdr-gpui owns that has a worktree.
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
        let writable = self.snapshot.access == Some(Access::ReadWrite);
        let branch = run
            .workspace
            .as_ref()
            .map(|workspace| workspace.branch.clone())
            .unwrap_or_default();
        let (stop_run, remove_run, agent) = (run.id.clone(), run.id.clone(), run.agent.clone());
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
            .when(own && writable && run.workspace.is_some(), |el| {
                el.child(
                    look.icon_button(("orchestrator-card-remove", index), "icons/trash.svg")
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
