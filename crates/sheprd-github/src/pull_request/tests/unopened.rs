//! Whether a branch's commits still need a pull request: only a settled
//! lookup can say so.
use super::super::{Cache, Input, State, View, fixture};
use std::time::Instant;

const KEY: &str = "/repo/.git";

fn input() -> Input {
    Input {
        checkout: None,
        repo_key: Some(KEY.into()),
        branch: "feature".into(),
    }
}

fn unopened(cache: &Cache, head: &str) -> bool {
    let view = View::default();
    let listed = cache.unopened(&view, KEY, "feature", head);
    assert_eq!(listed, cache.unopened_input(&view, &input(), head));
    listed
}

#[test]
fn no_lookup_yet_says_nothing() {
    assert!(!unopened(&Cache::default(), "tip"));
}

#[test]
fn a_finished_lookup_without_a_pull_request_needs_one() {
    let mut cache = Cache::default();
    cache.seed_none(input(), Instant::now());
    assert!(unopened(&cache, "tip"));
}

#[test]
fn an_open_pull_request_needs_nothing_even_behind_the_branch() {
    let mut cache = Cache::default();
    let mut pr = fixture().unwrap();
    pr.state = State::Open;
    pr.head_ref_oid = "older".into();
    cache.seed(input(), pr, Instant::now());
    assert!(!unopened(&cache, "tip"));
}

#[test]
fn a_squash_merged_branch_at_its_merged_head_needs_nothing() {
    let mut cache = Cache::default();
    let mut pr = fixture().unwrap();
    pr.state = State::Merged;
    pr.head_ref_oid = "tip".into();
    cache.seed(input(), pr, Instant::now());
    assert!(!unopened(&cache, "tip"));
}

#[test]
fn commits_past_a_merged_or_closed_pull_request_need_a_new_one() {
    for state in [State::Merged, State::Closed] {
        let mut cache = Cache::default();
        let mut pr = fixture().unwrap();
        pr.state = state;
        pr.head_ref_oid = "older".into();
        cache.seed(input(), pr, Instant::now());
        assert!(unopened(&cache, "tip"));
    }
}

#[test]
fn a_merged_pull_request_without_a_verified_head_says_nothing() {
    let mut cache = Cache::default();
    let mut pr = fixture().unwrap();
    pr.state = State::Merged;
    pr.head_ref_oid.clear();
    cache.seed(input(), pr, Instant::now());
    assert!(!unopened(&cache, "tip"));
}
