//! A source waiting for a sign-in offers it from its chip, naming the
//! provider, so GitLab opens its own page rather than GitHub's.
use super::*;
use crate::orchestrator::{
    GitLabRead, Request, SourceStatus, SyncState, Timing,
    view::{Event, Look, OrchestratorView},
};
use gpui::{AppContext as _, Modifiers, TestAppContext};
use herdr_orchestrator::gitlab::Scheme;
use std::{cell::RefCell, rc::Rc};

fn request(dir: &std::path::Path) -> Request {
    Request {
        target: herdr_client::ConnectTarget::Local,
        checkout: dir.to_string_lossy().into_owned(),
        workspace_id: "w1".into(),
        token: None,
        data_root: Some(dir.join("data")),
        gitlab_store: Default::default(),
        timing: Timing::default(),
    }
}

fn look() -> Look {
    let config = crate::config::Config::default();
    Look {
        theme: crate::config::Theme::default(),
        ui: config.ui.clone(),
        mono: config.terminal.clone(),
    }
}

#[gpui::test]
fn a_gitlab_source_waiting_for_a_token_asks_for_the_gitlab_sign_in(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let request = request(dir.path());
    let look = look();
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::with_repo(request, look, cx));
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(600.)));
    let asked = Rc::new(RefCell::new(Vec::new()));
    let seen = asked.clone();
    cx.update(|_, cx| {
        cx.subscribe(&view, move |_, event: &Event, _| {
            if let Event::SignIn(provider) = event {
                seen.borrow_mut().push(*provider);
            }
        })
        .detach();
        view.update(cx, |view, cx| {
            let gitlab = SourceKey {
                provider: Provider::Gitlab,
                host: "gitlab.com".into(),
                repository: "group/app".into(),
            };
            view.repo_mut().snapshot.sources = vec![SourceStatus {
                key: gitlab,
                state: SyncState::SignIn,
            }];
            cx.notify();
        });
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("orchestrator-sign-in-github").is_none());
    let chip = cx.debug_bounds("orchestrator-sign-in-gitlab").unwrap();
    cx.simulate_click(chip.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(*asked.borrow(), [Provider::Gitlab]);
    let (hosts, waiting): (Vec<GitLabRead>, bool) = cx.update(|_, cx| {
        let view = view.read(cx);
        (view.gitlab_hosts().collect(), view.gitlab_needs_sign_in())
    });
    assert_eq!(
        hosts,
        [GitLabRead {
            host: "gitlab.com".into(),
            scheme: Scheme::Https,
            // A checkout on this computer, whose .envrc may hold a token.
            checkout: Some(dir.path().to_path_buf()),
        }]
    );
    assert!(waiting);
}

#[gpui::test]
fn a_changed_sign_in_reaches_every_windows_orchestrator(cx: &mut TestAppContext) {
    let notified = Rc::new(RefCell::new(0));
    for _ in 0..2 {
        let (window, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        let seen = notified.clone();
        cx.update(|_, cx| {
            let view = window.update(cx, |window, cx| {
                let id = window.sections_view(cx);
                window.orchestrators[&id].view.clone()
            });
            cx.observe(&view, move |_, _| *seen.borrow_mut() += 1)
                .detach();
        });
        cx.run_until_parked();
    }
    *notified.borrow_mut() = 0;
    cx.update(crate::HerdrWindow::refresh_all_orchestrators);
    cx.run_until_parked();
    assert_eq!(*notified.borrow(), 2);
}

#[gpui::test]
fn settings_lists_the_gitlab_hosts_of_every_window(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let hosts = ["gitlab.example.com", "code.internal:8080"];
    for host in hosts {
        let (window, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        cx.run_until_parked();
        // Each window shows a repository read from a host of its own.
        window.update(cx, |window, cx| {
            let id = window.sections_view(cx);
            let view = cx.new(|cx| {
                let mut view = OrchestratorView::with_repo(request(dir.path()), look(), cx);
                view.repo_mut().snapshot.sources = vec![SourceStatus {
                    key: SourceKey {
                        provider: Provider::Gitlab,
                        host: host.into(),
                        repository: "group/app".into(),
                    },
                    state: SyncState::SignIn,
                }];
                view
            });
            window.orchestrators.get_mut(&id).unwrap().view = view;
        });
    }
    let mut listed: Vec<String> = cx.update(|cx| {
        crate::HerdrWindow::all_orchestrator_gitlab_hosts(cx)
            .into_iter()
            .map(|read| read.host)
            .collect()
    });
    listed.sort();
    assert_eq!(listed, ["code.internal:8080", "gitlab.example.com"]);
}

#[gpui::test]
fn the_gitlab_sign_in_opens_the_account_menus_gitlab_page(cx: &mut TestAppContext) {
    let (window, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.run_until_parked();
    let mut sign_in = |provider| {
        cx.update(|win, cx| {
            window.update(cx, |window, cx| {
                let id = window.sections_view(cx);
                window
                    .orchestrator_events
                    .push((id, Event::SignIn(provider)));
                window.poll_orchestrators(win, cx);
                window.menu.page
            })
        })
    };
    assert!(sign_in(Provider::Gitlab) == Some(crate::menu::Page::GitLab));
    assert!(sign_in(Provider::Github) == Some(crate::menu::Page::GitHub));
}
