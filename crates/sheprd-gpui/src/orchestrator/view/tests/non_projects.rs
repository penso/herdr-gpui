use super::*;
use crate::orchestrator::{
    Error,
    view::{RepoSpec, list::Empty, repo::Repo},
};
use gpui::TestAppContext;
use std::sync::Arc;

#[gpui::test]
fn non_projects_are_discarded_without_hiding_beads_or_real_errors(cx: &mut TestAppContext) {
    let (_dir, view, cx) = empty::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let request = view.repos[0].request.clone();
            let specs: Vec<_> = ["folder", "beads", "broken"]
                .into_iter()
                .map(|key| RepoSpec {
                    key: key.into(),
                    host: "local".into(),
                    request: request.clone(),
                })
                .collect();
            view.repos = specs.iter().cloned().map(Repo::start).collect();
            for repo in &mut view.repos {
                repo.service = None;
            }
            view.repos[0].snapshot.error = Some(Arc::new(Error::NotARepository));
            view.repos[1].snapshot.repo = Some(
                crate::orchestrator::repo::parse(
                    &request.target,
                    "common\t/notes/.beads\nmain\t/notes\nbeads\t1\n",
                )
                .unwrap(),
            );
            let bead = item(Provider::Beads, "b-1", "Notes", "2026-10-03T00:00:00Z");
            let key = bead.key.canonical();
            view.repos[1].snapshot.items = Arc::new(vec![bead]);
            view.repos[2].snapshot.error = Some(Arc::new(Error::NoDataDirectory));
            view.active = 1;
            view.projects.insert("folder".into());

            view.discard_non_projects();
            view.refresh_rows();
            assert_eq!(view.repo_keys(), ["beads", "broken"]);
            assert_eq!(view.repo().unwrap().key, "beads");
            assert!(view.projects.is_empty());
            assert_eq!(view.keys(), [key]);
            assert!(matches!(
                view.repos[1].snapshot.error.as_deref(),
                Some(Error::NoDataDirectory)
            ));

            // The host supplies these same folders every tick.
            view.set_repos(specs.clone(), cx);
            assert_eq!(view.repo_keys(), ["beads", "broken"]);
            assert!(view.repos.iter().all(|repo| repo.service.is_none()));

            // Refresh can discover a newly initialized Git or Beads project.
            view.refresh();
            view.set_repos(specs, cx);
            assert_eq!(view.repo_keys(), ["folder", "beads", "broken"]);
            assert!(view.repos[0].service.is_some());
            assert_eq!(view.repo().unwrap().key, "beads");
        });
    });
}

#[gpui::test]
fn only_non_projects_gives_an_empty_list_and_removal_forgets_them(cx: &mut TestAppContext) {
    let (_dir, view, cx) = empty::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repos[0].service = None;
            view.repos[0].snapshot.error = Some(Arc::new(Error::NotARepository));
            view.discard_non_projects();
            view.refresh_rows();
            assert!(view.repos.is_empty());
            assert_eq!(view.empty(), Empty::NoRepositories);
            assert!(view.snapshot().error.is_none());
            assert!(view.skipped_repos.contains("test"));
            view.set_repos(Vec::new(), cx);
            assert!(view.skipped_repos.is_empty());
        });
    });
}
