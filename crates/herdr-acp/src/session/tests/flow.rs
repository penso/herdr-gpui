use super::*;

#[test]
fn a_turn_streams_into_one_agent_entry_and_ends_idle() {
    let mut harness = ready();
    assert_eq!(harness.transcript().status, Status::Idle);
    assert_eq!(harness.transcript().session.as_ref().unwrap().id, "s1");
    assert_eq!(
        harness.transcript().modes.as_ref().unwrap().current,
        "default"
    );
    let id = prompt(&mut harness, 0, "fix the bug");
    assert_eq!(harness.transcript().status, Status::Working);
    harness.agent_says("On ");
    harness.agent_says("it.");
    harness.respond(id, json!({"stopReason": "end_turn"}));
    let entries = &harness.transcript().entries;
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].kind.text(), Some("fix the bug"));
    assert_eq!(entries[1].kind.text(), Some("On it."));
    let EntryKind::Turn(turn) = &entries[2].kind else {
        panic!("the turn ends with its summary");
    };
    // Lists say how long ago a chat last answered from this.
    assert!(turn.ended_ms.is_some_and(|ended| ended >= turn.duration_ms));
    assert_eq!(harness.transcript().status, Status::Idle);
    assert_eq!(
        *harness.reports.borrow(),
        [Activity::Idle, Activity::Working, Activity::Idle]
    );
}

#[test]
fn the_prompt_request_carries_the_text_and_session() {
    let mut harness = ready();
    harness.command(
        0,
        Command::Prompt {
            text: "hello".to_owned(),
            attachments: Vec::new(),
        },
    );
    let request = harness.last_request("session/prompt");
    assert_eq!(request["params"]["sessionId"], "s1");
    assert_eq!(
        request["params"]["prompt"],
        json!([{"type": "text", "text": "hello"}])
    );
}

