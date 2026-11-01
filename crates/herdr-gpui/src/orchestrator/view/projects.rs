//! The projects menu in the search row: every project listed, each with how
//! many of the shown list's items it holds and a check while picked, under
//! an "All projects" row. Picking some narrows the list to them; none picked
//! lists everything, as opening a workspace's issues picks its one. A
//! repository browsed to without a checkout can be closed from its row.

use super::{Menu, OrchestratorView, rows::Tab};
use crate::{i18n::fl, orchestrator::RemoteScope};
use gpui::{prelude::*, *};

impl OrchestratorView {
    /// The control's text: all projects, the one picked, or how many are.
    pub(super) fn project_filter_label(&self) -> String {
        let picked: Vec<&super::Repo> = self
            .projects_named()
            .filter(|repo| self.projects.contains(&repo.key))
            .collect();
        match picked.as_slice() {
            [] => fl!("orchestrator-projects-all"),
            [only] => self.project_label(only),
            many => fl!("orchestrator-projects-count", count = many.len()),
        }
    }

    /// The control, while there is more than one project to choose between.
    pub(super) fn render_project_filter_control(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        if self.projects_named().count() < 2 {
            return None;
        }
        let look = &self.look;
        Some(
            self.menu_control(
                "orchestrator-project-filter",
                Menu::Repos,
                self.project_filter_label(),
            )
            .child(look.icon("icons/git-branch.svg", 11., look.theme.subtext()))
            .on_click(cx.listener(|this, _, _, cx| {
                this.toggle_menu(Menu::Repos);
                cx.notify();
            })),
        )
    }

    pub(super) fn render_project_filter_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let look = &self.look;
        let theme = &look.theme;
        let tab = self.tab;
        let check = |on: bool| {
            div().w(px(12.)).flex_none().when(on, |el| {
                el.child(look.icon("icons/check.svg", 12., theme.primary()))
            })
        };
        let all = self
            .menu_row(
                "orchestrator-project-filter-all".into(),
                self.projects.is_empty(),
            )
            .child(check(self.projects.is_empty()))
            .child(fl!("orchestrator-projects-all"))
            .on_click(cx.listener(|this, _, _, cx| {
                this.projects.clear();
                this.refresh_rows();
                cx.notify();
            }));
        let rows = self.projects_named().enumerate().map(|(index, repo)| {
            let key = repo.key.clone();
            let on = self.projects.contains(&key);
            let label = self.project_label(repo);
            let rows = match tab {
                Tab::Issues => &repo.issue_rows,
                Tab::PullRequests => &repo.pull_request_rows,
            };
            let count = rows.iter().filter(|row| row.depth == 0).count();
            let close = matches!(repo.remote, Some(RemoteScope::Repository(_))).then(|| {
                let key = key.clone();
                let selector = format!("orchestrator-project-filter-close-{index}");
                look.icon_button(
                    SharedString::from(selector.clone()),
                    "icons/close.svg",
                    fl!("orchestrator-projects-remove", repository = label.as_str()),
                )
                .debug_selector(move || selector)
                .size(px(18.))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.stop_browsing(&key);
                    cx.notify();
                }))
            });
            self.menu_row(format!("orchestrator-project-filter-{index}").into(), on)
                .child(check(on))
                .child(div().flex_1().min_w_0().truncate().child(label))
                .when(count > 0, |el| {
                    el.child(div().text_color(rgb(theme.muted)).child(count.to_string()))
                })
                .children(close)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !this.projects.remove(&key) {
                        this.projects.insert(key.clone());
                    }
                    this.refresh_rows();
                    cx.notify();
                }))
        });
        deferred(crate::motion::pop_in(
            self.bounded_menu(
                260.,
                "orchestrator-project-filter-frame",
                div()
                    .id("orchestrator-project-filter-list")
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .child(all)
                    .children(rows),
            ),
            "orchestrator-project-filter-menu",
        ))
    }
}
