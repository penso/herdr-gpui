//! The Pull requests list's State menu, which narrows it to one state, and
//! its Projects menu, which browses each Azure DevOps organization's
//! projects and lists a repository picked there without a checkout
//! (`remote`). Both drop down under the search row, as Sort does.

use super::{
    Menu, OrchestratorView,
    inbox::failure_text,
    remote::MAX_BROWSED,
    rows::{Filters, PrState},
};
use crate::{
    config::corners,
    i18n::fl,
    orchestrator::{Provider, RemoteScope, SourceKey, SyncState, azure},
};
use gpui::{prelude::*, *};

/// The most repository rows the Projects menu draws; the rest are counted.
const MAX_ROWS: usize = 400;
/// Where a dropdown starts under the search row, and the room it leaves
/// below itself.
const MENU_TOP: f32 = 120.;
const MENU_GAP: f32 = 16.;

/// A choice of the State menu, from the filters it sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StateChoice {
    /// Open and draft, as the Open toggle lists.
    Open,
    All,
    Only(PrState),
}

impl StateChoice {
    pub(crate) const ALL: [Self; 5] = [
        Self::Open,
        Self::All,
        Self::Only(PrState::Draft),
        Self::Only(PrState::Merged),
        Self::Only(PrState::Closed),
    ];

    pub(crate) fn of(filters: Filters) -> Self {
        match (filters.state, filters.open_only) {
            (Some(state), _) => Self::Only(state),
            (None, true) => Self::Open,
            (None, false) => Self::All,
        }
    }

    pub(crate) fn apply(self, filters: &mut Filters) {
        filters.open_only = self == Self::Open;
        filters.state = match self {
            Self::Only(state) => Some(state),
            Self::Open | Self::All => None,
        };
    }

    pub(crate) fn label(self) -> String {
        match self {
            Self::Open => fl!("orchestrator-browse-state-open"),
            Self::All => fl!("orchestrator-browse-state-all"),
            Self::Only(PrState::Draft) => fl!("orchestrator-browse-state-draft"),
            Self::Only(PrState::Merged) => fl!("orchestrator-browse-state-merged"),
            Self::Only(PrState::Closed) => fl!("orchestrator-browse-state-closed"),
        }
    }

    /// The id its menu row is found by in tests.
    fn id(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::All => "all",
            Self::Only(PrState::Draft) => "draft",
            Self::Only(PrState::Merged) => "merged",
            Self::Only(PrState::Closed) => "closed",
        }
    }
}

impl OrchestratorView {
    /// Opens `menu`, or shuts it when it is the one open.
    pub(super) fn toggle_menu(&mut self, menu: Menu) {
        self.menu = (self.menu != Some(menu)).then_some(menu);
    }

    /// A dropdown's control: its text and a chevron.
    fn menu_control(&self, id: &'static str, menu: Menu, text: String) -> Stateful<Div> {
        let look = &self.look;
        let theme = &look.theme;
        let open = self.menu == Some(menu);
        div()
            .id(id)
            .debug_selector(move || id.to_owned())
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .cursor_pointer()
            .text_size(look.small())
            .text_color(rgb(if open {
                theme.foreground
            } else {
                theme.subtext()
            }))
            .whitespace_nowrap()
            .child(text)
            .child(look.icon("icons/chevron-down.svg", 10., theme.muted))
    }

    pub(super) fn render_state_control(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        self.menu_control(
            "orchestrator-state",
            Menu::State,
            StateChoice::of(self.filters).label(),
        )
        .on_click(cx.listener(|this, _, _, cx| {
            this.toggle_menu(Menu::State);
            cx.notify();
        }))
    }

    /// The Projects control, once an organization is known.
    pub(super) fn render_projects_control(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        if self.organizations().is_empty() {
            return None;
        }
        let look = &self.look;
        Some(
            self.menu_control(
                "orchestrator-projects",
                Menu::Projects,
                fl!("orchestrator-browse-projects"),
            )
            .child(look.icon("icons/azure-devops.svg", 11., look.theme.subtext()))
            .on_click(cx.listener(|this, _, _, cx| {
                this.toggle_menu(Menu::Projects);
                cx.notify();
            })),
        )
    }

    /// A dropdown's frame under the search row.
    fn menu_frame(&self, width: f32) -> Div {
        let theme = &self.look.theme;
        div()
            .absolute()
            .top(px(MENU_TOP))
            .right(px(16.))
            .w(px(width))
            .p_1()
            .flex()
            .flex_col()
            .rounded(px(corners::CONTROL))
            .bg(rgb(theme.surface))
            .border_1()
            .border_color(rgb(theme.active))
            .shadow_lg()
    }

