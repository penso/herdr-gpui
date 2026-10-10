use super::*;

#[test]
fn canonical_keys_match_agent_launcher() {
    let key = item(&github(), "pr/369", "Find bar").key;
    assert_eq!(key.source.canonical(), "github:github.com:penso/herdr-gpui");
    assert_eq!(key.canonical(), "github:github.com:penso/herdr-gpui:pr/369");
}

#[test]
fn canonical_keys_stay_injective_across_components() {
    let left = ItemKey {
        source: SourceKey {
            repository: "acme:widgets".into(),
            ..github()
        },
        native_id: "1".into(),
    };
    let right = ItemKey {
        source: SourceKey {
            repository: "acme".into(),
            ..github()
        },
        native_id: "widgets:1".into(),
    };
    assert_ne!(left.canonical(), right.canonical());
}

#[test]
fn canonical_keys_escape_the_escape_marker() {
    let escaped = SourceKey {
        repository: "acme%3Awidgets".into(),
        ..github()
    };
    let colon = SourceKey {
        repository: "acme:widgets".into(),
        ..github()
    };
    assert_eq!(escaped.canonical(), "github:github.com:acme%253Awidgets");
    assert_ne!(escaped.canonical(), colon.canonical());
}

#[test]
fn providers_and_owners_round_trip_and_reject_unknown_words() {
    for provider in [Provider::Github, Provider::Gitlab, Provider::Beads] {
        assert_eq!(provider.as_str().parse::<Provider>().unwrap(), provider);
    }
    for owner in [Owner::AgentLauncher, Owner::HerdrGpui] {
        assert_eq!(owner.as_str().parse::<Owner>().unwrap(), owner);
    }
    assert!(matches!("jira".parse::<Provider>(), Err(Error::Provider(word)) if word == "jira"));
    assert!(matches!("tmux".parse::<Owner>(), Err(Error::Owner(word)) if word == "tmux"));
}

#[test]
fn run_states_and_workspaces_use_agent_launcher_json() {
    assert_eq!(
        serde_json::to_string(&RunState::NeedsInput).unwrap(),
        "\"needs_input\""
    );
    let workspace: Workspace = serde_json::from_str(
        r#"{"backend":"herdr","id":"w95","host":null,"path":"/tmp/wt","branch":"agent/x"}"#,
    )
    .unwrap();
    assert_eq!(workspace.backend, Backend::Herdr);
    assert_eq!(
        workspace.path.as_deref(),
        Some(std::path::Path::new("/tmp/wt"))
    );
}

#[test]
fn checkpoints_keep_agent_launcher_markers() {
    let json = r#"{"etag":null,"last_full_at":"2026-10-09T02:24:37.055082Z","pr_cursor":358,"pr_details":{"pr/136":"activity-v1:x"},"updated_at":null}"#;
    let checkpoint: Checkpoint = serde_json::from_str(json).unwrap();
    assert_eq!(checkpoint.pr_cursor, Some(358));
    assert_eq!(checkpoint.pr_details["pr/136"], "activity-v1:x");
    assert_eq!(
        checkpoint.last_full_at,
        Some(at("2026-10-09T02:24:37.055082Z"))
    );
}

#[test]
fn canonical_keys_parse_back_and_stand_in_for_unlisted_items() {
    for key in [
        item(&github(), "pr/369", "Find bar").key,
        ItemKey {
            source: SourceKey {
                repository: "acme:wid%gets".into(),
                ..github()
            },
            native_id: "a:b%3A".into(),
        },
    ] {
        assert_eq!(key.canonical().parse::<ItemKey>().unwrap(), key);
    }
    for bad in [
        "github:github.com:penso/herdr-gpui",
        "nope:h:r:1",
        "github:h:r:",
    ] {
        assert!(bad.parse::<ItemKey>().is_err(), "{bad}");
    }
    let pr = Item::stand_in(item(&github(), "pr/369", "Find bar").key);
    assert_eq!(
        (pr.identifier.as_str(), pr.state.as_str()),
        ("#369", "unlisted")
    );
}
