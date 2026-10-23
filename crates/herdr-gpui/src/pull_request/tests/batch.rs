use super::*;
use crate::pull_request::{
    batch::{Batch, ask, matching_number, prepare, query},
    fetch::{Ask, Head, Target},
};

fn target(owner: &str, ask: Ask) -> (Input, Target) {
    (
        Input {
            checkout: None,
            repo_key: Some("/repo/.git".into()),
            branch: "local".into(),
        },
        Target {
            owner: owner.into(),
            repo: "project".into(),
            ask,
        },
    )
}

#[test]
fn one_query_asks_each_checkout_under_its_own_alias_and_variables() {
    let targets = [
        target(
            "example",
            Ask::Branch {
                branch: "feature".into(),
                head: Some(Head {
                    owner: "example".into(),
                    repo: "project".into(),
                    branch: "feature".into(),
                }),
            },
        ),
        target(
            "other\") { evil }",
            Ask::Branch {
                branch: "topic".into(),
                head: None,
            },
        ),
        target("example", Ask::Number(42)),
    ];
    let (text, variables, aliases) = query(&targets);
    assert_eq!(
        aliases,
        [
            ("p0".into(), "example/project".into()),
            ("p1".into(), "other\") { evil }/project".into()),
            ("p2".into(), "example/project".into()),
        ]
    );
    assert!(text.contains("p0: repository(owner: $o0, name: $r0)"));
    assert!(text.contains("pullRequests(first: $l1, headRefName: $b1"));
    assert!(text.contains("p2: repository(owner: $o2, name: $r2) { mergeCommitAllowed"));
    assert!(text.contains("pullRequest(number: $n2)"));
    assert!(text.contains("fragment PullRequestFields on PullRequest"));
    // Names travel only as variables, never in the query text.
    assert!(!text.contains("evil") && !text.contains("feature"));
    assert_eq!(variables["o1"], "other\") { evil }");
    assert_eq!(variables["b0"], "feature");
    assert_eq!(variables["l0"], crate::pull_request::batch::HEAD_LIMIT);
    assert_eq!(variables["l1"], 2);
    assert_eq!(variables["n2"], 42);
}

#[test]
fn each_alias_parses_as_a_single_lookup() {
    let (_, target) = target(
        "example",
        Ask::Branch {
            branch: "feature".into(),
            head: None,
        },
    );
    let mut nodes = response();
    nodes[0]["commits"] = serde_json::json!({"nodes": []});
    let answer = serde_json::json!({"data": {"repository": {
        "squashMergeAllowed": true,
        "pullRequests": {"pageInfo": {"hasNextPage": false}, "nodes": nodes},
    }}});
    let pr = crate::pull_request::batch::parse(answer, &target)
        .unwrap()
        .unwrap();
    assert_eq!(pr.number, 8);
}

fn branch(name: &str) -> (Input, Target) {
    let (mut input, target) = target(
        "example",
        Ask::Branch {
            branch: name.into(),
            head: None,
        },
    );
    input.branch = name.into();
    (input, target)
}

#[test]
fn preparation_stops_at_its_deadline_and_defers_the_rest() {
    use std::cell::Cell;
    let start = Instant::now();
    let clock = Cell::new(start);
    let deadline = start + Duration::from_secs(15);
    let inputs: Vec<Input> = ["fast", "broken", "slow", "late"]
        .into_iter()
        .map(|name| branch(name).0)
        .collect();
    let mut batch = Batch::default();
    let targets = prepare(
        inputs,
        deadline,
        || clock.get(),
        |input, limit| {
            assert_eq!(
                limit, deadline,
                "every checkout shares the batch's deadline"
            );
            match input.branch.as_str() {
                "broken" => Err(crate::pull_request::Error::Origin.into()),
                "slow" => {
                    clock.set(deadline);
                    Ok(branch("slow").1)
                }
                name => Ok(branch(name).1),
            }
        },
        &mut batch,
    );
    let names = |inputs: &mut dyn Iterator<Item = &Input>| {
        inputs.map(|input| input.branch.clone()).collect::<Vec<_>>()
    };
    assert_eq!(
        names(&mut targets.iter().map(|(input, _)| input)),
        ["fast", "slow"]
    );
    assert!(matches!(
        batch.found[..],
        [(
            _,
            Err(Error::PullRequest(crate::pull_request::Error::Origin))
        )]
    ));
    assert_eq!(names(&mut batch.deferred.iter()), ["late"]);
}

