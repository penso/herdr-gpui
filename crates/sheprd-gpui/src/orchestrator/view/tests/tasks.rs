//! The Tasks list: beads alone across projects, its filters, and a task's
//! page.
use super::*;
use crate::orchestrator::{
    Access, Request, Timing,
    view::{
        Event, RepoSpec,
        repo::Repo,
        rows::Tab,
        tasks::{Link, Links, TaskFilters, Who, status_label, task_project},
    },
};
use gpui::{Entity, VisualTestContext};
use std::{collections::HashSet, sync::Arc};

fn task(id: &str, state: &str, priority: i64, assignee: Option<&str>) -> Item {
    let mut task = item(Provider::Beads, id, id, "2026-10-01T00:00:00Z");
    task.state = state.into();
    task.priority = Some(priority);
    task.assignee = assignee.map(str::to_owned);
    task
}

fn listing() -> Vec<Item> {
    let mut child = task("hg-1.1", "in_progress", 2, Some("ada"));
    child.parent_id = Some("hg-1".into());
    vec![
        task("hg-1", "open", 1, None),
        child,
        task("hg-2", "closed", 1, Some("ada")),
        task("hg-3", "deferred", 3, Some("bob")),
        item(Provider::Github, "377", "An issue", "2026-10-04T00:00:00Z"),
    ]
}

fn listed(items: &[Item], tasks: &TaskFilters, filters: Filters, query: &str) -> Vec<String> {
    let runs = Runs::new(&[], &[], &[]);
    task_rows(
        items,
        &runs,
        query,
        filters,
        Sort::Title,
        &HashSet::new(),
        tasks,
    )
    .into_iter()
    .map(|row| items[row.item].identifier.clone())
    .collect()
}

fn filtered(tasks: TaskFilters) -> Vec<String> {
    listed(&listing(), &tasks, Filters::default(), "")
}

#[test]
fn the_list_holds_open_beads_and_no_other_provider() {
    assert_eq!(filtered(TaskFilters::default()), ["hg-1", "hg-1.1", "hg-3"]);
}

#[test]
fn a_pick_of_another_provider_on_the_issues_list_changes_nothing() {
    let filters = Filters {
        provider: Some(Provider::Github),
        ..Filters::default()
    };
    assert_eq!(
        listed(&listing(), &TaskFilters::default(), filters, ""),
        ["hg-1", "hg-1.1", "hg-3"]
    );
}

#[test]
fn closed_tasks_list_only_when_asked_for() {
    let closed = TaskFilters {
        status: Some("closed".into()),
        ..TaskFilters::default()
    };
    assert_eq!(filtered(closed), ["hg-2"]);
}

#[test]
fn a_status_a_project_made_up_filters_like_any_other() {
    let mut items = listing();
    items.push(task("hg-9", "review", 2, None));
    let review = TaskFilters {
        status: Some("review".into()),
        ..TaskFilters::default()
    };
    assert_eq!(listed(&items, &review, Filters::default(), ""), ["hg-9"]);
    assert_eq!(status_label("review"), "review");
}

#[test]
fn priority_and_assignee_narrow_and_a_match_keeps_its_parent() {
    let priority = TaskFilters {
        priority: Some(3),
        ..TaskFilters::default()
    };
    assert_eq!(filtered(priority), ["hg-3"]);
    let nobody = TaskFilters {
        who: Some(Who::Nobody),
        ..TaskFilters::default()
    };
    assert_eq!(filtered(nobody), ["hg-1"]);
    let ada = TaskFilters {
        who: Some(Who::Named("ada".into())),
        ..TaskFilters::default()
    };
    // hg-1 is nobody's, and stays as the parent of what matched.
    assert_eq!(filtered(ada), ["hg-1", "hg-1.1"]);
}

#[test]
fn search_reads_the_assignee_and_the_acceptance_criteria() {
    let mut items = listing();
    items[0].acceptance = Some("Passes the smoke suite".into());
    let tasks = TaskFilters::default();
    let filters = Filters::default();
    assert_eq!(listed(&items, &tasks, filters, "bob"), ["hg-3"]);
    assert_eq!(listed(&items, &tasks, filters, "smoke"), ["hg-1"]);
}

