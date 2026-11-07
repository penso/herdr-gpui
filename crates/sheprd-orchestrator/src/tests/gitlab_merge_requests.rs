use super::*;
use crate::gitlab::{
    self, Api, Error as GitlabError, Listed, MergeRequestNode, Scheme, merge_requests_url,
};
use chrono::TimeDelta;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
};

fn gitlab_key() -> SourceKey {
    SourceKey {
        provider: Provider::Gitlab,
        host: "gitlab.com".into(),
        repository: "group/sub/app".into(),
    }
}

fn merge_request(iid: u64, state: &str, extra: Value) -> Value {
    let mut value = json!({
        "id": 9000 + iid,
        "iid": iid,
        "title": format!("Merge request {iid}"),
        "description": "Body",
        "state": state,
        "web_url": format!("https://gitlab.com/group/sub/app/-/merge_requests/{iid}"),
        "created_at": "2026-10-08T23:23:37Z",
        "updated_at": "2026-10-09T01:00:00.500Z",
        "author": {"id": 1, "username": "penso"},
        "labels": ["bug"],
        "user_notes_count": 3,
        "source_branch": "feature/gitlab-mrs",
        "target_branch": "main",
        "sha": "headsha",
        "diff_refs": {"base_sha": "basesha", "head_sha": "diffheadsha", "start_sha": "s"},
    });
    value
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    value
}

fn item(value: Value) -> Item {
    serde_json::from_value::<MergeRequestNode>(value)
        .unwrap()
        .into_item(&gitlab_key())
}

#[test]
fn a_merge_request_reads_as_a_pull_request() {
    let item = item(merge_request(7, "opened", json!({})));
    assert_eq!(item.key.native_id, "pr/7");
    assert_eq!(item.key.canonical(), "gitlab:gitlab.com:group/sub/app:pr/7");
    assert_eq!(item.key.canonical().parse::<ItemKey>().unwrap(), item.key);
    assert_eq!(item.identifier, "!7");
    assert_eq!(item.state, "open");
    assert_eq!(item.author.as_deref(), Some("penso"));
    assert_eq!(item.labels, ["bug"]);
    assert_eq!(
        item.url.as_deref(),
        Some("https://gitlab.com/group/sub/app/-/merge_requests/7")
    );
    assert_eq!(item.updated_at, Some(at("2026-10-09T01:00:00.5+00:00")));
    assert_eq!(item.activity.and_then(|a| a.comments), Some(3));
    let pr = item.pull_request.as_ref().unwrap();
    assert_eq!(pr.number, 7);
    assert_eq!(pr.head_ref, "feature/gitlab-mrs");
    assert_eq!(pr.base_ref, "main");
    assert_eq!(pr.head_sha, "headsha");
    assert_eq!(pr.base_sha, "basesha");
    assert_eq!(pr.head_repository, None);
    assert!(
        item.github_pull_request().is_none(),
        "GitHub's status and review paths must not take a merge request"
    );
}

#[test]
fn a_merge_requests_state_follows_draft_and_the_gitlab_word() {
    let state = |state: &str, extra: Value| item(merge_request(1, state, extra)).state;
    assert_eq!(state("opened", json!({"draft": true})), "draft");
    // GitLab before 14.0.
    assert_eq!(state("opened", json!({"work_in_progress": true})), "draft");
    // Newer GitLab sends both names.
    assert_eq!(
        state("opened", json!({"draft": true, "work_in_progress": true})),
        "draft"
    );
    assert_eq!(state("locked", json!({})), "open");
    assert_eq!(state("merged", json!({"draft": true})), "merged");
    assert_eq!(state("closed", json!({})), "closed");
}

#[test]
fn a_merge_request_without_optional_fields_still_decodes() {
    let item = item(json!({
        "iid": 2, "title": "Bare", "description": "", "state": "opened",
        "web_url": "https://gitlab.com/g/a/-/merge_requests/2",
        "created_at": "2026-10-08T23:23:37Z", "updated_at": "2026-10-08T23:23:37Z",
        "author": null, "source_branch": "topic", "target_branch": "main",
        "sha": null,
    }));
    let pr = item.pull_request.unwrap();
    assert_eq!((pr.head_sha.as_str(), pr.base_sha.as_str()), ("", ""));
    assert!(item.description.is_none() && item.author.is_none() && item.labels.is_empty());
}

#[test]
fn a_missing_head_sha_falls_back_to_the_diff_refs() {
    let item = item(merge_request(3, "opened", json!({"sha": null})));
    assert_eq!(item.pull_request.unwrap().head_sha, "diffheadsha");
}

#[test]
fn an_unlisted_merge_request_is_called_by_its_bang_number() {
    let stand_in = Item::stand_in(item(merge_request(7, "opened", json!({}))).key);
    assert_eq!(stand_in.identifier, "!7");
    let issue = Item::stand_in(ItemKey {
        source: gitlab_key(),
        native_id: "7".into(),
    });
    assert_eq!(issue.identifier, "#7");
}

