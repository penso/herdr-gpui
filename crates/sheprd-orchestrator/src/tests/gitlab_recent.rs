//! A picker's GitLab issues: one page of the most recently updated open
//! issues, never the full listing the cache sync reads.
use crate::{
    Provider, SourceKey,
    gitlab::{Api, Scheme, recent_issues_url},
};
use std::collections::HashMap;

fn query(count: usize) -> (String, HashMap<String, String>) {
    let source = SourceKey {
        provider: Provider::Gitlab,
        host: "gitlab.example.com".into(),
        repository: "group/sub/app".into(),
    };
    let api = Api::new("gitlab.example.com", Scheme::Https, &[]);
    let url = recent_issues_url((&source, &api), count).unwrap();
    let pairs = url.query_pairs().into_owned().collect();
    (
        format!("{}{}", url.origin().ascii_serialization(), url.path()),
        pairs,
    )
}

#[test]
fn recent_issues_are_one_page_newest_first() {
    let (address, query) = query(50);
    // The project's full path is one escaped segment, as GitLab asks.
    assert_eq!(
        address,
        "https://gitlab.example.com/api/v4/projects/group%2Fsub%2Fapp/issues"
    );
    for (key, value) in [
        ("state", "opened"),
        ("order_by", "updated_at"),
        ("sort", "desc"),
        ("per_page", "50"),
    ] {
        assert_eq!(query.get(key).map(String::as_str), Some(value), "{key}");
    }
    assert!(!query.contains_key("created_after"));
}

#[test]
fn a_recent_page_holds_at_least_one_issue_and_at_most_a_full_page() {
    assert_eq!(query(0).1["per_page"], "1");
    assert_eq!(query(5_000).1["per_page"], "100");
}
