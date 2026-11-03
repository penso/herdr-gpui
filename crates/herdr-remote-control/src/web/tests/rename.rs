//! Renaming from the phone: a chat's pane label, a terminal's tab label.

use super::*;
use crate::{
    hub::{ApiMethod, ChatRow, Workspace},
    web::terminal::{MAX_LABEL, RenameTarget, rename_call},
};

/// A computer with one chat (`c1`) and one terminal tab (`t2`).
fn with_chat_and_terminal(dir: &std::path::Path) -> Shared {
    let layout = Layout {
        connected: true,
        boot: "b".into(),
        workspaces: vec![Workspace {
            id: "w1".into(),
            label: "app".into(),
            branch: None,
            cwd: "/code/app".into(),
            worktree: None,
            chats: vec![ChatRow {
                pane: "c1".into(),
                tab: "t1".into(),
                endpoint: herdr_chat_protocol::EndpointId::random(),
                agent: Some("claude".into()),
                title: "Claude".into(),
                status: herdr_client::protocol::AgentStatus::Idle,
            }],
            terminals: vec![crate::hub::layout_terminal("t2", "zsh")],
        }],
    };
    let state = Arc::try_unwrap(state(dir)).ok().unwrap();
    Arc::new(State {
        hub: Hub::offline(layout),
        ..state
    })
}

#[test]
fn a_chat_is_named_by_its_pane_label() {
    let (method, params) =
        rename_call(RenameTarget::Chat { pane: "w1:p2" }, "  release notes ").unwrap();
    assert_eq!(method, ApiMethod::PaneRename);
    assert_eq!(
        params,
        json!({ "pane_id": "w1:p2", "label": "release notes" })
    );
}

#[test]
fn an_empty_name_gives_a_chat_its_agents_title_back() {
    let (method, params) = rename_call(RenameTarget::Chat { pane: "p" }, "   ").unwrap();
    assert_eq!(method, ApiMethod::PaneRename);
    assert_eq!(params, json!({ "pane_id": "p" }));
}

#[test]
fn a_terminal_is_named_by_its_tab_and_needs_a_name() {
    let (method, params) = rename_call(RenameTarget::Tab { tab: "w1:t3" }, "logs").unwrap();
    assert_eq!(method, ApiMethod::TabRename);
    assert_eq!(params, json!({ "tab_id": "w1:t3", "label": "logs" }));
    assert!(matches!(
        rename_call(RenameTarget::Tab { tab: "t" }, " "),
        Err(ApiError::Invalid(_))
    ));
}

#[test]
fn a_name_is_bounded_in_characters() {
    let longest = "é".repeat(MAX_LABEL);
    assert!(rename_call(RenameTarget::Chat { pane: "p" }, &longest).is_ok());
    assert!(matches!(
        rename_call(RenameTarget::Chat { pane: "p" }, &format!("{longest}x")),
        Err(ApiError::Invalid(_))
    ));
}

#[tokio::test]
async fn renaming_needs_a_session_and_a_known_pane() {
    let dir = tempfile::tempdir().unwrap();
    let state = with_chat_and_terminal(dir.path());
    let rename = |pane: &str| post(&format!("/api/term/{pane}/rename"), json!({ "label": "x" }));
    assert_eq!(send(&state, rename("c1")).await.0, StatusCode::UNAUTHORIZED);
    let (_, token) = signed_in(&state, 7);
    let authorized = |pane: &str| terminal::authorized(rename(pane), &token);
    assert_eq!(
        send(&state, authorized("p9")).await.0,
        StatusCode::NOT_FOUND
    );
    // Both reach Herdr, which this test has none of.
    for pane in ["t2-p1", "c1"] {
        let (status, _, body) = send(&state, authorized(pane)).await;
        assert_ne!(status, StatusCode::NOT_FOUND, "{pane}: {body}");
        assert_ne!(status, StatusCode::OK, "{pane}: {body}");
    }
}
