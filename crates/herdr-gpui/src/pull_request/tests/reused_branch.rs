use super::*;

fn with(number: u64, state: &str, owner: &str) -> serde_json::Value {
    let mut pr = response()[0].clone();
    pr["number"] = number.into();
    pr["url"] = format!("https://github.com/example/project/pull/{number}").into();
    pr["state"] = state.into();
    pr["headRepositoryOwner"] = serde_json::json!({"login": owner});
    pr
}

fn number(nodes: serde_json::Value) -> crate::Result<Option<u64>> {
    parse(&nodes.to_string(), "example", "project", "feature").map(|pr| pr.map(|pr| pr.number))
}

#[test]
fn a_reused_branch_name_picks_its_open_pull_request() {
    let nodes = serde_json::json!([with(661, "MERGED", "example"), with(678, "OPEN", "example")]);
    assert_eq!(number(nodes).unwrap(), Some(678));
}

#[test]
fn a_reused_branch_name_with_none_open_picks_the_most_recently_updated() {
    let nodes = serde_json::json!([with(9, "MERGED", "example"), with(4, "CLOSED", "example")]);
    assert_eq!(number(nodes).unwrap(), Some(9));
}

#[test]
fn another_owners_pull_request_from_the_same_branch_name_is_ignored() {
    let nodes = serde_json::json!([with(5, "OPEN", "fork"), with(3, "MERGED", "example")]);
    assert_eq!(number(nodes).unwrap(), Some(3));
}

#[test]
fn two_open_pull_requests_stay_ambiguous() {
    let nodes = serde_json::json!([with(1, "OPEN", "example"), with(2, "OPEN", "example")]);
    assert!(matches!(
        number(nodes),
        Err(Error::PullRequest(crate::pull_request::Error::Ambiguous))
    ));
    let strangers = serde_json::json!([with(1, "OPEN", "fork"), with(2, "OPEN", "other")]);
    assert!(matches!(
        number(strangers),
        Err(Error::PullRequest(crate::pull_request::Error::Ambiguous))
    ));
}

#[test]
fn a_crowded_reused_branch_name_picks_its_open_pull_request() {
    use crate::pull_request::{batch::matching_number, fetch::Head};
    let head = Head {
        owner: "example".into(),
        repo: "project".into(),
        branch: "feature".into(),
    };
    let node = |number: u64, state: &str, owner: &str| {
        serde_json::json!({
            "number": number,
            "state": state,
            "headRepositoryOwner": {"login": owner},
            "headRepository": {"name": "project"},
        })
    };
    let heads = |nodes: serde_json::Value| {
        serde_json::json!({"data": {"repository": {"pullRequests": {
            "pageInfo": {"hasNextPage": false}, "nodes": nodes,
        }}}})
    };
    let mut nodes: Vec<_> = (10..14).map(|n| node(n, "OPEN", "fork")).collect();
    nodes.extend([node(661, "MERGED", "example"), node(678, "OPEN", "example")]);
    assert_eq!(
        matching_number(&heads(nodes.into()), &head).unwrap(),
        Some(678)
    );
    let closed = serde_json::json!([node(9, "MERGED", "example"), node(4, "CLOSED", "example")]);
    assert_eq!(matching_number(&heads(closed), &head).unwrap(), Some(9));
}
