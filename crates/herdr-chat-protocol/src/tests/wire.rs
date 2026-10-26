use serde_json::json;

use super::*;

#[test]
fn messages_use_stable_tagged_json() {
    let hello = ClientMessage::Hello {
        client: ClientKind::Gui,
        since: None,
        version: VERSION,
    };
    assert_eq!(
        serde_json::to_value(&hello).unwrap(),
        json!({"type": "hello", "client": "gui", "since": null, "version": VERSION})
    );
    let answer = Command::Answer {
        option: "allow".to_owned(),
        permission: PermissionId(4),
    };
    assert_eq!(
        serde_json::to_value(&answer).unwrap(),
        json!({"command": "answer", "option": "allow", "permission": 4})
    );
}

#[test]
fn unknown_enum_values_from_a_newer_peer_fall_back() {
    let kind: ToolKind = serde_json::from_value(json!("teleport")).unwrap();
    assert_eq!(kind, ToolKind::Other);
    let kind: PermissionKind = serde_json::from_value(json!("ask_later")).unwrap();
    assert_eq!(kind, PermissionKind::Other);
    let rejection: Rejection = serde_json::from_value(json!("rate_limited")).unwrap();
    assert_eq!(rejection, Rejection::Other);
    let client: ClientKind = serde_json::from_value(json!("phone")).unwrap();
    assert_eq!(client, ClientKind::Other);
}

#[test]
fn transcripts_without_newer_fields_still_decode() {
    let transcript: Transcript = serde_json::from_value(json!({"agent": "Claude"})).unwrap();
    assert_eq!(transcript, Transcript::new("Claude"));
}

#[test]
fn a_snapshot_round_trips_every_entry_kind() {
    let mut transcript = Transcript::new("Claude");
    transcript.entries = vec![
        user(1, "fix it"),
        Entry {
            id: EntryId(2),
            kind: EntryKind::Thought {
                text: "hmm".to_owned(),
            },
        },
        tool(3, ToolStatus::InProgress),
        Entry {
            id: EntryId(4),
            kind: EntryKind::Tool(Tool {
                call_id: "c2".to_owned(),
                content: vec![
                    ToolContent::Diff {
                        new: "b".to_owned(),
                        old: Some("a".to_owned()),
                        path: "/x".to_owned(),
                    },
                    ToolContent::Text {
                        text: "ok".to_owned(),
                    },
                    ToolContent::Terminal {
                        id: "t1".to_owned(),
                    },
                ],
                kind: ToolKind::Edit,
                locations: vec!["/x".to_owned()],
                status: ToolStatus::Completed,
                title: "Edit x".to_owned(),
            }),
        },
        agent(5, "done"),
        Entry {
            id: EntryId(6),
            kind: EntryKind::Notice {
                answers: Vec::new(),
                details: None,
                level: NoticeLevel::Warning,
                text: "cancelled".to_owned(),
            },
        },
    ];
    transcript.permissions = vec![permission(1)];
    transcript.status = Status::Failed {
        message: "boom".to_owned(),
    };
    let message = ServerMessage::Snapshot {
        cursor: Cursor {
            epoch: uuid::Uuid::new_v4(),
            seq: 9,
        },
        transcript: Box::new(transcript),
    };
    let bytes = encode_frame(&message).unwrap();
    assert_eq!(
        read_frame::<_, ServerMessage>(&mut bytes.as_slice()).unwrap(),
        Some(message)
    );
}