#[test]
fn a_response_too_large_is_halved_until_each_part_fits() {
    let targets: Vec<_> = (0..4).map(|i| branch(&i.to_string())).collect();
    let mut sizes = Vec::new();
    let mut batch = Batch::default();
    ask(
        targets,
        |_, _, aliases, _| {
            sizes.push(aliases.len());
            if aliases.len() > 1 {
                return Err(Error::GitHub(crate::github::Error::Size));
            }
            Ok(vec![Ok(serde_json::Value::Null)])
        },
        |_, _, _| Some(Ok(None)),
        (&mut batch, &mut None),
    );
    assert_eq!(sizes, [4, 2, 1, 1, 2, 1, 1]);
    let answered: Vec<_> = batch
        .found
        .iter()
        .map(|(input, _)| input.branch.as_str())
        .collect();
    assert_eq!(answered, ["0", "1", "2", "3"]);
    assert!(batch.failed.is_empty() && batch.deferred.is_empty());
}

#[test]
fn a_refusal_fails_its_group_and_defers_the_others() {
    let targets: Vec<_> = (0..4).map(|i| branch(&i.to_string())).collect();
    let mut batch = Batch::default();
    let mut asked = 0;
    ask(
        targets,
        |_, _, aliases, cooldown| {
            asked += 1;
            if aliases.len() > 2 {
                return Err(Error::GitHub(crate::github::Error::Size));
            }
            *cooldown = Some(Duration::from_secs(600));
            Err(Error::GitHub(crate::github::Error::RateLimit {
                reset: None,
            }))
        },
        |_, _, _| Some(Ok(None)),
        (&mut batch, &mut None),
    );
    assert_eq!(asked, 2, "nothing more is asked after a rate limit");
    assert!(batch.found.is_empty());
    assert!(matches!(
        &batch.failed[..],
        [(inputs, Error::GitHub(crate::github::Error::RateLimit { .. }))] if inputs.len() == 2
    ));
    assert_eq!(batch.deferred.len(), 2);
}

#[test]
fn a_crowded_branch_keeps_only_its_own_heads_pull_request() {
    let head = Head {
        owner: "example".into(),
        repo: "project".into(),
        branch: "main".into(),
    };
    let heads = |nodes: serde_json::Value, more: bool| {
        serde_json::json!({"data": {"repository": {"pullRequests": {
            "pageInfo": {"hasNextPage": more}, "nodes": nodes,
        }}}})
    };
    let node = |number: u64, owner: &str| {
        serde_json::json!({
            "number": number,
            "state": "OPEN",
            "headRepositoryOwner": {"login": owner},
            "headRepository": {"name": "project"},
        })
    };
    let others = serde_json::json!([node(1, "fork"), node(2, "Example"), node(3, "other")]);
    assert_eq!(
        matching_number(&heads(others, false), &head).unwrap(),
        Some(2)
    );
    let none = serde_json::json!([node(1, "fork")]);
    assert_eq!(matching_number(&heads(none, false), &head).unwrap(), None);
    let two = serde_json::json!([node(1, "example"), node(2, "example")]);
    assert!(matches!(
        matching_number(&heads(two, false), &head),
        Err(Error::PullRequest(crate::pull_request::Error::Ambiguous))
    ));
    assert!(matches!(
        matching_number(&heads(serde_json::json!([]), true), &head),
        Err(Error::PullRequest(crate::pull_request::Error::Ambiguous))
    ));
}

#[test]
fn a_pause_set_mid_batch_holds_and_stops_further_lookups() {
    let targets: Vec<_> = (0..3).map(|i| branch(&i.to_string())).collect();
    let mut batch = Batch::default();
    let mut cooldown = None;
    let mut lookups = Vec::new();
    ask(
        targets,
        |_, _, aliases, pause| {
            // A successful reply reports no pause of its own.
            *pause = None;
            Ok(aliases
                .iter()
                .map(|_| Ok(serde_json::Value::Null))
                .collect())
        },
        |_, target, network| {
            let Ask::Branch { branch, .. } = &target.ask else {
                unreachable!()
            };
            // Each answer here would need GitHub again.
            let pause = network?;
            lookups.push(branch.clone());
            if branch == "0" {
                *pause = Some(Duration::from_secs(600));
                return Some(Err(Error::GitHub(crate::github::Error::RateLimit {
                    reset: None,
                })));
            }
            // A later success must not lift the pause.
            *pause = None;
            Some(Ok(None))
        },
        (&mut batch, &mut cooldown),
    );
    assert_eq!(lookups, ["0"], "nothing more is asked once paused");
    assert_eq!(cooldown, Some(Duration::from_secs(600)));
    assert!(matches!(
        batch.found[..],
        [(
            _,
            Err(Error::GitHub(crate::github::Error::RateLimit { .. }))
        )]
    ));
    assert_eq!(batch.deferred.len(), 2);
}
