//! The projects menu lists Azure DevOps projects, not the workers that read
//! them: an organization's own pull requests span several projects, and a
//! checkout and the organization may list the same pull request.
use super::*;
use crate::orchestrator::{
    PullRequest, RemoteScope, Request, Timing,
    repo::{Remote, RepoInfo},
    view::{Look, OrchestratorView, repo::Repo, rows::Tab},
};
use gpui::{Modifiers, TestAppContext, VisualTestContext};
use std::sync::Arc;

fn azure(repository: &str) -> SourceKey {
    SourceKey {
        provider: Provider::Azure,
        host: "dev.azure.com".into(),
        repository: repository.into(),
    }
}

fn pull(repository: &str, id: u64, updated: &str) -> Item {
    let mut pull = item(Provider::Azure, &format!("pr/{id}"), "Change", updated);
    pull.key.source = azure(repository);
    pull.identifier = format!("!{id}");
    pull.pull_request = Some(PullRequest {
        number: id,
        additions: None,
        deletions: None,
        base_ref: "main".into(),
        head_ref: format!("feature/{id}"),
        base_sha: String::new(),
        head_sha: String::new(),
        head_repository: None,
    });
    pull
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    let selector: &'static str = Box::leak(selector.to_owned().into_boxed_str());
    let at = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector}"))
        .center();
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();
}

/// A checkout of `contoso/Fabrikam/web` listing pull request 1, and the
/// organization's own listing 1 again, 2 in Fabrikam's `api`, and 3 in
/// another project, Tools.
fn view(cx: &mut TestAppContext) -> (gpui::Entity<OrchestratorView>, &mut VisualTestContext) {
    let dir = tempfile::tempdir().unwrap();
    let request = Request {
        target: herdr_client::ConnectTarget::Local,
        checkout: dir.path().join("web").to_string_lossy().into_owned(),
        workspace_id: "w1".into(),
        token: None,
        data_root: Some(dir.path().join("data")),
        gitlab_store: Default::default(),
        timing: Timing::default(),
    };
    let config = crate::config::Config::default();
    let look = Look {
        theme: crate::config::Theme::default(),
        ui: config.ui.clone(),
        mono: config.terminal.clone(),
    };
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::with_repo(request, look, cx));
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let checkout = view.repo_mut();
            checkout.snapshot.repo = Some(RepoInfo {
                repository: crate::orchestrator::Repository::Local("/tmp/web/.git".into()),
                main_root: "/tmp/web".into(),
                beads: false,
                remote: Some(Remote {
                    name: "origin".into(),
                    url: "https://dev.azure.com/contoso/Fabrikam/_git/web".into(),
                    source: azure("contoso/Fabrikam/web"),
                }),
            });
            checkout.snapshot.items = Arc::new(vec![pull(
                "contoso/Fabrikam/web",
                1,
                "2026-10-04T00:00:00Z",
            )]);
            let mut mine = Repo::start_remote(RemoteScope::Mine {
                organization: "contoso".into(),
            });
            mine.snapshot.items = Arc::new(vec![
                pull("contoso/Fabrikam/web", 1, "2026-10-04T00:00:00Z"),
                pull("contoso/Fabrikam/api", 2, "2026-10-03T00:00:00Z"),
                pull("contoso/Tools/cli", 3, "2026-10-02T00:00:00Z"),
            ]);
            view.repos.push(mine);
            view.repos
                .push(Repo::start_remote(RemoteScope::Repository(azure(
                    "contoso/Tools/cli",
                ))));
            view.tab = Tab::PullRequests;
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
    (view, cx)
}

fn listed(view: &gpui::Entity<OrchestratorView>, cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|_, cx| {
        view.read(cx)
            .keys()
            .into_iter()
            .map(|key| key.rsplit(':').next().unwrap_or_default().to_owned())
            .collect()
    })
}

#[gpui::test]
fn the_menu_lists_projects_and_narrows_to_their_rows(cx: &mut TestAppContext) {
    let (view, cx) = view(cx);
    let label =
        |cx: &mut VisualTestContext| cx.update(|_, cx| view.read(cx).project_filter_label());
    // Pull request 1, which both list, is one row.
    assert_eq!(listed(&view, cx), ["pr/1", "pr/2", "pr/3"]);
    assert_eq!(label(cx), "All projects");
    click(cx, "orchestrator-project-filter");
    // contoso › Fabrikam, then Tools.
    click(cx, "orchestrator-project-filter-0");
    assert_eq!(listed(&view, cx), ["pr/1", "pr/2"]);
    assert_eq!(label(cx), "Fabrikam");
    click(cx, "orchestrator-project-filter-1");
    assert_eq!(listed(&view, cx), ["pr/1", "pr/2", "pr/3"]);
    assert_eq!(
        label(cx).replace(['\u{2068}', '\u{2069}'], ""),
        "2 projects"
    );
    click(cx, "orchestrator-project-filter-0");
    assert_eq!(listed(&view, cx), ["pr/3"]);
    assert_eq!(label(cx), "Tools");
    click(cx, "orchestrator-project-filter-all");
    assert_eq!(label(cx), "All projects");
    // A browsed repository closes from under its project.
    click(
        cx,
        "orchestrator-project-filter-close-azure\u{1f}contoso/Tools/cli",
    );
    assert!(
        !cx.update(|_, cx| view.read(cx).repo_keys())
            .iter()
            .any(|key| key == "azure\u{1f}contoso/Tools/cli")
    );
}

#[gpui::test]
fn opening_a_checkouts_issues_picks_its_project(cx: &mut TestAppContext) {
    let (view, cx) = view(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let key = view.repo_keys()[0].clone();
            view.only_project(key, cx);
        })
    });
    // The organization's own pull request in Fabrikam shows too.
    assert_eq!(listed(&view, cx), ["pr/1", "pr/2"]);
    assert_eq!(
        cx.update(|_, cx| view.read(cx).project_filter_label()),
        "Fabrikam"
    );
}