    /// A row of a dropdown, with the wash while it is the one picked.
    fn menu_row(&self, id: SharedString, on: bool) -> Stateful<Div> {
        let theme = &self.look.theme;
        let hover = theme.active;
        let selector = id.to_string();
        div()
            .id(id)
            .debug_selector(move || selector)
            .px_2()
            .h(px(28.))
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(corners::SMALL))
            .cursor_pointer()
            .hover(move |style| style.bg(rgb(hover)))
            .when(on, |el| el.bg(rgb(theme.primary_wash())))
    }

    pub(super) fn render_state_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let picked = StateChoice::of(self.filters);
        deferred(crate::motion::pop_in(
            self.menu_frame(200.)
                .children(StateChoice::ALL.into_iter().map(|choice| {
                    self.menu_row(
                        format!("orchestrator-state-{}", choice.id()).into(),
                        choice == picked,
                    )
                    .child(choice.label())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        choice.apply(&mut this.filters);
                        this.menu = None;
                        this.refresh_rows();
                        cx.notify();
                    }))
                })),
            "orchestrator-state-menu",
        ))
    }

    /// What the Projects menu says of organization `repo` before its
    /// catalog is read: why its read failed, and whether it did, else that
    /// it is reading. A later read that succeeds replaces it.
    pub(super) fn catalog_note(&self, repo: usize) -> (bool, String) {
        let failed = self.repos.get(repo).and_then(|repo| {
            repo.snapshot
                .sources
                .iter()
                .find_map(|status| match &status.state {
                    SyncState::Failed(error) => Some(failure_text(
                        Provider::Azure,
                        &crate::i18n::errors::Localize::localize(error),
                    )),
                    _ => None,
                })
        });
        match failed {
            Some(text) => (true, text),
            None => (false, fl!("orchestrator-browse-projects-loading")),
        }
    }

    pub(super) fn render_projects_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let look = &self.look;
        let theme = &look.theme;
        let browsed = self
            .repos
            .iter()
            .filter(|repo| matches!(repo.remote, Some(RemoteScope::Repository(_))))
            .count();
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut drawn = 0;
        let mut left_out = 0;
        let note = |text: String| {
            div()
                .px_2()
                .py_1()
                .text_size(look.small())
                .text_color(rgb(theme.muted))
                .child(text)
                .into_any_element()
        };
        for organization in self.organizations() {
            let repo = organization.repo;
            rows.push(
                div()
                    .px_2()
                    .h(px(28.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(look.icon("icons/azure-devops.svg", 12., theme.subtext()))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(organization.name.to_owned()),
                    )
                    .child(
                        look.icon_button(
                            SharedString::from(format!("orchestrator-projects-refresh-{repo}")),
                            "icons/refresh.svg",
                            fl!(
                                "orchestrator-browse-projects-refresh",
                                organization = organization.name
                            ),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.refresh_organization(repo);
                            cx.notify();
                        })),
                    )
                    .into_any_element(),
            );
            let Some(catalog) = organization.catalog else {
                let (failed, text) = self.catalog_note(repo);
                let selector = if failed {
                    "orchestrator-projects-failed"
                } else {
                    "orchestrator-projects-loading"
                };
                rows.push(
                    div()
                        .debug_selector(move || selector.to_owned())
                        .child(note(text))
                        .into_any_element(),
                );
                continue;
            };
            if catalog.projects.is_empty() {
                rows.push(note(fl!("orchestrator-browse-projects-empty")));
            }
            for project in &catalog.projects {
                if drawn >= MAX_ROWS {
                    left_out += project.repositories.len();
                    continue;
                }
                rows.push(
                    div()
                        .px_2()
                        .pt_1()
                        .truncate()
                        .text_size(look.small())
                        .text_color(rgb(theme.muted))
                        .child(project.name.clone())
                        .into_any_element(),
                );
                for name in &project.repositories {
                    if drawn >= MAX_ROWS {
                        left_out += 1;
                        continue;
                    }
                    drawn += 1;
                    let source = SourceKey {
                        provider: Provider::Azure,
                        host: azure::HOST.to_owned(),
                        repository: format!("{}/{}/{name}", catalog.organization, project.name),
                    };
                    let on = self.browsed(&source);
                    let id = format!("orchestrator-projects-repo-{}", source.repository);
                    rows.push(
                        self.menu_row(id.into(), on)
                            .pl_4()
                            .child(div().w(px(12.)).flex_none().when(on, |el| {
                                el.child(look.icon("icons/check.svg", 12., theme.primary()))
                            }))
                            .child(div().flex_1().min_w_0().truncate().child(name.clone()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_browsed(source.clone());
                                cx.notify();
                            }))
                            .into_any_element(),
                    );
                }
            }
        }
        if left_out > 0 {
            rows.push(note(fl!(
                "orchestrator-browse-projects-more",
                count = left_out
            )));
        }
        if browsed >= MAX_BROWSED {
            rows.push(note(fl!(
                "orchestrator-browse-projects-full",
                count = MAX_BROWSED
            )));
        }
        // The space below the control bounds it, so a short window scrolls
        // the list rather than cutting the menu off.
        deferred(crate::motion::pop_in(
            div()
                .absolute()
                .top(px(MENU_TOP))
                .bottom(px(MENU_GAP))
                .right(px(16.))
                .w(px(320.))
                .flex()
                .flex_col()
                .child(
                    div()
                        .debug_selector(|| "orchestrator-projects-frame".to_owned())
                        .min_h_0()
                        .max_h(px(420.))
                        .p_1()
                        .flex()
                        .flex_col()
                        .rounded(px(corners::CONTROL))
                        .bg(rgb(theme.surface))
                        .border_1()
                        .border_color(rgb(theme.active))
                        .shadow_lg()
                        .child(
                            div()
                                .id("orchestrator-projects-list")
                                .min_h_0()
                                .overflow_y_scroll()
                                .flex()
                                .flex_col()
                                .children(rows),
                        ),
                ),
            "orchestrator-projects-menu",
        ))
    }
}
