//! What the list shows, across every repository: the repositories asked
//! for, each one's rows by the search, filters, and sort, then one list of
//! all of them narrowed by the backend and project pills and grouped.

use super::{
    OrchestratorView, RepoSpec,
    repo::Repo,
    rows::{self, ItemRow, Runs, Tab},
};
use crate::orchestrator::{Item, Provider, Snapshot};
use gpui::Context;
use std::{collections::HashSet, sync::Arc};

impl OrchestratorView {
    /// The repositories the header, pills, and banners name: every one
    /// asked for but folders outside Git.
    pub(super) fn projects_named(&self) -> impl Iterator<Item = &Repo> {
        self.repos.iter().filter(|repo| !repo.outside_git())
    }

    /// Lists the projects in `specs`: a repository no longer asked for stops
    /// its worker, a new candidate starts its own. The active one stays active.
    pub(crate) fn set_repos(&mut self, mut specs: Vec<RepoSpec>, cx: &mut Context<Self>) {
        self.skipped_repos
            .retain(|key| specs.iter().any(|spec| spec.key == *key));
        specs.retain(|spec| !self.skipped_repos.contains(&spec.key));
        let same = specs.len() == self.repos.len()
            && specs
                .iter()
                .zip(&self.repos)
                .all(|(spec, repo)| spec.key == repo.key);
        if same {
            return;
        }
        let active = self.repo().map(|repo| repo.key.clone());
        let mut kept: Vec<Repo> = std::mem::take(&mut self.repos);
        self.repos = specs
            .into_iter()
            .map(
                |spec| match kept.iter().position(|repo| repo.key == spec.key) {
                    Some(index) => kept.swap_remove(index),
                    None => Repo::start(spec),
                },
            )
            .collect();
        self.active = active
            .and_then(|key| self.repos.iter().position(|repo| repo.key == key))
            .unwrap_or(0);
        self.projects
            .retain(|key| self.repos.iter().any(|repo| repo.key == *key));
        self.refresh_rows();
        cx.notify();
    }

    /// Keep ordinary folders out of the project list without restarting a
    /// failed discovery on every window tick. Other failures remain visible.
    pub(super) fn discard_non_projects(&mut self) {
        let active = self.repo().map(|repo| repo.key.clone());
        self.repos.retain(|repo| {
            if matches!(
                repo.snapshot.error.as_deref(),
                Some(crate::orchestrator::Error::NotARepository)
            ) {
                self.skipped_repos.insert(repo.key.clone());
                false
            } else {
                true
            }
        });
        self.active = active
            .and_then(|key| self.repos.iter().position(|repo| repo.key == key))
            .unwrap_or(0);
        self.projects
            .retain(|key| !self.skipped_repos.contains(key));
    }

    /// Lists only the project `key` names, as a menu opening one workspace's
    /// issues asks.
    pub(crate) fn only_project(&mut self, key: String, cx: &mut Context<Self>) {
        if !self.repos.iter().any(|repo| repo.key == key) {
            return;
        }
        self.projects = HashSet::from([key]);
        self.refresh_rows();
        cx.notify();
    }

    /// The item a row shows.
    pub(super) fn row_item(&self, row: &ItemRow) -> &Item {
        &self.repos[row.repo].snapshot.items[row.item]
    }

    /// The repository listing `key`: the one its row in the shown list
    /// came from, so a page and its actions go to the checkout the row is,
    /// else the first holding it.
    pub(super) fn repo_of(&self, key: &str) -> Option<usize> {
        self.lines
            .iter()
            .find_map(|line| match line {
                rows::Line::Row(row) if self.row_item(row).key.canonical() == key => Some(row.repo),
                _ => None,
            })
            .or_else(|| self.repos.iter().position(|repo| repo.item(key).is_some()))
    }

    /// The repositories listing the item `key`, beyond the one its row shows.
    pub(super) fn twins(&self, key: &str) -> usize {
        self.holders
            .get(key)
            .map_or(0, |repos| repos.len().saturating_sub(1))
    }

    /// Makes the repository listing `key` the active one, so its page and
    /// actions go to it.
    pub(super) fn activate(&mut self, key: &str) {
        if let Some(repo) = self.repo_of(key) {
            self.active = repo;
        }
    }

