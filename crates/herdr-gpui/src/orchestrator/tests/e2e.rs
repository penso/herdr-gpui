//! The Pull requests and Issues sections end to end, as on every client
//! platform: a real repository on this machine, opened from a linked worktree
//! whose remote is written with a URL rewrite, read with gix, listed from a
//! GitHub that answers on loopback, stored, and rendered as rows.

use super::{
    fake_github::{FakeGithub, ISSUE_TITLE, PULL_TITLE},
    *,
};
use crate::orchestrator::{
    Request, Timing,
    view::{Look, OrchestratorView, Tab},
};
use gpui::{TestAppContext, VisualTestContext};
use std::{
    path::Path,
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// A main checkout at `dir/app` whose `origin` is `gh:penso/e2e-app.git`,
/// rewritten to GitHub by its own config, and a linked worktree of it.
fn repository(dir: &Path) -> std::path::PathBuf {
    let main = dir.join("app");
    std::fs::create_dir_all(&main).unwrap();
    git(&main, &["init", "-q"]);
    git(&main, &["commit", "-q", "--allow-empty", "-m", "base"]);
    git(&main, &["remote", "add", "origin", "gh:penso/e2e-app.git"]);
    git(
        &main,
        &["config", "url.https://github.com/.insteadOf", "gh:"],
    );
    let linked = dir.join("linked");
    git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            &linked.to_string_lossy(),
            "-b",
            "side",
        ],
    );
    linked
}

fn row(id: &str) -> String {
    let key = ItemKey {
        source: SourceKey {
            provider: Provider::Github,
            host: "github.com".into(),
            repository: "penso/e2e-app".into(),
        },
        native_id: id.into(),
    };
    format!("orchestrator-row-{}", key.canonical())
}

/// Polls the worker into the view, as the window's tick does, until
/// `selector` renders or ten seconds pass.
fn wait_for_row(view: &gpui::Entity<OrchestratorView>, cx: &mut VisualTestContext, selector: &str) {
    let selector: &'static str = Box::leak(selector.to_owned().into_boxed_str());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.update(|_, cx| view.update(cx, |view, cx| view.poll(cx)));
        cx.run_until_parked();
        if cx.debug_bounds(selector).is_some() {
            return;
        }
        let snapshot = cx.update(|_, cx| {
            let view = view.read(cx);
            format!("{:?}", view.repo_snapshot_for_tests())
        });
        assert!(
            Instant::now() < deadline,
            "{selector} never rendered; last snapshot: {snapshot}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[gpui::test]
fn pull_requests_and_issues_list_a_local_checkout(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let linked = repository(dir.path());
    let token = format!("e2e-{}", std::process::id());
    let github = FakeGithub::start(&token);
    let main = crate::github::Profile::fixture("penso", Arc::new(token.as_str().into()), &[]);
    let request = Request {
        target: herdr_client::ConnectTarget::Local,
        checkout: linked.to_string_lossy().into_owned(),
        workspace_id: "w1".into(),
        token: Some(crate::github::Tokens::new(&main, std::iter::empty())),
        data_root: Some(dir.path().join("data")),
        gitlab_store: Default::default(),
        timing: Timing {
            reload: Duration::from_millis(20),
            sync: Duration::from_secs(3600),
        },
    };
    let config = crate::config::Config::default();
    let look = Look {
        theme: crate::config::Theme::default(),
        ui: config.ui.clone(),
        mono: config.terminal.clone(),
    };
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::with_repo(request, look, cx));
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(600.)));

    // Issues show first; the rewrite named the repository GitHub was asked for.
    wait_for_row(&view, cx, &row("7"));
    assert_eq!(
        github.asked.lock().unwrap().first().unwrap(),
        "penso/e2e-app"
    );
    let shown = cx.update(|_, cx| view.read(cx).repo_snapshot_for_tests());
    assert!(shown.items.iter().any(|item| item.title == ISSUE_TITLE));
    let repo = shown.repo.unwrap();
    assert_eq!(
        Path::new(&repo.main_root).canonicalize().unwrap(),
        dir.path().join("app").canonicalize().unwrap()
    );

    cx.update(|_, cx| view.update(cx, |view, cx| view.show_list(Tab::PullRequests, cx)));
    wait_for_row(&view, cx, &row("pr/8"));
    let shown = cx.update(|_, cx| view.read(cx).repo_snapshot_for_tests());
    assert!(shown.items.iter().any(|item| item.title == PULL_TITLE));
}
