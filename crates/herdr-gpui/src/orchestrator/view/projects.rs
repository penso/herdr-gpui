//! The projects menu in the search row. Azure DevOps lists by project, not by
//! the worker that read it: an organization's own pull requests span all
//! its projects, so each Azure DevOps project its items name, or its catalog
//! holds, is an entry under its organization, and picking one shows its rows
//! whichever checkout or organization listed them. Every other checkout is
//! an entry by its repository's name. Each shows how many of the shown
//! list's items it holds and a check while picked, under "All projects";
//! none picked lists everything. A repository browsed to without a checkout
//! is listed under its project, where it can be closed.

use super::{Menu, OrchestratorView, repo::Repo, rows::Tab};
use crate::{
    i18n::fl,
    orchestrator::{Item, Provider, RemoteScope, SourceKey},
};
use gpui::{prelude::*, *};
use std::collections::{HashMap, HashSet};

/// What an Azure DevOps project's pick key starts with, apart from every
/// checkout's key.
const AZURE_PROJECT: &str = "azure-project\u{1f}";
/// The most entries the menu draws.
const MAX_ENTRIES: usize = 300;

/// The key Azure DevOps source `source`'s project is picked by.
pub(super) fn azure_project(source: &SourceKey) -> Option<String> {
    if source.provider != Provider::Azure {
        return None;
    }
    let mut parts = source.repository.splitn(3, '/');
    let (organization, project) = (parts.next()?, parts.next()?);
    Some(format!("{AZURE_PROJECT}{organization}/{project}"))
}

/// Whether `key` picks an Azure DevOps project rather than a checkout.
pub(super) fn is_azure_project(key: &str) -> bool {
    key.starts_with(AZURE_PROJECT)
}

/// One row of the menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Entry {
    /// A checkout that is not Azure DevOps's, picked by its key.
    Repo { key: String, label: String },
    /// An Azure DevOps organization's heading.
    Organization(String),
    /// One of its projects, picked by its [`azure_project`] key.
    Project { key: String, name: String },
    /// A repository browsed to without a checkout, which closes.
    Browsed { key: String, name: String },
}

/// Organizations by name, each with its projects and, under each, the
/// repositories browsed there as `(key, name)`.
type Organizations = Vec<(String, Vec<(String, Vec<(String, String)>)>)>;

/// Adds `project` of `organization` to `azure`, with `browsed` under it.
fn add(
    azure: &mut Organizations,
    organization: &str,
    project: &str,
    browsed: Option<(String, String)>,
) {
    let at = match azure.iter().position(|(name, _)| name == organization) {
        Some(at) => at,
        None => {
            azure.push((organization.to_owned(), Vec::new()));
            azure.len() - 1
        }
    };
    let projects = &mut azure[at].1;
    let at = match projects.iter().position(|(name, _)| name == project) {
        Some(at) => at,
        None => {
            projects.push((project.to_owned(), Vec::new()));
            projects.len() - 1
        }
    };
    projects[at].1.extend(browsed);
}

impl OrchestratorView {
    /// Whether `item`, as `repo` lists it, is among the projects picked.
    pub(super) fn picked(&self, repo: &Repo, item: &Item) -> bool {
        self.projects.is_empty()
            || match azure_project(&item.key.source) {
                Some(project) => self.projects.contains(&project),
                None => self.projects.contains(&repo.key),
            }
    }

    /// The pick that lists only checkout `key`: its Azure DevOps project
    /// when it is one's, else itself.
    pub(super) fn pick_of(&self, key: &str) -> Option<String> {
        let repo = self.repos.iter().find(|repo| repo.key == key)?;
        let remote = repo
            .snapshot
            .repo
            .as_ref()
            .and_then(|info| info.remote.as_ref());
        Some(
            remote
                .and_then(|remote| azure_project(&remote.source))
                .unwrap_or_else(|| key.to_owned()),
        )
    }

    /// The menu's entries: checkouts first, then each organization's
    /// projects by name, with what was browsed under its project.
    pub(super) fn project_entries(&self) -> Vec<Entry> {
        let mut entries: Vec<Entry> = self
            .projects_named()
            .filter(|repo| !repo.reads_azure())
            .map(|repo| Entry::Repo {
                key: repo.key.clone(),
                label: self.project_label(repo),
            })
            .collect();
        let mut azure: Organizations = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for repo in &self.repos {
            let remote = repo
                .snapshot
                .repo
                .as_ref()
                .and_then(|info| info.remote.as_ref());
            let sources = remote
                .map(|remote| &remote.source)
                .into_iter()
                .chain(repo.snapshot.items.iter().map(|item| &item.key.source));
            for source in sources {
                if source.provider != Provider::Azure || !seen.insert(&source.repository) {
                    continue;
                }
                let mut parts = source.repository.splitn(3, '/');
                if let (Some(organization), Some(project)) = (parts.next(), parts.next()) {
                    add(&mut azure, organization, project, None);
                }
            }
            if let Some(catalog) = &repo.snapshot.catalog {
                for project in &catalog.projects {
                    add(&mut azure, &catalog.organization, &project.name, None);
                }
            }
            if let Some(RemoteScope::Repository(source)) = &repo.remote {
                let mut parts = source.repository.splitn(3, '/');
                if let (Some(organization), Some(project), Some(name)) =
                    (parts.next(), parts.next(), parts.next())
                {
                    let browsed = (repo.key.clone(), name.to_owned());
                    add(&mut azure, organization, project, Some(browsed));
                }
            }
        }
        azure.sort_by_key(|(name, _)| name.to_lowercase());
        for (organization, mut projects) in azure {
            projects.sort_by_key(|(name, _)| name.to_lowercase());
            entries.push(Entry::Organization(organization.clone()));
            for (project, browsed) in projects {
                entries.push(Entry::Project {
                    key: format!("{AZURE_PROJECT}{organization}/{project}"),
                    name: project,
                });
                entries.extend(
                    browsed
                        .into_iter()
                        .map(|(key, name)| Entry::Browsed { key, name }),
                );
            }
        }
        entries.truncate(MAX_ENTRIES);
        entries
    }

