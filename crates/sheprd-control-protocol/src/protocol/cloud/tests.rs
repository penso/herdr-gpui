#![allow(clippy::unwrap_used)]
use super::*;
use crate::protocol::{Call, Caller, MAX_WAIT_SECONDS, Request, Response};

fn device(state: DeviceState) -> DeviceInfo {
    DeviceInfo {
        provider: "coder".into(),
        id: "w1".into(),
        endpoint: "coder:w1".into(),
        label: "Build box".into(),
        state,
        step: None,
        error: None,
    }
}

#[test]
fn cloud_requests_have_a_stable_wire_form() {
    let request = Request::Cloud(Call {
        caller: Caller::default(),
        op: CloudOp::Destroy {
            devices: vec!["coder:w1".into()],
            batch: None,
        },
    });
    let text = serde_json::to_string(&request).unwrap();
    assert_eq!(
        text,
        r#"{"method":"cloud","caller":{"pane_id":null,"workspace_id":null,"daemon_socket":null},"op":{"op":"destroy","devices":["coder:w1"]}}"#
    );
    assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
    for op in [
        CloudOp::Destroy {
            devices: Vec::new(),
            batch: Some(3),
        },
        CloudOp::Wait {
            batch: 3,
            wait_seconds: 30,
        },
        CloudOp::Batches,
        CloudOp::Create(CreateOp {
            provider: "coder".into(),
            count: 10,
            template: Some("dev".into()),
            preset: None,
            shape: Shape::Machine,
            install: true,
            setup: true,
        }),
        CloudOp::Exec {
            device: "coder:w1".into(),
            command: vec!["git".into(), "push".into()],
            cwd: Some("~/code/app".into()),
            wait_seconds: 120,
        },
    ] {
        let request = Request::Cloud(Call {
            caller: Caller::default(),
            op,
        });
        let text = serde_json::to_string(&request).unwrap();
        assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
    }
}

#[test]
fn unknown_cloud_operations_and_fields_are_rejected() {
    for invalid in [
        r#"{"method":"cloud","caller":{},"op":{"op":"create"}}"#,
        r#"{"method":"cloud","caller":{},"op":{"op":"wait"}}"#,
        r#"{"method":"cloud","caller":{},"op":{"op":"wait","batch":-1,"wait_seconds":1}}"#,
        r#"{"method":"cloud","caller":{},"op":{"op":"destroy","force":true}}"#,
        r#"{"method":"cloud","caller":{},"op":{"op":"exec","device":"d","command":["ls"]}}"#,
    ] {
        assert!(
            serde_json::from_str::<Request>(invalid).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn a_wait_is_bounded_like_every_other() {
    let request = Request::Cloud(Call {
        caller: Caller::default(),
        op: CloudOp::Wait {
            batch: 1,
            wait_seconds: u64::MAX,
        },
    });
    assert_eq!(request.wait_seconds(), MAX_WAIT_SECONDS);
}

#[test]
fn replies_carry_each_device_and_round_trip() {
    let mut failed = device(DeviceState::Failed);
    failed.error = Some("Coder denied access".into());
    let response = Response::Cloud {
        reply: Reply::Batch {
            batch: BatchInfo {
                id: 3,
                kind: BatchKind::Destroy,
                phase: Phase::Done,
                devices: vec![device(DeviceState::Destroyed), failed],
            },
        },
    };
    let text = serde_json::to_string(&response).unwrap();
    assert!(text.contains(r#""status":"cloud""#), "{text}");
    assert!(text.contains(r#""state":"destroyed""#), "{text}");
    assert!(text.contains(r#""error":"Coder denied access""#), "{text}");
    assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), response);
}

#[test]
fn a_batch_succeeds_only_once_done_without_a_failure() {
    let batch = |phase, states: &[DeviceState]| BatchInfo {
        id: 1,
        kind: BatchKind::Destroy,
        phase,
        devices: states.iter().copied().map(device).collect(),
    };
    assert!(
        batch(
            Phase::Done,
            &[DeviceState::Destroyed, DeviceState::Forgotten]
        )
        .succeeded()
    );
    assert!(!batch(Phase::Done, &[DeviceState::Destroyed, DeviceState::Failed]).succeeded());
    assert!(!batch(Phase::Running, &[DeviceState::Destroying]).succeeded());
    assert!(!batch(Phase::Cancelled, &[DeviceState::Cancelled]).succeeded());
    assert!(Phase::Cancelled.settled() && !Phase::Confirming.settled());
    assert!(DeviceState::Forgotten.settled() && !DeviceState::Waiting.settled());
}

#[test]
fn phases_are_named_as_the_wire_names_them() {
    for phase in [
        Phase::Confirming,
        Phase::Running,
        Phase::Done,
        Phase::Cancelled,
    ] {
        assert_eq!(serde_json::to_value(phase).unwrap(), phase.name());
    }
}

#[test]
fn a_command_s_answer_keeps_its_streams_apart() {
    let response = Response::Cloud {
        reply: Reply::Exec(Box::new(ExecInfo {
            device: "coder:w1".into(),
            status: Some(1),
            stdout: "out".into(),
            stderr: "err".into(),
            truncated: false,
        })),
    };
    let text = serde_json::to_string(&response).unwrap();
    assert!(text.contains(r#""kind":"exec""#), "{text}");
    assert!(text.contains(r#""stderr":"err""#), "{text}");
    assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), response);
}

#[test]
fn a_create_request_leaves_defaults_off_the_wire() {
    let op = CloudOp::Create(CreateOp {
        provider: "azure".into(),
        count: 2,
        template: None,
        preset: None,
        shape: Shape::Machine,
        install: false,
        setup: false,
    });
    let text = serde_json::to_string(&op).unwrap();
    assert_eq!(
        text,
        r#"{"op":"create","provider":"azure","count":2,"install":false,"setup":false}"#
    );
    let sandbox: CloudOp = serde_json::from_str(
        r#"{"op":"create","provider":"azure","count":1,"shape":"sandbox","install":true,"setup":true}"#,
    )
    .unwrap();
    assert!(matches!(
        sandbox,
        CloudOp::Create(CreateOp {
            shape: Shape::Sandbox,
            ..
        })
    ));
}
