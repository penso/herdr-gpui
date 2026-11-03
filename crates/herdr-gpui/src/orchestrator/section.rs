//! The activity bar's Pull requests and Issues sections: one orchestrator
//! view the window owns, listing every repository the connected hosts'
//! workspaces hold, drawn across the window in place of its terminals on the
//! list the section names. The menus and `herdr-gpui orchestrator` open it
//! narrowed to one workspace's project.

use super::{List, OrchestratorView, ViewId};
use crate::{HerdrWindow, browser::OrchestratorRepo, i18n::fl, sidebar::ActivityPanel};
use gpui::{prelude::*, *};

impl HerdrWindow {
    /// Keeps the shown section on the focused workspace's view, making it
    /// when the workspace has none and the shown host can run the
    /// orchestrator's scripts. Called from the tick and when a section is
    /// picked; a new view, or a new section, gets its list picked here.
    pub(crate) fn sync_activity_section(&mut self, cx: &mut Context<Self>) {
        let Some(list) = self.activity_section().and_then(list_of) else {
            return;
        };
        let id = self.sections_view(cx);
        let changed =
            Some(id) != self.activity.section_view || self.activity.section_list != Some(list);
        self.activity.section_view = Some(id);
        let project = self.activity.section_project.take();
        if !changed && project.is_none() {
            return;
        }
        let Some(view) = self.orchestrator_view(id).cloned() else {
            return;
        };
        let specs = self.repo_specs();
        view.update(cx, |view, cx| {
            view.set_repos(specs, cx);
            if changed {
                view.show_list(list, cx);
            }
            if let Some(key) = project {
                view.only_project(key, cx);
            }
        });
        self.activity.section_list = Some(list);
        cx.notify();
    }

    /// Makes the sections' view before either section is shown, so the bar
    /// counts open pull requests and issues from the start rather than from
    /// the first click. Waits for a repository to list: until a host
    /// connects there is nothing to sync.
    pub(crate) fn prepare_activity_sections(&mut self, cx: &mut Context<Self>) {
        let made = self
            .activity
            .section_view
            .is_some_and(|id| self.orchestrators.contains_key(&id));
        if made || !self.activity_bar_shown() || self.repo_specs().is_empty() {
            return;
        }
        // The tick hands it the repositories and accounts right after.
        self.activity.section_view = Some(self.sections_view(cx));
    }

    /// Shows the focused workspace's issues.
    pub(crate) fn open_orchestrator(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_, workspace)) = self.browser_key() else {
            self.show_flash(
                crate::window::Flash::warning(fl!("orchestrator-no-workspace")),
                cx,
            );
            return;
        };
        self.open_orchestrator_in(workspace, None, window, cx);
    }

    /// Shows the issues of workspace `workspace`, listing the repository of
    /// `checkout` when given, else of its focused pane. Another workspace
    /// than the focused one is switched to first.
    pub(crate) fn open_orchestrator_in(
        &mut self,
        workspace: String,
        checkout: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The sections are the bar's: without it there is nowhere to show them.
        if !self.activity_bar_shown() {
            self.show_flash(
                crate::window::Flash::warning(fl!("orchestrator-activity-bar-off")),
                cx,
            );
            return;
        }
        if !self.orchestrator_host() {
            self.show_flash(
                crate::window::Flash::warning(fl!("orchestrator-needs-host")),
                cx,
            );
            return;
        }
        // The section lists every project, narrowed to the one asked for:
        // `checkout`'s when it is a listed repository, else the workspace's.
        let asked = checkout
            .and_then(|path| OrchestratorRepo::new(path).ok())
            .and_then(|repo| {
                self.repo_specs()
                    .into_iter()
                    .find(|spec| spec.request.checkout == repo.checkout)
                    .map(|spec| spec.key)
            });
        self.activity.section_project = asked.or_else(|| self.workspace_repo_key(&workspace));
        self.dismiss_menu(window, cx);
        if self
            .browser_key()
            .is_none_or(|(_, focused)| focused != workspace)
        {
            self.navigate(crate::NavigationTarget::Workspace(&workspace), cx);
        }
        self.show_activity_panel(ActivityPanel::Issues, window, cx);
    }

    /// Back to Workspaces after the section's view opened something that
    /// shows there, such as a run's pane or a page, so it is not left hidden
    /// behind the section. Another view's events leave the section alone.
    pub(crate) fn leave_activity_section(&mut self, id: ViewId, cx: &mut Context<Self>) {
        if self.activity_section().and_then(list_of).is_none()
            || self.activity.section_view != Some(id)
        {
            return;
        }
        let left = self.activity.panel;
        self.activity.pick(ActivityPanel::Workspaces);
        self.activity.return_to = Some(left);
        self.save_chrome();
        cx.notify();
    }

    fn section_view(&self) -> Option<Entity<OrchestratorView>> {
        self.orchestrator_view(self.activity.section_view?).cloned()
    }

    /// Gives the section's view the keyboard.
    pub(crate) fn focus_activity_section(&self, window: &mut Window, cx: &mut App) {
        let Some(view) = self.section_view() else {
            return;
        };
        let focus = view.read(cx).focus_handle().clone();
        window.focus(&focus, cx);
    }

    /// The shown Pull requests or Issues section across the window's body.
    pub(crate) fn render_orchestrator_section(&self) -> Option<AnyElement> {
        let section = div()
            .id("activity-section")
            .debug_selector(|| "activity-section".into())
            .flex()
            .flex_1()
            .size_full()
            .min_h_0()
            .min_w_0();
        if let Some(view) = self.section_view() {
            return Some(section.child(view).into_any_element());
        }
        let theme = &self.theme;
        Some(
            section
                .flex_col()
                .items_center()
                .justify_center()
                .text_color(rgb(theme.muted))
                .child(
                    div()
                        .debug_selector(|| "activity-section-empty".into())
                        .child(self.orchestrator_unavailable()),
                )
                .into_any_element(),
        )
    }

    /// Why a section has no view to show.
    fn orchestrator_unavailable(&self) -> String {
        if self
            .endpoints
            .iter()
            .any(|endpoint| super::host::scripts_run_on(&endpoint.connection.target))
        {
            fl!("orchestrator-opening")
        } else {
            fl!("orchestrator-needs-host")
        }
    }
}

/// The list a section shows, for the sections that show one.
pub(crate) fn list_of(section: ActivityPanel) -> Option<List> {
    match section {
        ActivityPanel::Issues => Some(List::Issues),
        ActivityPanel::PullRequests => Some(List::PullRequests),
        ActivityPanel::Overview
        | ActivityPanel::Security
        | ActivityPanel::Automations
        | ActivityPanel::Workspaces
        | ActivityPanel::Usage => None,
    }
}
