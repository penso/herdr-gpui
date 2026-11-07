//! Going back to before an earlier prompt.

use super::*;
use sheprd_chat_protocol::{EntryId, Rejection};

/// A session whose agent's handshake says it can go back, with two turns.
fn rewinding() -> (Harness, EntryId, EntryId) {
    let mut harness = harness_with(config());
    harness.session.start();
    let initialize = harness.last_request("initialize");
    harness.respond(
        initialize["id"].as_i64().unwrap(),
        json!({"protocolVersion": 1, "agentCapabilities": {"_meta": {"sheprd.rewind": true}}}),
    );
    let new = harness.last_request("session/new");
    harness.respond(new["id"].as_i64().unwrap(), json!({"sessionId": "s1"}));
    assert!(harness.transcript().session.as_ref().unwrap().rewind);
    let first = turn(&mut harness, "one");
    let second = turn(&mut harness, "two");
    (harness, first, second)
}

/// Runs a whole turn and returns its prompt's entry.
fn turn(harness: &mut Harness, text: &str) -> EntryId {
    let request = prompt(harness, 0, text);
    let id = harness
        .transcript()
        .entries
        .iter()
        .rfind(|entry| matches!(entry.kind, EntryKind::User { .. }))
        .unwrap()
        .id;
    harness.agent_says("done");
    harness.respond(request, json!({"stopReason": "end_turn"}));
    id
}

fn rewind(before: EntryId, text: &str) -> Command {
    Command::Rewind {
        before,
        text: text.to_owned(),
        attachments: Vec::new(),
    }
}

