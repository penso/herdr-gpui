use super::*;

#[test]
fn a_permission_answered_from_one_client_resolves_everywhere() {
    let mut harness = ready();
    let gui = harness.subscribe(1, false, None);
    let local = harness.subscribe(0, true, None);
    prompt(&mut harness, 0, "test");
    let permission = ask_permission(&mut harness, 77);
    assert!(harness.transcript().is_blocked());
    assert_eq!(harness.reports.borrow().last(), Some(&Activity::Blocked));
    let asked = harness.transcript().permissions[0].clone();
    assert_eq!(asked.title, "Run cargo test");
    assert_eq!(asked.options.len(), 2);

    harness.command(
        1,
        Command::Answer {
            option: "allow".to_owned(),
            permission,
        },
    );
    let response = harness
        .sent()
        .into_iter()
        .find(|message| message["id"] == 77)
        .unwrap();
    assert_eq!(
        response["result"]["outcome"],
        json!({"outcome": "selected", "optionId": "allow"})
    );
    assert!(!harness.transcript().is_blocked());
    assert_eq!(harness.reports.borrow().last(), Some(&Activity::Working));
    for receiver in [&gui, &local] {
        assert!(drain(receiver).iter().any(|message| matches!(
            message,
            ServerMessage::Change { change: Change::PermissionResolved { id }, .. } if *id == permission
        )));
    }

    // The in-pane view answering a moment later finds nothing to answer.
    harness.command(
        0,
        Command::Answer {
            option: "deny".to_owned(),
            permission,
        },
    );
    assert_eq!(
        harness.session.handled.back().unwrap().1,
        Some(Rejection::UnknownPermission)
    );
    assert!(harness.sent().is_empty());
}

#[test]
fn an_unknown_option_is_refused_and_the_question_stays() {
    let mut harness = ready();
    prompt(&mut harness, 0, "test");
    let permission = ask_permission(&mut harness, 5);
    harness.command(
        0,
        Command::Answer {
            option: "maybe".to_owned(),
            permission,
        },
    );
    assert_eq!(
        harness.session.handled.back().unwrap().1,
        Some(Rejection::UnknownOption)
    );
    assert!(harness.transcript().is_blocked());
}

#[test]
fn cancelling_declines_every_pending_question() {
    let mut harness = ready();
    prompt(&mut harness, 0, "test");
    ask_permission(&mut harness, 5);
    ask_permission(&mut harness, 6);
    harness.command(0, Command::Cancel);
    let responses: Vec<_> = harness
        .sent()
        .into_iter()
        .filter(|message| message.get("result").is_some())
        .map(|message| {
            (
                message["id"].clone(),
                message["result"]["outcome"]["outcome"].clone(),
            )
        })
        .collect();
    assert_eq!(
        responses,
        [
            (json!(5), json!("cancelled")),
            (json!(6), json!("cancelled"))
        ]
    );
    assert!(!harness.transcript().is_blocked());
}

#[test]
fn the_permission_names_the_tool_call_it_is_about() {
    let mut harness = ready();
    prompt(&mut harness, 0, "test");
    harness.update(json!({"sessionUpdate": "tool_call", "toolCallId": "call-1",
        "title": "cargo test", "kind": "execute"}));
    ask_permission(&mut harness, 5);
    let tools = harness
        .transcript()
        .entries
        .iter()
        .filter(|entry| matches!(entry.kind, EntryKind::Tool(_)))
        .count();
    assert_eq!(tools, 1);
    assert_eq!(
        harness.transcript().permissions[0].call_id.as_deref(),
        Some("call-1")
    );
}

#[test]
fn other_agent_requests_are_refused() {
    let mut harness = ready();
    harness.input(Input::Agent(Inbound::Request {
        id: wire::RequestId::Number(9),
        method: "fs/read_text_file".to_owned(),
        params: json!({"sessionId": "s1", "path": "/etc/passwd"}),
    }));
    let response = harness.sent().pop().unwrap();
    assert_eq!(response["id"], 9);
    assert_eq!(response["error"]["code"], -32601);
    harness.input(Input::Agent(Inbound::Request {
        id: wire::RequestId::Number(10),
        method: "session/request_permission".to_owned(),
        params: json!({"sessionId": "other", "toolCall": {"toolCallId": "x"}, "options": []}),
    }));
    let response = harness.sent().pop().unwrap();
    assert_eq!(response["error"]["code"], -32602);
    assert!(!harness.transcript().is_blocked());
}
