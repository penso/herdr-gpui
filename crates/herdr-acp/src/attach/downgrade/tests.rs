#![allow(clippy::unwrap_used, clippy::expect_used)]

use herdr_chat_protocol::{
    Activity, ActivityKind, ActivityState, Cursor, PermissionId, Status, Transcript,
};

use super::*;

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

fn change(change: Change, seq: u64, epoch: Cursor) -> ServerMessage {
    ServerMessage::Change {
        change,
        cursor: Cursor { seq, ..epoch },
    }
}

fn snapshot(clients: u32, cursor: Cursor) -> ServerMessage {
    let mut transcript = Transcript::new("Claude");
    transcript.clients = clients;
    ServerMessage::Snapshot {
        cursor,
        transcript: Box::new(transcript),
    }
}

#[test]
fn a_current_client_gets_every_message_unchanged() {
    let mut downgrade = Downgrade::new(REVISION);
    let epoch = Cursor::new_epoch();
    let message = change(activity(), 1, epoch);
    assert_eq!(downgrade.message(message.clone()), Some(message));
}

#[test]
fn an_older_client_gets_the_client_count_it_holds_at_the_same_cursor() {
    let mut downgrade = Downgrade::new(0);
    let epoch = Cursor::new_epoch();
    assert!(downgrade.message(snapshot(2, epoch)).is_some());
    assert_eq!(
        downgrade.message(change(activity(), 1, epoch)),
        Some(change(Change::Clients { count: 2 }, 1, epoch))
    );
    let count = change(Change::Clients { count: 3 }, 2, epoch);
    assert_eq!(downgrade.message(count.clone()), Some(count));
    assert_eq!(
        downgrade.message(change(activity(), 3, epoch)),
        Some(change(Change::Clients { count: 3 }, 3, epoch)),
        "the count follows the changes the client applied"
    );
}

#[test]
fn an_older_client_gets_changes_it_knows_unchanged() {
    let mut downgrade = Downgrade::new(0);
    let epoch = Cursor::new_epoch();
    downgrade.message(snapshot(1, epoch));
    for known in [
        Change::Status {
            status: Status::Working,
        },
        Change::PermissionResolved {
            id: PermissionId(4),
        },
    ] {
        let message = change(known, 1, epoch);
        assert_eq!(downgrade.message(message.clone()), Some(message));
    }
}

#[test]
fn an_older_client_without_a_snapshot_is_dropped_rather_than_guessed_at() {
    let mut downgrade = Downgrade::new(0);
    let epoch = Cursor::new_epoch();
    assert_eq!(downgrade.message(change(activity(), 1, epoch)), None);
}
