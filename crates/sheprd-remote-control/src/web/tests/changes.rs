use super::*;
use crate::control::{
    Request, Response,
    changes::{
        Answer, Ask, Check, CheckLog, CheckState, Checks, Failure, FileDiff, FileList, MAX_PATH,
        Scope, Wanted,
    },
};
use crate::hub::Workspace;
use std::time::Duration;

fn with_workspace(dir: &std::path::Path) -> Shared {
    state_with(
        dir,
        Hub::offline(Layout {
            connected: true,
            boot: "b".into(),
            working: 0,
            workspaces: vec![Workspace {
                id: "w1".into(),
                label: "App".into(),
                cwd: "/code/app".into(),
                branch: Some("topic".into()),
                worktree: None,
                chats: Vec::new(),
                terminals: Vec::new(),
            }],
        }),
    )
}

fn authed(token: &str, path: &str) -> HttpRequest<Body> {
    HttpRequest::get(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

/// Stands in for Sheprd: takes one question over the control socket's
/// handler and answers it.
fn desktop(state: &Shared, respond: impl FnOnce(&Ask) -> Answer + Send + 'static) {
    let state = state.clone();
    std::thread::spawn(move || {
        let (stop, _) = tokio::sync::watch::channel(false);
        let Response::ChangeAsks { asks } =
            crate::server::answer_for_tests(&state, &stop, Request::ChangeAsks)
        else {
            panic!("the daemon did not hand over the question");
        };
        let ask = asks.first().expect("a question");
        let answer = respond(ask);
        let id = ask.id;
        assert_eq!(
            crate::server::answer_for_tests(&state, &stop, Request::ChangeAnswer { id, answer }),
            Response::Done
        );
    });
}

/// Sheprd is open, as its status poll says.
fn open(state: &Shared) {
    let (stop, _) = tokio::sync::watch::channel(false);
    crate::server::answer_for_tests(state, &stop, Request::Status);
}

#[tokio::test]
async fn every_route_needs_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_workspace(dir.path());
    open(&state);
    for path in [
        "/api/v2/workspaces/w1/changes",
        "/api/v2/workspaces/w1/changes/diff?path=a.rs",
        "/api/v2/workspaces/w1/checks",
        "/api/v2/workspaces/w1/checks/7/log",
    ] {
        let (status, ..) = send(&state, get(path)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}");
    }
}

#[tokio::test]
async fn a_phone_gets_the_files_sheprd_lists_for_its_workspace_and_session() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_workspace(dir.path());
    let (_, token) = signed_in(&state, 3);
    open(&state);
    desktop(&state, |ask| {
        assert_eq!(ask.boot_id, "b");
        assert_eq!(ask.workspace_id, "w1");
        assert_eq!(
            ask.wanted,
            Wanted::Files {
                scope: Scope::Branch
            }
        );
        Answer::Files(FileList::bounded(Some("main".into()), []))
    });
    let path = "/api/v2/workspaces/w1/changes?scope=branch";
    let (status, _, body) = send(&state, authed(&token, path)).await;
    assert_eq!(status, StatusCode::OK);
    let list: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(list["base"], "main");
    assert_eq!(list["total"], 0);
}

#[tokio::test]
async fn uncommitted_work_is_what_a_phone_sees_unless_it_says_otherwise() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_workspace(dir.path());
    let (_, token) = signed_in(&state, 3);
    open(&state);
    desktop(&state, |ask| {
        assert_eq!(
            ask.wanted,
            Wanted::Files {
                scope: Scope::Uncommitted
            }
        );
        Answer::Files(FileList::default())
    });
    let (status, ..) = send(&state, authed(&token, "/api/v2/workspaces/w1/changes")).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn a_workspace_this_daemon_does_not_list_never_reaches_sheprd() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_workspace(dir.path());
    let (_, token) = signed_in(&state, 3);
    open(&state);
    let (status, ..) = send(&state, authed(&token, "/api/v2/workspaces/other/changes")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(state.changes.take(Duration::ZERO).is_empty());
}

#[tokio::test]
async fn a_phone_is_told_at_once_when_sheprd_is_closed() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_workspace(dir.path());
    let (_, token) = signed_in(&state, 3);
    let (status, _, body) = send(&state, authed(&token, "/api/v2/workspaces/w1/changes")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(body.contains("not open"));
}

#[tokio::test]
async fn a_failure_keeps_its_meaning_for_the_phone() {
    for (failure, expected) in [
        (Failure::Gone, StatusCode::NOT_FOUND),
        (Failure::NoFile, StatusCode::NOT_FOUND),
        (Failure::NoBase, StatusCode::CONFLICT),
        (Failure::NoLog, StatusCode::CONFLICT),
        (Failure::NoRepository, StatusCode::CONFLICT),
        (Failure::Unreadable, StatusCode::BAD_GATEWAY),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let state = with_workspace(dir.path());
        let (_, token) = signed_in(&state, 3);
        open(&state);
        desktop(&state, move |_| Answer::Failed(failure));
        let (status, _, body) = send(&state, authed(&token, "/api/v2/workspaces/w1/changes")).await;
        assert_eq!(status, expected, "{failure:?}");
        assert!(body.contains(&failure.to_string()), "{body}");
    }
}

#[tokio::test]
async fn a_diff_names_its_file_and_a_path_past_the_bound_is_refused_here() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_workspace(dir.path());
    let (_, token) = signed_in(&state, 3);
    open(&state);
    let long = "p".repeat(MAX_PATH + 1);
    let path = format!("/api/v2/workspaces/w1/changes/diff?path={long}");
    let (status, ..) = send(&state, authed(&token, &path)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(state.changes.take(Duration::ZERO).is_empty());
    desktop(&state, |ask| {
        assert_eq!(
            ask.wanted,
            Wanted::Diff {
                scope: Scope::Uncommitted,
                path: "src/a b.rs".into()
            }
        );
        Answer::Diff(FileDiff::bounded("src/a b.rs", []))
    });
    let path = "/api/v2/workspaces/w1/changes/diff?path=src%2Fa%20b.rs";
    let (status, ..) = send(&state, authed(&token, path)).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn checks_and_a_failing_log_reach_the_phone() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_workspace(dir.path());
    let (_, token) = signed_in(&state, 3);
    open(&state);
    desktop(&state, |_| {
        Answer::Checks(Checks {
            number: Some(9),
            checks: vec![Check {
                name: "test".into(),
                state: CheckState::Failed,
                job: Some(7),
                url: None,
            }],
        })
    });
    let (status, _, body) = send(&state, authed(&token, "/api/v2/workspaces/w1/checks")).await;
    assert_eq!(status, StatusCode::OK);
    let checks: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(checks["checks"][0]["state"], "failed");
    desktop(&state, |ask| {
        assert_eq!(ask.wanted, Wanted::CheckLog { job: 7 });
        Answer::CheckLog(CheckLog {
            excerpt: "boom".into(),
        })
    });
    let path = "/api/v2/workspaces/w1/checks/7/log";
    let (status, _, body) = send(&state, authed(&token, path)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("boom"));
}

#[tokio::test]
async fn an_answer_to_another_question_is_a_failure_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_workspace(dir.path());
    let (_, token) = signed_in(&state, 3);
    open(&state);
    desktop(&state, |_| Answer::Checks(Checks::default()));
    let (status, ..) = send(&state, authed(&token, "/api/v2/workspaces/w1/changes")).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
}