#[test]
fn links_stay_within_the_tasks_own_database() {
    let mut blocked = task("hg-2", "open", 1, None);
    blocked.blocked_by = vec!["hg-1".into(), "gone".into()];
    blocked.parent_id = Some("hg-0".into());
    let mut child = task("hg-2.1", "open", 1, None);
    child.parent_id = Some("hg-2".into());
    let mut downstream = task("hg-3", "open", 1, None);
    downstream.blocked_by = vec!["hg-2".into()];
    let mut twin = task("hg-1", "open", 1, None);
    twin.key.source.repository = "penso/other".into();
    twin.blocked_by = vec!["hg-2".into()];
    let items = vec![
        twin,
        task("hg-0", "open", 1, None),
        task("hg-1", "open", 1, None),
        blocked,
        child,
        downstream,
    ];
    let links = Links::of(&items[3], &items);
    assert_eq!(
        links.parent,
        Some(Link {
            id: "hg-0",
            item: Some(&items[1])
        })
    );
    assert_eq!(
        links.blocked_by,
        [
            Link {
                id: "hg-1",
                item: Some(&items[2])
            },
            // A blocker the sync did not read is still named.
            Link {
                id: "gone",
                item: None
            },
        ]
    );
    let ids = |links: &[Link<'_>]| {
        links
            .iter()
            .map(|link| link.id.to_owned())
            .collect::<Vec<_>>()
    };
    // Not the other database's hg-1, though it names this task's id.
    assert_eq!(ids(&links.blocks), ["hg-3"]);
    assert_eq!(ids(&links.children), ["hg-2.1"]);
}

/// The view with a second project beside the first, each with its Beads.
fn two_projects(
    view: &mut crate::orchestrator::view::OrchestratorView,
    dir: &std::path::Path,
    first: Vec<Item>,
    second: Vec<Item>,
) {
    let request = Request {
        target: sheprd_client::ConnectTarget::Local,
        checkout: dir.join("other").to_string_lossy().into_owned(),
        workspace_id: "other".into(),
        token: None,
        data_root: Some(dir.join("other-data")),
        azure_store: Default::default(),
        gitlab_store: Default::default(),
        timing: Timing::default(),
    };
    view.repos.push(Repo::start(RepoSpec {
        key: "other".into(),
        host: "local".into(),
        request,
    }));
    for (repo, items) in view.repos.iter_mut().zip([first, second]) {
        repo.snapshot.items = Arc::new(items);
        repo.snapshot.access = Some(Access::ReadWrite);
        repo.snapshot.repo = Some(crate::orchestrator::repo::RepoInfo {
            repository: crate::orchestrator::Repository::Local("/r/.git".into()),
            main_root: "/r".into(),
            beads: true,
            beads_root: None,
            remote: None,
        });
    }
    view.tab = Tab::Tasks;
    view.refresh_rows();
}

fn in_other_database(mut task: Item) -> Item {
    task.key.source.repository = "penso/other".into();
    task
}

#[gpui::test]
fn the_same_id_in_two_databases_is_two_tasks_each_going_to_its_own(cx: &mut gpui::TestAppContext) {
    let (dir, view, cx) = empty::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let ours = task("hg-1", "open", 1, None);
            let theirs = in_other_database(task("hg-1", "open", 1, None));
            let their_key = theirs.key.canonical();
            two_projects(view, dir.path(), vec![ours], vec![theirs]);
            assert_eq!(view.keys().len(), 2);
            view.open_detail(their_key, cx);
            assert_eq!(view.active, 1);
        })
    });
}

#[gpui::test]
fn projects_sharing_a_database_list_its_tasks_once(cx: &mut gpui::TestAppContext) {
    let (dir, view, cx) = empty::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            // Their sources are one, as `RepoInfo::beads_source` keys them.
            let shared = task("hg-1", "open", 1, None);
            two_projects(view, dir.path(), vec![shared.clone()], vec![shared]);
            assert_eq!(view.keys().len(), 1);
        })
    });
}

#[gpui::test]
fn only_projects_with_beads_get_a_pill_and_a_column(cx: &mut gpui::TestAppContext) {
    let (dir, view, cx) = empty::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            two_projects(
                view,
                dir.path(),
                vec![task("hg-1", "open", 1, None)],
                vec![item(Provider::Github, "1", "Issue", "2026-10-04T00:00:00Z")],
            );
            assert!(view.has_tasks());
            view.repos[1].snapshot.repo.as_mut().unwrap().beads = false;
            assert_eq!(view.projects_listing().count(), 1);
            view.tab = Tab::Issues;
            assert_eq!(view.projects_listing().count(), 2);
        })
    });
}

#[gpui::test]
fn a_project_without_a_database_leaves_the_others_listed(cx: &mut gpui::TestAppContext) {
    let (dir, view, cx) = empty::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            two_projects(
                view,
                dir.path(),
                vec![task("hg-1", "open", 1, None)],
                vec![in_other_database(task("hg-2", "open", 1, None))],
            );
            view.repos[1].snapshot.error = Some(Arc::new(crate::orchestrator::Error::BeadsMissing));
            view.repos[1].snapshot.items = Arc::default();
            view.refresh_rows();
            assert_eq!(view.keys().len(), 1);
        })
    });
}

