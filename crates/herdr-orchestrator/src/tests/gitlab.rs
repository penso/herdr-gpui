use super::*;
use crate::gitlab::{
    self, Api, Credential, IssueNode, MAX_OPEN, Scheme, environment_token, full, issues_url,
    parse_cli, retry_after, status_error,
};
use crate::service::retry_wait;
use chrono::TimeDelta;
use secrecy::{ExposeSecret, SecretString};
use std::{
    collections::HashMap,
    ffi::OsString,
    time::{Duration, SystemTime},
};

fn gitlab() -> SourceKey {
    SourceKey {
        provider: Provider::Gitlab,
        host: "gitlab.com".into(),
        repository: "group/sub/app".into(),
    }
}

/// The API of a remote on `key`'s host, with no GitLab CLI instances.
fn own_api(key: &SourceKey, scheme: Scheme) -> Api {
    Api::new(&key.host, scheme, &[])
}

fn node_at(iid: u64, created: DateTime<Utc>) -> IssueNode {
    serde_json::from_value(serde_json::json!({
        "id": 9000 + iid,
        "iid": iid,
        "title": format!("Issue {iid}"),
        "description": "Body",
        "state": "opened",
        "web_url": format!("https://gitlab.com/group/sub/app/-/issues/{iid}"),
        "created_at": created.to_rfc3339(),
        "updated_at": "2026-10-09T01:00:00.500Z",
        "author": {"id": 1, "username": "penso", "name": "Penso"},
        "labels": ["bug"],
        "user_notes_count": 2,
    }))
    .unwrap()
}

fn node(iid: u64) -> IssueNode {
    node_at(iid, at("2026-10-08T23:23:37+00:00"))
}

/// A project's open issues as GitLab answers the listing: created at or
/// after the bound, oldest first, a page at a time.
struct Project {
    open: Vec<(u64, DateTime<Utc>)>,
}

impl Project {
    /// `count` issues, one created each second.
    fn new(count: u64) -> Self {
        let base = at("2026-01-01T00:00:00+00:00");
        Self {
            open: (1..=count)
                .map(|iid| (iid, base + TimeDelta::seconds(iid as i64)))
                .collect(),
        }
    }

    fn answer(&self, after: Option<DateTime<Utc>>) -> Vec<IssueNode> {
        let mut open: Vec<_> = self
            .open
            .iter()
            .filter(|(_, created)| after.is_none_or(|after| *created >= after))
            .copied()
            .collect();
        open.sort_by_key(|(iid, created)| (*created, *iid));
        open.into_iter()
            .take(100)
            .map(|(iid, created)| node_at(iid, created))
            .collect()
    }
}

fn listed(listing: &gitlab::Listing) -> Vec<u64> {
    let mut iids: Vec<u64> = listing
        .items
        .iter()
        .map(|item| item.key.native_id.parse().unwrap())
        .collect();
    iids.sort_unstable();
    iids
}

#[test]
fn an_issue_reads_as_agent_launcher_stores_it() {
    let item = node(42).into_item(&gitlab());
    assert_eq!(item.key.native_id, "42");
    assert_eq!(
        item.key.canonical(),
        "gitlab:gitlab.com:group/sub/app:42",
        "keyed by the project's iid, not GitLab's global id"
    );
    assert_eq!(item.key.canonical().parse::<ItemKey>().unwrap(), item.key);
    assert_eq!(item.identifier, "#42");
    assert_eq!(item.state, "open");
    assert_eq!(item.author.as_deref(), Some("penso"));
    assert_eq!(item.labels, ["bug"]);
    assert_eq!(item.description.as_deref(), Some("Body"));
    assert_eq!(item.updated_at, Some(at("2026-10-09T01:00:00.5+00:00")));
    assert_eq!(
        item.activity.and_then(|activity| activity.comments),
        Some(2)
    );
    assert!(item.pull_request.is_none());
}

