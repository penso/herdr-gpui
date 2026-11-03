//! Search finds what was said in a chat, ignoring case and line breaks,
//! newest first and within bounds, and refuses queries out of range.

use super::*;
use crate::hub::{ChatRow, Hub, Layout, Workspace};
use crate::web::tests::{get, send, signed_in, state_with};
use axum::http::{StatusCode, header};
use herdr_chat_protocol::{EndpointId, Entry};
use herdr_client::protocol::AgentStatus;

fn needle(query: &str) -> Needle {
    Needle::parse(query).unwrap()
}

fn transcript(entries: Vec<EntryKind>) -> Transcript {
    Transcript {
        entries: entries
            .into_iter()
            .enumerate()
            .map(|(id, kind)| Entry {
                id: EntryId(id as u64),
                kind,
            })
            .collect(),
        ..Transcript::default()
    }
}

fn user(text: &str) -> EntryKind {
    EntryKind::User {
        text: text.into(),
        files: 0,
    }
}

fn agent(text: &str) -> EntryKind {
    EntryKind::Agent { text: text.into() }
}

#[test]
fn a_query_ignores_case_and_runs_of_whitespace() {
    assert_eq!(find("Fix the Splitter", &needle("the splitter")), Some(4));
    assert_eq!(
        find("fix the\n\n  splitter", &needle("THE   Splitter")),
        Some(4)
    );
    assert_eq!(find("Größe ändern", &needle("ÄNDERN")), Some(8));
    assert_eq!(find("nothing here", &needle("splitter")), None);
}

#[test]
fn a_query_out_of_range_is_refused() {
    assert!(matches!(
        Needle::parse(" a "),
        Err(ApiError::Invalid("that search is too short"))
    ));
    assert!(matches!(
        Needle::parse(&"x".repeat(MAX_QUERY + 1)),
        Err(ApiError::Invalid("that search is too long"))
    ));
    assert!(Needle::parse(&"x".repeat(MAX_QUERY)).is_ok());
}

#[test]
fn a_snippet_keeps_the_words_around_the_match_on_one_line() {
    assert_eq!(snippet("short\nanswer", 0), "short answer");
    let text = format!("{}needle{}", "a".repeat(100), " b".repeat(200));
    let at = text.find("needle").unwrap();
    let cut = snippet(&text, at);
    assert!(cut.starts_with('…'), "{cut}");
    assert!(cut.ends_with('…'), "{cut}");
    assert_eq!(cut.chars().count(), SNIPPET);
    assert_eq!(cut.find("needle"), Some('…'.len_utf8() + BEFORE));
}

#[test]
fn only_messages_match_newest_first_and_bounded_per_chat() {
    let transcript = transcript(vec![
        user("rename the splitter"),
        EntryKind::Thought {
            text: "the splitter".into(),
        },
        agent("Renamed the splitter."),
        user("splitter again"),
        agent("splitter done"),
        agent("splitter, finally"),
    ]);
    let (hits, more) = matches(&transcript, &needle("splitter"), MAX_HITS);
    let ids: Vec<_> = hits.iter().map(|(id, author, _)| (id.0, *author)).collect();
    assert_eq!(
        ids,
        [(5, Author::Agent), (4, Author::Agent), (3, Author::User)]
    );
    assert!(more);
    // The answer's room bounds it too.
    let (hits, more) = matches(&transcript, &needle("splitter"), 1);
    assert_eq!(hits.len(), 1);
    assert!(more);
    let (hits, more) = matches(&transcript, &needle("again"), MAX_HITS);
    assert_eq!(hits.len(), 1);
    assert!(!more);
}

fn chats() -> Layout {
    Layout {
        connected: true,
        boot: "b".into(),
        workspaces: vec![Workspace {
            id: "w1".into(),
            label: "app".into(),
            branch: None,
            cwd: "/work".into(),
            worktree: None,
            chats: vec![ChatRow {
                pane: "p1".into(),
                tab: "t1".into(),
                endpoint: EndpointId::random(),
                agent: Some("claude".into()),
                title: "Claude".into(),
                status: AgentStatus::Idle,
            }],
            terminals: Vec::new(),
        }],
    }
}

#[tokio::test]
async fn search_needs_a_session_and_a_valid_query() {
    let dir = tempfile::tempdir().unwrap();
    let state = state_with(dir.path(), Hub::offline(chats()));
    let (_, token) = signed_in(&state, 8);
    let authorized = |path: &str| {
        let mut request = get(path);
        request.headers_mut().insert(
            header::AUTHORIZATION,
            format!("Bearer {token}").parse().unwrap(),
        );
        request
    };
    let (status, _, _) = send(&state, get("/api/v2/search?q=splitter")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _, _) = send(&state, authorized("/api/v2/search?q=a")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    // A chat the daemon is not following has nothing to search.
    let (status, _, body) = send(&state, authorized("/api/v2/search?q=splitter")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).unwrap(),
        serde_json::json!({ "hits": [], "truncated": false })
    );
}