    /// The control's text: all projects, the one picked, or how many are.
    pub(super) fn project_filter_label(&self) -> String {
        if self.projects.len() > 1 {
            return fl!("orchestrator-projects-count", count = self.projects.len());
        }
        let Some(only) = self.projects.iter().next() else {
            return fl!("orchestrator-projects-all");
        };
        match only.strip_prefix(AZURE_PROJECT) {
            Some(path) => path
                .split_once('/')
                .map_or(path, |(_, name)| name)
                .to_owned(),
            None => self
                .repos
                .iter()
                .find(|repo| repo.key == *only)
                .map_or_else(
                    || fl!("orchestrator-projects-all"),
                    |repo| self.project_label(repo),
                ),
        }
    }

    /// The control, while there is more than one project to choose between.
    pub(super) fn render_project_filter_control(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let choices = self
            .project_entries()
            .iter()
            .filter(|entry| matches!(entry, Entry::Repo { .. } | Entry::Project { .. }))
            .count();
        if choices < 2 {
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

    /// How many top-level rows of the shown tab each pick holds, an item
    /// several repositories list counted once.
    fn pick_counts(&self) -> HashMap<String, usize> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        let mut counted = HashSet::new();
        for repo in &self.repos {
            let rows = match self.tab {
                Tab::Issues => &repo.issue_rows,
                Tab::PullRequests => &repo.pull_request_rows,
            };
            for row in rows.iter().filter(|row| row.depth == 0) {
                let item = &repo.snapshot.items[row.item];
                if !counted.insert(item.key.canonical()) {
                    continue;
                }
                let pick = azure_project(&item.key.source).unwrap_or_else(|| repo.key.clone());
                *counts.entry(pick).or_default() += 1;
            }
        }
        counts
    }

    pub(super) fn render_project_filter_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let look = &self.look;
        let theme = &look.theme;
        let counts = self.pick_counts();
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
            }))
            .into_any_element();
        let mut rows = vec![all];
        let mut choice = 0;
        for entry in self.project_entries() {
            let (key, label, indent) = match entry {
                Entry::Organization(name) => {
                    rows.push(
                        div()
                            .px_2()
                            .pt_1()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(look.small())
                            .text_color(rgb(theme.muted))
                            .child(look.icon("icons/azure-devops.svg", 11., theme.subtext()))
                            .child(div().min_w_0().truncate().child(name))
                            .into_any_element(),
                    );
                    continue;
                }
                Entry::Browsed { key, name } => {
                    let selector = format!("orchestrator-project-filter-close-{key}");
                    rows.push(
                        div()
                            .pl(px(36.))
                            .pr_2()
                            .h(px(24.))
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(look.small())
                            .text_color(rgb(theme.subtext()))
                            .child(div().flex_1().min_w_0().truncate().child(name.clone()))
                            .child(
                                look.icon_button(
                                    SharedString::from(selector.clone()),
                                    "icons/close.svg",
                                    fl!("orchestrator-projects-remove", repository = name.as_str()),
                                )
                                .debug_selector(move || selector)
                                .size(px(18.))
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.stop_browsing(&key);
                                        cx.notify();
                                    },
                                )),
                            )
                            .into_any_element(),
                    );
                    continue;
                }
                Entry::Repo { key, label } => (key, label, false),
                Entry::Project { key, name } => (key, name, true),
            };
            let on = self.projects.contains(&key);
            let count = counts.get(&key).copied().unwrap_or(0);
            let index = choice;
            choice += 1;
            rows.push(
                self.menu_row(format!("orchestrator-project-filter-{index}").into(), on)
                    .when(indent, |el| el.pl_4())
                    .child(check(on))
                    .child(div().flex_1().min_w_0().truncate().child(label))
                    .when(count > 0, |el| {
                        el.child(div().text_color(rgb(theme.muted)).child(count.to_string()))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.projects.remove(&key) {
                            this.projects.insert(key.clone());
                        }
                        this.refresh_rows();
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
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
                    .children(rows),
            ),
            "orchestrator-project-filter-menu",
        ))
    }
}