#[test]
fn an_issue_without_optional_fields_still_decodes() {
    let node: IssueNode = serde_json::from_value(serde_json::json!({
        "iid": 1, "title": "Bare", "description": null, "state": "closed",
        "web_url": "https://gitlab.com/g/a/-/issues/1",
        "created_at": "2026-10-08T23:23:37Z", "updated_at": "2026-10-08T23:23:37Z",
        "author": null,
    }))
    .unwrap();
    let item = node.into_item(&gitlab());
    assert_eq!(item.state, "closed");
    assert!(item.description.is_none() && item.author.is_none() && item.labels.is_empty());
}

#[test]
fn the_project_path_is_one_escaped_segment() {
    let url = issues_url((&gitlab(), &own_api(&gitlab(), Scheme::Https)), None).unwrap();
    assert_eq!(url.host_str(), Some("gitlab.com"));
    assert_eq!(url.path(), "/api/v4/projects/group%2Fsub%2Fapp/issues");
    let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(query["state"], "opened");
    assert_eq!(query["order_by"], "created_at");
    assert_eq!(query["sort"], "asc");
    assert_eq!(query["per_page"], "100");
    assert!(!query.contains_key("page") && !query.contains_key("created_after"));

    let after = at("2026-10-08T23:23:37.123456+00:00");
    let url = issues_url((&gitlab(), &own_api(&gitlab(), Scheme::Https)), Some(after)).unwrap();
    let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(query["created_after"], "2026-10-08T23:23:37.123Z");

    let managed = SourceKey {
        host: "gitlab.acme.com:8443".into(),
        ..gitlab()
    };
    let url = issues_url((&managed, &own_api(&managed, Scheme::Https)), None).unwrap();
    assert_eq!(url.scheme(), "https");
    assert_eq!(url.port(), Some(8443));
}

/// A self-managed GitLab serving plain HTTP is read over HTTP at the port its
/// remote names; the shared source key keeps only the host, as before.
#[test]
fn an_http_remote_is_read_over_http() {
    let remote = "http://gitlab.internal:8080/group/app.git";
    let key = crate::repo::parse_remote_url(remote).unwrap();
    assert_eq!(key.host, "gitlab.internal:8080");
    assert_eq!(Scheme::of_remote(remote), Scheme::Http);
    let url = issues_url((&key, &own_api(&key, Scheme::of_remote(remote))), None).unwrap();
    assert!(
        url.as_str()
            .starts_with("http://gitlab.internal:8080/api/v4/projects/group%2Fapp/issues?"),
        "{url}"
    );

    for remote in [
        "https://gitlab.acme.com/group/app.git",
        "ssh://git@gitlab.acme.com:2222/group/app.git",
        "git@gitlab.acme.com:group/app.git",
    ] {
        assert_eq!(Scheme::of_remote(remote), Scheme::Https, "{remote}");
    }
}

#[test]
fn a_full_listing_reads_every_page_and_keeps_agent_launchers_markers() {
    let project = Project::new(205);
    let previous = Checkpoint {
        pr_details: HashMap::from([("pr/1".into(), "marker".into())]),
        pr_cursor: Some(7),
        ..Checkpoint::default()
    };
    let mut asked = Vec::new();
    let listing = full(&gitlab(), previous, &mut |after| {
        asked.push(after);
        Ok(project.answer(after))
    })
    .unwrap();
    assert!(listing.complete);
    assert_eq!(listed(&listing), (1..=205).collect::<Vec<_>>());
    // Each page continues from the creation time the last one reached.
    assert_eq!(asked[0], None);
    assert_eq!(asked[1], Some(project.open[99].1));
    assert_eq!(listing.checkpoint.pr_details["pr/1"], "marker");
    assert_eq!(listing.checkpoint.pr_cursor, Some(7));
    assert!(listing.checkpoint.last_full_at.is_some());
}

