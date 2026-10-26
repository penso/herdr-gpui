//! A section's page: the repository and its sources, search and filters,
//! and the list, with an item's details in a pane beside it once opened.
//! Issues keep their filters in a column that stays folded until asked for,
//! the active ones showing as chips. Lists are virtualized, so thousands of
//! beads cost only the rows on screen.

use super::{
    Event, OrchestratorView, PaneDrag, Resize,
    look::{BLUE, GREEN, Look, MAGENTA, RED, YELLOW},
    rows::{Sort, Tab},
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
        let pane = self
            .detail
            .clone()
            .map(|detail| self.render_pane(detail, window, cx));
        let filters =
            (self.tab == Tab::Issues && self.filters_open).then(|| self.render_facets(cx));
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.render_header(cx))
            .child(self.render_search_row(window, cx))
            .children(self.render_pills(cx))
            .children(self.render_chips(cx))
            .children(self.render_banners(cx))
            .child(
                div()
                    .id("orchestrator-body")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .on_drag_move(cx.listener(
                        |this, event: &DragMoveEvent<PaneDrag>, window, cx| {
                            match Resize::of(event, window) {
                                Resize::Close => this.detail = None,
                                Resize::Width(width) => this.pane_width = width,
                            }
                            cx.notify();
                        },
                    ))
                    .children(filters)
                    .child(self.render_list(cx))
                    .children(pane),
            )
            .when(self.sort_open, |el| el.child(self.render_sort_menu(cx)))
            .into_any_element()
    }

    /// The open item's page beside the list; its left edge drags to resize
    /// it, or shut.
    fn render_pane(
        &mut self,
        detail: super::Detail,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let handle = super::pane::handle(&self.look.theme, "orchestrator-pane-handle");
        div()
            .debug_selector(|| "orchestrator-pane".into())
            .flex_none()
            .w(px(self.pane_width))
            .h_full()
            .flex()
            .child(handle)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(self.render_detail(detail, window, cx)),
            )
    }

    /// How many of the Issues filters narrow the list.
    pub(super) fn active_filters(&self) -> usize {
        usize::from(self.filters.provider.is_some())
            + usize::from(self.filters.has_run)
            + usize::from(self.filters.blocked)
            + usize::from(self.label.is_some())
    }

    /// The Issues filters' column: state, and the labels the open issues
    /// carry, most used first. Backends and projects are the pills'.
    fn render_facets(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let look = &self.look;
        let theme = &look.theme;
        // Counted across every repository listed.
        let open: Vec<&crate::orchestrator::Item> = self
            .repos
            .iter()
            .flat_map(|repo| repo.snapshot.items.iter())
            .filter(|item| item.pull_request.is_none() && !super::rows::closed(item))
            .collect();
        let mut labels: Vec<(String, usize)> = Vec::new();
        for label in open.iter().flat_map(|item| &item.labels) {
            match labels.iter_mut().find(|(name, _)| name == label) {
                Some((_, count)) => *count += 1,
                None => labels.push((label.clone(), 1)),
            }
        }
        labels.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        labels.truncate(FACET_LABELS);
        let hover = theme.surface;
        let option = |id: SharedString, label: SharedString, count: Option<usize>, on: bool| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py(px(3.))
                .rounded(px(corners::SMALL))
                .cursor_pointer()
                .hover(move |style| style.bg(rgb(hover)))
                .when(on, |el| el.bg(rgb(theme.primary_wash())))
                .child(
                    div()
                        .flex_none()
                        .size(px(11.))
                        .rounded(px(2.))
                        .border_1()
                        .border_color(rgb(if on { theme.primary() } else { theme.muted }))
                        .when(on, |el| el.bg(rgb(theme.primary()))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(look.small())
                        .child(label),
                )
                .children(count.map(|count| look.muted(count.to_string())))
        };
        let heading = |text: &'static str| look.label(text).pt_2().pb_1();
        let has_run = open
            .iter()
            .filter(|item| {
                let key = item.key.canonical();
                self.repos
                    .iter()
                    .flat_map(|repo| repo.snapshot.runs.iter())
                    .any(|run| run.item_key == key)
            })
            .count();
        let blocked = open
            .iter()
            .filter(|item| !item.blocked_by.is_empty() || item.state == "blocked")
            .count();
        div()
            .id("orchestrator-facets")
            .debug_selector(|| "orchestrator-facets".into())
            .flex_none()
            .w(px(FACETS_WIDTH))
            .h_full()
            .overflow_y_scroll()
            .px_3()
            .py_2()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(rgb(theme.active))
            .child(heading("STATE"))
            .child(
                option(
                    "orchestrator-facet-run".into(),
                    "Has agent".into(),
                    Some(has_run).filter(|count| *count > 0),
                    self.filters.has_run,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.filters.has_run = !this.filters.has_run;
                    this.refresh_rows();
                    cx.notify();
                })),
            )
            .child(
                option(
                    "orchestrator-facet-blocked".into(),
                    "Blocked".into(),
                    Some(blocked).filter(|count| *count > 0),
                    self.filters.blocked,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.filters.blocked = !this.filters.blocked;
                    this.refresh_rows();
                    cx.notify();
                })),
            )
            .when(!labels.is_empty(), |el| {
                el.child(heading("LABELS"))
                    .children(labels.into_iter().map(|(label, count)| {
                        let on = self.label.as_deref() == Some(label.as_str());
                        let picked = label.clone();
                        option(
                            format!("orchestrator-facet-label-{label}").into(),
                            label.into(),
                            Some(count),
                            on,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.label = (this.label.as_deref() != Some(picked.as_str()))
                                .then(|| picked.clone());
                            this.refresh_rows();
                            cx.notify();
                        }))
                    }))
            })
    }

    /// While the Issues filters are folded, the ones in use as chips that
    /// clear them.
    fn render_chips(&self, cx: &mut Context<Self>) -> Option<Div> {
        if self.tab != Tab::Issues || self.filters_open || self.active_filters() == 0 {
            return None;
        }
        let look = &self.look;
        let theme = &look.theme;
        let chip = |id: &'static str, text: String, clear: fn(&mut Self)| {
            div()
                .id(id)
                .flex_none()
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .py(px(1.))
                .rounded_full()
                .bg(rgb(theme.primary_wash()))
                .cursor_pointer()
                .text_size(look.small())
                .child(text)
                .child(look.icon("icons/close.svg", 9., theme.muted))
                .on_click(cx.listener(move |this, _, _, cx| {
                    clear(this);
                    this.refresh_rows();
                    cx.notify();
                }))
        };
        let mut row = div()
            .debug_selector(|| "orchestrator-chips".into())
            .flex_none()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .px_4()
            .pb_2()
            .child(look.muted("Showing only"));
        if let Some(provider) = self.filters.provider {
            row = row.child(chip(
                "orchestrator-chip-source",
                provider_name(provider).into(),
                |this| {
                    this.filters.provider = None;
                },
            ));
        }
        if self.filters.has_run {
            row = row.child(chip("orchestrator-chip-run", "Has agent".into(), |this| {
                this.filters.has_run = false;
            }));
        }
        if self.filters.blocked {
            row = row.child(chip(
                "orchestrator-chip-blocked",
                "Blocked".into(),
                |this| {
                    this.filters.blocked = false;
                },
            ));
        }
        if let Some(label) = &self.label {
            row = row.child(chip("orchestrator-chip-label", label.clone(), |this| {
                this.label = None;
            }));
        }
        Some(row)
    }

    fn render_header(&self, cx: &mut Context<Self>) -> Div {
        let look = &self.look;
        let theme = &look.theme;
        let listed: Vec<_> = self.projects_named().collect();
        let title = match listed.as_slice() {
            [] => "No repositories yet".to_owned(),
            [only] => only.name(),
            many => format!("{} projects", many.len()),
        };
        // Each backend once, across every repository, with the items each
        // repository holds from it for a sync that has not reported a count.
        let mut providers: Vec<(Provider, Vec<(&SyncState, usize)>)> = Vec::new();
        for repo in &self.repos {
            for status in &repo.snapshot.sources {
                let held = repo
                    .snapshot
                    .items
                    .iter()
                    .filter(|item| item.key.source == status.key)
                    .count();
                let state = (&status.state, held);
                match providers
                    .iter_mut()
                    .find(|(provider, _)| *provider == status.key.provider)
                {
                    Some((_, states)) => states.push(state),
                    None => providers.push((status.key.provider, vec![state])),
                }
            }
        }
        let github = providers
            .iter()
            .any(|(provider, _)| *provider == Provider::Github);
        let sources = providers
            .into_iter()
            .enumerate()
            .map(|(index, (provider, states))| {
                let (state, text) = source_summary(&states);
                let sign_in = state == SourceState::SignIn;
                let indicator = match state {
                    SourceState::Syncing => spinner(
                        ("orchestrator-source-spin", index).into(),
                        10.,
                        look.hue(BLUE),
                    ),
                    SourceState::Failed => look.dot(look.hue(RED), 6.).into_any_element(),
                    SourceState::SignIn => look.dot(look.hue(YELLOW), 6.).into_any_element(),
                    SourceState::Synced => look.dot(look.hue(GREEN), 6.).into_any_element(),
                    SourceState::Unsupported => look.dot(theme.muted, 6.).into_any_element(),
                };
                source_chip(look, ("orchestrator-source", index), provider)
                    .child(indicator)
                    .when(!text.is_empty(), |el| {
                        el.child(div().text_color(rgb(theme.muted)).child(text))
                    })
                    .when(sign_in, |el| {
                        el.cursor_pointer()
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(Event::SignIn)))
                    })
            });
        // A repository without a GitHub remote has no GitHub source, which
        // reads like a missing account without this.
        let no_github = (!github && !self.repos.is_empty()).then(|| {
            source_chip(look, ("orchestrator-source-none", 0), Provider::Github)
                .child(div().text_color(rgb(theme.muted)).child("no GitHub remote"))
        });
        let dispatchable =
            self.selected.is_some() && self.snapshot().access == Some(Access::ReadWrite);
        div()
            .flex_none()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .px_4()
            .py_2()
            .child(look.icon("icons/git-branch.svg", 14., theme.subtext()))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
            .children(sources)
            .children(no_github)
            .child(div().flex_1())
            .child(self.render_refresh(cx))
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

    /// Refresh, spinning and inert while a sync it would ask for runs.
    fn render_refresh(&self, cx: &mut Context<Self>) -> AnyElement {
        let look = &self.look;
        if !self.refreshing() {
            return look
                .icon_button("orchestrator-refresh", "icons/refresh.svg", "Refresh")
                .debug_selector(|| "orchestrator-refresh".into())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.ask_refresh();
                    cx.notify();
                }))
                .into_any_element();
        }
        div()
            .id("orchestrator-refresh")
            .debug_selector(|| "orchestrator-refresh-busy".into())
            .flex_none()
            .size(px(24.))
            .flex()
            .items_center()
            .justify_center()
            .child(spinner(
                "orchestrator-refresh-spin".into(),
                14.,
                look.theme.muted,
            ))
            .into_any_element()
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
                let active = self.active_filters();
                let open = self.filters_open;
                row = row.child(
                    div()
                        .id("orchestrator-filters")
                        .debug_selector(|| "orchestrator-filters".into())
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap_1()
                        .px_2()
                        .py(px(3.))
                        .rounded(px(corners::CONTROL))
                        .border_1()
                        .border_color(rgb(if open { theme.primary() } else { theme.active }))
                        .when(open, |el| el.bg(rgb(theme.primary_wash())))
                        .cursor_pointer()
                        .text_size(look.small())
                        .child(look.icon("icons/panel-left.svg", 12., theme.subtext()))
                        .child("Filters")
                        .when(active > 0, |el| {
                            el.child(
                                div()
                                    .px(px(5.))
                                    .rounded_full()
                                    .bg(rgb(theme.primary()))
                                    .text_color(rgb(theme.text_on(theme.primary())))
                                    .text_size(look.tiny())
                                    .child(active.to_string()),
                            )
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.filters_open = !this.filters_open;
                            cx.notify();
                        })),
                );
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
                    .when(
                        self.repos.iter().any(|repo| repo.reading_login().is_some()),
                        |row| {
                            row.child(toggle("orchestrator-mine", "Mine", self.mine, |this| {
                                this.mine = !this.mine;
                            }))
                        },
                    )
                    .child(toggle(
                        "orchestrator-pr-has-run",
                        "Has agent",
                        filters.has_run,
                        |this| {
                            this.filters.has_run = !this.filters.has_run;
                        },
                    ));
            }
        }
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
    }

    /// One outlined field, the magnifier inside: the outline turns accent
    /// while the field has the keyboard, and a clear button shows once there
    /// is text.
    fn render_search_field(&self, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let look = &self.look;
        let focused = self.search.read(cx).focus.is_focused(window);
        look.search_frame("orchestrator-search", focused)
            .child(div().flex_1().min_w_0().child(self.search.clone()))
            .when(!self.query.is_empty(), |field| {
                field.child(
                    look.icon_button(
                        "orchestrator-search-clear",
                        "icons/close.svg",
                        "Clear search",
                    )
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
        deferred(crate::motion::pop_in(
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
            "orchestrator-sort-menu",
        ))
    }

    fn render_banners(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let look = &self.look;
        let mut banners = Vec::new();
        let named = self.projects_named().count() > 1;
        for repo in self.projects_named() {
            // With several projects listed, each says whose trouble it is.
            let of = |text: String| {
                if named {
                    format!("{}: {text}", repo.name())
                } else {
                    text
                }
            };
            let snapshot = &repo.snapshot;
            if let Some(Access::ReadOnly { found }) = snapshot.access {
                banners.push(banner(
                    look,
                    RED,
                    "icons/lock.svg",
                    of(format!(
                        "Shared data is schema {found}; this herdr-gpui understands up to {}. Read-only until updated.",
                        crate::orchestrator::SCHEMA_VERSION
                    )),
                ));
            }
            if let Some(error) = &snapshot.error {
                banners.push(banner(look, RED, "icons/x.svg", of(error.to_string())));
            }
            for status in &snapshot.sources {
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
                banners.push(banner(look, hue, "icons/refresh.svg", of(text)));
            }
        }
        banners
            .into_iter()
            .enumerate()
            .map(|(index, banner)| {
                banner
                    .debug_selector(|| "orchestrator-banner".into())
                    .id(("orchestrator-banner", index))
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.ask_refresh();
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect()
    }
}

/// The filters' column width, and the most labels it offers.
const FACETS_WIDTH: f32 = 200.;
/// One turn of the busy Refresh icon.
const SPIN: std::time::Duration = std::time::Duration::from_millis(900);
const FACET_LABELS: usize = 12;

/// The busy Refresh icon, turning, also standing for a source's sync.
fn spinner(id: ElementId, size: f32, color: u32) -> AnyElement {
    svg()
        .path("icons/refresh.svg")
        .size(px(size))
        .flex_none()
        .text_color(rgb(color))
        .with_animation(
            id,
            Animation::new(crate::motion::duration(SPIN)).repeat(),
            |icon, delta| icon.with_transformation(Transformation::rotate(percentage(delta))),
        )
        .into_any_element()
}

/// What a backend's chip shows across every repository reading it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SourceState {
    Syncing,
    Failed,
    SignIn,
    Synced,
    Unsupported,
}

/// A backend's state and count from each repository's `(state, held)`,
/// where `held` is how many of its items that repository shows now, counted
/// for a first sync that has no count of its own yet. A sync keeps the count
/// the chip showed, with a spinner for the dot; a failure or missing sign-in
/// says so instead. When it synced is left to the log.
pub(super) fn source_summary(states: &[(&SyncState, usize)]) -> (SourceState, String) {
    let any = |wanted: fn(&SyncState) -> bool| states.iter().any(|(state, _)| wanted(state));
    let syncing = any(|state| matches!(state, SyncState::Syncing { .. }));
    if !syncing {
        if any(|state| matches!(state, SyncState::Failed(_))) {
            return (SourceState::Failed, "failed".to_owned());
        }
        if any(|state| matches!(state, SyncState::SignIn)) {
            return (SourceState::SignIn, "sign in".to_owned());
        }
    }
    let counted = any(|state| state.count().is_some());
    let count: usize = states
        .iter()
        .map(|(state, held)| match state {
            SyncState::Syncing { count: None } => *held,
            state => state.count().unwrap_or(0),
        })
        .sum();
    // A first sync has no count until it holds something.
    let text = if counted || count > 0 {
        count.to_string()
    } else {
        String::new()
    };
    if syncing {
        (SourceState::Syncing, text)
    } else if counted {
        (SourceState::Synced, text)
    } else {
        (SourceState::Unsupported, "not supported yet".to_owned())
    }
}

pub(super) fn provider_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Github => "GitHub",
        Provider::Beads => "Beads",
        Provider::Gitlab => "GitLab",
    }
}

/// A backend's chip: its mark and name, for its state to follow.
fn source_chip(look: &Look, id: (&'static str, usize), provider: Provider) -> Stateful<Div> {
    let theme = &look.theme;
    div()
        .id(id)
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
        .child(provider_mark(look, provider))
        .child(provider_name(provider))
}

/// A backend's mark: GitHub's logo, else a short colored tag.
pub(super) fn provider_mark(look: &Look, provider: Provider) -> AnyElement {
    match provider {
        Provider::Github => look
            .icon("icons/github.svg", 12., look.theme.subtext())
            .into_any_element(),
        Provider::Beads => look.badge("bd", MAGENTA).into_any_element(),
        Provider::Gitlab => look.badge("gl", RED).into_any_element(),
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
