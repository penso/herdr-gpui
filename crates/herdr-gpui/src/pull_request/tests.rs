#![allow(clippy::unwrap_used)]

use super::{
    Cache, Input, Lookup, Result,
    cache::{CACHE_LIMIT, ERROR_BACKOFF, REFRESH},
    clean,
    fetch::{OUTPUT_LIMIT, TIMEOUT, fetch, local_repository, remote_host, worktree_checkout},
    fixture,
    lookup::Worker,
    model::{MergeState, State},
    parse::{parse, parse_graphql},
    run,
};
use crate::Error;
use std::{
    process::Command,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

struct Peer {
    cache: Cache,
    incoming: mpsc::Receiver<(u64, Input, Arc<secrecy::SecretString>)>,
    outgoing: mpsc::SyncSender<(u64, Result, Option<Duration>)>,
}

impl Peer {
    fn new() -> Self {
        let (requests, incoming) = mpsc::sync_channel(1);
        let (outgoing, results) = mpsc::sync_channel(1);
        let mut cache = Cache::default();
        cache.lookup.worker = Some(Worker { requests, results });
        cache.scope((0, 1, "boot".into()), Arc::new("fixture".into()));
        Self {
            cache,
            incoming,
            outgoing,
        }
    }

    fn complete(&mut self, now: Instant, result: Result, cooldown: Option<Duration>) -> Input {
        let (generation, input, _) = self.incoming.try_recv().unwrap();
        self.outgoing.send((generation, result, cooldown)).unwrap();
        self.cache.poll(now);
        input
    }
}

fn input(branch: &str) -> Input {
    Input {
        checkout: None,
        repo_key: "/repo/.git".into(),
        branch: branch.into(),
    }
}

#[test]
fn explicit_refresh_keeps_cached_details_and_coalesces_in_flight_requests() {
    let mut peer = Peer::new();
    let now = Instant::now();
    let input = input("feature");
    peer.cache.seed(input.clone(), fixture().unwrap(), now);
    peer.cache.refresh(input.clone(), now);
    peer.cache.refresh(input.clone(), now);
    assert_eq!(peer.cache.queue.len(), 1);
    assert!(!peer.cache.scan_due(now));
    assert!(peer.incoming.try_recv().is_err(), "input only queues work");
    peer.cache.poll(now);
    assert_eq!(
        peer.cache
            .peek(&input.repo_key, &input.branch)
            .unwrap()
            .number,
        8
    );
    peer.cache.refresh(input.clone(), now);
    assert!(peer.cache.queue.is_empty(), "reuse the in-flight lookup");
    let mut updated = fixture().unwrap();
    updated.is_draft = true;
    updated.checks_summary = "2 pending".into();
    peer.complete(now, Ok(Some(updated)), None);
    let pr = peer.cache.peek(&input.repo_key, &input.branch).unwrap();
    assert!(pr.is_draft);
    assert_eq!(pr.checks_summary, "2 pending");
    assert!(peer.incoming.try_recv().is_err());
}

#[test]
fn explicit_refresh_respects_account_backoff_and_bounds_the_queue() {
    let mut peer = Peer::new();
    let now = Instant::now();
    let input = input("feature");
    peer.cache.seed(input.clone(), fixture().unwrap(), now);
    peer.cache.paused_until = Some(now + ERROR_BACKOFF);
    peer.cache
        .schedule((0..CACHE_LIMIT).map(|i| self::input(&i.to_string())), now);
    peer.cache.refresh(input.clone(), now);
    assert_eq!(peer.cache.queue.len(), CACHE_LIMIT);
    assert_eq!(peer.cache.queue.front(), Some(&input));
    peer.cache.poll(now);
    assert!(peer.incoming.try_recv().is_err());
    peer.cache.poll(now + ERROR_BACKOFF);
    assert_eq!(peer.complete(now + ERROR_BACKOFF, Ok(None), None), input);
    assert!(peer.cache.peek(&input.repo_key, &input.branch).is_none());
}

#[test]
fn cache_prefetches_without_menu_and_refreshes_at_ttl_with_stale_data() {
    let mut peer = Peer::new();
    let now = Instant::now();
    let input = input("feature");
    peer.cache.schedule([input.clone(), input.clone()], now);
    assert_eq!(peer.cache.queue.len(), 1);
    peer.cache.poll(now);
    assert_eq!(
        peer.complete(now, Ok(Some(fixture().unwrap())), None),
        input
    );
    let mut view = Lookup::default();
    peer.cache.present(&input, &mut view, now);
    assert_eq!(view.value.as_ref().unwrap().number, 8);
    assert!(!view.loading);
    assert!(peer.incoming.try_recv().is_err(), "menu read is I/O free");
    peer.cache
        .schedule([input.clone()], now + REFRESH - Duration::from_secs(1));
    peer.cache.poll(now + REFRESH - Duration::from_secs(1));
    assert!(peer.incoming.try_recv().is_err());
    let due = now + REFRESH;
    peer.cache.schedule([input.clone()], due);
    peer.cache.poll(due);
    peer.cache.present(&input, &mut view, due);
    assert!(
        view.value.is_some(),
        "refresh does not replace cached data with loading"
    );
    peer.complete(
        due,
        Err(std::io::Error::other("network unavailable").into()),
        None,
    );
    peer.cache.present(&input, &mut view, due);
    assert!(view.value.is_some());
    assert!(view.message.is_some());
    peer.cache.schedule(
        [input.clone()],
        due + ERROR_BACKOFF - Duration::from_secs(1),
    );
    assert!(peer.cache.queue.is_empty());
    peer.cache.schedule([input.clone()], due + ERROR_BACKOFF);
    peer.cache.poll(due + ERROR_BACKOFF);
    peer.complete(due + ERROR_BACKOFF, Ok(None), None);
    peer.cache.present(&input, &mut view, due + ERROR_BACKOFF);
    assert!(view.value.is_none() && view.message.is_none() && !view.loading);
    peer.cache.schedule([input], due + ERROR_BACKOFF);
    assert!(
        peer.cache.queue.is_empty(),
        "negative results also have a TTL"
    );
}

#[test]
fn cache_fences_auth_scope_removed_branch_and_late_results() {
    let now = Instant::now();
    for change in 0..6 {
        let mut peer = Peer::new();
        peer.cache.seed(input("cached"), fixture().unwrap(), now);
        peer.cache.schedule([input("old")], now);
        peer.cache.poll(now);
        let (generation, _, _) = peer.incoming.try_recv().unwrap();
        let token = peer.cache.token.as_ref().unwrap().clone();
        match change {
            0 => peer.cache.clear(), // sign-out/disconnect
            1 => peer
                .cache
                .scope((0, 1, "boot".into()), Arc::new("other-account".into())),
            2 => peer.cache.scope((1, 1, "boot".into()), token),
            3 => peer.cache.scope((0, 2, "boot".into()), token),
            4 => peer.cache.scope((0, 1, "new-boot".into()), token),
            _ => peer.cache.retain(|input| input.branch == "new"),
        }
        assert!(peer.cache.entries.is_empty());
        assert!(peer.cache.queue.is_empty());
        peer.outgoing
            .send((generation, Ok(Some(fixture().unwrap())), None))
            .unwrap();
        peer.cache.poll(now);
        assert!(
            peer.cache.entries.is_empty(),
            "late result restored sensitive data: {change}"
        );
        assert!(peer.incoming.try_recv().is_err());
    }
}

#[test]
fn signout_drains_private_results_without_starting_queued_work() {
    let mut peer = Peer::new();
    let now = Instant::now();
    peer.cache.schedule([input("active"), input("queued")], now);
    peer.cache.poll(now);
    let (generation, _, _) = peer.incoming.try_recv().unwrap();
    peer.outgoing
        .send((generation, Ok(Some(fixture().unwrap())), None))
        .unwrap();
    peer.cache.clear();
    assert!(!peer.cache.lookup.busy);
    assert!(
        peer.cache
            .lookup
            .worker
            .as_ref()
            .unwrap()
            .results
            .try_recv()
            .is_err()
    );
    assert!(peer.incoming.try_recv().is_err());
    assert!(peer.cache.token.is_none());
}

#[test]
fn cache_is_bounded_lru_and_does_not_refetch_fresh_entries_under_pressure() {
    let mut peer = Peer::new();
    let now = Instant::now();
    peer.cache
        .schedule((0..CACHE_LIMIT * 2).map(|i| input(&i.to_string())), now);
    assert_eq!(peer.cache.queue.len(), CACHE_LIMIT);
    peer.cache.poll(now);
    for _ in 0..CACHE_LIMIT {
        peer.complete(now, Ok(None), None);
    }
    assert_eq!(peer.cache.entries.len(), CACHE_LIMIT);
    assert!(peer.incoming.try_recv().is_err());
    peer.cache.schedule([input("overflow")], now);
    peer.cache.poll(now);
    assert!(
        peer.incoming.try_recv().is_err(),
        "do not evict fresh data to hammer GitHub"
    );
    let mut view = Lookup::default();
    peer.cache
        .present(&input("0"), &mut view, now + Duration::from_secs(1));
    peer.cache.schedule([input("overflow")], now + REFRESH);
    peer.cache.poll(now + REFRESH);
    peer.complete(now + REFRESH, Ok(None), None);
    assert_eq!(peer.cache.entries.len(), CACHE_LIMIT);
    assert!(peer.cache.entries.iter().any(|e| e.input.branch == "0"));
    assert!(!peer.cache.entries.iter().any(|e| e.input.branch == "1"));
    assert!(
        peer.cache
            .entries
            .iter()
            .any(|e| e.input.branch == "overflow")
    );
}

#[test]
fn local_failures_do_not_starve_other_repos_but_rate_limits_pause_account() {
    let mut peer = Peer::new();
    let now = Instant::now();
    peer.cache.schedule(
        [input("local-error"), input("limited"), input("waiting")],
        now,
    );
    peer.cache.poll(now);
    assert_eq!(
        peer.complete(now, Err(Error::PrOrigin), None).branch,
        "local-error"
    );
    assert_eq!(
        peer.complete(
            now,
            Err(Error::GitHubRateLimit),
            Some(Duration::from_secs(3600))
        )
        .branch,
        "limited"
    );
    peer.cache.poll(now + Duration::from_secs(3599));
    assert!(peer.incoming.try_recv().is_err());
    let mut view = Lookup::default();
    peer.cache.present(&input("waiting"), &mut view, now);
    assert!(view.message.as_deref().unwrap().contains("paused"));
    peer.cache.poll(now + Duration::from_secs(3600));
    assert_eq!(
        peer.complete(now + Duration::from_secs(3600), Ok(None), None)
            .branch,
        "waiting"
    );
}

#[test]
fn badge_colors_report_readiness_and_preserve_terminal_lifecycles() {
    let theme = crate::config::Theme::default();
    for (merge, review, checks, expected) in [
        ("CLEAN", "APPROVED", vec![], theme.palette[2]),
        ("CLEAN", "", vec!["SUCCESS", "SKIPPED"], theme.palette[2]),
        ("CLEAN", "", vec!["PENDING"], theme.palette[3]),
        ("CLEAN", "REVIEW_REQUIRED", vec![], theme.palette[3]),
        ("CLEAN", "CHANGES_REQUESTED", vec![], theme.palette[1]),
        (
            "CLEAN",
            "APPROVED",
            vec!["PENDING", "FAILURE"],
            theme.palette[1],
        ),
        (
            "DIRTY",
            "REVIEW_REQUIRED",
            vec!["PENDING"],
            theme.palette[1],
        ),
        ("UNSTABLE", "", vec![], theme.palette[1]),
        ("BLOCKED", "", vec![], theme.palette[208]),
        ("BEHIND", "", vec![], theme.palette[208]),
        ("BLOCKED", "", vec!["PENDING"], theme.palette[3]),
        ("HAS_HOOKS", "", vec![], theme.palette[3]),
        ("UNKNOWN", "", vec![], theme.palette[3]),
        ("FUTURE_STATE", "", vec![], theme.palette[3]),
        ("DRAFT", "", vec![], theme.palette[3]),
    ] {
        let mut wire = response();
        wire[0]["mergeStateStatus"] = merge.into();
        wire[0]["reviewDecision"] = review.into();
        wire[0]["statusCheckRollup"] = checks
            .iter()
            .map(|state| serde_json::json!({"__typename": "StatusContext", "state": state}))
            .collect::<Vec<_>>()
            .into();
        let mut pr = parse(&wire.to_string(), "example", "project", "feature")
            .unwrap()
            .unwrap();
        assert_eq!(pr.color(&theme), expected, "{merge}, {review}, {checks:?}");
        pr.is_draft = true;
        assert_eq!(pr.color(&theme), theme.muted);
        pr.state = State::Merged;
        assert_eq!(pr.color(&theme), theme.palette[5]);
        pr.state = State::Closed;
        assert_eq!(pr.color(&theme), theme.palette[1]);
    }
    // CheckRun status takes priority over a conclusion until completion, and a
    // failed check wins over pending checks and a nominally clean merge state.
    let mut pr = fixture().unwrap();
    pr.merge_state_status = MergeState::Clean;
    pr.review_decision = Default::default();
    assert_eq!(pr.color(&theme), theme.palette[1]);
    pr.status_check_rollup = serde_json::from_value(serde_json::json!([
        {"__typename":"CheckRun", "status":"IN_PROGRESS", "conclusion":"SUCCESS"}
    ]))
    .unwrap();
    assert_eq!(pr.color(&theme), theme.palette[3]);
}

#[test]
fn parses_identity_lifecycle_and_check_categories() {
    let mut pr = fixture().unwrap();
    assert_eq!(pr.checks(), "1 passed / 1 failed / 1 pending");
    assert_eq!(pr.lifecycle(), "Open");
    assert_eq!(pr.review(), "Review required");
    pr.is_draft = true;
    assert_eq!(pr.lifecycle(), "Draft");
    pr.state = State::Merged;
    assert_eq!(pr.lifecycle(), "Merged");
    pr.state = State::Closed;
    assert_eq!(pr.lifecycle(), "Closed");
    pr.status_check_rollup = Some(vec![]);
    assert_eq!(pr.checks(), "No checks reported");
    pr.status_check_rollup = serde_json::from_value(serde_json::json!([
        {"__typename":"CheckRun", "status":"COMPLETED", "conclusion":"SKIPPED"},
        {"__typename":"CheckRun", "status":"COMPLETED", "conclusion":"NEUTRAL"},
        {"__typename":"CheckRun", "status":"COMPLETED", "conclusion":"TIMED_OUT"},
        {"__typename":"CheckRun", "status":"COMPLETED", "conclusion":null}
    ]))
    .unwrap();
    assert_eq!(pr.checks(), "1 failed / 1 pending / 2 skipped");
    for (wire, label) in [
        ("CLEAN", "No merge conflicts"),
        ("DIRTY", "Merge conflicts"),
        ("BEHIND", "Branch behind base"),
        ("BLOCKED", "Merge blocked"),
        ("UNSTABLE", "Checks need attention"),
        ("DRAFT", "Not ready for review"),
        ("HAS_HOOKS", "Merge hooks required"),
        ("UNKNOWN", "Merge status unavailable"),
        // A status added upstream degrades instead of failing the lookup.
        ("FUTURE_VALUE", "Merge status unavailable"),
    ] {
        pr.merge_state_status = MergeState::from(wire.to_owned());
        assert_eq!(pr.merge_status(), label, "{wire}");
    }
}

fn response() -> serde_json::Value {
    serde_json::json!([{
        "number":8, "url":"https://github.com/example/project/pull/8", "title":"Title\n\u{202e}safe",
        "state":"OPEN", "isDraft":false, "headRefName":"feature", "baseRefName":"main",
        "additions":1, "deletions":0, "changedFiles":1, "updatedAt":"now",
        "mergeStateStatus":"UNKNOWN", "reviewDecision":"", "headRepositoryOwner":{"login":"example"}
    }])
}

#[test]
fn native_graphql_normalizes_nullable_reviews_and_bounded_checks() {
    let mut pr = response()[0].clone();
    pr["reviewDecision"] = serde_json::Value::Null;
    pr["commits"] = serde_json::json!({"nodes":[{"commit":{"statusCheckRollup":{"contexts":{
        "pageInfo":{"hasNextPage":true}, "nodes":[{"__typename":"StatusContext","state":"SUCCESS"}]
    }}}}]});
    let response = serde_json::json!({"data":{"repository":{"pullRequests":{"nodes":[pr]}}}});
    let pr = parse_graphql(response.clone(), "example", "project", "feature")
        .unwrap()
        .unwrap();
    assert_eq!(pr.review(), "No review decision");
    assert!(pr.checks_summary.contains("1 passed"));
    assert!(pr.checks_summary.contains("first 100"));
    assert!(parse_graphql(response, "wrong", "project", "feature").is_err());
    assert!(
        parse_graphql(
            serde_json::json!({"data":{"repository":null}}),
            "a",
            "b",
            "c"
        )
        .is_err()
    );
    assert!(
        parse_graphql(
            serde_json::json!({"data":{"repository":{"pullRequests":{"nodes":[]}}}}),
            "a",
            "b",
            "c"
        )
        .unwrap()
        .is_none()
    );
}

#[test]
fn rejects_malformed_ambiguous_oversized_and_mismatched_responses() {
    let parse_value =
        |v: &serde_json::Value| parse(&v.to_string(), "example", "project", "feature");
    assert!(
        parse("[]", "example", "project", "feature")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        parse_value(&response()).unwrap().unwrap().title,
        "Title  safe"
    );
    for (key, value) in [
        (
            "url",
            serde_json::json!("https://github.com.evil.test/example/project/pull/8"),
        ),
        (
            "url",
            serde_json::json!("https://github.com/example/project/pull/9"),
        ),
        (
            "url",
            serde_json::json!("http://github.com/example/project/pull/8"),
        ),
        ("headRefName", serde_json::json!("other")),
        ("headRepositoryOwner", serde_json::json!({"login":"fork"})),
        ("number", serde_json::json!(0)),
        ("additions", serde_json::json!(-1)),
        ("state", serde_json::json!("UNKNOWN")),
        ("isDraft", serde_json::Value::Null),
    ] {
        let mut v = response();
        v[0][key] = value;
        assert!(parse_value(&v).is_err(), "{key}");
    }
    let v = response();
    assert!(parse_value(&serde_json::json!([v[0], v[0]])).is_err());
    assert!(parse("{}", "a", "b", "c").is_err());
    assert!(parse(&" ".repeat(OUTPUT_LIMIT + 1), "a", "b", "c").is_err());
    assert_eq!(clean(&"x".repeat(2000)).len(), 512);
}

#[test]
fn github_origins_are_strictly_validated() {
    assert_eq!(
        crate::avatars::github_repo("git@github.com:Some-Owner/repo.git"),
        Some(("some-owner".into(), "repo".into()))
    );
    // Enterprise managed users own repositories under an `_shortcode` login.
    assert_eq!(
        crate::avatars::github_repo("https://github.com/fabienpenso_microsoft/repo"),
        Some(("fabienpenso_microsoft".into(), "repo".into()))
    );
    for remote in [
        "https://github.com/a/b/c",
        "https://github.com@evil.test/a/b",
        "https://other.test/a/b",
        "https://github.com/a/b?x",
    ] {
        assert!(crate::avatars::github_repo(remote).is_none());
    }
}

#[test]
fn worker_discards_stale_results_and_runs_only_requested_jobs() {
    let (requests, incoming) = mpsc::sync_channel(1);
    let (outgoing, results) = mpsc::sync_channel(1);
    let mut lookup = Lookup::default();
    lookup.worker = Some(Worker { requests, results });
    let input = Input {
        checkout: Some("/fixture".into()),
        repo_key: "/fixture/.git".into(),
        branch: "feature".into(),
    };
    lookup.request(input.clone(), Arc::new("fixture".into()));
    lookup.poll();
    let (old, _, _) = incoming.try_recv().unwrap();
    lookup.clear();
    lookup.request(input, Arc::new("fixture".into()));
    lookup.poll();
    assert!(incoming.try_recv().is_err(), "single in-flight request");
    outgoing
        .send((old, Ok(Some(fixture().unwrap())), None))
        .unwrap();
    lookup.poll();
    assert!(lookup.value.is_none());
    assert!(lookup.loading);
    let (current, _, _) = incoming.try_recv().unwrap();
    outgoing
        .send((current, Ok(Some(fixture().unwrap())), None))
        .unwrap();
    assert!(lookup.poll());
    assert!(!lookup.loading);
    assert_eq!(lookup.value.as_ref().unwrap().number, 8);
    assert!(!lookup.poll());
    assert!(incoming.try_recv().is_err(), "no automatic polling");
    lookup.clear();
    assert!(lookup.value.is_none());
}

#[test]
fn worktree_registry_requires_unique_exact_branch_and_absolute_checkout() {
    let checkout = std::env::temp_dir().join("repo with spaces\nline");
    let checkout = checkout.to_str().unwrap();
    let entry = format!("worktree {checkout}\0HEAD abc\0branch refs/heads/feature\0\0");
    assert_eq!(worktree_checkout(&entry, "feature").unwrap(), checkout);
    assert!(worktree_checkout(&entry, "feat").is_err());
    assert!(
        worktree_checkout(&entry.repeat(2), "feature")
            .unwrap_err()
            .to_string()
            .contains("Multiple")
    );
    for invalid in [
        "worktree relative\0branch refs/heads/feature\0\0".to_owned(),
        format!("worktree {checkout}\0HEAD abc\0detached\0\0"),
        format!("worktree {checkout}\0branch refs/remotes/feature\0\0"),
        format!("worktree {checkout}\0bare\0\0"),
    ] {
        assert!(worktree_checkout(&invalid, "feature").is_err());
    }
}

#[cfg(unix)]
#[test]
fn subprocess_success_errors_limits_timeout_and_cancellation() {
    let deadline = || Instant::now() + Duration::from_secs(5);
    let mut command = Command::new("/bin/sh");
    command.args([
        "-c",
        "printf '%s' \"$GH_HOST:$GH_PROMPT_DISABLED:$GIT_TERMINAL_PROMPT\"",
    ]);
    assert_eq!(
        run(&mut command, deadline(), &|| false).unwrap(),
        (true, "github.com:1:0".into())
    );
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "printf failure >&2; exit 1"]);
    assert_eq!(
        run(&mut command, deadline(), &|| false).unwrap(),
        (false, "failure".into())
    );
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "printf '\\377'"]);
    assert!(
        run(&mut command, deadline(), &|| false).is_err(),
        "non-UTF8 Git paths must fail closed, not be lossily mapped"
    );
    // Draining OUTPUT_LIMIT costs one 10ms sleep per WouldBlock, so the wall
    // time scales with the host's socketpair buffer size. Give the limit its
    // own generous deadline: this asserts that oversized output is rejected,
    // not how fast the host refills a socket. Timeouts are asserted below.
    assert!(
        run(
            &mut Command::new("/usr/bin/yes"),
            Instant::now() + Duration::from_secs(60),
            &|| false
        )
        .unwrap_err()
        .to_string()
        .contains("size limit")
    );
    let mut sleep = Command::new("/bin/sleep");
    sleep.arg("5");
    assert!(
        run(
            &mut sleep,
            Instant::now() + Duration::from_millis(30),
            &|| false
        )
        .unwrap_err()
        .to_string()
        .contains("timed out")
    );
    assert!(
        run(&mut Command::new("/not/an/executable"), deadline(), &|| {
            true
        })
        .unwrap_err()
        .to_string()
        .contains("cancelled")
    );
    assert!(
        run(&mut Command::new("/not/an/executable"), deadline(), &|| {
            false
        })
        .unwrap_err()
        .to_string()
        .contains("install git")
    );
    let calls = std::cell::Cell::new(0);
    let mut sleep = Command::new("/bin/sleep");
    sleep.arg("5");
    assert!(
        run(&mut sleep, deadline(), &|| {
            calls.set(calls.get() + 1);
            calls.get() > 1
        })
        .unwrap_err()
        .to_string()
        .contains("cancelled")
    );
}

