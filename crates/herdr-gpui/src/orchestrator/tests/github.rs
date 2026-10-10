use super::*;
use crate::orchestrator::github::{IssueNode, PullRequestNode, connection};
use serde_json::json;

fn node(state: &str, draft: bool) -> serde_json::Value {
    json!({
        "number": 369, "title": "Keep the find bar", "body": "", "state": state,
        "isDraft": draft, "url": "https://github.com/penso/herdr-gpui/pull/369",
        "createdAt": "2026-10-08T01:00:00Z", "updatedAt": "2026-10-08T02:00:00Z",
        "author": {"login": "penso"}, "labels": {"nodes": [{"name": "ui"}]},
        "comments": {"totalCount": 2}, "commits": {"totalCount": 4},
        "additions": 312, "deletions": 40,
        "baseRefName": "main", "headRefName": "feat/pane-find-memory",
        "baseRefOid": "a".repeat(40), "headRefOid": "b".repeat(40),
        "headRepository": {"nameWithOwner": "penso/herdr-gpui"}
    })
}

#[test]
fn pull_requests_map_to_agent_launcher_rows() {
    let response = json!({"data": {"repository": {"pullRequests": {
        "pageInfo": {"hasNextPage": false, "endCursor": null},
        "nodes": [node("OPEN", false)]
    }}}});
    let page = connection::<PullRequestNode>(response, "pullRequests").unwrap();
    let item = page.nodes.into_iter().next().unwrap().into_item(&github());
    assert_eq!(
        item.key.canonical(),
        "github:github.com:penso/herdr-gpui:pr/369"
    );
    assert_eq!(item.identifier, "#369");
    assert_eq!(item.state, "open");
    assert_eq!(item.description, None);
    assert_eq!(item.labels, ["ui"]);
    let activity = item.activity.unwrap();
    assert_eq!(
        (
            activity.comments,
            activity.commits,
            activity.review_comments
        ),
        (Some(2), Some(4), None)
    );
    let pr = item.pull_request.unwrap();
    assert_eq!(
        (pr.number, pr.additions, pr.deletions),
        (369, Some(312), Some(40))
    );
    assert_eq!(
        (pr.base_ref.as_str(), pr.head_ref.as_str()),
        ("main", "feat/pane-find-memory")
    );
    assert_eq!(pr.head_repository.as_deref(), Some("penso/herdr-gpui"));
}

#[test]
fn pull_request_states_use_agent_launchers_words() {
    let state = |value: &str, draft: bool| {
        serde_json::from_value::<PullRequestNode>(node(value, draft))
            .unwrap()
            .into_item(&github())
            .state
    };
    assert_eq!(state("OPEN", true), "draft");
    assert_eq!(state("MERGED", false), "merged");
    assert_eq!(state("CLOSED", true), "closed");
}

#[test]
fn issues_keep_their_number_as_native_id_and_lowercase_state() {
    let issue: IssueNode = serde_json::from_value(json!({
        "number": 377, "title": "Icons", "body": "Tofu", "state": "OPEN",
        "url": "https://github.com/penso/herdr-gpui/issues/377",
        "createdAt": "2026-10-07T00:00:00Z", "updatedAt": "2026-10-08T00:00:00Z",
        "author": null, "labels": {"nodes": []}, "comments": {"totalCount": 0}
    }))
    .unwrap();
    let item = issue.into_item(&github());
    assert_eq!(
        (item.key.native_id.as_str(), item.state.as_str()),
        ("377", "open")
    );
    assert_eq!(item.author, None);
    assert_eq!(item.description.as_deref(), Some("Tofu"));
}

#[test]
fn a_response_without_the_connection_is_a_typed_error() {
    let missing = json!({"data": {"repository": null}});
    assert!(matches!(
        connection::<IssueNode>(missing, "issues"),
        Err(Error::Output { .. })
    ));
}
