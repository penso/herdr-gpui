#![allow(clippy::unwrap_used)]
use super::*;
use crate::control::protocol::{Call, Caller, Request, Response};

fn round_trip(op: TabOp) {
    let request = Request::Tab(Call {
        caller: Caller::default(),
        op,
    });
    let text = serde_json::to_string(&request).unwrap();
    assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
}

#[test]
fn tab_operations_round_trip() {
    for op in [
        TabOp::List { pages: true },
        TabOp::Focus { id: 3 },
        TabOp::Close { id: 3 },
        TabOp::Navigate {
            id: 3,
            url: "https://a.test/".into(),
        },
        TabOp::Back { id: 3 },
        TabOp::Forward { id: 3 },
        TabOp::Text { id: 3 },
        TabOp::Screenshot {
            id: 3,
            path: "/tmp/a.png".into(),
        },
        TabOp::Zoom {
            id: 3,
            change: ZoomChange::Percent { percent: 150 },
        },
        TabOp::Zoom {
            id: 3,
            change: ZoomChange::Reset,
        },
        TabOp::Code {
            path: "/src/main.rs".into(),
            line: Some(42),
        },
        TabOp::Edit {
            path: "/src/main.rs".into(),
            line: None,
        },
        TabOp::Devices,
        TabOp::Orchestrator {
            repo: Some("/src".into()),
        },
        TabOp::Review,
    ] {
        round_trip(op);
    }
}

#[test]
fn tab_requests_have_a_stable_wire_form() {
    let request: Request = serde_json::from_str(
        r#"{"method":"tab","caller":{"workspace_id":"w_1"},"op":{"op":"code","path":"/a.rs"}}"#,
    )
    .unwrap();
    let Request::Tab(call) = request else {
        panic!("not a tab request");
    };
    assert_eq!(
        call.op,
        TabOp::Code {
            path: "/a.rs".into(),
            line: None
        }
    );
    assert_eq!(
        serde_json::from_str::<TabOp>(r#"{"op":"list"}"#).unwrap(),
        TabOp::List { pages: false }
    );
    assert_eq!(
        serde_json::from_str::<TabOp>(r#"{"op":"zoom","id":1,"change":{"change":"in"}}"#).unwrap(),
        TabOp::Zoom {
            id: 1,
            change: ZoomChange::In
        }
    );
    for invalid in [
        r#"{"op":"zoom","id":1}"#,
        r#"{"op":"zoom","id":1,"change":{"change":"percent"}}"#,
        r#"{"op":"zoom","id":1,"change":{"change":"twice"}}"#,
        r#"{"op":"eval","id":1}"#,
        r#"{"op":"text","id":1,"script":"alert(1)"}"#,
        r#"{"op":"focus"}"#,
        r#"{"op":"focus","id":-1}"#,
    ] {
        assert!(serde_json::from_str::<TabOp>(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn page_and_arrival_operations_extend_the_answer_timeout() {
    let wait = |op: TabOp| {
        Request::Tab(Call {
            caller: Caller::default(),
            op,
        })
        .wait_seconds()
    };
    assert_eq!(wait(TabOp::Text { id: 1 }), PAGE_WAIT_SECONDS);
    assert_eq!(
        wait(TabOp::Screenshot {
            id: 1,
            path: "/a.png".into()
        }),
        PAGE_WAIT_SECONDS
    );
    assert_eq!(wait(TabOp::Review), ARRIVE_WAIT_SECONDS);
    assert_eq!(wait(TabOp::Focus { id: 1 }), 0);
}

#[test]
fn replies_round_trip() {
    let tab = TabInfo {
        id: 7,
        kind: TabKind::Code,
        title: "main.rs".into(),
        location: Some("/src/main.rs".into()),
        workspace: "w_1".into(),
        host: "local:/tmp/herdr.sock".into(),
        opened_by: None,
    };
    for reply in [
        Reply::Tabs { tabs: vec![tab] },
        Reply::Opened {
            id: 7,
            workspace: "w_1".into(),
        },
        Reply::Text {
            text: "Hello\nworld".into(),
        },
    ] {
        let response = Response::Tab { reply };
        let text = serde_json::to_string(&response).unwrap();
        assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), response);
    }
    assert_eq!(
        serde_json::to_string(&Reply::Opened {
            id: 7,
            workspace: "w_1".into()
        })
        .unwrap(),
        r#"{"kind":"opened","id":7,"workspace":"w_1"}"#
    );
}

#[test]
fn kinds_are_named_as_json_spells_them() {
    for kind in [
        TabKind::Blank,
        TabKind::Web,
        TabKind::File,
        TabKind::Code,
        TabKind::Review,
        TabKind::Orchestrator,
        TabKind::Devices,
        TabKind::Editor,
    ] {
        assert_eq!(
            serde_json::to_string(&kind).unwrap(),
            format!("\"{}\"", kind.name())
        );
        assert_eq!(kind.is_page(), matches!(kind, TabKind::Web | TabKind::File));
    }
}