fn prompts(harness: &Harness) -> Vec<String> {
    harness
        .transcript()
        .entries
        .iter()
        .filter_map(|entry| match &entry.kind {
            EntryKind::User { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn rejection(messages: &[ServerMessage], id: CommandId) -> Option<Option<Rejection>> {
    messages.iter().find_map(|message| match message {
        ServerMessage::Ack {
            id: acked,
            rejected,
        } if *acked == id => Some(*rejected),
        _ => None,
    })
}

#[test]
fn the_later_turns_leave_the_transcript_and_the_edit_goes_out() {
    let (mut harness, _, second) = rewinding();
    let client = harness.subscribe(1, false, None);
    drain(&client);
    let id = harness.command(1, rewind(second, "two, better"));
    let asked = harness.last_request(crate::harness::out::REWIND);
    assert_eq!(asked["params"]["text"], "two");
    assert_eq!(asked["params"]["occurrence"], 1);
    assert_eq!(asked["params"]["sessionId"], "s1");
    assert_eq!(harness.transcript().status, Status::Starting);
    assert_eq!(
        prompts(&harness),
        ["one", "two"],
        "nothing drops until it worked"
    );
    assert_eq!(rejection(&drain(&client), id), None, "not answered yet");

    harness.respond(asked["id"].as_i64().unwrap(), json!({"sessionId": "s2"}));

    assert_eq!(prompts(&harness), ["one", "two, better"]);
    let sent = harness.last_request("session/prompt");
    assert_eq!(sent["params"]["sessionId"], "s2");
    assert_eq!(sent["params"]["prompt"][0]["text"], "two, better");
    assert_eq!(
        harness.opened.borrow().last().map(String::as_str),
        Some("s2")
    );
    let messages = drain(&client);
    assert_eq!(rejection(&messages, id), Some(None));
    let snapshot = messages.iter().find_map(|message| match message {
        ServerMessage::Snapshot { transcript, .. } => Some(transcript),
        _ => None,
    });
    let snapshot = snapshot.expect("a rewind replaces what clients hold");
    assert!(!snapshot.entries.iter().any(|entry| entry.id >= second));
}

#[test]
fn a_new_prompt_never_reuses_a_dropped_entrys_id() {
    let (mut harness, _, second) = rewinding();
    let last = harness.transcript().last_entry_id().unwrap();
    harness.command(0, rewind(second, "edit"));
    let asked = harness.last_request(crate::harness::out::REWIND);
    harness.respond(asked["id"].as_i64().unwrap(), json!({}));
    let edited = harness
        .transcript()
        .entries
        .iter()
        .rfind(|entry| matches!(entry.kind, EntryKind::User { .. }))
        .unwrap();
    assert!(edited.id > last);
}

#[test]
fn a_client_behind_the_rewind_starts_over_from_a_snapshot() {
    let (mut harness, _, second) = rewinding();
    let client = harness.subscribe(1, false, None);
    let cursor = drain(&client).iter().find_map(|message| match message {
        ServerMessage::Snapshot { cursor, .. } => Some(*cursor),
        _ => None,
    });
    harness.input(Input::Detach(ConnId(1)));
    harness.command(0, rewind(second, "edit"));
    let asked = harness.last_request(crate::harness::out::REWIND);
    harness.respond(asked["id"].as_i64().unwrap(), json!({}));
    let again = harness.subscribe(2, false, cursor);
    assert!(
        drain(&again)
            .iter()
            .any(|message| matches!(message, ServerMessage::Snapshot { .. })),
        "its cursor belongs to the old epoch"
    );
}

#[test]
fn a_failure_leaves_the_conversation_and_says_why() {
    let (mut harness, _, second) = rewinding();
    let client = harness.subscribe(1, false, None);
    drain(&client);
    let id = harness.command(1, rewind(second, "edit"));
    let asked = harness.last_request(crate::harness::out::REWIND);
    let before = harness.transcript().entries.len();
    harness.fail(
        asked["id"].as_i64().unwrap(),
        wire::Error::new(-32603, "the thread is gone"),
    );
    assert_eq!(prompts(&harness), ["one", "two"]);
    assert_eq!(harness.transcript().entries.len(), before + 1);
    assert_eq!(harness.transcript().status, Status::Idle);
    assert_eq!(
        rejection(&drain(&client), id),
        Some(Some(Rejection::RewindFailed))
    );
    assert!(harness.notices().iter().any(|(level, text)| {
        *level == NoticeLevel::Warning && text.contains("the thread is gone")
    }));
    // The chat goes on.
    prompt(&mut harness, 1, "three");
}

#[test]
fn an_agent_that_cannot_refuses_and_nothing_is_asked() {
    let mut harness = ready();
    assert!(!harness.transcript().session.as_ref().unwrap().rewind);
    let client = harness.subscribe(1, false, None);
    let first = turn(&mut harness, "one");
    drain(&client);
    harness.sent();
    let id = harness.command(1, rewind(first, "edit"));
    assert_eq!(
        rejection(&drain(&client), id),
        Some(Some(Rejection::CannotRewind))
    );
    assert!(harness.sent().is_empty());
}

#[test]
fn a_running_turn_and_an_unknown_prompt_are_refused() {
    let (mut harness, first, _) = rewinding();
    let client = harness.subscribe(1, false, None);
    drain(&client);
    let id = harness.command(1, rewind(EntryId(9999), "edit"));
    assert_eq!(
        rejection(&drain(&client), id),
        Some(Some(Rejection::UnknownPrompt))
    );
    prompt(&mut harness, 1, "three");
    drain(&client);
    let id = harness.command(1, rewind(first, "edit"));
    assert_eq!(
        rejection(&drain(&client), id),
        Some(Some(Rejection::TurnRunning))
    );
    assert_eq!(harness.transcript().status, Status::Working);
}

#[test]
fn a_repeated_prompt_is_found_by_how_many_followed_it() {
    let (mut harness, _, _) = rewinding();
    let again = turn(&mut harness, "again");
    turn(&mut harness, "again");
    harness.command(0, rewind(again, "edit"));
    let asked = harness.last_request(crate::harness::out::REWIND);
    assert_eq!(asked["params"]["text"], "again");
    assert_eq!(asked["params"]["occurrence"], 2);
}

#[test]
fn a_prompt_with_files_is_matched_on_what_was_typed() {
    let dir = tempfile::tempdir().unwrap();
    let notes = dir.path().join("notes.md");
    std::fs::write(&notes, "# Notes\n").unwrap();
    let (mut harness, _, _) = rewinding();
    let attachment = sheprd_chat_protocol::Attachment {
        path: notes.display().to_string(),
    };
    harness.command(
        0,
        Command::Prompt {
            text: "look".to_owned(),
            attachments: vec![attachment.clone()],
        },
    );
    let request = harness.last_request("session/prompt")["id"]
        .as_i64()
        .unwrap();
    harness.respond(request, json!({"stopReason": "end_turn"}));
    let files = &harness.transcript().prompt_files;
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].attachments, [attachment]);
    let id = files[0].id;
    harness.command(0, rewind(id, "edit"));
    let asked = harness.last_request(crate::harness::out::REWIND);
    assert_eq!(asked["params"]["text"], "look");
    harness.respond(asked["id"].as_i64().unwrap(), json!({}));
    assert!(harness.transcript().prompt_files.is_empty());
}

#[test]
fn a_restarted_agent_opens_the_fork_then_takes_the_edit() {
    let (sender, restarted_agent) = bounded(64);
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let asked = std::sync::Arc::clone(&seen);
    let restart: Restart = Box::new(move |rewind| {
        asked.lock().unwrap().push(rewind.clone());
        Ok(Restarted {
            outbox: Outbox(sender.clone()),
            session: "fork".to_owned(),
        })
    });
    let (mut harness, _, second) = rewinding();
    harness.session.config.restart = Some(restart);
    harness.sent();
    let client = harness.subscribe(1, false, None);
    drain(&client);
    let id = harness.command(1, rewind(second, "edit"));
    assert_eq!(seen.lock().unwrap()[0].text, "two");

    harness.agent = restarted_agent;
    let initialize = harness.last_request("initialize");
    harness.respond(
        initialize["id"].as_i64().unwrap(),
        json!({"protocolVersion": 1, "agentCapabilities": {
            "loadSession": true, "_meta": {"sheprd.rewind": true}}}),
    );
    let load = harness.last_request("session/load");
    assert_eq!(load["params"]["sessionId"], "fork");
    harness.respond(load["id"].as_i64().unwrap(), json!({}));

    assert_eq!(prompts(&harness), ["one", "edit"]);
    assert_eq!(harness.transcript().session.as_ref().unwrap().id, "fork");
    assert_eq!(
        harness.last_request("session/prompt")["params"]["sessionId"],
        "fork"
    );
    assert_eq!(rejection(&drain(&client), id), Some(None));
}

#[test]
fn a_prompt_the_log_lacks_leaves_the_running_agent_alone() {
    let (mut harness, _, second) = rewinding();
    let restart: Restart = Box::new(|_| Err(crate::rewind::Error::NoPrompt.into()));
    harness.session.config.restart = Some(restart);
    harness.sent();
    let client = harness.subscribe(1, false, None);
    drain(&client);
    let id = harness.command(1, rewind(second, "edit"));
    assert_eq!(
        rejection(&drain(&client), id),
        Some(Some(Rejection::RewindFailed))
    );
    assert_eq!(harness.transcript().status, Status::Idle);
    prompt(&mut harness, 1, "three");
}

#[test]
fn an_agent_that_does_not_come_back_refuses_the_rewind() {
    let (mut harness, _, second) = rewinding();
    let (sender, _agent) = bounded(64);
    let restart: Restart = Box::new(move |_| {
        Ok(Restarted {
            outbox: Outbox(sender.clone()),
            session: "fork".to_owned(),
        })
    });
    harness.session.config.restart = Some(restart);
    let client = harness.subscribe(1, false, None);
    drain(&client);
    let id = harness.command(1, rewind(second, "edit"));
    harness.input(Input::AgentExited {
        code: Some(1),
        stderr: vec!["unknown option".to_owned()],
    });
    assert_eq!(
        rejection(&drain(&client), id),
        Some(Some(Rejection::RewindFailed))
    );
    assert_eq!(prompts(&harness), ["one", "two"]);
}