#[test]
fn the_merge_request_address_asks_for_open_ones_oldest_first() {
    let api = Api::new("gitlab.com", Scheme::Https, &[]);
    let after = at("2026-10-08T23:23:37.123456+00:00");
    let url = merge_requests_url((&gitlab_key(), &api), Some(after)).unwrap();
    assert_eq!(
        url.as_str(),
        "https://gitlab.com/api/v4/projects/group%2Fsub%2Fapp/merge_requests?state=opened&scope=all&order_by=created_at&sort=asc&per_page=100&created_after=2026-10-08T23%3A23%3A37.123Z"
    );
}

/// Merge requests page by creation time as issues do, 100 at a time.
#[test]
fn merge_requests_are_listed_across_pages() {
    let base = at("2026-01-01T00:00:00+00:00");
    let all: Vec<(u64, DateTime<Utc>)> = (1..=150)
        .map(|iid| (iid, base + TimeDelta::seconds(iid as i64)))
        .collect();
    let mut asked = Vec::new();
    let (items, complete) = gitlab::list::<MergeRequestNode>(&gitlab_key(), &mut |after| {
        asked.push(after);
        Ok(all
            .iter()
            .filter(|(_, created)| after.is_none_or(|after| *created >= after))
            .take(100)
            .map(|(iid, created)| {
                serde_json::from_value(merge_request(
                    *iid,
                    "opened",
                    json!({"created_at": created.to_rfc3339()}),
                ))
                .unwrap()
            })
            .collect())
    })
    .unwrap();
    assert!(complete);
    assert_eq!(items.len(), 150);
    assert!(items.iter().all(|item| item.pull_request.is_some()));
    assert_eq!(asked, [None, Some(all[99].1)]);
}

/// Answers each connection with the next `(status, body)`, returning the
/// request lines it saw.
fn serve(
    answers: Vec<(u16, String)>,
) -> (std::net::SocketAddr, std::thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        answers
            .into_iter()
            .map(|(status, body)| {
                let (stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                reader.read_line(&mut request).unwrap();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line.trim_end().is_empty() {
                        break;
                    }
                }
                (&stream)
                    .write_all(
                        format!(
                            "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .unwrap();
                request
            })
            .collect()
    });
    (address, server)
}

fn local_api(address: std::net::SocketAddr) -> Api {
    Api {
        instance: "gitlab.com".into(),
        host: address.to_string(),
        scheme: Scheme::Http,
    }
}

fn issue(iid: u64) -> Value {
    json!({
        "iid": iid, "title": "Issue", "description": null, "state": "opened",
        "web_url": "https://gitlab.com/group/sub/app/-/issues/1",
        "created_at": "2026-10-08T23:23:37Z", "updated_at": "2026-10-08T23:23:37Z",
        "author": null,
    })
}

#[test]
fn a_sync_lists_issues_and_merge_requests_together() {
    let (address, server) = serve(vec![
        (200, json!([issue(1)]).to_string()),
        (
            200,
            json!([merge_request(1, "opened", json!({}))]).to_string(),
        ),
    ]);
    let listing = gitlab::sync(
        (&gitlab_key(), &local_api(address)),
        None,
        None,
        &|| false,
        &mut None,
    )
    .unwrap();
    assert!(listing.complete);
    let keys: Vec<_> = listing
        .items
        .iter()
        .map(|item| item.key.native_id.as_str())
        .collect();
    // The same iid is a different item as an issue and as a merge request.
    assert_eq!(keys, ["1", "pr/1"]);
    let requests = server.join().unwrap();
    assert!(requests[1].contains("/merge_requests?"), "{requests:?}");
}

#[test]
fn a_refused_second_merge_request_page_does_not_complete_the_sync() {
    for status in [403, 404] {
        let page: Vec<_> = (1..=100)
            .map(|iid| merge_request(iid, "opened", json!({})))
            .collect();
        let (address, server) = serve(vec![
            (200, json!([issue(1)]).to_string()),
            (200, json!(page).to_string()),
            (status, "{}".into()),
        ]);
        let result = gitlab::sync(
            (&gitlab_key(), &local_api(address)),
            None,
            None,
            &|| false,
            &mut None,
        );
        server.join().unwrap();
        assert!(matches!(
            result,
            Err(Error::Gitlab(GitlabError::SignIn { .. }))
        ));
    }
}

#[test]
fn other_merge_request_failures_fail_the_sync() {
    let (address, server) = serve(vec![
        (200, json!([issue(1)]).to_string()),
        (429, "{}".into()),
    ]);
    let mut cooldown = None;
    let error = gitlab::sync(
        (&gitlab_key(), &local_api(address)),
        None,
        None,
        &|| false,
        &mut cooldown,
    )
    .err()
    .unwrap();
    server.join().unwrap();
    assert!(matches!(error, Error::Gitlab(GitlabError::RateLimit)));
    assert!(cooldown.is_some());
}