/// An issue closed or edited while later pages are read must not make the
/// listing skip one that is still open, or the complete listing would drop it
/// from the shared cache.
#[test]
fn issues_changing_during_a_listing_never_hide_an_open_one() {
    let mut project = Project::new(250);
    let mut calls = 0;
    let listing = full(&gitlab(), Checkpoint::default(), &mut |after| {
        calls += 1;
        if calls == 2 {
            // Issues already read close, and a new one is opened.
            project.open.retain(|(iid, _)| ![3, 50, 99].contains(iid));
            project.open.push((251, at("2026-10-16T00:00:00+00:00")));
        }
        Ok(project.answer(after))
    })
    .unwrap();
    assert!(listing.complete);
    let iids = listed(&listing);
    for (iid, _) in &project.open {
        assert!(iids.contains(iid), "open issue {iid} was skipped");
    }
}

#[test]
fn a_listing_past_the_cap_is_shown_but_not_complete() {
    let project = Project::new(5_000);
    let listing = full(&gitlab(), Checkpoint::default(), &mut |after| {
        Ok(project.answer(after))
    })
    .unwrap();
    assert!(!listing.complete);
    assert!((MAX_OPEN..MAX_OPEN + 100).contains(&listing.items.len()));
}

#[test]
fn a_page_of_issues_created_at_one_instant_ends_the_listing_incomplete() {
    let mut calls = 0;
    let listing = full(&gitlab(), Checkpoint::default(), &mut |_| {
        calls += 1;
        Ok((1..=100).map(node).collect())
    })
    .unwrap();
    assert_eq!(calls, 2, "a page that adds nothing stops the listing");
    assert!(!listing.complete);
    assert_eq!(listing.items.len(), 100);
}

#[test]
fn a_failed_page_fails_the_listing() {
    let project = Project::new(150);
    let result = full(&gitlab(), Checkpoint::default(), &mut |after| match after {
        None => Ok(project.answer(after)),
        Some(_) => Err(gitlab::Error::RateLimit.into()),
    });
    assert!(matches!(
        result,
        Err(Error::Gitlab(gitlab::Error::RateLimit))
    ));
}

#[test]
fn refusals_say_whether_a_token_is_needed_or_wrong() {
    let token = SecretString::from("glpat-abc");
    let sent = Credential::Token(&token);
    for status in [401, 403, 404] {
        assert!(matches!(
            status_error(status, "g/app", Credential::Anonymous),
            gitlab::Error::SignIn { repository } if repository == "g/app"
        ));
        assert!(matches!(
            status_error(status, "g/app", Credential::Withheld),
            gitlab::Error::InsecureRemote { repository } if repository == "g/app"
        ));
    }
    assert!(matches!(
        status_error(401, "g/app", sent),
        gitlab::Error::Authentication
    ));
    assert!(matches!(
        status_error(403, "g/app", sent),
        gitlab::Error::Forbidden { .. }
    ));
    assert!(matches!(
        status_error(404, "g/app", sent),
        gitlab::Error::NotFound { .. }
    ));
    assert!(matches!(
        status_error(429, "g/app", sent),
        gitlab::Error::RateLimit
    ));
    assert!(matches!(
        status_error(502, "g/app", sent),
        gitlab::Error::Status(502)
    ));
    let error = Error::from(status_error(404, "g/app", Credential::Anonymous));
    assert!(error.to_string().contains("GITLAB_TOKEN"));
    let error = Error::from(status_error(404, "g/app", Credential::Withheld));
    assert!(error.to_string().contains("https://"));
}

/// A token is never sent over plain HTTP, where anyone watching the
/// connection could take it; the read goes anonymously instead.
#[test]
fn a_token_travels_only_over_https() {
    let token = SecretString::from("glpat-abc");
    assert!(matches!(
        Credential::new(Scheme::Https, Some(&token)),
        Credential::Token(sent) if sent.expose_secret() == "glpat-abc"
    ));
    assert!(matches!(
        Credential::new(Scheme::Http, Some(&token)),
        Credential::Withheld
    ));
    assert!(matches!(
        Credential::new(Scheme::Http, None),
        Credential::Anonymous
    ));
    assert!(matches!(
        Credential::new(Scheme::Https, None),
        Credential::Anonymous
    ));
}