/// A window on the page of `hg-2`, which `hg-1` blocks, drawn with `items`.
fn on_a_page(
    view: &Entity<crate::orchestrator::view::OrchestratorView>,
    cx: &mut VisualTestContext,
    items: Vec<Item>,
    key: String,
) {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let repo = view.repo_mut();
            repo.snapshot.items = Arc::new(items);
            repo.snapshot.access = Some(Access::ReadWrite);
            view.tab = Tab::Tasks;
            view.refresh_rows();
            view.open_detail(key, cx);
            cx.notify();
        })
    });
    cx.run_until_parked();
}

fn blocked_page() -> (Vec<Item>, String, String) {
    let blocker = task("hg-1", "open", 1, None);
    let mut blocked = task("hg-2", "in_progress", 1, Some("ada"));
    blocked.blocked_by = vec!["hg-1".into()];
    blocked.acceptance = Some("Tests pass".into());
    let (blocker_key, blocked_key) = (blocker.key.canonical(), blocked.key.canonical());
    (vec![blocker, blocked], blocker_key, blocked_key)
}

#[gpui::test]
fn a_tasks_page_shows_what_finishes_it_and_opens_its_blocker(cx: &mut gpui::TestAppContext) {
    let (_dir, view, cx) = empty::open(cx);
    let (items, blocker, blocked) = blocked_page();
    on_a_page(&view, cx, items, blocked);
    assert!(cx.debug_bounds("orchestrator-task-facts").is_some());
    assert!(cx.debug_bounds("orchestrator-task-acceptance").is_some());
    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.open_detail(blocker.clone(), cx));
        assert_eq!(view.read(cx).detail.as_ref().unwrap().key, blocker);
    });
    cx.run_until_parked();
    // The blocker has no acceptance criteria to show.
    assert!(cx.debug_bounds("orchestrator-task-acceptance").is_none());
}

#[gpui::test]
fn open_project_asks_for_the_repositorys_workspace(cx: &mut gpui::TestAppContext) {
    let (_dir, view, cx) = empty::open(cx);
    let (items, _, blocked) = blocked_page();
    on_a_page(&view, cx, items, blocked);
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = asked.clone();
    cx.update(|_, cx| {
        cx.subscribe(&view, move |_, event: &Event, _| {
            if let Event::OpenProject { repo } = event {
                seen.lock().unwrap().push(repo.clone());
            }
        })
        .detach();
    });
    let button = cx.debug_bounds("orchestrator-task-project").unwrap();
    cx.simulate_click(button.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(*asked.lock().unwrap(), ["test"]);
}

#[gpui::test]
fn the_dialog_says_it_claims_and_offers_no_other_host(cx: &mut gpui::TestAppContext) {
    let (_dir, view, cx) = empty::open(cx);
    let (items, _, blocked) = blocked_page();
    let key = blocked.clone();
    on_a_page(&view, cx, items, blocked);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_dispatch(key, window, cx));
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("orchestrator-claims-task").is_some());
}

#[gpui::test]
fn a_filter_in_use_clears_from_its_chip(cx: &mut gpui::TestAppContext) {
    let (_dir, view, cx) = empty::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let repo = view.repo_mut();
            repo.snapshot.items = Arc::new(listing());
            view.tab = Tab::Tasks;
            view.task_filters.who = Some(Who::Named("bob".into()));
            view.refresh_rows();
            assert_eq!(view.keys().len(), 1);
            assert_eq!(view.active_filters(), 1);
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("orchestrator-chips").is_some());
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let chips = view.task_chips();
            assert_eq!(chips.len(), 1);
            (chips[0].2)(view);
            view.refresh_rows();
            assert_eq!(view.keys().len(), 3);
        })
    });
}

#[test]
fn closed_tasks_offer_their_own_priorities_and_assignees() {
    let items = listing();
    let offered = |filters: &TaskFilters| -> Vec<&str> {
        items
            .iter()
            .filter(|item| filters.offers(item))
            .map(|item| item.key.native_id.as_str())
            .collect()
    };
    assert_eq!(
        offered(&TaskFilters::default()),
        ["hg-1", "hg-1.1", "hg-3"],
        "closed tasks stay out until asked for"
    );
    let closed = TaskFilters {
        status: Some("closed".into()),
        ..TaskFilters::default()
    };
    assert_eq!(offered(&closed), ["hg-2"]);
    let deferred = TaskFilters {
        status: Some("deferred".into()),
        ..TaskFilters::default()
    };
    assert_eq!(offered(&deferred), ["hg-3"]);
}

#[test]
fn a_task_page_names_its_own_project_by_folder() {
    let mut bead = task("hg-1", "open", 1, None);
    bead.key.source.repository = "/Users/me/src/atlas-api".into();
    assert_eq!(task_project(&bead).as_deref(), Some("atlas-api"));
    let issue = item(Provider::Github, "377", "An issue", "2026-10-04T00:00:00Z");
    assert_eq!(task_project(&issue), None);
}
