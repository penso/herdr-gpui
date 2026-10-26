#![allow(clippy::unwrap_used)]
use super::*;
use crate::protocol::{Call, Caller, Request, Response};

#[test]
fn chat_requests_have_a_stable_wire_form() {
    let request = Request::Chat(Call {
        caller: Caller::default(),
        op: ChatOp::View {
            switch: Switch::Off,
            pane: Some("w1:p2".into()),
        },
    });
    let text = serde_json::to_string(&request).unwrap();
    assert_eq!(
        text,
        r#"{"method":"chat","caller":{"pane_id":null,"workspace_id":null,"daemon_socket":null},"op":{"op":"view","switch":"off","pane":"w1:p2"}}"#
    );
    assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
    let status: Request =
        serde_json::from_str(r#"{"method":"chat","caller":{},"op":{"op":"status"}}"#).unwrap();
    assert!(matches!(
        status,
        Request::Chat(Call {
            op: ChatOp::Status,
            ..
        })
    ));
    for invalid in [
        r#"{"method":"chat","caller":{},"op":{"op":"prompt","text":"x"}}"#,
        r#"{"method":"chat","caller":{},"op":{"op":"view","switch":"on","path":"/tmp/x"}}"#,
    ] {
        assert!(
            serde_json::from_str::<Request>(invalid).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn chat_replies_round_trip() {
    for reply in [
        Reply::Chats {
            chats: vec![ChatInfo {
                pane_id: "w1:p1".into(),
                agent: "Claude".into(),
                status: "working".into(),
                attached: true,
                shown: true,
                permissions: 1,
                queued: 0,
                model: None,
                input_mode: InputMode::Chat,
                session_id: None,
                context: None,
            }],
        },
        Reply::View {
            pane_id: "w1:p1".into(),
            shown: false,
        },
    ] {
        let response = Response::Chat { reply };
        let text = serde_json::to_string(&response).unwrap();
        assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), response);
    }
}

#[test]
fn chat_new_has_a_stable_wire_form() {
    let request = Request::Chat(Call {
        caller: Caller::default(),
        op: ChatOp::New {
            agent: "claude".into(),
            pane: None,
        },
    });
    let text = serde_json::to_string(&request).unwrap();
    assert!(
        text.ends_with(r#""op":{"op":"new","agent":"claude","pane":null}}"#),
        "{text}"
    );
    assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
    let started = Response::Chat {
        reply: Reply::Started {
            agent: "claude".into(),
        },
    };
    let text = serde_json::to_string(&started).unwrap();
    assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), started);
}
