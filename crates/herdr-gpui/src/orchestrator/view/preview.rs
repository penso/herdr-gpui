//! The preview beside the list: the selected item or run at a glance, with
//! the actions it offers. Its left edge drags to resize it, or shut.

use super::{
    Event, OrchestratorView, PreviewDrag,
    list::{diff_squares, source_mark},
    look::{GREEN, RED, age, agent_icon},
    rows::{RunRow, Runs, Tab},
};
use crate::{
    config::corners,
    orchestrator::{Access, Item},
};
use gpui::{prelude::*, *};

/// How much of a description the preview shows.
const EXCERPT: usize = 600;

impl OrchestratorView {
    pub(super) fn render_preview_pane(&self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let look = &self.look;
        let theme = &look.theme;
        let body = match self.tab {
            Tab::Runs => self
                .selected
                .as_deref()
                .and_then(|id| self.snapshot.runs.iter().position(|run| run.id == id))
                .map(|run| self.render_run_preview(run, cx)),
            _ => self
                .selected
                .as_deref()
                .and_then(|key| self.item(key))
                .map(|item| self.render_item_preview(item, cx)),
        };
        div()
            .flex_none()
            .w(px(self.preview_width))
            .h_full()
            .flex()
            .child(
                div()
                    .id("orchestrator-preview-handle")
                    .flex_none()
                    .w(px(6.))
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_l_1()
                    .border_color(rgb(theme.active))
                    .cursor(CursorStyle::ResizeLeftRight)
                    .on_drag(PreviewDrag, |drag, _, _, cx| cx.new(|_| *drag))
                    .child(
                        div()
                            .w(px(2.))
                            .h(px(28.))
                            .rounded_full()
                            .bg(rgb(theme.active)),
                    ),
            )
            .child(
                div()
                    .id("orchestrator-preview")
                    .flex_1()
                    .min_w_0()
                    .overflow_y_scroll()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .children(body),
            )
            .into_any_element()
    }

