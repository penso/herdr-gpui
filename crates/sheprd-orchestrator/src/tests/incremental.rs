use super::*;
use crate::github::{FULL_EVERY, Resume, advance, incremental, merge};
use chrono::TimeDelta;

fn item(native_id: &str, state: &str, updated: &str) -> Item {
    let pull_request = native_id.strip_prefix("pr/").map(|number| PullRequest {
        number: number.parse().unwrap(),
        additions: None,
        deletions: None,
        base_ref: "main".into(),
        head_ref: "topic".into(),
        base_sha: String::new(),
        head_sha: String::new(),
        head_repository: None,
    });
    Item {
        key: ItemKey {
            source: github(),
            native_id: native_id.into(),
        },
        identifier: format!("#{}", native_id.trim_start_matches("pr/")),
        title: native_id.into(),
        description: None,
        state: state.into(),
        url: None,
        author: None,
        labels: Vec::new(),
        parent_id: None,
        blocked_by: Vec::new(),
        priority: None,
        created_at: None,
        updated_at: Some(at(updated)),
        pull_request,
        activity: None,
    }
}

fn ids(items: &[Item]) -> Vec<&str> {
    let mut ids: Vec<&str> = items
        .iter()
        .map(|item| item.key.native_id.as_str())
        .collect();
    ids.sort_unstable();
    ids
}

#[test]
fn only_a_recent_full_listing_with_its_items_is_continued() {
    let now = at("2026-10-13T12:00:00Z");
    let checkpoint = Checkpoint {
        updated_at: Some(at("2026-10-13T11:59:00Z")),
        last_full_at: Some(now - FULL_EVERY + TimeDelta::seconds(1)),
        ..Checkpoint::default()
    };
    let cached = || Some(vec![item("1", "open", "2026-10-13T11:00:00Z")]);
    let (since, items) = incremental(&checkpoint, cached(), None, now).unwrap();
    assert_eq!(
        since.issues,
        at("2026-10-13T11:59:00Z"),
        "from the checkpoint"
    );
    assert_eq!(since.pulls, since.issues);
    assert_eq!(items.len(), 1);
    let kept = Resume {
        issues: at("2026-10-13T11:30:00Z"),
        pulls: at("2026-10-13T11:58:00Z"),
    };
    assert_eq!(
        incremental(&checkpoint, cached(), Some(kept), now)
            .unwrap()
            .0,
        kept
    );
    assert!(
        incremental(&checkpoint, None, None, now).is_none(),
        "nothing cached"
    );
    let stale = Checkpoint {
        last_full_at: Some(now - FULL_EVERY),
        ..checkpoint.clone()
    };
    assert!(
        incremental(&stale, Some(Vec::new()), Some(kept), now).is_none(),
        "time for a full listing"
    );
    assert!(incremental(&Checkpoint::default(), Some(Vec::new()), None, now).is_none());
}

#[test]
fn changes_replace_their_old_copies_and_closed_issues_leave() {
    let cached = vec![
        item("1", "open", "2026-10-13T10:00:00Z"),
        item("2", "open", "2026-10-13T10:00:00Z"),
        item("pr/3", "open", "2026-10-13T10:00:00Z"),
    ];
    // Newest first, as GitHub lists them; a repeat from a shifted page is older.
    let changed = vec![
        item("pr/3", "merged", "2026-10-13T11:30:00Z"),
        item("2", "closed", "2026-10-13T11:20:00Z"),
        item("4", "open", "2026-10-13T11:10:00Z"),
        item("pr/3", "open", "2026-10-13T11:00:00Z"),
    ];
    let items = merge(cached, changed);
    assert_eq!(ids(&items), ["1", "4", "pr/3"]);
    let pr = items
        .iter()
        .find(|item| item.key.native_id == "pr/3")
        .unwrap();
    assert_eq!(pr.state, "merged", "the newest copy wins");
}

#[test]
fn only_the_newest_settled_pull_requests_are_kept() {
    let cached: Vec<Item> = (0..60)
        .map(|n| {
            item(
                &format!("pr/{n}"),
                "closed",
                &format!("2026-10-13T10:{n:02}:00Z"),
            )
        })
        .chain([item("pr/100", "open", "2026-10-01T00:00:00Z")])
        .collect();
    let items = merge(
        cached,
        vec![item("pr/200", "merged", "2026-10-13T12:00:00Z")],
    );
    assert_eq!(items.len(), 51, "fifty settled and the open one");
    assert!(items.iter().any(|item| item.key.native_id == "pr/100"));
    assert!(items.iter().any(|item| item.key.native_id == "pr/200"));
    assert!(!items.iter().any(|item| item.key.native_id == "pr/10"));
    assert!(items.iter().any(|item| item.key.native_id == "pr/11"));
}

#[test]
fn each_listing_resumes_from_the_newest_change_it_saw() {
    let since = at("2026-10-13T11:00:00Z");
    let issues = [
        item("1", "open", "2026-10-13T11:10:00Z"),
        item("2", "closed", "2026-10-13T11:05:00Z"),
    ];
    assert_eq!(advance(since, &issues), at("2026-10-13T11:10:00Z"));
    // A quiet listing keeps its place; it neither moves nor holds back the
    // other, which advances on its own.
    assert_eq!(advance(since, &[]), since);
    let older = [item("3", "open", "2026-10-13T10:00:00Z")];
    assert_eq!(advance(since, &older), since, "never before `since`");
}
