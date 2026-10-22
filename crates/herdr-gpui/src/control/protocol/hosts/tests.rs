#![allow(clippy::unwrap_used)]
use super::*;
use crate::control::protocol::{Call, Caller, MAX_WAIT_SECONDS, Request, Response};

mod remote_files;
mod security;

#[test]
fn host_requests_have_a_stable_wire_form() {
    let request = Request::Host(Call {
        caller: Caller::default(),
        op: HostOp::Forward { port: 3000 },
    });
    let text = serde_json::to_string(&request).unwrap();
    assert_eq!(
        text,
        r#"{"method":"host","caller":{"pane_id":null,"workspace_id":null,"daemon_socket":null},"op":{"op":"forward","port":3000}}"#
    );
    assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
    for op in [
        HostOp::List,
        HostOp::Select {
            id: "devbox".into(),
        },
        HostOp::Pr,
        HostOp::PrComments,
        HostOp::PrOpen,
        HostOp::Ports,
        HostOp::Unforward { port: 1 },
        HostOp::OpenPort { port: 8080 },
        HostOp::Usage,
        HostOp::System,
        HostOp::Security,
        HostOp::Cloud,
        HostOp::CloudSetup { id: "w1".into() },
    ] {
        let request = Request::Host(Call {
            caller: Caller::default(),
            op,
        });
        let text = serde_json::to_string(&request).unwrap();
        assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
    }
}

#[test]
fn unknown_host_operations_and_fields_are_rejected() {
    for invalid in [
        r#"{"method":"host","caller":{},"op":{"op":"destroy"}}"#,
        r#"{"method":"host","caller":{},"op":{"op":"forward"}}"#,
        r#"{"method":"host","caller":{},"op":{"op":"forward","port":70000}}"#,
        r#"{"method":"host","caller":{},"op":{"op":"select","id":"a","all":true}}"#,
    ] {
        assert!(
            serde_json::from_str::<Request>(invalid).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn only_waiting_operations_extend_the_answer_timeout() {
    let wait = |op| {
        Request::Host(Call {
            caller: Caller::default(),
            op,
        })
        .wait_seconds()
    };
    assert_eq!(wait(HostOp::PrComments), COMMENTS_WAIT_SECONDS);
    assert_eq!(wait(HostOp::OpenPort { port: 80 }), FORWARD_WAIT_SECONDS);
    assert_eq!(wait(HostOp::Pr), 0);
    assert!(COMMENTS_WAIT_SECONDS.max(FORWARD_WAIT_SECONDS) <= MAX_WAIT_SECONDS);
}

#[test]
fn host_replies_round_trip() {
    let pr = PrInfo {
        number: 8,
        url: "https://github.com/example/project/pull/8".into(),
        title: "Title".into(),
        state: "Open".into(),
        draft: false,
        head: "feature".into(),
        base: "main".into(),
        additions: 1,
        deletions: 2,
        changed_files: 3,
        review: Some("Review required".into()),
        merge_status: None,
        checks: vec![CheckInfo {
            name: "test".into(),
            outcome: "passed".into(),
        }],
    };
    for reply in [
        Reply::Pr {
            pr: Some(Box::new(pr)),
            note: None,
        },
        Reply::Pr {
            pr: None,
            note: Some("Still looking".into()),
        },
        Reply::Comments {
            comments: vec![CommentInfo {
                author: "octocat".into(),
                kind: "approved".into(),
                created_at: "2026-01-01T00:00:00Z".into(),
                body: "Looks good".into(),
            }],
        },
        Reply::Ports(Box::new(Ports {
            host: "Local".into(),
            workspace: "w_1".into(),
            listening: vec![PortInfo {
                port: 3000,
                bind: "*:3000".into(),
                process: Some("node".into()),
                local_url: Some("http://localhost:3000/".into()),
            }],
            forwards: vec![ForwardInfo {
                host: "me@devbox".into(),
                port: 5173,
                state: "ended".into(),
                local_url: None,
                reason: Some("ssh exited".into()),
            }],
            note: None,
        })),
        Reply::Usage {
            host: "Local".into(),
            usage: vec![UsageInfo {
                provider: "Claude".into(),
                account: None,
                plan: Some("Max".into()),
                windows: vec![UsageWindow {
                    label: "Session".into(),
                    used_percent: 12.5,
                    resets_in_seconds: Some(60),
                }],
                error: None,
            }],
            note: None,
        },
        Reply::System {
            load: None,
            note: Some("Not sampled".into()),
        },
    ] {
        let response = Response::Host { reply };
        let text = serde_json::to_string(&response).unwrap();
        assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), response);
    }
}