    fn render_item_preview(&self, item: &Item, cx: &mut Context<Self>) -> AnyElement {
        let look = &self.look;
        let theme = &look.theme;
        let key = item.key.canonical();
        let runs = Runs::new(&self.snapshot.runs, &self.snapshot.sessions, &self.live);
        let latest = runs.of_item(&key).into_iter().next();
        let now = chrono::Utc::now();
        let comments = item
            .activity
            .and_then(|activity| activity.comments)
            .unwrap_or(0);
        let meta = format!(
            "{} \u{00b7} updated {} ago{}",
            item.author.as_deref().unwrap_or("unknown"),
            age(item.updated_at, now),
            if comments > 0 {
                format!(" \u{00b7} {comments} comments")
            } else {
                String::new()
            }
        );
        let excerpt = item
            .description
            .as_deref()
            .map(|text| excerpt(text, EXCERPT))
            .filter(|text| !text.is_empty());
        let writable = self.snapshot.access == Some(Access::ReadWrite);
        let url = item.url.clone();
        let open_key = key.clone();
        let dispatch_key = key.clone();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(source_mark(look, item.key.source.provider))
                    .child(
                        look.mono(item.identifier.clone())
                            .text_color(rgb(theme.muted)),
                    )
                    .child(look.badge(item.state.replace('_', " "), look.state_hue(&item.state)))
                    .when_some(item.priority, |el, priority| {
                        el.child(look.badge(format!("P{priority}"), look.priority_hue(priority)))
                    })
                    .children(
                        item.labels
                            .iter()
                            .take(3)
                            .map(|label| look.chip(label.clone())),
                    ),
            )
            .child(
                div()
                    .text_size(px(look.ui.size + 3.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(item.title.clone()),
            )
            .child(look.muted(meta))
            .when_some(item.pull_request.as_ref(), |el, pr| {
                el.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_1()
                        .child(
                            look.mono(pr.head_ref.clone())
                                .text_color(rgb(theme.subtext())),
                        )
                        .child(look.icon("icons/arrow-right.svg", 11., theme.muted))
                        .child(
                            look.mono(pr.base_ref.clone())
                                .text_color(rgb(theme.subtext())),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_size(look.small())
                        .child(diff_squares(look, pr.additions, pr.deletions))
                        .child(
                            div()
                                .text_color(rgb(look.hue(GREEN)))
                                .child(format!("+{}", pr.additions.unwrap_or(0))),
                        )
                        .child(
                            div()
                                .text_color(rgb(look.hue(RED)))
                                .child(format!("\u{2212}{}", pr.deletions.unwrap_or(0))),
                        ),
                )
            })
            .when(!item.blocked_by.is_empty(), |el| {
                el.child(look.muted(format!("Blocked by {}", item.blocked_by.join(", "))))
            })
            .when_some(latest, |el, latest| {
                el.child(self.run_card_small(&runs, latest, cx))
            })
            .when_some(excerpt, |el, text| {
                el.child(div().text_color(rgb(theme.subtext())).child(text))
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .when(writable, |el| {
                        el.child(
                            look.button(
                                "orchestrator-preview-dispatch",
                                "Dispatch agent",
                                latest.is_none(),
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.open_dispatch(dispatch_key.clone(), window, cx);
                                },
                            )),
                        )
                    })
                    .child(
                        look.button("orchestrator-preview-open", "Details", false)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_detail(open_key.clone(), cx)
                            })),
                    )
                    .when_some(url, |el, url| {
                        el.child(
                            look.icon_button("orchestrator-preview-web", "icons/external.svg")
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    cx.emit(Event::OpenUrl(url.clone()));
                                })),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_run_preview(&self, run: usize, cx: &mut Context<Self>) -> AnyElement {
        let look = &self.look;
        let runs = Runs::new(&self.snapshot.runs, &self.snapshot.sessions, &self.live);
        let Some(status) = runs.status(run) else {
            return div().into_any_element();
        };
        let found = &self.snapshot.runs[run];
        let item = self.item(&found.item_key);
        let key = found.item_key.clone();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_size(px(look.ui.size + 3.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(item.map_or_else(|| found.item_key.clone(), |item| item.title.clone())),
            )
            .child(self.run_card_small(&runs, RunRow { run, status }, cx))
            .when(item.is_some(), |el| {
                el.child(
                    look.button("orchestrator-run-item", "Open the item", false)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.tab = Tab::Issues;
                            this.open_detail(key.clone(), cx);
                        })),
                )
            })
            .into_any_element()
    }

    /// A run in a few lines: agent, host, branch, status, and the agent's
    /// own title while Herdr shows it, with Open when it can be reached.
    pub(super) fn run_card_small(
        &self,
        runs: &Runs<'_>,
        row: RunRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let look = &self.look;
        let theme = &look.theme;
        let run = &runs.runs[row.run];
        let live = runs.live(row.run);
        let color = look.status_hue(row.status);
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
        let title = live.and_then(|agent| agent.title.clone());
        let index = row.run;
        div()
            .p_3()
            .flex()
            .flex_col()
            .gap_1()
            .rounded(px(corners::CONTROL))
            .bg(rgb(theme.surface))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(look.icon(agent_icon(&run.agent), 16., theme.foreground))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(format!("{} on {host}", run.agent)),
                    )
                    .child(look.dot(color, 7.))
                    .child(
                        div()
                            .text_size(look.small())
                            .text_color(rgb(color))
                            .child(row.status.label()),
                    ),
            )
            .when(!branch.is_empty(), |el| {
                el.child(look.mono(branch).text_color(rgb(theme.muted)).truncate())
            })
            .when_some(title, |el, title| {
                el.child(
                    div()
                        .text_size(look.small())
                        .text_color(rgb(theme.subtext()))
                        .truncate()
                        .child(title),
                )
            })
            .when(live.is_some(), |el| {
                el.child(
                    div().pt_1().child(
                        look.button(("orchestrator-open-run", index), "Open agent", true)
                            .on_click(cx.listener(move |this, _, _, cx| this.open_run(index, cx))),
                    ),
                )
            })
            .into_any_element()
    }
}

/// The start of `text`, on one paragraph's worth of characters.
fn excerpt(text: &str, limit: usize) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(limit + 1)
        .collect();
    let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > limit {
        let cut: String = flat.chars().take(limit).collect();
        format!("{}\u{2026}", cut.trim_end())
    } else {
        flat
    }
}
