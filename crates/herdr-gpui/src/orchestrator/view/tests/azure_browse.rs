//! Azure DevOps beyond the open checkouts: an organization's own pull
//! requests as one more project, the State menu, and the Projects menu,
//! which lists a repository without a checkout.
use super::*;
use crate::orchestrator::{
    PullRequest, RemoteScope, Request, Timing,
    azure::organization::{Catalog, Project},
    repo::{Remote, RepoInfo},
    view::{
        Look, OrchestratorView,
        browse::StateChoice,
        rows::{PrState, Tab},
    },
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

fn pull(repository: &str, id: u64, state: &str, updated: &str) -> Item {
    let mut pull = item(Provider::Azure, &format!("pr/{id}"), "Change", updated);
    pull.key.source = azure(repository);
    pull.identifier = format!("!{id}");
    pull.state = state.into();
    pull.author = Some("Dev One".into());
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

#[test]
fn the_state_menu_narrows_to_one_state_and_open_stays_the_toggles() {
    let mut filters = Filters::default();
    assert_eq!(StateChoice::of(filters), StateChoice::Open);
    StateChoice::Only(PrState::Merged).apply(&mut filters);
    assert!(!filters.open_only);
    assert_eq!(StateChoice::of(filters), StateChoice::Only(PrState::Merged));
    StateChoice::All.apply(&mut filters);
    assert_eq!((filters.open_only, filters.state), (false, None));
    StateChoice::Open.apply(&mut filters);
    assert_eq!(filters, Filters::default());

    let items = [
        pull("contoso/Fabrikam/web", 1, "open", "2026-10-04T00:00:00Z"),
        pull("contoso/Fabrikam/web", 2, "draft", "2026-10-03T00:00:00Z"),
        pull("contoso/Fabrikam/web", 3, "merged", "2026-10-02T00:00:00Z"),
        pull("contoso/Fabrikam/web", 4, "closed", "2026-10-01T00:00:00Z"),
    ];
    let listed = |choice: StateChoice| {
        let mut filters = Filters::default();
        choice.apply(&mut filters);
        pull_request_rows(
            &items,
            &Runs::new(&[], &[], &[]),
            "",
            filters,
            None,
            false,
            Sort::Updated,
        )
        .iter()
        .map(|row| row.item)
        .collect::<Vec<_>>()
    };
    assert_eq!(listed(StateChoice::Open), [0, 1]);
    assert_eq!(listed(StateChoice::All), [0, 1, 2, 3]);
    assert_eq!(listed(StateChoice::Only(PrState::Draft)), [1]);
    assert_eq!(listed(StateChoice::Only(PrState::Merged)), [2]);
    assert_eq!(listed(StateChoice::Only(PrState::Closed)), [3]);
}

fn listed(view: &gpui::Entity<OrchestratorView>, cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|_, cx| {
        let view = view.read(cx);
        view.keys()
            .into_iter()
            .map(|key| {
                let item = view.item(&key).unwrap();
                format!("{}{}", item.key.source.repository, item.identifier)
            })
            .collect()
    })
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

#[gpui::test]
fn an_organizations_own_pull_requests_and_browsed_repositories_join_the_list(
    cx: &mut TestAppContext,
) {
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
    let mine_key = "azure\u{1f}mine\u{1f}contoso";
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let checkout = view.repo_mut();
            checkout.snapshot.repo = Some(RepoInfo {
                repository: crate::orchestrator::Repository::Local(dir.path().join(".git")),
                main_root: dir.path().to_string_lossy().into_owned(),
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
                "open",
                "2026-10-04T00:00:00Z",
            )]);
            // The checkout names the organization: its own pull requests
            // become one more project, once however often it is asked.
            assert!(view.sync_remotes());
            assert!(!view.sync_remotes());
            assert_eq!(view.repo_keys()[1], mine_key);
            let mine = &mut view.repos[1];
            assert_eq!(mine.name(), "contoso");
            mine.snapshot.items = Arc::new(vec![
                // The checkout lists this one too: it shows once.
                pull("contoso/Fabrikam/web", 1, "open", "2026-10-04T00:00:00Z"),
                pull("contoso/Fabrikam/api", 2, "merged", "2026-10-03T00:00:00Z"),
            ]);
            mine.snapshot.viewer = Some("Dev One".into());
            mine.snapshot.catalog = Some(Arc::new(Catalog {
                organization: "contoso".into(),
                projects: vec![Project {
                    name: "Fabrikam".into(),
                    repositories: vec!["api".into(), "web".into()],
                }],
                capped: false,
            }));
            view.tab = Tab::PullRequests;
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert_eq!(listed(&view, cx), ["contoso/Fabrikam/web!1"]);
    // The State menu shows every state.
    click(cx, "orchestrator-state");
    click(cx, "orchestrator-state-all");
    assert_eq!(
        listed(&view, cx),
        ["contoso/Fabrikam/web!1", "contoso/Fabrikam/api!2"]
    );
    assert!(cx.debug_bounds("orchestrator-state-menu").is_none());
    // The Open toggle still narrows to open and draft ones.
    click(cx, "orchestrator-open");
    assert_eq!(listed(&view, cx), ["contoso/Fabrikam/web!1"]);
    click(cx, "orchestrator-state");
    click(cx, "orchestrator-state-merged");
    assert_eq!(listed(&view, cx), ["contoso/Fabrikam/api!2"]);
    // Picking a repository in the Projects menu lists it with no checkout;
    // picking it again stops.
    let browsed = "azure\u{1f}contoso/Fabrikam/api";
    let keys = |cx: &mut VisualTestContext| cx.update(|_, cx| view.read(cx).repo_keys());
    click(cx, "orchestrator-projects");
    click(cx, "orchestrator-projects-repo-contoso/Fabrikam/api");
    assert!(keys(cx).iter().any(|key| key == browsed));
    click(cx, "orchestrator-projects-repo-contoso/Fabrikam/api");
    assert!(!keys(cx).iter().any(|key| key == browsed));
    click(cx, "orchestrator-projects-repo-contoso/Fabrikam/api");
    click(cx, "orchestrator-projects");
    assert!(cx.debug_bounds("orchestrator-projects-frame").is_none());
    // Stopping it from elsewhere drops it too.
    cx.update(|_, cx| view.update(cx, |view, _| view.stop_browsing(browsed)));
    assert!(!keys(cx).iter().any(|key| key == browsed));

    // Repositories without a checkout outlast the checkouts' changes, and
    // are on no host whose account would be pushed to them.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.toggle_browsed(azure("contoso/Fabrikam/web"));
            view.set_repos(Vec::new(), cx);
            assert_eq!(
                view.repo_keys(),
                [mine_key, "azure\u{1f}contoso/Fabrikam/web"]
            );
            assert!(view.targets().is_empty());
            assert!(matches!(
                view.repos[0].remote,
                Some(RemoteScope::Mine { .. })
            ));
        })
    });
}

