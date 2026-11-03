use super::*;
use crate::github::full;
use serde_json::{Value, json};

fn pull_request(number: u64, state: &str, updated: &str) -> Value {
    json!({
        "number": number, "title": "fixture", "body": "", "state": state,
        "isDraft": false, "url": "https://github.com/sheprd-labs/sheprd/pull/7",
        "createdAt": "2026-10-08T01:00:00Z", "updatedAt": updated,
        "author": null, "labels": {"nodes": []},
        "comments": {"totalCount": 0}, "commits": {"totalCount": 0},
        "additions": 0, "deletions": 0,
        "baseRefName": "main", "headRefName": "topic",
        "baseRefOid": "a".repeat(40), "headRefOid": "b".repeat(40),
        "headRepository": null
    })
}

fn page(field: &str, nodes: Vec<Value>) -> Value {
    json!({"data": {"repository": {field: {
        "pageInfo": {"hasNextPage": false, "endCursor": null},
        "nodes": nodes
    }}}})
}

/// A pull request closed after the open listing was read must not move the
/// next sync past one opened in between, which neither listing saw.
#[test]
fn a_full_listing_resumes_from_when_it_began() {
    let before = Utc::now();
    let mut reads = 0;
    let listing = full(
        &github(),
        ("penso", "herdr-gpui"),
        Checkpoint::default(),
        &mut |_, _| {
            reads += 1;
            Ok(match reads {
                1 => page("issues", Vec::new()),
                2 => page("pullRequests", Vec::new()),
                _ => page(
                    "pullRequests",
                    vec![pull_request(7, "CLOSED", "2099-01-01T00:00:00Z")],
                ),
            })
        },
    )
    .unwrap();
    assert_eq!(listing.items.len(), 1);
    assert!(listing.complete);
    assert!(listing.resume.pulls <= before, "not from the closed one");
    assert_eq!(listing.resume.issues, listing.resume.pulls);
    assert_eq!(listing.checkpoint.updated_at, Some(listing.resume.oldest()));
}