#[test]
fn a_rate_limit_waits_as_long_as_gitlab_asked_within_bounds() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    let mut headers = ureq::http::HeaderMap::new();
    assert_eq!(retry_after(500, &headers, now), None);
    assert_eq!(
        retry_after(429, &headers, now),
        Some(Duration::from_secs(3600))
    );
    headers.insert("ratelimit-reset", "1000900".parse().unwrap());
    assert_eq!(
        retry_after(429, &headers, now),
        Some(Duration::from_secs(900))
    );
    headers.insert("retry-after", "10".parse().unwrap());
    assert_eq!(
        retry_after(429, &headers, now),
        Some(Duration::from_secs(300)),
        "never sooner than five minutes"
    );
}

#[test]
fn gitlab_failures_wait_for_the_user_or_the_rate_limit() {
    let half_hour = Duration::from_secs(1800);
    let hour = Duration::from_secs(3600);
    let sign_in = Error::from(gitlab::Error::SignIn {
        repository: "g/app".into(),
    });
    assert_eq!(retry_wait(&sign_in, None), half_hour);
    let refused = Error::from(gitlab::Error::Authentication);
    assert_eq!(retry_wait(&refused, None), half_hour);
    let limited = Error::from(gitlab::Error::RateLimit);
    assert_eq!(retry_wait(&limited, Some(hour)), hour);
    assert_eq!(retry_wait(&limited, None), Duration::from_secs(300));
}

fn environment(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
    let pairs: HashMap<String, OsString> = pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), OsString::from(value)))
        .collect();
    move |name| pairs.get(name).cloned()
}

#[test]
fn an_environment_token_reaches_only_the_host_it_is_meant_for() {
    let token = |host, pairs: &[(&str, &str)]| {
        environment_token(host, environment(pairs)).map(|token| token.expose_secret().to_owned())
    };
    let plain = [("GITLAB_TOKEN", "glpat-abc")];
    assert_eq!(token("gitlab.com", &plain).as_deref(), Some("glpat-abc"));
    assert_eq!(token("gitlab.acme.com", &plain), None);

    let managed = [
        ("GITLAB_HOST", "https://gitlab.acme.com/"),
        ("GITLAB_ACCESS_TOKEN", "glpat-acme"),
    ];
    assert_eq!(
        token("gitlab.acme.com", &managed).as_deref(),
        Some("glpat-acme")
    );
    assert_eq!(token("gitlab.com", &managed), None);

    let bare = [
        ("GITLAB_HOST", "gitlab.acme.com:8443"),
        ("GITLAB_TOKEN", "t"),
    ];
    assert_eq!(token("gitlab.acme.com:8443", &bare).as_deref(), Some("t"));

    let first = [("GITLAB_TOKEN", "one"), ("GITLAB_ACCESS_TOKEN", "two")];
    assert_eq!(token("gitlab.com", &first).as_deref(), Some("one"));
    let broken = [
        ("GITLAB_TOKEN", "has space"),
        ("GITLAB_ACCESS_TOKEN", "two"),
    ];
    assert_eq!(token("gitlab.com", &broken).as_deref(), Some("two"));
}

#[test]
fn the_cli_answer_is_one_token_line_or_none() {
    let read = |output: &[u8]| parse_cli(output).map(|token| token.expose_secret().to_owned());
    assert_eq!(read(b"glpat-abc\n").as_deref(), Some("glpat-abc"));
    assert_eq!(read(b"\n"), None);
    assert_eq!(read(b""), None);
    assert_eq!(read(b"one\ntwo\n"), None);
    assert_eq!(read(b"not a token\n"), None);
    assert_eq!(read(&[0xff, 0xfe]), None);
}
