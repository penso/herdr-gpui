//! An incremental sync reads both listings' first pages in one request, and
//! only a listing with more pages continues on its own query.

use super::*;
use crate::github::{Resume, changes};
use serde_json::{Value, json};

fn issue(number: u64, updated: &str) -> Value {
    json!({
        "number": number, "title": "fixture", "body": "", "state": "OPEN",
        "url": "https://github.com/penso/herdr-gpui/issues/1",
        "createdAt": "2026-10-08T01:00:00Z", "updatedAt": updated,
        "author": null, "labels": {"nodes": []}, "comments": {"totalCount": 0}
    })
}

fn pull_request(number: u64, updated: &str) -> Value {
    let mut node = issue(number, updated);
    let extra = json!({
        "isDraft": false, "commits": {"totalCount": 0},
        "additions": 0, "deletions": 0,
        "baseRefName": "main", "headRefName": "topic",
        "baseRefOid": "a".repeat(40), "headRefOid": "b".repeat(40),
        "headRepository": null
    });
    for (field, value) in extra.as_object().unwrap() {
        node[field] = value.clone();
    }
    node
}

fn connection(nodes: Vec<Value>, next: Option<&str>) -> Value {
    json!({
        "pageInfo": {"hasNextPage": next.is_some(), "endCursor": next},
        "nodes": nodes
    })
}

fn since() -> Resume {
    let at = at("2026-10-13T11:00:00Z");
    Resume {
        issues: at,
        pulls: at,
    }
}

fn checkpoint() -> Checkpoint {
    Checkpoint {
        updated_at: Some(at("2026-10-13T11:00:00Z")),
        last_full_at: Some(at("2026-10-13T10:50:00Z")),
        ..Checkpoint::default()
    }
}

#[test]
fn a_quiet_repository_costs_one_request() {
    let mut queries = Vec::new();
    let listing = changes(
        &github(),
        ("penso", "herdr-gpui"),
        since(),
        Vec::new(),
        &checkpoint(),
        &mut |query, variables| {
            queries.push((query.to_owned(), variables));
            Ok(json!({"data": {"repository": {
                "issues": connection(vec![issue(1, "2026-10-13T11:30:00Z")], None),
                "pullRequests": connection(vec![
                    pull_request(7, "2026-10-13T11:20:00Z"),
                    pull_request(6, "2026-10-13T10:00:00Z"),
                ], Some("more")),
            }}}))
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(queries.len(), 1, "the older pull request ends the listing");
    let (query, variables) = &queries[0];
    assert!(query.contains("issues(") && query.contains("pullRequests("));
    assert_eq!(variables["since"], "2026-10-13T11:00:00+00:00");
    let mut ids: Vec<_> = listing
        .items
        .iter()
        .map(|item| item.key.native_id.as_str())
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, ["1", "pr/7"]);
    assert_eq!(listing.resume.issues, at("2026-10-13T11:30:00Z"));
    assert_eq!(listing.resume.pulls, at("2026-10-13T11:20:00Z"));
}

#[test]
fn a_listing_with_more_pages_continues_from_the_combined_cursor() {
    let mut queries: Vec<(String, Value)> = Vec::new();
    let listing = changes(
        &github(),
        ("penso", "herdr-gpui"),
        since(),
        Vec::new(),
        &checkpoint(),
        &mut |query, variables| {
            queries.push((query.to_owned(), variables));
            Ok(match queries.len() {
                1 => json!({"data": {"repository": {
                    "issues": connection(vec![issue(1, "2026-10-13T11:30:00Z")], Some("issues-1")),
                    "pullRequests": connection(
                        vec![pull_request(9, "2026-10-13T11:40:00Z")],
                        Some("pulls-1"),
                    ),
                }}}),
                2 => json!({"data": {"repository": {
                    "issues": connection(vec![issue(2, "2026-10-13T11:10:00Z")], None),
                }}}),
                _ => json!({"data": {"repository": {
                    "pullRequests": connection(vec![pull_request(8, "2026-10-13T11:05:00Z")], None),
                }}}),
            })
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(queries.len(), 3);
    assert_eq!(queries[1].1["after"], "issues-1");
    assert!(queries[1].0.contains("issues(") && !queries[1].0.contains("pullRequests("));
    assert_eq!(queries[2].1["after"], "pulls-1");
    assert!(queries[2].0.contains("pullRequests(") && !queries[2].0.contains("issues("));
    assert_eq!(listing.items.len(), 4);
}
