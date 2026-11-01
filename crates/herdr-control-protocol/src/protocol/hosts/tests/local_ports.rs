use super::*;

const _: () = assert!(LOCAL_PORTS_WAIT_SECONDS <= MAX_WAIT_SECONDS);

#[test]
fn local_port_requests_have_a_stable_wire_form() {
    let request = Request::Host(Call {
        caller: Caller::default(),
        op: HostOp::SignalPort {
            port: 3000,
            action: PortAction::Pause,
        },
    });
    let text = serde_json::to_string(&request).unwrap();
    assert!(
        text.ends_with(r#""op":{"op":"signal_port","port":3000,"action":"pause"}}"#),
        "{text}"
    );
    assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
    let listing = serde_json::to_string(&HostOp::LocalPorts).unwrap();
    assert_eq!(listing, r#"{"op":"local_ports"}"#);
    assert_eq!(HostOp::LocalPorts.wait_seconds(), LOCAL_PORTS_WAIT_SECONDS);
    assert_eq!(
        HostOp::SignalPort {
            port: 1,
            action: PortAction::Stop
        }
        .wait_seconds(),
        0
    );
}

#[test]
fn port_actions_are_named_as_the_cli_spells_them() {
    for action in [PortAction::Stop, PortAction::Pause, PortAction::Resume] {
        assert_eq!(
            serde_json::to_string(&action).unwrap(),
            format!("\"{}\"", action.name())
        );
    }
}

#[test]
fn a_local_ports_reply_leaves_out_what_is_unknown() {
    let reply = Reply::LocalPorts(Box::new(LocalPorts {
        listening: vec![
            LocalPortInfo {
                port: 3000,
                bind: "localhost:3000".into(),
                process: "node".into(),
                pid: 4242,
                directory: Some("~/app".into()),
                uptime_seconds: Some(90),
                paused: true,
                workspace: Some("w1".into()),
                url: Some("http://localhost:3000/".into()),
            },
            LocalPortInfo {
                port: 8080,
                bind: "*:8080".into(),
                process: "python3".into(),
                pid: 4343,
                directory: None,
                uptime_seconds: None,
                paused: false,
                workspace: None,
                url: None,
            },
        ],
        note: None,
    }));
    let text = serde_json::to_string(&reply).unwrap();
    assert!(
        text.starts_with(r#"{"kind":"local_ports","listening":["#),
        "{text}"
    );
    assert!(text.contains(r#""paused":true"#));
    assert!(!text.contains(r#""paused":false"#));
    assert!(!text.contains("note"));
    assert_eq!(serde_json::from_str::<Reply>(&text).unwrap(), reply);
}
