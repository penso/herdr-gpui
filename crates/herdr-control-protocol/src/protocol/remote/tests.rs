#![allow(clippy::unwrap_used)]
use super::*;
use crate::protocol::{Call, Caller, Request, Response};
use herdr_remote_control::control::DeviceId;

#[test]
fn remote_requests_have_a_stable_wire_form() {
    let request = Request::Remote(Call {
        caller: Caller::default(),
        op: RemoteOp::Revoke {
            device: "Fabien's iPhone".into(),
        },
    });
    let text = serde_json::to_string(&request).unwrap();
    assert_eq!(
        text,
        r#"{"method":"remote","caller":{"pane_id":null,"workspace_id":null,"daemon_socket":null},"op":{"op":"revoke","device":"Fabien's iPhone"}}"#
    );
    assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
    for (text, op) in [
        (r#"{"op":"status"}"#, RemoteOp::Status),
        (r#"{"op":"pair"}"#, RemoteOp::Pair),
        (r#"{"op":"devices"}"#, RemoteOp::Devices),
    ] {
        let request: Request = serde_json::from_str(&format!(
            r#"{{"method":"remote","caller":{{}},"op":{text}}}"#
        ))
        .unwrap();
        assert_eq!(
            request,
            Request::Remote(Call {
                caller: Caller::default(),
                op
            })
        );
    }
    for invalid in [
        r#"{"method":"remote","caller":{},"op":{"op":"approve","id":"p1"}}"#,
        r#"{"method":"remote","caller":{},"op":{"op":"revoke"}}"#,
    ] {
        assert!(
            serde_json::from_str::<Request>(invalid).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn remote_replies_round_trip() {
    for reply in [
        Reply::Status {
            enabled: false,
            status: None,
            error: None,
        },
        Reply::Status {
            enabled: true,
            status: None,
            error: Some("could not reach the remote control daemon".into()),
        },
        Reply::Status {
            enabled: true,
            status: Some(Box::new(Status {
                version: "dev".into(),
                port: 7443,
                addresses: vec!["https://192.168.1.20:7443".into()],
                fingerprint: "5F:2A".into(),
                herdr: true,
                chats: 2,
                devices: 1,
                connected: 1,
            })),
            error: None,
        },
        Reply::Devices {
            devices: vec![Device {
                id: DeviceId("d1".into()),
                name: "Pixel".into(),
                created_ms: 1,
                last_seen_ms: None,
                push: true,
                connected: false,
            }],
        },
    ] {
        let response = Response::Remote { reply };
        let text = serde_json::to_string(&response).unwrap();
        assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), response);
    }
}

#[test]
fn daemon_requests_wait_for_the_daemon_and_pairing_does_not() {
    assert_eq!(RemoteOp::Pair.wait_seconds(), 0);
    assert!(
        RemoteOp::Status.wait_seconds() > herdr_remote_control::control::RESPONSE_TIMEOUT.as_secs()
    );
    let revoke = RemoteOp::Revoke { device: "d".into() };
    assert!(revoke.wait_seconds() > 2 * herdr_remote_control::control::RESPONSE_TIMEOUT.as_secs());
}
