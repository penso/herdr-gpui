use super::*;
use crate::github::full;
use serde_json::{Value, json};

fn pull_request(number: u64) -> Value {
    json!({
        "number": number, "title": "fixture", "body": "", "state": "OPEN",
        "isDraft": false, "url": format!("https://github.com/sheprd-labs/sheprd/pull/{number}"),
        "createdAt": "2026-10-08T01:00:00Z", "updatedAt": "2026-10-08T01:00:00Z",
        "author": null, "labels": {"nodes": []},
        "comments": {"totalCount": 0}, "commits": {"totalCount": 0},
        "additions": 0, "deletions": 0,
        "baseRefName": "main", "headRefName": "topic",
        "baseRefOid": "a".repeat(40), "headRefOid": "b".repeat(40),
        "headRepository": null
    })
}

fn page(field: &str, nodes: Vec<Value>, next: Option<&str>) -> Value {
    json!({"data": {"repository": {field: {
        "pageInfo": {"hasNextPage": next.is_some(), "endCursor": next},
        "nodes": nodes
    }}}})
}

fn status(code: u16) -> Error {
    Error::Github(Box::new(sheprd_github::Error::Status(code)))
}

/// What one fake read was asked: the open pull request page size and cursor,
/// or `None` for the issue and closed listings.
fn asked(variables: &Value) -> Option<(u64, Value)> {
    (variables["states"] == json!(["OPEN"])).then(|| {
        (
            variables["count"].as_u64().unwrap(),
            variables["after"].clone(),
        )
    })
}

/// A page GitHub gave up on is read again from the same cursor at half the
/// size, and later pages keep the smaller size.
#[test]
fn a_timed_out_page_is_read_again_smaller() {
    let mut open = Vec::new();
    let listing = full(
        &github(),
        ("penso", "herdr-gpui"),
        Checkpoint::default(),
        &mut |_, variables| {
            let Some(read) = asked(&variables) else {
                let field = if variables.get("states").is_some() {
                    "pullRequests"
                } else {
                    "issues"
                };
                return Ok(page(field, Vec::new(), None));
            };
            open.push(read.clone());
            match open.len() {
                1 => Err(status(502)),
                2 => Ok(page("pullRequests", vec![pull_request(1)], Some("one"))),
                3 => Err(status(504)),
                _ => Ok(page("pullRequests", vec![pull_request(2)], None)),
            }
        },
    )
    .unwrap();
    assert_eq!(
        open,
        [
            (100, Value::Null),
            (50, Value::Null),
            (50, json!("one")),
            (25, json!("one")),
        ]
    );
    assert_eq!(listing.items.len(), 2);
    assert!(listing.complete);
}

/// At the smallest page a timeout is GitHub's answer, and other failures are
/// never retried.
#[test]
fn only_timeouts_above_the_smallest_page_are_retried() {
    for (code, reads) in [(502, 3), (504, 3), (500, 1), (503, 1)] {
        let mut open = 0;
        let error = full(
            &github(),
            ("penso", "herdr-gpui"),
            Checkpoint::default(),
            &mut |_, variables| {
                if asked(&variables).is_none() {
                    return Ok(page("issues", Vec::new(), None));
                }
                open += 1;
                Err(status(code))
            },
        )
        .err()
        .unwrap();
        assert_eq!(open, reads, "HTTP {code}");
        assert!(
            matches!(&error, Error::Github(error) if matches!(**error, sheprd_github::Error::Status(c) if c == code)),
            "HTTP {code}: {error:?}"
        );
    }
}