    pub(super) fn refresh_rows(&mut self) {
        // A folder outside Git has no pill (`projects_named`), so a pick of
        // it, as opening its workspace's issues makes, could not be undone.
        let repos = &self.repos;
        self.projects.retain(|key| {
            !repos
                .iter()
                .any(|repo| repo.key == *key && repo.outside_git())
        });
        for (index, repo) in self.repos.iter_mut().enumerate() {
            let found = crate::orchestrator::naming::found_runs(
                &repo.snapshot.items,
                &repo.recorded_runs,
                &self.live_branches,
                &repo.snapshot.branches,
                &self.live,
            );
            repo.snapshot.runs = if found.is_empty() {
                repo.recorded_runs.clone()
            } else {
                Arc::new(repo.recorded_runs.iter().cloned().chain(found).collect())
            };
            repo.stand_ins = stand_ins(&repo.snapshot);
            let items = &repo.snapshot.items;
            let runs = Runs::new(&repo.snapshot.runs, &repo.snapshot.sessions, &self.live);
            let mut issues = rows::issue_rows(
                items,
                &runs,
                &self.query,
                self.filters,
                self.sort,
                &self.collapsed,
            );
            let mut pulls = rows::pull_request_rows(
                items,
                &runs,
                &self.query,
                self.filters,
                repo.reading_login(),
                self.mine,
                self.sort,
            );
            if let Some(label) = &self.label {
                issues.retain(|row| items[row.item].labels.iter().any(|has| has == label));
                // A child kept without its parent stands on its own.
                for row in &mut issues {
                    row.depth = 0;
                    row.children = None;
                }
            }
            for row in issues.iter_mut().chain(&mut pulls) {
                row.repo = index;
            }
            repo.issue_rows = issues;
            repo.pull_request_rows = pulls;
        }
        // The backends and projects picked narrow what every repository
        // lists, then all of it is one list.
        let shown = |repo: &Repo| self.projects.is_empty() || self.projects.contains(&repo.key);
        let backend = |row: &ItemRow| {
            self.backends.is_empty()
                || self
                    .backends
                    .contains(&self.row_item(row).key.source.provider)
        };
        let lists: Vec<Vec<ItemRow>> = self
            .repos
            .iter()
            .filter(|repo| shown(repo))
            .map(|repo| {
                let rows = match self.tab {
                    Tab::Issues => &repo.issue_rows,
                    Tab::PullRequests => &repo.pull_request_rows,
                };
                // A child whose parent's backend is filtered out goes with it.
                let mut kept = Vec::new();
                let mut parent_kept = false;
                for row in rows {
                    if row.depth == 0 {
                        parent_kept = backend(row);
                    }
                    if parent_kept {
                        kept.push(*row);
                    }
                }
                kept
            })
            .collect();
        let merged = rows::merge(lists, |row| self.row_item(row), self.sort);
        let (merged, holders) = rows::dedupe(merged, |row| self.row_item(row).key.canonical());
        self.holders = holders;
        let lines = match self.tab {
            Tab::Issues => rows::issue_lines(&merged, |row| self.row_item(row)),
            Tab::PullRequests => rows::pull_request_lines(&merged, |row| {
                let repo = &self.repos[row.repo];
                let item = self.row_item(row);
                rows::Next::of(
                    item,
                    rows::status_of(item, &repo.snapshot.pr_status),
                    repo.reading_login(),
                )
            }),
        };
        let mut providers: Vec<Provider> = Vec::new();
        for line in &lines {
            if let rows::Line::Row(row) = line {
                let provider = self.row_item(row).key.source.provider;
                if !providers.contains(&provider) {
                    providers.push(provider);
                }
            }
        }
        self.mixed_sources = providers.len() > 1;
        self.lines = lines;
        self.revision += 1;
        if self.selected.is_none() || !self.selection_listed() {
            self.selected = self.keys().into_iter().next();
        }
    }

    /// The selectable keys of the shown list, in order.
    pub(super) fn keys(&self) -> Vec<String> {
        self.lines
            .iter()
            .filter_map(|line| match line {
                rows::Line::Row(row) => Some(self.row_item(row).key.canonical()),
                rows::Line::Heading { .. } => None,
            })
            .collect()
    }

    pub(super) fn selection_listed(&self) -> bool {
        self.selected
            .as_ref()
            .is_some_and(|selected| self.keys().contains(selected))
    }

    /// The item `key` names, or the stand-in of one a run works on that is
    /// no longer listed.
    pub(super) fn item(&self, key: &str) -> Option<&Item> {
        // The copy of the checkout the row shows: another checkout may have
        // read the item differently.
        self.repo_of(key)
            .and_then(|repo| self.repos[repo].item(key))
            .or_else(|| self.repos.iter().find_map(|repo| repo.item(key)))
    }

    /// Every repository's items and runs, those found from branches
    /// included, with the host it is on, for the bar's counts and the
    /// Overview.
    pub(crate) fn listed(
        &self,
    ) -> impl Iterator<
        Item = (
            &herdr_client::ConnectTarget,
            &[Item],
            &[crate::orchestrator::Run],
        ),
    > {
        self.repos.iter().map(|repo| {
            (
                &repo.request.target,
                repo.snapshot.items.as_slice(),
                repo.snapshot.runs.as_slice(),
            )
        })
    }

    /// The list line showing `key`, counting the headings.
    pub(super) fn line_of(&self, key: &str) -> usize {
        self.lines
            .iter()
            .position(|line| {
                matches!(line, rows::Line::Row(row) if self.row_item(row).key.canonical() == key)
            })
            .unwrap_or(0)
    }
}

/// A stand-in for each item a run works on that the snapshot no longer lists.
fn stand_ins(snapshot: &Snapshot) -> Vec<Item> {
    let listed: HashSet<String> = snapshot
        .items
        .iter()
        .map(|item| item.key.canonical())
        .collect();
    let mut keys: Vec<&str> = snapshot
        .runs
        .iter()
        .map(|run| run.item_key.as_str())
        .filter(|key| !listed.contains(*key))
        .collect();
    keys.sort_unstable();
    keys.dedup();
    keys.into_iter()
        .filter_map(|key| key.parse().ok())
        .map(Item::stand_in)
        .collect()
}
