use super::*;
use crate::control::pull_requests::{MAX_PULL_REQUESTS, PullRequests, Tone, WorkspacePullRequest};

fn snapshot() -> PullRequests {
    PullRequests {
        boot_id: "boot".into(),
        entries: vec![WorkspacePullRequest {
            workspace_id: "w1".into(),
            cwd: "/code/app".into(),
            branch: "topic".into(),
            number: 850,
            url: "https://github.com/penso/herdr-gpui/pull/850".into(),
            tone: Tone::Held,
        }],
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
