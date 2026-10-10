use super::*;
use crate::pull_request::parse::parse_by_number;

fn numbered(pr: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"data":{"repository":{"mergeCommitAllowed":true,"pullRequest":pr}}})
}

fn same_repository() -> serde_json::Value {
    let mut pr = response()[0].clone();
    pr["isCrossRepository"] = false.into();
    pr["headRepositoryOwner"] = serde_json::json!({"login":"example"});
    pr["headRepository"] = serde_json::json!({"name":"project"});
    pr
}

#[test]
fn any_pull_request_is_found_by_number_and_checked_against_its_own_head() {
    let pr = parse_by_number(numbered(same_repository()), "example", "project", 8)
        .unwrap()
        .unwrap();
    assert_eq!(pr.number, 8);
    assert_eq!(pr.url, "https://github.com/example/project/pull/8");
    assert_eq!(pr.merge_methods, [crate::pull_request::MergeMethod::Merge]);
    // The number asked for is the one answered.
    assert!(matches!(
        parse_by_number(numbered(same_repository()), "example", "project", 9),
        Err(Error::PrIdentity)
    ));
    assert!(
        parse_by_number(numbered(serde_json::Value::Null), "example", "project", 8)
            .unwrap()
            .is_none()
    );
    // Another repository's pull request does not pass for this one's.
    assert!(matches!(
        parse_by_number(numbered(same_repository()), "example", "other", 8),
        Err(Error::PrIdentity)
    ));
}
