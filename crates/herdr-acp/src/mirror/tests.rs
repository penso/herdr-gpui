#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::session::Attach;
use herdr_chat_protocol::{Command, CommandId, Entry, EntryId, EntryKind, PermissionId, Status};

fn transcript(lines: &[&str]) -> Transcript {
    Transcript {
        agent: "Codex".into(),
        entries: lines
            .iter()
            .enumerate()
            .map(|(index, text)| Entry {
                id: EntryId(index as u64 + 1),
                kind: EntryKind::Agent {
                    text: (*text).into(),
                },
            })
            .collect(),
        read_only: true,
        status: Status::Idle,
        ..Transcript::default()
    }
}

#[test]
fn every_client_command_is_refused_without_changing_the_transcript() {
    let initial = transcript(&["Existing conversation"]);
    let mut mirror = Mirror::new(initial.clone(), Some("pane7".into()));
    let (outbox, queue) = bounded(16);
    let conn = ConnId(1);
    mirror.input(Input::Attach(Attach {
        conn,
        local: false,
        outbox,
        since: None,
    }));
    assert!(
        matches!(queue.recv().unwrap(), ServerMessage::Welcome { pane: Some(pane), .. } if pane == "pane7")
    );
    assert!(
        matches!(queue.recv().unwrap(), ServerMessage::Snapshot { transcript, .. } if transcript.read_only)
    );
    for command in [
        Command::Prompt {
            text: "Do work".into(),
            attachments: vec![],
        },
        Command::Cancel,
        Command::Answer {
            option: "allow".into(),
            permission: PermissionId(1),
        },
        Command::SetAutoAccept { on: true },
        Command::SetMode {
            id: "bypass".into(),
        },
    ] {
        let id = CommandId::random();
        mirror.input(Input::Command { command, conn, id });
        assert_eq!(
            queue.recv().unwrap(),
            ServerMessage::Ack {
                id,
                rejected: Some(Rejection::NotReady)
            }
        );
        assert_eq!(mirror.transcript, initial);
    }
}

#[test]
fn appended_and_changed_entries_use_the_shared_reducer_without_a_snapshot() {
    let previous = transcript(&["one", "stream"]);
    let next = transcript(&["one", "stream finished", "three"]);
    let changes = changes(&previous, &next).unwrap();
    assert!(matches!(
        &changes[..],
        [Change::Replace { .. }, Change::Push { .. }]
    ));
    let mut actual = previous;
    for change in changes {
        actual.apply(&change).unwrap();
    }
    assert_eq!(actual, next);
}

#[test]
fn tail_truncation_is_reduced_but_a_rewrite_needs_a_snapshot() {
    let previous = transcript(&["one", "two", "three"]);
    let mut next = previous.clone();
    next.entries.remove(0);
    next.truncated = true;
    let mut actual = previous.clone();
    for change in changes(&previous, &next).unwrap() {
        actual.apply(&change).unwrap();
    }
    assert_eq!(actual, next);
    next.entries.clear();
    assert!(changes(&previous, &next).is_none());
    assert!(changes(&previous, &transcript(&["replacement"])).is_none());
}

#[test]
fn slow_clients_are_dropped_instead_of_growing_the_queue() {
    let mut mirror = Mirror::new(transcript(&["one"]), None);
    let (outbox, _queue) = bounded(2);
    mirror.input(Input::Attach(Attach {
        conn: ConnId(1),
        local: false,
        outbox,
        since: None,
    }));
    mirror.update(transcript(&["one", "two"]));
    assert!(mirror.clients.is_empty());
}

#[test]
fn older_transcripts_remain_writable_and_mirrors_keep_the_wire_flag() {
    let old: Transcript = serde_json::from_value(serde_json::json!({"agent": "Codex"})).unwrap();
    assert!(!old.read_only);
    let mirror = transcript(&["Read-only"]);
    let roundtrip: Transcript =
        serde_json::from_slice(&serde_json::to_vec(&mirror).unwrap()).unwrap();
    assert!(roundtrip.read_only);
}

#[test]
fn reported_metadata_updates_without_restarting_the_view() {
    let previous = transcript(&["one"]);
    let mut next = previous.clone();
    next.usage = Some(herdr_chat_protocol::Usage {
        size: 100,
        used: 40,
        cost: None,
    });
    let mut actual = previous;
    for change in changes(&actual, &next).unwrap() {
        actual.apply(&change).unwrap();
    }
    assert_eq!(actual, next);
}