#[gpui::test]
fn mine_on_azure_devops_matches_the_reader_by_identity(cx: &mut TestAppContext) {
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
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let checkout = view.repo_mut();
            checkout.snapshot.repo = Some(RepoInfo {
                repository: crate::orchestrator::Repository::Local(dir.path().join(".git")),
                main_root: String::new(),
                beads: false,
                remote: Some(Remote {
                    name: "origin".into(),
                    url: "https://dev.azure.com/contoso/Fabrikam/_git/web".into(),
                    source: azure("contoso/Fabrikam/web"),
                }),
            });
            // Both carry the reader's display name; only the first is theirs.
            let mine = pull("contoso/Fabrikam/web", 1, "open", "2026-10-04T00:00:00Z");
            let teammate = pull("contoso/Fabrikam/web", 2, "open", "2026-10-03T00:00:00Z");
            checkout.snapshot.authored =
                Arc::new(std::collections::HashSet::from([mine.key.canonical()]));
            checkout.snapshot.items = Arc::new(vec![mine, teammate]);
            checkout.snapshot.viewer = Some("Dev One".into());
            view.tab = Tab::PullRequests;
            view.mine = true;
            view.refresh_rows();
        })
    });
    assert_eq!(listed(&view, cx), ["contoso/Fabrikam/web!1"]);
}

#[gpui::test]
fn azure_mine_keeps_the_readers_own_and_not_a_namesakes(cx: &mut TestAppContext) {
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
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let checkout = view.repo_mut();
            checkout.snapshot.repo = Some(RepoInfo {
                repository: crate::orchestrator::Repository::Local(dir.path().join(".git")),
                main_root: dir.path().to_string_lossy().into_owned(),
                beads: false,
                remote: Some(Remote {
                    name: "origin".into(),
                    url: "https://dev.azure.com/contoso/Fabrikam/_git/web".into(),
                    source: azure("contoso/Fabrikam/web"),
                }),
            });
            // Both carry the reader's display name; only the first is theirs.
            let own = pull("contoso/Fabrikam/web", 1, "open", "2026-10-04T00:00:00Z");
            let namesake = pull("contoso/Fabrikam/web", 2, "open", "2026-10-03T00:00:00Z");
            checkout.snapshot.authored =
                Arc::new(std::collections::HashSet::from([own.key.canonical()]));
            checkout.snapshot.items = Arc::new(vec![own, namesake]);
            checkout.snapshot.viewer = Some("Dev One".into());
            view.tab = Tab::PullRequests;
            view.mine = true;
            view.refresh_rows();
        })
    });
    assert_eq!(listed(&view, cx), ["contoso/Fabrikam/web!1"]);
}