#[test]
fn local_git_verification_rejects_wrong_checkout_branch_and_remote_before_gh() {
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("herdr-pr-{}-{suffix}", std::process::id());
    let directory = Directory(std::env::temp_dir().join(name));
    std::fs::create_dir(&directory.0).unwrap();
    let git = |args: &[&str]| {
        let mut command = Command::new("git");
        command.arg("-C").arg(&directory.0).args(args);
        let (ok, text) = run(&mut command, Instant::now() + TIMEOUT, &|| false).unwrap();
        assert!(ok, "fixture git failed: {text}");
    };
    git(&["init", "--quiet", "--template=", "-b", "feature"]);
    git(&[
        "config",
        "--local",
        "remote.origin.url",
        "https://unsupported.invalid/example/project.git",
    ]);
    let mut input = Input {
        checkout: Some(directory.0.to_str().unwrap().into()),
        repo_key: directory.0.join(".git").to_str().unwrap().into(),
        branch: "feature".into(),
    };
    assert!(
        fetch(&input, &"fixture".into(), || false)
            .unwrap_err()
            .to_string()
            .contains("GitHub.com origins only")
    );
    let mut registry_input = input.clone();
    registry_input.checkout = None;
    assert!(
        fetch(&registry_input, &"fixture".into(), || false)
            .unwrap_err()
            .to_string()
            .contains("GitHub.com origins only")
    );
    git(&[
        "config",
        "--local",
        "remote.origin.url",
        "https://github.com/example/project.git",
    ]);
    assert_eq!(
        local_repository(&registry_input, Instant::now() + TIMEOUT, &|| false).unwrap(),
        ("example".into(), "project".into())
    );
    registry_input.branch = "missing".into();
    assert!(
        local_repository(&registry_input, Instant::now() + TIMEOUT, &|| false)
            .unwrap_err()
            .to_string()
            .contains("No local worktree")
    );
    input.branch = "other".into();
    assert!(
        fetch(&input, &"fixture".into(), || false)
            .unwrap_err()
            .to_string()
            .contains("branch changed")
    );
    input.repo_key = directory.0.to_str().unwrap().into();
    assert!(
        fetch(&input, &"fixture".into(), || false)
            .unwrap_err()
            .to_string()
            .contains("does not match daemon metadata")
    );
    input.checkout = Some("relative".into());
    assert!(
        fetch(&input, &"fixture".into(), || false)
            .unwrap_err()
            .to_string()
            .contains("absolute checkout")
    );
}

#[test]
fn remote_host_drops_credentials_and_path() {
    for (remote, host) in [
        ("git@github.com:owner/repo.git", "github.com"),
        ("github-work:owner_shortcode/repo", "github-work"),
        (
            "https://user:secret@github.example.com/owner/repo",
            "github.example.com",
        ),
        ("ssh://git@gitlab.com:22/owner/repo", "gitlab.com:22"),
    ] {
        assert_eq!(remote_host(remote), host, "{remote}");
    }
}
