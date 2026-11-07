use super::*;
use crate::pull_request::{
    batch::matching_number,
    fetch::Head,
    parse::{moved_head, parse_by_number, parse_graphql},
};
use serde_json::{Value, json};

const OLD: (&str, &str) = ("old-owner", "old-name");

/// A pull request as GitHub reports it once the repository has moved: every
/// name is the new one, whichever name it was asked with.
fn moved_pr() -> Value {
    let mut pr = response()[0].clone();
    pr["url"] = "https://github.com/new-owner/new-name/pull/8".into();
    pr["isCrossRepository"] = false.into();
    pr["headRepositoryOwner"] = json!({"login": "new-owner"});
    pr["headRepository"] = json!({"name": "new-name"});
    pr
}

fn by_number(pr: Value, name: &str) -> Value {
    json!({"data": {"repository": {
        "nameWithOwner": name, "mergeCommitAllowed": true, "pullRequest": pr,
    }}})
}

fn by_branch(pr: Value, name: &str) -> Value {
    json!({"data": {"repository": {
        "nameWithOwner": name, "pullRequests": {"nodes": [pr]},
    }}})
}

fn head_on(owner: &str, repo: &str, branch: &str) -> Head {
    Head {
        owner: owner.to_owned(),
        repo: repo.to_owned(),
        branch: branch.to_owned(),
    }
}

fn branch_of(pr: &Value) -> String {
    pr["headRefName"].as_str().unwrap().to_owned()
}

#[test]
fn a_pull_request_read_through_the_old_name_is_found_under_the_new_one() {
    let reply = by_number(moved_pr(), "new-owner/new-name");
    let pr = parse_by_number(reply, OLD.0, OLD.1, 8).unwrap().unwrap();
    assert_eq!(pr.url, "https://github.com/new-owner/new-name/pull/8");
}

#[test]
fn a_branch_with_an_upstream_on_the_old_name_keeps_its_pull_request() {
    let pr = moved_pr();
    let branch = branch_of(&pr);
    let head = head_on(OLD.0, OLD.1, &branch);
    let reply = by_branch(pr, "new-owner/new-name");
    let found = parse_graphql(reply, OLD.0, OLD.1, &branch, Some(&head)).unwrap();
    assert_eq!(found.map(|pr| pr.number), Some(8));
}

#[test]
fn a_branch_without_an_upstream_keeps_its_pull_request() {
    let pr = moved_pr();
    let branch = branch_of(&pr);
    let reply = by_branch(pr, "new-owner/new-name");
    let found = parse_graphql(reply, OLD.0, OLD.1, &branch, None).unwrap();
    assert_eq!(found.map(|pr| pr.number), Some(8));
}

#[test]
fn a_crowded_branch_on_the_old_name_finds_its_head() {
    let repository = json!({"nameWithOwner": "new-owner/new-name"});
    let head = moved_head(&repository, OLD, &head_on(OLD.0, OLD.1, "feature"));
    let heads = json!({"data": {"repository": {"pullRequests": {"nodes": [{
        "number": 8, "state": "OPEN",
        "headRepositoryOwner": {"login": "new-owner"},
        "headRepository": {"name": "new-name"},
    }]}}}});
    assert_eq!(matching_number(&heads, &head).unwrap(), Some(8));
}

#[test]
fn a_forks_head_keeps_its_own_name_when_the_upstream_moves() {
    let repository = json!({"nameWithOwner": "new-owner/new-name"});
    let fork = head_on("contributor", "old-name", "feature");
    assert_eq!(moved_head(&repository, OLD, &fork), fork);
}

#[test]
fn a_pull_request_of_another_repository_is_still_refused() {
    let reply = by_number(moved_pr(), "someone-else/project");
    assert!(matches!(
        parse_by_number(reply, OLD.0, OLD.1, 8),
        Err(Error::PullRequest(crate::pull_request::Error::Identity))
    ));
}

#[test]
fn a_reply_without_a_usable_name_is_held_to_the_name_asked_with() {
    for name in [
        Value::Null,
        "no-slash".into(),
        "a/b/c".into(),
        "a/..".into(),
    ] {
        let mut reply = by_number(moved_pr(), "");
        reply["data"]["repository"]["nameWithOwner"] = name;
        assert!(matches!(
            parse_by_number(reply, OLD.0, OLD.1, 8),
            Err(Error::PullRequest(crate::pull_request::Error::Identity))
        ));
    }
}

#[test]
fn every_lookup_query_asks_for_the_current_name() {
    use crate::pull_request::{
        batch::{HEADS, query},
        fetch::{Ask, NUMBER_QUERY, Target},
    };
    let input = Input {
        checkout: None,
        repo_key: Some("/repo/.git".into()),
        branch: "local".into(),
    };
    let target = Target {
        owner: OLD.0.into(),
        repo: OLD.1.into(),
        ask: Ask::Number(8),
    };
    let (batched, ..) = query(&[(input, target)]);
    for text in [batched.as_str(), HEADS, NUMBER_QUERY] {
        assert!(text.contains("nameWithOwner"), "{text}");
    }
}
