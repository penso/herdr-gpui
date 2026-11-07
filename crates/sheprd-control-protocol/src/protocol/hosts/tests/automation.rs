use super::*;

#[test]
fn an_approval_request_keeps_its_shape_on_the_wire() {
    let request = Request::Host(Call {
        caller: Caller::default(),
        op: HostOp::AutomationApprove {
            path: "/repo/.herdr/automations/nightly.luau".into(),
            host: None,
        },
    });
    let text = serde_json::to_string(&request).unwrap();
    assert!(
        text.ends_with(
            r#""op":{"op":"automation_approve","path":"/repo/.herdr/automations/nightly.luau"}}"#
        ),
        "{text}"
    );
    assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
    let named = HostOp::AutomationApprove {
        path: "/a.luau".into(),
        host: Some("buildbox".into()),
    };
    let text = serde_json::to_string(&named).unwrap();
    assert_eq!(
        text,
        r#"{"op":"automation_approve","path":"/a.luau","host":"buildbox"}"#
    );
    assert_eq!(serde_json::from_str::<HostOp>(&text).unwrap(), named);
    assert_eq!(named.wait_seconds(), 0);
    // An approval cannot ride along: the op has no such field.
    assert!(
        serde_json::from_str::<HostOp>(
            r#"{"op":"automation_approve","path":"/a.luau","hash":"abc"}"#
        )
        .is_err()
    );
}
