use super::*;

fn cursor_of(message: &ServerMessage) -> Option<Cursor> {
    match message {
        ServerMessage::Snapshot { cursor, .. } | ServerMessage::Change { cursor, .. } => {
            Some(*cursor)
        }
        _ => None,
    }
}

/// Replays what a client received into its own transcript.
fn replay(messages: Vec<ServerMessage>, transcript: &mut Transcript) -> Option<Cursor> {
    let mut last = None;
    for message in messages {
        match message {
            ServerMessage::Snapshot {
                cursor,
                transcript: snapshot,
            } => {
                *transcript = *snapshot;
                last = Some(cursor);
            }
            ServerMessage::Change { change, cursor } => {
                if let Some(previous) = last {
                    assert_eq!(cursor.seq, previous.seq + 1, "gap in changes");
                }
                transcript.apply(&change).unwrap();
                last = Some(cursor);
            }
            _ => {}
        }
    }
    last
}

#[test]
fn a_new_client_gets_welcome_then_a_snapshot_then_live_changes() {
    let mut harness = ready();
    prompt(&mut harness, 0, "hi");
    harness.agent_says("hello");
    let client = harness.subscribe(1, false, None);
    let messages = drain(&client);
    assert!(matches!(
        &messages[0],
        ServerMessage::Welcome { agent, pane: Some(pane), version: VERSION }
            if agent == "Claude" && pane == "p_1"
    ));
    assert!(matches!(&messages[1], ServerMessage::Snapshot { .. }));
    let mut mirror = Transcript::default();
    let mut cursor = replay(messages, &mut mirror);
    harness.agent_says(" there");
    cursor = replay(drain(&client), &mut mirror).or(cursor);
    assert_eq!(&mirror, harness.transcript());
    assert_eq!(cursor.unwrap().seq, harness.session.seq);
}

#[test]
fn remote_clients_are_counted_but_the_local_view_is_not() {
    let mut harness = ready();
    let _local = harness.subscribe(0, true, None);
    assert_eq!(harness.transcript().clients, 0);
    let _gui = harness.subscribe(1, false, None);
    assert_eq!(harness.transcript().clients, 1);
    harness.input(Input::Detach(ConnId(1)));
    assert_eq!(harness.transcript().clients, 0);
}

#[test]
fn a_reconnecting_client_catches_up_from_its_cursor() {
    let mut harness = ready();
    let client = harness.subscribe(1, false, None);
    let mut mirror = Transcript::default();
    let cursor = replay(drain(&client), &mut mirror).unwrap();
    harness.input(Input::Detach(ConnId(1)));
    prompt(&mut harness, 0, "while away");
    harness.agent_says("missed");
    let client = harness.subscribe(2, false, Some(cursor));
    let messages = drain(&client);
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, ServerMessage::Snapshot { .. }))
    );
    let mut last = cursor;
    for message in messages {
        if let ServerMessage::Change { change, cursor } = message {
            assert_eq!(cursor.seq, last.seq + 1);
            mirror.apply(&change).unwrap();
            last = cursor;
        }
    }
    assert_eq!(&mirror, harness.transcript());
}

#[test]
fn a_cursor_from_another_run_or_too_old_gets_a_snapshot() {
    let mut harness = ready();
    let stale = Cursor {
        epoch: Uuid::new_v4(),
        seq: 1,
    };
    let client = harness.subscribe(1, false, Some(stale));
    assert!(matches!(&drain(&client)[1], ServerMessage::Snapshot { .. }));
    let ahead = Cursor {
        epoch: harness.session.epoch,
        seq: harness.session.seq + 5,
    };
    let client = harness.subscribe(2, false, Some(ahead));
    assert!(matches!(&drain(&client)[1], ServerMessage::Snapshot { .. }));
}

#[test]
fn a_client_that_falls_behind_is_dropped_not_waited_on() {
    let mut harness = ready();
    let (outbox, slow) = bounded(4);
    harness.input(Input::Attach(Attach {
        conn: ConnId(1),
        local: false,
        outbox,
        since: None,
    }));
    assert_eq!(harness.transcript().clients, 1);
    let id = prompt(&mut harness, 0, "go");
    for _ in 0..10 {
        harness.agent_says("x");
    }
    harness.respond(id, json!({"stopReason": "end_turn"}));
    assert!(harness.session.subscribers.is_empty());
    assert_eq!(harness.transcript().clients, 0);
    // Whatever it did receive is a consistent prefix.
    let received: Vec<_> = slow.try_iter().filter_map(|m| cursor_of(&m)).collect();
    assert!(
        received
            .windows(2)
            .all(|pair| pair[1].seq == pair[0].seq + 1)
    );
}

#[test]
fn shutdown_says_goodbye_to_every_client() {
    let mut harness = ready();
    let client = harness.subscribe(1, false, None);
    assert_eq!(harness.input(Input::Shutdown), Flow::Exit);
    assert!(matches!(
        drain(&client).last(),
        Some(ServerMessage::Bye {
            reason: ByeReason::Exiting
        })
    ));
}