#[test]
fn tool_calls_split_the_agent_text_and_take_updates() {
    let mut harness = ready();
    let id = prompt(&mut harness, 0, "test");
    harness.agent_says("Running tests.");
    harness.update(json!({"sessionUpdate": "tool_call", "toolCallId": "t1",
        "title": "cargo test", "kind": "execute", "status": "pending"}));
    harness.update(
        json!({"sessionUpdate": "tool_call_update", "toolCallId": "t1",
        "status": "completed", "content": [{"type": "diff", "path": "/work/a.rs",
        "oldText": "a", "newText": "b"}]}),
    );
    harness.agent_says("All green.");
    harness.respond(id, json!({"stopReason": "end_turn"}));
    let kinds: Vec<_> = harness
        .transcript()
        .entries
        .iter()
        .map(|entry| match &entry.kind {
            EntryKind::User { .. } => "user",
            EntryKind::Agent { .. } => "agent",
            EntryKind::Turn(_) => "turn",
            EntryKind::Tool(tool) => {
                assert_eq!(tool.status, herdr_chat_protocol::ToolStatus::Completed);
                assert_eq!(tool.content.len(), 1);
                "tool"
            }
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["user", "agent", "tool", "agent", "turn"]);
}

#[test]
fn updates_for_another_session_or_unknown_kinds_are_ignored() {
    let mut harness = ready();
    harness.input(Input::Agent(Inbound::Notification {
        method: "session/update".to_owned(),
        params: json!({"sessionId": "other", "update": {"sessionUpdate": "agent_message_chunk",
            "content": {"type": "text", "text": "x"}}}),
    }));
    harness.update(json!({"sessionUpdate": "from_the_future", "x": 1}));
    assert!(harness.transcript().entries.is_empty());
}

#[test]
fn how_a_turn_ended_is_summarised_and_a_failure_explained() {
    let mut harness = ready();
    let id = prompt(&mut harness, 0, "go");
    harness.respond(id, json!({"stopReason": "max_tokens"}));
    let last_end = |harness: &Harness| match &harness.transcript().entries.last().unwrap().kind {
        EntryKind::Turn(summary) => summary.end,
        kind => panic!("not a turn summary: {kind:?}"),
    };
    assert_eq!(last_end(&harness), herdr_chat_protocol::TurnEnd::Limit);
    assert!(harness.notices().is_empty());
    let id = prompt(&mut harness, 0, "go");
    harness.fail(id, wire::Error::internal_error());
    assert_eq!(last_end(&harness), herdr_chat_protocol::TurnEnd::Failed);
    assert_eq!(harness.notices()[0].0, NoticeLevel::Error);
    assert_eq!(harness.transcript().status, Status::Idle);
}

#[test]
fn authentication_errors_say_how_to_sign_in() {
    let mut harness = harness_with(config());
    harness.session.start();
    let id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(id, json!({"protocolVersion": 1, "agentCapabilities": {}}));
    let id = harness.last_request("session/new")["id"].as_i64().unwrap();
    harness.fail(id, wire::Error::auth_required());
    let Status::Failed { message } = &harness.transcript().status else {
        panic!("expected failure");
    };
    assert!(message.contains("/login"), "{message}");
    harness.command(
        0,
        Command::Prompt {
            text: "hi".to_owned(),
            attachments: Vec::new(),
        },
    );
    assert_eq!(
        harness.session.handled.back().unwrap().1,
        Some(Rejection::NotReady)
    );
}

#[test]
fn load_continues_a_session_when_the_agent_supports_it() {
    let mut harness = harness_with(Config {
        load: Some("old".to_owned()),
        ..config()
    });
    harness.session.start();
    let id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(
        id,
        json!({"protocolVersion": 1, "agentCapabilities": {"loadSession": true}}),
    );
    let load = harness.last_request("session/load");
    assert_eq!(load["params"]["sessionId"], "old");
    // History replays before the load answers.
    harness.input(Input::Agent(Inbound::Notification {
        method: "session/update".to_owned(),
        params: json!({"sessionId": "old", "update": {"sessionUpdate": "user_message_chunk",
            "content": {"type": "text", "text": "earlier"}}}),
    }));
    harness.respond(load["id"].as_i64().unwrap(), json!({}));
    assert_eq!(harness.transcript().entries[0].kind.text(), Some("earlier"));
    assert_eq!(harness.transcript().session.as_ref().unwrap().id, "old");
    assert_eq!(harness.transcript().status, Status::Idle);
}

#[test]
fn load_falls_back_to_a_new_session_with_a_warning() {
    let mut harness = harness_with(Config {
        load: Some("old".to_owned()),
        ..config()
    });
    harness.session.start();
    let id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(id, json!({"protocolVersion": 1, "agentCapabilities": {}}));
    harness.last_request("session/new");
    assert_eq!(harness.notices()[0].0, NoticeLevel::Warning);
}

#[test]
fn agent_exit_ends_the_session_and_headless_mode_exits_with_it() {
    let mut harness = ready();
    let flow = harness.input(Input::AgentExited {
        code: Some(1),
        stderr: vec!["boom".to_owned()],
    });
    assert_eq!(flow, Flow::Continue);
    assert!(matches!(harness.transcript().status, Status::Exited { .. }));
    assert!(harness.notices()[0].1.contains("boom"));

    let mut headless = harness_with(Config {
        exit_with_agent: true,
        ..config()
    });
    let receiver = headless.subscribe(1, false, None);
    let flow = headless.input(Input::AgentExited {
        code: Some(0),
        stderr: Vec::new(),
    });
    assert_eq!(flow, Flow::Exit);
    assert!(matches!(
        drain(&receiver).last(),
        Some(ServerMessage::Bye {
            reason: ByeReason::Exiting
        })
    ));
}

#[test]
fn a_stalled_agent_fails_the_session_instead_of_blocking() {
    let (sender, _agent) = bounded(1);
    let mut session = Session::new(config(), Outbox(sender), |_| {});
    session.start();
    session.handle(Input::Agent(Inbound::Response {
        id: wire::RequestId::Number(1),
        result: Ok(json!({"protocolVersion": 1, "agentCapabilities": {}})),
    }));
    assert!(matches!(session.transcript().status, Status::Failed { .. }));
}

#[test]
fn the_transcript_stays_bounded() {
    let mut harness = ready();
    let id = prompt(&mut harness, 0, "go");
    for index in 0..MAX_ENTRIES + 10 {
        harness.update(
            json!({"sessionUpdate": "tool_call", "toolCallId": format!("t{index}"),
            "title": "x"}),
        );
    }
    harness.respond(id, json!({"stopReason": "end_turn"}));
    let transcript = harness.transcript();
    assert!(transcript.entries.len() <= MAX_ENTRIES);
    assert!(transcript.truncated);
    // A dropped call's update starts a new entry instead of failing.
    harness.update(
        json!({"sessionUpdate": "tool_call_update", "toolCallId": "t0",
        "status": "completed"}),
    );
    assert!(harness.transcript().entries.len() <= MAX_ENTRIES);

    let mut harness = ready();
    let id = prompt(&mut harness, 0, "go");
    let chunk = "x".repeat(200 * 1024);
    for index in 0..20 {
        harness.update(
            json!({"sessionUpdate": "agent_message_chunk", "messageId": format!("m{index}"),
            "content": {"type": "text", "text": chunk}}),
        );
    }
    harness.respond(id, json!({"stopReason": "end_turn"}));
    let total: usize = harness.transcript().entries.iter().map(entry_size).sum();
    assert!(total <= MAX_TRANSCRIPT_TEXT, "{total}");
}
