use crate::{
    Error, Violation,
    event::{Lines, MAX_LINE, Notice, TerminalChange, parse},
    wire::StatusState,
};
use serde_json::json;

fn notice(value: serde_json::Value) -> Option<Notice> {
    parse(&serde_json::to_vec(&value).unwrap()).unwrap()
}

fn event(kind: &str, data: serde_json::Value) -> TerminalChange {
    match notice(json!({"jsonrpc": "2.0", "method": "event",
        "params": {"seq": 4, "type": kind, "terminal": "t7", "data": data}}))
    {
        Some(Notice::Event(event)) => {
            assert_eq!((event.seq, event.terminal.as_str()), (4, "t7"));
            event.change
        }
        other => panic!("not an event: {other:?}"),
    }
}

#[test]
fn every_event_type_has_typed_data() {
    assert!(
        matches!(event("created", super::terminal("t7")), TerminalChange::Created(t) if t.address == "t7")
    );
    let status = event(
        "status",
        json!({"status": {"state": "blocked", "kind": "permission"}, "records": {}, "reported": true}),
    );
    assert!(
        matches!(status, TerminalChange::Status { reported: true, status: Some(s) } if s.state == StatusState::Blocked)
    );
    assert_eq!(
        event("title", json!({"title": "vim"})),
        TerminalChange::Title(Some("vim".into()))
    );
    assert_eq!(
        event("size", json!({"cols": 100, "rows": 30})),
        TerminalChange::Size {
            cols: 100,
            rows: 30
        }
    );
    assert_eq!(
        event("keep", json!({"keep": true})),
        TerminalChange::Keep(true)
    );
    assert_eq!(event("removed", json!({})), TerminalChange::Removed);
    assert_eq!(
        event("foreground", json!(null)),
        TerminalChange::Foreground(None)
    );
    assert!(
        matches!(event("finished", json!({"reason": "exited", "exit_code": 0, "signal": null, "finished_ms": 9})),
        TerminalChange::Finished(ended) if ended.exit_code == Some(0))
    );
    assert_eq!(
        event("turn", json!({"turn": 3, "state": "done"})),
        TerminalChange::Turn {
            state: Some("done".into()),
            turn: 3
        }
    );
}

#[test]
fn newer_words_do_not_fail() {
    assert_eq!(
        event("teleported", json!({})),
        TerminalChange::Unknown("teleported".into())
    );
    let status = event(
        "status",
        json!({"status": {"state": "dreaming"}, "reported": true}),
    );
    assert!(
        matches!(status, TerminalChange::Status { status: Some(s), .. } if s.state == StatusState::Other)
    );
    assert_eq!(
        notice(json!({"jsonrpc": "2.0", "method": "news", "params": {}})),
        None
    );
}

#[test]
fn heartbeats_and_missed_snapshots_parse() {
    assert_eq!(
        notice(json!({"method": "heartbeat", "params": {"seq": 42}})),
        Some(Notice::Heartbeat { seq: 42 })
    );
    let missed = notice(
        json!({"method": "missed", "params": {"seq": 57, "terminals": [super::terminal("t1")]}}),
    );
    assert!(matches!(missed, Some(Notice::Missed { seq: 57, terminals }) if terminals.len() == 1));
}

#[test]
fn lines_join_across_frames_and_stay_bounded() {
    let mut lines = Lines::default();
    assert!(lines.push(b"{\"a\":").unwrap().is_empty());
    assert_eq!(
        lines.push(b"1}\n\n{\"b\":2}\n{").unwrap(),
        [b"{\"a\":1}".to_vec(), b"{\"b\":2}".to_vec()]
    );
    let mut long = Lines::default();
    assert!(matches!(
        long.push(&vec![b'x'; MAX_LINE + 2]),
        Err(Error::Protocol(Violation::LongLine(_)))
    ));
}
