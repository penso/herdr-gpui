use super::*;

fn request(harness: &mut Harness, request: i64, options: Value) {
    harness.input(Input::Agent(Inbound::Request {
        id: wire::RequestId::Number(request),
        method: "session/request_permission".to_owned(),
        params: json!({"sessionId": "s1",
            "toolCall": {"toolCallId": format!("call-{request}"), "title": "Edit", "kind": "edit"},
            "options": options}),
    }));
}

fn answer_to(harness: &Harness, request: i64) -> Option<Value> {
    harness
        .sent()
        .into_iter()
        .find(|message| message["id"] == request)
        .map(|message| message["result"]["outcome"].clone())
}

fn selected(option: &str) -> Value {
    json!({"outcome": "selected", "optionId": option})
}

#[test]
fn turning_it_on_answers_what_waits_and_what_comes() {
    let mut harness = ready();
    let gui = harness.subscribe(1, false, None);
    prompt(&mut harness, 0, "edit");
    let waiting = ask_permission(&mut harness, 10);
    assert!(harness.transcript().is_blocked());
    harness.command(1, Command::SetAutoAccept { on: true });
    assert_eq!(answer_to(&harness, 10), Some(selected("allow")));
    assert!(harness.transcript().auto_accept);
    assert!(!harness.transcript().is_blocked());
    assert!(drain(&gui).iter().any(|message| matches!(
        message,
        ServerMessage::Change { change: Change::PermissionResolved { id }, .. } if *id == waiting
    )));

    // A later request is answered as it comes, preferring once to always.
    request(
        &mut harness,
        11,
        json!([
            {"optionId": "always", "name": "Always", "kind": "allow_always"},
            {"optionId": "once", "name": "Once", "kind": "allow_once"},
            {"optionId": "no", "name": "No", "kind": "reject_once"}]),
    );
    assert_eq!(answer_to(&harness, 11), Some(selected("once")));
    assert!(harness.transcript().permissions.is_empty());
}

#[test]
fn a_request_without_an_allow_option_still_waits() {
    let mut harness = ready();
    prompt(&mut harness, 0, "edit");
    harness.command(0, Command::SetAutoAccept { on: true });
    request(
        &mut harness,
        12,
        json!([{"optionId": "no", "name": "No", "kind": "reject_once"}]),
    );
    assert_eq!(answer_to(&harness, 12), None);
    assert!(harness.transcript().is_blocked());
}

#[test]
fn turning_it_off_asks_again() {
    let mut harness = ready();
    prompt(&mut harness, 0, "edit");
    harness.command(0, Command::SetAutoAccept { on: true });
    harness.command(0, Command::SetAutoAccept { on: false });
    assert!(!harness.transcript().auto_accept);
    ask_permission(&mut harness, 13);
    assert_eq!(answer_to(&harness, 13), None);
    assert!(harness.transcript().is_blocked());
}

#[test]
fn a_newer_clients_command_is_refused_as_unsupported() {
    let mut harness = ready();
    let gui = harness.subscribe(1, false, None);
    let id = CommandId::random();
    harness.input(Input::Unsupported {
        conn: ConnId(1),
        id,
    });
    assert!(drain(&gui).iter().any(|message| matches!(
        message,
        ServerMessage::Ack { id: acked, rejected: Some(Rejection::Unsupported) } if *acked == id
    )));
}

#[test]
fn a_question_only_a_person_answers_still_waits() {
    let mut harness = ready();
    prompt(&mut harness, 0, "ask me");
    harness.command(0, Command::SetAutoAccept { on: true });
    let manual = json!({"herdr.manual": true});
    request(
        &mut harness,
        12,
        json!([
            {"optionId": "answer-0", "name": "Postgres", "kind": "allow_once", "_meta": manual},
            {"optionId": "answer-1", "name": "SQLite", "kind": "allow_once", "_meta": manual},
            {"optionId": "reject", "name": "Answer in the chat", "kind": "reject_once"}]),
    );
    assert_eq!(answer_to(&harness, 12), None);
    let asked = &harness.transcript().permissions[0];
    assert!(
        asked.options[..2]
            .iter()
            .all(|option| option.kind == sheprd_chat_protocol::PermissionKind::Other)
    );
    assert!(asked.allow_option().is_none());
}
