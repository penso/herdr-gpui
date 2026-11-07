#![allow(clippy::unwrap_used)]
use super::*;
use crate::protocol::{Call, Caller, Request};

#[test]
fn open_carries_its_folder_and_refuses_extra_fields() {
    let request = Request::App(Call {
        caller: Caller::default(),
        op: AppOp::Open {
            path: r"C:\Users\me\project".into(),
        },
    });
    let text = serde_json::to_string(&request).unwrap();
    assert!(text.contains(r#""op":"open""#), "{text}");
    assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
    for invalid in [
        r#"{"method":"app","caller":{},"op":{"op":"open"}}"#,
        r#"{"method":"app","caller":{},"op":{"op":"open","path":"/a","cwd":"/b"}}"#,
    ] {
        assert!(
            serde_json::from_str::<Request>(invalid).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn agent_group_ops_keep_their_wire_form() {
    let pick: UiOp = serde_json::from_str(r#"{"op":"agent_group","name":"demo"}"#).unwrap();
    assert_eq!(
        pick,
        UiOp::AgentGroup {
            name: Some("demo".to_owned())
        }
    );
    // No name is every group, so a script may leave it out.
    let all: UiOp = serde_json::from_str(r#"{"op":"agent_group"}"#).unwrap();
    assert_eq!(all, UiOp::AgentGroup { name: None });
    let list: UiOp = serde_json::from_str(r#"{"op":"agent_groups"}"#).unwrap();
    assert_eq!(list, UiOp::AgentGroups);
    assert!(serde_json::from_str::<UiOp>(r#"{"op":"agent_group","group":"demo"}"#).is_err());
}
