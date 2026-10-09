//! The inbox: the repository and its sources, the Issues / Pull requests /
//! Runs tabs, search and filters, and the list beside its preview. Lists are
//! virtualized, so thousands of beads cost only the rows on screen.

use super::{
    Event, OrchestratorView, PREVIEW_MAX_SHARE, PREVIEW_MIN, PreviewDrag,
    look::{BLUE, GREEN, Look, MAGENTA, RED, YELLOW, age},
    rows::{Runs, Sort, Tab},
};
use crate::{
    config::corners,
    orchestrator::{Access, Provider, SyncState},
};
use gpui::{prelude::*, *};

impl OrchestratorView {
    pub(super) fn render_inbox(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let preview = self
            .preview_open
            .then(|| self.render_preview_pane(window, cx));
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.render_header(cx))
            .child(self.render_tabs(cx))
            .child(self.render_search_row(window, cx))
            .children(self.render_banners(cx))
            .child(
                div()
                    .id("orchestrator-body")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .on_drag_move(cx.listener(
                        |this, event: &DragMoveEvent<PreviewDrag>, window, cx| {
                            let right = event.bounds.right();
                            let width = f32::from(right - event.event.position.x);
                            let max = f32::from(window.viewport_size().width) * PREVIEW_MAX_SHARE;
                            // Dragged nearly shut, the preview closes.
                            if width < PREVIEW_MIN / 2. {
                                this.preview_open = false;
                            } else {
                                this.preview_width = width.clamp(PREVIEW_MIN, max.max(PREVIEW_MIN));
                            }
                            cx.notify();
                        },
                    ))
                    .child(self.render_list(cx))
                    .children(preview),
            )
            .when(self.sort_open, |el| el.child(self.render_sort_menu(cx)))
            .into_any_element()
    }

    fn render_header(&self, cx: &mut Context<Self>) -> Div {
        let look = &self.look;
        let theme = &look.theme;
        let info = self.snapshot.repo.as_ref();
        let name = info
            .and_then(|info| info.remote.as_ref())
            .map(|remote| remote.source.repository.clone())
            .or_else(|| {
                info.map(|info| {
                    info.main_root
                        .rsplit('/')
                        .next()
                        .unwrap_or(&info.main_root)
                        .to_owned()
                })
            })
            .unwrap_or_else(|| "Reading the repository\u{2026}".into());
        let host = match &self.request.target {
            herdr_client::ConnectTarget::Ssh { target, .. } => target.clone(),
            _ => "local".into(),
        };
        let sources = self
            .snapshot
            .sources
            .iter()
            .enumerate()
            .map(|(index, status)| {
                let (text, color, sign_in) = match &status.state {
                    SyncState::Syncing => ("syncing\u{2026}".to_owned(), look.hue(BLUE), false),
                    SyncState::Synced { count, at } => (
                        format!("{count} \u{00b7} {}", synced(*at)),
                        look.hue(GREEN),
                        false,
                    ),
                    SyncState::Capped { count } => {
                        (format!("first {count}"), look.hue(YELLOW), false)
                    }
                    SyncState::SignIn => ("sign in".to_owned(), look.hue(YELLOW), true),
                    SyncState::Unsupported => ("not supported yet".to_owned(), theme.muted, false),
                    SyncState::Failed(_) => ("failed".to_owned(), look.hue(RED), false),
                };
                let provider = status.key.provider;
                div()
                    .id(("orchestrator-source", index))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py(px(2.))
                    .rounded(px(corners::CONTROL))
                    .border_1()
                    .border_color(rgb(theme.active))
                    .text_size(look.small())
                    .child(match provider {
                        Provider::Github => look
                            .icon("icons/github.svg", 12., theme.subtext())
                            .into_any_element(),
                        Provider::Beads => look.badge("bd", MAGENTA).into_any_element(),
                        Provider::Gitlab => look.badge("gl", RED).into_any_element(),
                    })
                    .child(match provider {
                        Provider::Github => "GitHub",
                        Provider::Beads => "Beads",
                        Provider::Gitlab => "GitLab",
                    })
                    .child(look.dot(color, 6.))
                    .child(div().text_color(rgb(theme.muted)).child(text))
                    .when(sign_in, |el| {
                        el.cursor_pointer()
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(Event::SignIn)))
                    })
            });
        let dispatchable = self.tab != Tab::Runs
            && self.selected.is_some()
            && self.snapshot.access == Some(Access::ReadWrite);
        div()
            .flex_none()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .px_4()
            .py_2()
            .child(look.icon("icons/git-branch.svg", 14., theme.subtext()))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(name))
            .child(look.chip(host))
            .children(sources)
            .child(div().flex_1())
            .child(
                look.icon_button("orchestrator-refresh", "icons/refresh.svg")
                    .on_click(cx.listener(|this, _, _, _| this.refresh())),
            )
            .when(!self.detached, |el| {
                el.child(
                    look.icon_button("orchestrator-window", "icons/window-maximize.svg")
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(Event::OpenWindow))),
                )
            })
            .child(
                look.icon_button("orchestrator-preview", "icons/panel-right.svg")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.preview_open = !this.preview_open;
                        cx.notify();
                    })),
            )
            .when(dispatchable, |el| {
                el.child(
                    look.button("orchestrator-dispatch", "Dispatch agent", true)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if let Some(item) = this.selected.clone() {
                                this.open_dispatch(item, window, cx);
                            }
                        })),
                )
            })
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> Div {
        let look = &self.look;
        let theme = &look.theme;
        let runs = Runs::new(&self.snapshot.runs, &self.snapshot.sessions, &self.live);
        let waiting = super::rows::attention(&runs);
        let counts = [
            self.issue_rows.len(),
            self.pull_request_rows.len(),
            self.snapshot.runs.len(),
        ];
        div()
            .flex_none()
            .flex()
            .items_end()
            .gap_4()
            .px_4()
            .border_b_1()
            .border_color(rgb(theme.active))
            .children(Tab::ALL.into_iter().zip(counts).map(|(tab, count)| {
                let on = tab == self.tab;
                div()
                    .id(SharedString::from(format!(
                        "orchestrator-tab-{}",
                        tab.label()
                    )))
                    .flex()
                    .items_center()
                    .gap_2()
                    .pb(px(6.))
                    .cursor_pointer()
                    .when(on, |el| el.border_b_2().border_color(rgb(theme.primary())))
                    .text_color(rgb(if on { theme.foreground } else { theme.muted }))
                    .child(tab.label())
                    .child(
                        div()
                            .px(px(5.))
                            .rounded_full()
                            .bg(rgb(theme.active))
                            .text_size(look.tiny())
                            .child(count.to_string()),
                    )
                    .when(tab == Tab::Runs && waiting > 0, |el| {
                        el.child(look.dot(look.hue(YELLOW), 6.))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.select_tab(tab, cx)))
            }))
    }

    fn render_search_row(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let look = &self.look;
        let theme = &look.theme;
        let filters = self.filters;
        let mut row = div()
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .py_2()
            .child(self.render_search_field(window, cx));
        let toggle = |id: &'static str, label: &'static str, on: bool, flip: fn(&mut Self)| {
            look.toggle(id, label, on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    flip(this);
                    this.refresh_rows();
                    cx.notify();
                }))
        };
        match self.tab {
            Tab::Issues => {
                row = row
                    .child(toggle(
                        "orchestrator-beads",
                        "Beads",
                        filters.provider == Some(Provider::Beads),
                        |this| {
                            this.filters.provider = match this.filters.provider {
                                Some(Provider::Beads) => None,
                                _ => Some(Provider::Beads),
                            };
                        },
                    ))
                    .child(toggle(
                        "orchestrator-has-run",
                        "Has agent",
                        filters.has_run,
                        |this| {
                            this.filters.has_run = !this.filters.has_run;
                        },
                    ))
                    .child(toggle(
                        "orchestrator-blocked",
                        "Blocked",
                        filters.blocked,
                        |this| {
                            this.filters.blocked = !this.filters.blocked;
                        },
                    ));
            }
            Tab::PullRequests => {
                row = row
                    .child(toggle(
                        "orchestrator-open",
                        "Open",
                        filters.open_only,
                        |this| {
                            this.filters.open_only = !this.filters.open_only;
                        },
                    ))
                    .when(self.login.is_some(), |row| {
                        row.child(toggle("orchestrator-mine", "Mine", self.mine, |this| {
                            this.mine = !this.mine;
                        }))
                    })
                    .child(toggle(
                        "orchestrator-pr-has-run",
                        "Has agent",
                        filters.has_run,
                        |this| {
                            this.filters.has_run = !this.filters.has_run;
                        },
                    ));
            }
            Tab::Runs => {
                row = row.child(toggle(
                    "orchestrator-active",
                    "Active",
                    self.active_runs,
                    |this| {
                        this.active_runs = !this.active_runs;
                    },
                ));
            }
        }
        row.when(self.tab != Tab::Runs, |row| {
            row.child(
                div()
                    .id("orchestrator-sort")
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_1()
                    .cursor_pointer()
                    .text_size(look.small())
                    .text_color(rgb(theme.subtext()))
                    .child(self.sort.label())
                    .child(look.icon("icons/chevron-down.svg", 10., theme.muted))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.sort_open = !this.sort_open;
                        cx.notify();
                    })),
            )
        })
    }

    /// One outlined field, the magnifier inside: the outline turns accent
    /// while the field has the keyboard, and a clear button shows once there
    /// is text.
    fn render_search_field(&self, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let look = &self.look;
        let theme = &look.theme;
        let focused = self.search.read(cx).focus.is_focused(window);
        div()
            .id("orchestrator-search")
            .flex_1()
            .min_w_0()
            .h(px(look.ui.line_height() + 12.))
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .rounded(px(corners::CONTROL))
            .border_1()
            .border_color(rgb(if focused {
                theme.primary()
            } else {
                theme.active
            }))
            .bg(rgb(theme.background))
            .child(look.icon(
                "icons/search.svg",
                13.,
                if focused {
                    theme.foreground
                } else {
                    theme.muted
                },
            ))
            .child(div().flex_1().min_w_0().child(self.search.clone()))
            .when(!self.query.is_empty(), |field| {
                field.child(
                    look.icon_button("orchestrator-search-clear", "icons/close.svg")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.search.update(cx, |search, cx| search.clear(cx));
                            window.focus(&this.search.read(cx).focus.clone(), cx);
                        })),
                )
            })
    }

    fn render_sort_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let look = &self.look;
        let theme = &look.theme;
        deferred(
            div()
                .absolute()
                .top(px(120.))
                .right(px(16.))
                .w(px(200.))
                .p_1()
                .flex()
                .flex_col()
                .rounded(px(corners::CONTROL))
                .bg(rgb(theme.surface))
                .border_1()
                .border_color(rgb(theme.active))
                .shadow_lg()
                .children(Sort::ALL.into_iter().map(|sort| {
                    let on = sort == self.sort;
                    let hover = theme.active;
                    div()
                        .id(SharedString::from(format!(
                            "orchestrator-sort-{}",
                            sort.label()
                        )))
                        .px_2()
                        .h(px(28.))
                        .flex()
                        .items_center()
                        .rounded(px(corners::SMALL))
                        .cursor_pointer()
                        .hover(move |style| style.bg(rgb(hover)))
                        .when(on, |el| el.bg(rgb(theme.primary_wash())))
                        .child(sort.label())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.sort = sort;
                            this.sort_open = false;
                            this.refresh_rows();
                            cx.notify();
                        }))
                })),
        )
    }

    fn render_banners(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let look = &self.look;
        let mut banners = Vec::new();
        if let Some(Access::ReadOnly { found }) = self.snapshot.access {
            banners.push(banner(
                look,
                RED,
                "icons/lock.svg",
                format!(
                    "Shared data is schema {found}; this herdr-gpui understands up to {}. Read-only until updated.",
                    crate::orchestrator::SCHEMA_VERSION
                ),
            ));
        }
        if let Some(error) = &self.snapshot.error {
            banners.push(banner(look, RED, "icons/x.svg", error.to_string()));
        }
        for status in &self.snapshot.sources {
            let text = match &status.state {
                SyncState::Failed(error) => {
                    format!("{}: {error}", provider_name(status.key.provider))
                }
                SyncState::Capped { count } => format!(
                    "{} has more than {count} items; the newest are shown and not cached.",
                    provider_name(status.key.provider)
                ),
                _ => continue,
            };
            let hue = if matches!(status.state, SyncState::Capped { .. }) {
                YELLOW
            } else {
                RED
            };
            banners.push(banner(look, hue, "icons/refresh.svg", text));
        }
        banners
            .into_iter()
            .enumerate()
            .map(|(index, banner)| {
                banner
                    .id(("orchestrator-banner", index))
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, _| this.refresh()))
                    .into_any_element()
            })
            .collect()
    }
}

/// "synced now", "synced 4m ago": when a source last finished a sync.
fn synced(at: chrono::DateTime<chrono::Utc>) -> String {
    match age(Some(at), chrono::Utc::now()).as_str() {
        "now" => "synced now".into(),
        ago => format!("synced {ago} ago"),
    }
}

fn provider_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Github => "GitHub",
        Provider::Beads => "Beads",
        Provider::Gitlab => "GitLab",
    }
}

pub(super) fn banner(look: &Look, hue: usize, icon: &'static str, text: String) -> Div {
    div()
        .flex_none()
        .mx_4()
        .mb_2()
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .py_2()
        .rounded(px(corners::CONTROL))
        .bg(rgb(look.wash(hue)))
        .child(look.icon(icon, 13., look.hue(hue)))
        .child(div().flex_1().min_w_0().text_size(look.small()).child(text))
}