#[gpui::test]
fn organizations_kept_for_the_session_count_toward_the_cap(cx: &mut TestAppContext) {
    use crate::orchestrator::view::{remote::MAX_ORGANIZATIONS, repo::Repo};
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
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            // Organizations read earlier, whose checkouts have since closed.
            for index in 0..MAX_ORGANIZATIONS {
                view.repos.push(Repo::start_remote(RemoteScope::Mine {
                    organization: format!("contoso-{index}"),
                }));
            }
            view.repo_mut().snapshot.repo = Some(RepoInfo {
                repository: crate::orchestrator::Repository::Local(dir.path().join(".git")),
                main_root: dir.path().to_string_lossy().into_owned(),
                beads: false,
                remote: Some(Remote {
                    name: "origin".into(),
                    url: "https://dev.azure.com/fabrikam/Tools/_git/cli".into(),
                    source: azure("fabrikam/Tools/cli"),
                }),
            });
            assert!(!view.sync_remotes());
            assert!(!view.repo_keys().iter().any(|key| key.ends_with("fabrikam")));
            assert_eq!(view.repos.len(), MAX_ORGANIZATIONS + 1);
        })
    });
}

/// A view whose only project is organization `contoso`'s own pull requests,
/// at `height`, on the Pull requests list.
fn organization_view(
    cx: &mut TestAppContext,
    height: f32,
) -> (gpui::Entity<OrchestratorView>, &mut VisualTestContext) {
    let config = crate::config::Config::default();
    let look = Look {
        theme: crate::config::Theme::default(),
        ui: config.ui.clone(),
        mono: config.terminal.clone(),
    };
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::new(look, cx));
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(height)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repos
                .push(crate::orchestrator::view::repo::Repo::start_remote(
                    RemoteScope::Mine {
                        organization: "contoso".into(),
                    },
                ));
            view.tab = Tab::PullRequests;
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
    (view, cx)
}

#[gpui::test]
fn the_projects_menu_fits_a_short_window(cx: &mut TestAppContext) {
    let (view, cx) = organization_view(cx, 300.);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repos[0].snapshot.catalog = Some(Arc::new(Catalog {
                organization: "contoso".into(),
                projects: vec![Project {
                    name: "Fabrikam".into(),
                    repositories: (0..40).map(|index| format!("repo-{index}")).collect(),
                }],
                capped: false,
            }));
            cx.notify();
        })
    });
    click(cx, "orchestrator-projects");
    let frame = cx.debug_bounds("orchestrator-projects-frame").unwrap();
    assert!(
        frame.bottom() <= gpui::px(300.),
        "the menu ends at {:?}",
        frame.bottom()
    );
    assert!(frame.size.height > gpui::px(0.));
}

#[gpui::test]
fn the_projects_menu_says_why_reading_failed(cx: &mut TestAppContext) {
    use crate::orchestrator::{Error, SourceStatus, SyncState, azure};
    let (view, cx) = organization_view(cx, 700.);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repos[0].snapshot.sources = vec![SourceStatus {
                key: azure("contoso"),
                state: SyncState::Failed(Arc::new(Error::Azure(azure::Error::SignIn {
                    repository: "contoso".into(),
                }))),
            }];
            cx.notify();
        })
    });
    click(cx, "orchestrator-projects");
    assert!(cx.debug_bounds("orchestrator-projects-failed").is_some());
    assert!(cx.debug_bounds("orchestrator-projects-loading").is_none());
    let (failed, text) = cx.update(|_, cx| view.read(cx).catalog_note(0));
    assert!(failed);
    // Fluent isolates the placeable; the words are the banner's.
    let plain: String = text
        .chars()
        .filter(|ch| !matches!(ch, '\u{2068}' | '\u{2069}'))
        .collect();
    assert!(plain.contains("Sign in to read contoso"), "{plain}");
    // A later read that succeeds replaces it.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repos[0].snapshot.sources[0].state = SyncState::Synced {
                at: Utc::now(),
                count: 0,
            };
            view.repos[0].snapshot.catalog = Some(Arc::new(Catalog {
                organization: "contoso".into(),
                projects: vec![Project {
                    name: "Fabrikam".into(),
                    repositories: vec!["web".into()],
                }],
                capped: false,
            }));
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("orchestrator-projects-failed").is_none());
    assert!(
        cx.debug_bounds("orchestrator-projects-repo-contoso/Fabrikam/web")
            .is_some()
    );
}
