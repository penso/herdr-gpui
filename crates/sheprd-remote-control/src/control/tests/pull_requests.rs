use super::*;
use crate::control::pull_requests::{
    MAX_PULL_REQUESTS, PullRequests, Tone, UnopenedBranch, WorkspacePullRequest,
};

fn snapshot() -> PullRequests {
    PullRequests {
        boot_id: "boot".into(),
        entries: vec![WorkspacePullRequest {
            workspace_id: "w1".into(),
            cwd: "/code/app".into(),
            branch: "topic".into(),
            number: 850,
            url: "https://github.com/sheprd-labs/sheprd/pull/850".into(),
            tone: Tone::Held,
        }],
        unopened: Vec::new(),
    }
}

#[test]
fn pr_presentation_round_trips_and_unknown_tones_are_muted() {
    let request = Request::PullRequests {
        snapshot: snapshot(),
    };
    let encoded = serde_json::to_string(&request).unwrap();
    assert_eq!(serde_json::from_str::<Request>(&encoded).unwrap(), request);
    assert_eq!(
        serde_json::from_str::<Tone>("\"future\"").unwrap(),
        Tone::Muted
    );
}

#[test]
fn badges_are_bounded_and_links_cannot_execute_script_or_carry_credentials() {
    let mut value = snapshot();
    assert!(value.valid());
    for url in [
        "javascript:alert(1)",
        "https://user:secret@example.com/pr/1",
        "file:///secret",
    ] {
        value.entries[0].url = url.into();
        assert!(!value.valid());
    }
    value = snapshot();
    value.entries.push(value.entries[0].clone());
    assert!(!value.valid(), "duplicate workspace identity");
    value.entries = (0..=MAX_PULL_REQUESTS)
        .map(|n| {
            let mut entry = snapshot().entries.remove(0);
            entry.workspace_id = format!("w{n}");
            entry
        })
        .collect();
    assert!(!value.valid());
}

fn unopened(id: &str) -> UnopenedBranch {
    UnopenedBranch {
        workspace_id: id.into(),
        cwd: "/code/app".into(),
        branch: "topic".into(),
        commits: 3,
    }
}

#[test]
fn branches_waiting_for_a_pull_request_are_bounded() {
    let mut value = snapshot();
    value.unopened.push(unopened("w2"));
    assert!(value.valid());
    value.unopened.push(unopened("w2"));
    assert!(!value.valid(), "duplicate workspace identity");
    value.unopened = vec![UnopenedBranch {
        commits: 0,
        ..unopened("w2")
    }];
    assert!(!value.valid(), "nothing waiting");
    value.unopened = (0..=MAX_PULL_REQUESTS)
        .map(|n| unopened(&format!("w{n}")))
        .collect();
    assert!(!value.valid());
}

#[test]
fn an_older_app_sends_no_waiting_branches() {
    let value: PullRequests = serde_json::from_str(r#"{"boot_id":"boot","entries":[]}"#).unwrap();
    assert!(value.unopened.is_empty());
    assert!(value.valid());
}
