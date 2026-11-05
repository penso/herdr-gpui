use super::*;
use sheprd_chat_protocol::{Choice, ConfigOption, Mode, Modes};

fn set_mode(id: &str) -> Command {
    Command::SetMode { id: id.into() }
}

fn with_modes(current: &str) -> Transcript {
    let mut transcript = Transcript::new("claude");
    transcript.modes = Some(Modes {
        available: ["default", "plan", "bypassPermissions"]
            .into_iter()
            .map(|id| Mode {
                description: None,
                id: id.into(),
                name: id.into(),
            })
            .collect(),
        current: current.into(),
    });
    transcript
}

fn with_option(category: OptionCategory) -> Transcript {
    let choice = |id: &str| Choice {
        description: None,
        group: None,
        id: id.into(),
        name: id.into(),
    };
    let mut transcript = Transcript::new("opencode");
    transcript.options = vec![ConfigOption {
        category,
        description: None,
        id: "mode".into(),
        kind: OptionKind::Select {
            choices: vec![choice("build"), choice("plan")],
            current: "build".into(),
        },
        name: "Mode".into(),
    }];
    transcript
}

fn set_option(choice: &str) -> Command {
    Command::SetOption {
        id: "mode".into(),
        value: OptionValue::Choice { id: choice.into() },
    }
}

#[test]
fn a_new_chat_takes_the_last_mode_and_auto_accept() {
    let mut last = LastModes::default();
    last.remember("claude", set_mode("plan"));
    last.remember("claude", set_mode("bypassPermissions"));
    last.remember("claude", Command::SetAutoAccept { on: true });
    assert_eq!(
        last.commands("claude", &with_modes("default")),
        vec![
            set_mode("bypassPermissions"),
            Command::SetAutoAccept { on: true }
        ]
    );
}

#[test]
fn each_agent_keeps_its_own_choice() {
    let mut last = LastModes::default();
    last.remember("claude", set_mode("plan"));
    assert!(last.commands("codex", &with_modes("default")).is_empty());
}

#[test]
fn a_chat_already_there_or_without_the_mode_gets_nothing() {
    let mut last = LastModes::default();
    last.remember("claude", set_mode("plan"));
    assert!(last.commands("claude", &with_modes("plan")).is_empty());
    last.remember("claude", set_mode("gone"));
    assert!(last.commands("claude", &with_modes("default")).is_empty());
}

#[test]
fn the_mode_setting_follows_like_the_older_modes() {
    let transcript = with_option(OptionCategory::Mode);
    assert!(picked(&transcript, &set_option("plan")));
    let mut last = LastModes::default();
    last.remember("opencode", set_option("plan"));
    assert_eq!(
        last.commands("opencode", &transcript),
        vec![set_option("plan")]
    );
    last.remember("opencode", set_option("build"));
    assert!(last.commands("opencode", &transcript).is_empty());
}

#[test]
fn only_mode_choices_are_followed() {
    let other = with_option(OptionCategory::Model);
    assert!(!picked(&other, &set_option("plan")));
    assert!(!picked(&other, &Command::Cancel));
    assert!(picked(&other, &set_mode("plan")));
    assert!(picked(&other, &Command::SetAutoAccept { on: false }));
}

#[test]
fn choices_too_long_to_keep_are_refused() {
    let mut last = LastModes::default();
    last.remember("claude", set_mode("plan"));
    last.remember("claude", set_mode(&"x".repeat(MAX_FIELD_BYTES + 1)));
    last.remember(&"x".repeat(MAX_FIELD_BYTES + 1), set_mode("plan"));
    last.remember("", set_mode("plan"));
    assert_eq!(last.agents.len(), 1);
    assert_eq!(
        last.commands("claude", &with_modes("default")),
        vec![set_mode("plan")]
    );
}

#[test]
fn the_least_recently_changed_agent_goes_first() {
    let mut last = LastModes::default();
    for index in 0..=MAX_AGENTS {
        last.remember(&format!("agent-{index}"), set_mode("plan"));
    }
    assert_eq!(last.agents.len(), MAX_AGENTS);
    assert!(last.agents.iter().all(|kept| kept.agent != "agent-0"));
}

#[test]
fn choices_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    let mut last = LastModes::at(Some(path.clone()));
    last.remember("claude", set_mode("bypassPermissions"));
    last.remember("claude", Command::SetAutoAccept { on: true });
    last.finish();
    let restored = LastModes::at(Some(path));
    assert_eq!(
        restored.commands("claude", &with_modes("default")),
        vec![
            set_mode("bypassPermissions"),
            Command::SetAutoAccept { on: true }
        ]
    );
}

#[test]
fn a_file_cannot_replay_anything_but_a_mode() {
    let saved = serde_json::json!({"agents": [
        {"agent": "claude", "mode": {"command": "cancel"}},
        {"agent": "codex", "mode": {"command": "set_mode", "id": "plan"}},
    ]});
    let agents = parse(&serde_json::to_vec(&saved).unwrap()).unwrap();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].agent, "codex");
}

#[test]
fn a_damaged_file_starts_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    std::fs::write(&path, "not json").unwrap();
    assert!(LastModes::at(Some(path)).agents.is_empty());
}

#[test]
fn the_most_a_store_keeps_fits_the_file_it_reads_back() {
    let long = "\"".repeat(MAX_FIELD_BYTES);
    let agents = (0..MAX_AGENTS)
        .map(|index| Last {
            agent: format!("{index:03}{}", &long[3..]),
            auto_accept: true,
            mode: Some(Command::SetOption {
                id: long.clone(),
                value: OptionValue::Choice { id: long.clone() },
            }),
        })
        .collect();
    let bytes = serde_json::to_vec(&Saved { agents }).unwrap();
    assert!(bytes.len() as u64 <= MAX_FILE_BYTES);
}

fn target(endpoint: &EndpointId, pane: &str) -> Target {
    Target {
        endpoint: endpoint.clone(),
        existing: None,
        pane_id: pane.into(),
        route: super::super::Route::Local,
        surface: super::super::Surface::Herdr,
    }
}

#[test]
fn a_fresh_chat_is_known_by_its_endpoint_or_its_pane() {
    let endpoint = EndpointId::random();
    let chat = target(&endpoint, "w1:p9");
    assert!(Fresh::Endpoint(endpoint).is(&chat));
    assert!(Fresh::Pane("w1:p9".into()).is(&chat));
    assert!(!Fresh::Endpoint(EndpointId::random()).is(&chat));
    assert!(!Fresh::Pane("w1:p1".into()).is(&chat));
}

#[test]
fn expected_chats_are_bounded_and_expire() {
    let now = Instant::now();
    let mut chats = Chats::default();
    for index in 0..=super::super::MAX_CHATS {
        chats.expect_fresh(Fresh::Pane(format!("p{index}")), now);
    }
    assert_eq!(chats.fresh.len(), super::super::MAX_CHATS);
    assert!(!chats.awaits(&Fresh::Pane("p0".into())));
    chats.settle_fresh(None, now + SETTLE - Duration::from_secs(1));
    assert_eq!(chats.fresh.len(), super::super::MAX_CHATS);
    chats.settle_fresh(None, now + SETTLE);
    assert!(chats.fresh.is_empty());
}
