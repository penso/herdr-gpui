#![allow(clippy::unwrap_used)]
use super::*;
use crate::protocol::{Call, Caller, Request, Response};

#[test]
fn work_requests_have_a_stable_wire_form() {
    let request = Request::Work(Call {
        caller: Caller {
            pane_id: Some("w_1:p1".into()),
            workspace_id: Some("w_1".into()),
            daemon_socket: None,
            window: None,
        },
        op: WorkOp::FanOut(FanOutRequest {
            prompt: "fix it".into(),
            agents: vec![("claude".into(), 2)],
        }),
    });
    let text = serde_json::to_string(&request).unwrap();
    assert_eq!(
        text,
        r#"{"method":"work","caller":{"pane_id":"w_1:p1","workspace_id":"w_1","daemon_socket":null},"op":{"op":"fan_out","prompt":"fix it","agents":[["claude",2]]}}"#
    );
    assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
    for op in [
        WorkOp::Notes,
        WorkOp::Note,
        WorkOp::SetNote { text: "x".into() },
        WorkOp::Restore { id: None },
        WorkOp::Restore {
            id: Some("0001700000000000".into()),
        },
        WorkOp::Script { kind: Script::Run },
        WorkOp::Teleport { host: "box".into() },
    ] {
        let request = Request::Work(Call {
            caller: Caller::default(),
            op,
        });
        let text = serde_json::to_string(&request).unwrap();
        assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
    }
    let minimal: Request =
        serde_json::from_str(r#"{"method":"work","caller":{},"op":{"op":"restore"}}"#).unwrap();
    assert!(matches!(
        minimal,
        Request::Work(Call {
            op: WorkOp::Restore { id: None },
            ..
        })
    ));
}

#[test]
fn malformed_work_requests_are_rejected() {
    for invalid in [
        r#"{"method":"work","caller":{},"op":{"op":"restore","force":true}}"#,
        r#"{"method":"work","caller":{},"op":{"op":"script","kind":"deploy"}}"#,
        r#"{"method":"work","caller":{},"op":{"op":"set_note"}}"#,
        r#"{"method":"work","caller":{},"op":{"op":"fan_out","prompt":"x","agents":[["claude",300]]}}"#,
        r#"{"method":"work","caller":{},"op":{"op":"launch"}}"#,
    ] {
        assert!(
            serde_json::from_str::<Request>(invalid).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn work_replies_round_trip() {
    let note = NoteInfo {
        host: "local".into(),
        repository: "/src/app/.git".into(),
        branch: "main".into(),
        text: "waiting on review".into(),
    };
    for response in [
        Response::Work {
            reply: Reply::Notes {
                notes: vec![note.clone()],
                omitted: 2,
            },
        },
        Response::Work {
            reply: Reply::Note { note: Some(note) },
        },
        Response::Work {
            reply: Reply::Note { note: None },
        },
    ] {
        let text = serde_json::to_string(&response).unwrap();
        assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), response);
    }
}
