//! An issue two checkouts list (the same repository on two hosts) is one
//! row, and that row's page and actions go to the checkout it shows.
use super::*;
use crate::orchestrator::{
    Request, Timing,
    view::{RepoSpec, repo::Repo},
};
use std::sync::Arc;

fn row(repo: usize, item: usize, run: Option<RunRow>) -> ItemRow {
    ItemRow {
        repo,
        item,
        depth: 0,
        children: None,
        run,
    }
}

#[test]
fn twins_show_once_as_the_copy_an_agent_worked_on() {
    let keys = ["a", "b", "a"];
    let run = RunRow {
        run: 0,
        status: Status::Working,
    };
    let rows = vec![row(0, 0, None), row(0, 1, None), row(1, 0, Some(run))];
    let (kept, holders) = dedupe(rows, |row| keys[row.repo * 2 + row.item].to_owned());
    assert_eq!(kept.len(), 2);
    // "a" keeps its place, first, as the second repository's copy.
    assert_eq!((kept[0].repo, kept[0].run), (1, Some(run)));
    assert_eq!(kept[1].repo, 0);
    assert_eq!(holders["a"], [0, 1]);
    assert_eq!(holders["b"], [0]);
}

#[gpui::test]
fn one_issue_from_two_hosts_is_one_row_of_the_checkout_it_shows(cx: &mut gpui::TestAppContext) {
    let (dir, view, cx) = empty::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let issue = item(Provider::Github, "733", "Scrolling", "2026-10-04T00:00:00Z");
            let key = issue.key.canonical();
            let request = Request {
                target: herdr_client::ConnectTarget::Local,
                checkout: dir.path().join("other").to_string_lossy().into_owned(),
                workspace_id: "other".into(),
                token: None,
                data_root: Some(dir.path().join("other-data")),
                gitlab_store: Default::default(),
                timing: Timing::default(),
            };
            view.repos.push(Repo::start(RepoSpec {
                key: "other".into(),
                host: "9950x3d.local".into(),
                request,
            }));
            for repo in &mut view.repos {
                repo.snapshot.items = Arc::new(vec![issue.clone()]);
            }
            view.tab = Tab::Issues;
            view.refresh_rows();
            assert_eq!(view.keys(), std::slice::from_ref(&key));
            assert_eq!(view.twins(&key), 1);

            // Only the second checkout lists it once the first is not shown:
            // its row, page, and actions are that checkout's.
            view.projects = std::collections::HashSet::from(["other".to_owned()]);
            view.refresh_rows();
            assert_eq!(view.twins(&key), 0);
            view.open_detail(key.clone(), cx);
            assert_eq!(view.active, 1);
            view.projects.clear();
            view.refresh_rows();
            assert_eq!(view.repo_of(&key), Some(0));
        })
    });
}
