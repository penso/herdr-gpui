use herdr_chat_protocol::{Activity, ActivityKind, ActivityState, Change, REVISION, Transcript};

use super::*;

/// The client's own end: reading what it is sent, and its writer, which
/// keeps it attached while held.
fn attached(fixture: &Fixture, revision: u32, since: Cursor) -> (Reader, Writer, Attach) {
    let (reader, mut writer) = connect(fixture);
    greet(&mut writer, VERSION, revision, Some(since));
    let Input::Attach(attach) = fixture.inputs.recv_timeout(WAIT).unwrap() else {
        panic!("expected attach");
    };
    (reader, writer, attach)
}

fn activity() -> Change {
    Change::Activity {
        activity: Activity {
            call_id: "task".to_owned(),
            detail: None,
            finished_ms: None,
            kind: ActivityKind::Subagent,
            label: "Review the reducer tests".to_owned(),
            started_ms: 1_000,
            state: ActivityState::Running,
        },
    }
}

fn snapshot(cursor: Cursor) -> ServerMessage {
    let mut transcript = Transcript::new("Claude");
    transcript.clients = 1;
    ServerMessage::Snapshot {
        cursor,
        transcript: Box::new(transcript),
    }
}

#[test]
fn an_older_client_starts_from_a_snapshot_and_never_sees_an_activity() {
    let fixture = server();
    let since = Cursor::new_epoch();
    let (mut reader, _writer, attach) = attached(&fixture, 0, since);
    assert_eq!(
        attach.since, None,
        "an older client resumes from a snapshot"
    );
    attach.outbox.send(snapshot(since)).unwrap();
    let cursor = Cursor { seq: 1, ..since };
    attach
        .outbox
        .send(ServerMessage::Change {
            change: activity(),
            cursor,
        })
        .unwrap();
    assert!(matches!(
        read_frame::<_, ServerMessage>(&mut reader).unwrap(),
        Some(ServerMessage::Snapshot { .. })
    ));
    assert_eq!(
        read_frame::<_, ServerMessage>(&mut reader).unwrap(),
        Some(ServerMessage::Change {
            change: Change::Clients { count: 1 },
            cursor,
        })
    );
}

#[test]
fn a_current_client_resumes_and_sees_activities() {
    let fixture = server();
    let since = Cursor::new_epoch();
    let (mut reader, _writer, attach) = attached(&fixture, REVISION, since);
    assert_eq!(attach.since, Some(since));
    let message = ServerMessage::Change {
        change: activity(),
        cursor: Cursor { seq: 1, ..since },
    };
    attach.outbox.send(message.clone()).unwrap();
    assert_eq!(
        read_frame::<_, ServerMessage>(&mut reader).unwrap(),
        Some(message)
    );
}
